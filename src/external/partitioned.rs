use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, hash_map::RandomState},
    hash::BuildHasher,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use tempfile::NamedTempFile;

use crate::{Error, layout::Layout};

use super::{DiskBudget, ExternalBuilder, dedup::DiskTable, positioned, temporary, write_slot};

pub(super) struct Partitioned {
    parts: Vec<Spill>,
    state: RandomState,
    #[cfg(test)]
    forced_hash: Option<u64>,
}

struct Spill {
    output: BufWriter<NamedTempFile>,
    bytes: u64,
}

impl Spill {
    fn new(directory: &Path) -> Result<Self, Error> {
        Ok(Self {
            output: BufWriter::new(
                NamedTempFile::new_in(directory).map_err(temporary("create partition"))?,
            ),
            bytes: 0,
        })
    }

    fn reserve(&mut self, count: u64, budget: &mut DiskBudget) -> Result<(), Error> {
        let bytes = self
            .bytes
            .checked_add(count)
            .ok_or(Error::Limit("partition bytes"))?;
        budget.reserve(count)?;
        self.bytes = bytes;
        Ok(())
    }

    fn finish(mut self) -> Result<(NamedTempFile, u64), Error> {
        self.output.flush().map_err(temporary("flush partition"))?;
        let file = self
            .output
            .into_inner()
            .map_err(|error| temporary("finish partition")(error.into_error()))?;
        Ok((file, self.bytes))
    }
}

struct Processed {
    values: NamedTempFile,
    value_bytes: u64,
    references: NamedTempFile,
    reference_bytes: u64,
    spans: Spill,
}

#[derive(Clone, Copy)]
struct Span {
    offset: u64,
    length: u64,
    ordinal: u32,
}

impl Span {
    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            offset: u64_at(bytes, 0)?,
            length: u64_at(bytes, 8)?,
            ordinal: crate::layout::u32_at(bytes, 16)?,
        })
    }

    fn write(self, output: &mut impl Write) -> Result<(), Error> {
        output
            .write_all(&self.offset.to_le_bytes())
            .map_err(temporary("write partition span"))?;
        output
            .write_all(&self.length.to_le_bytes())
            .map_err(temporary("write partition span"))?;
        output
            .write_all(&self.ordinal.to_le_bytes())
            .map_err(temporary("write partition span"))?;
        Ok(())
    }
}

impl Partitioned {
    pub(super) fn new(directory: &Path, partitions: usize) -> Result<Self, Error> {
        if !partitions.is_power_of_two() || partitions > 64 {
            return Err(Error::Invalid("payload partition count"));
        }
        let mut parts = Vec::new();
        parts.try_reserve_exact(partitions)?;
        for _ in 0..partitions {
            parts.push(Spill::new(directory)?);
        }
        Ok(Self {
            parts,
            state: RandomState::new(),
            #[cfg(test)]
            forced_hash: None,
        })
    }

    pub(super) fn push(
        &mut self,
        id: u32,
        payload: &[u8],
        budget: &mut DiskBudget,
    ) -> Result<(), Error> {
        let hash = self.state.hash_one(payload);
        #[cfg(test)]
        let hash = self.forced_hash.unwrap_or(hash);
        self.push_hashed(id, payload, hash, budget)
    }

    fn push_hashed(
        &mut self,
        id: u32,
        payload: &[u8],
        hash: u64,
        budget: &mut DiskBudget,
    ) -> Result<(), Error> {
        let at = (hash >> 58) as usize & (self.parts.len() - 1);
        let part = self
            .parts
            .get_mut(at)
            .ok_or(Error::Invalid("payload partition"))?;
        part.reserve(20 + payload.len() as u64, budget)?;
        part.output
            .write_all(&id.to_le_bytes())
            .map_err(temporary("write partition"))?;
        part.output
            .write_all(&(payload.len() as u64).to_le_bytes())
            .map_err(temporary("write partition"))?;
        part.output
            .write_all(&hash.to_le_bytes())
            .map_err(temporary("write partition"))?;
        part.output
            .write_all(payload)
            .map_err(temporary("write partition"))?;
        Ok(())
    }

    pub(super) fn finish(self, builder: &mut ExternalBuilder) -> Result<(), Error> {
        let mut parts = Vec::new();
        parts.try_reserve_exact(self.parts.len())?;
        // Flush all inputs before processing to make their buffered bytes available
        // through independent comparison handles, including on Windows.
        let mut inputs = Vec::new();
        inputs.try_reserve_exact(self.parts.len())?;
        for part in self.parts {
            inputs.push(part.finish()?);
        }
        for (file, bytes) in inputs {
            parts.push(process(file, bytes, builder)?);
        }
        merge_values(&mut parts, builder)?;

        let mut resolved = Vec::new();
        resolved.try_reserve_exact(parts.len())?;
        for part in parts {
            resolved.push(resolve(part, builder)?);
        }
        merge_references(resolved, builder)
    }
}

fn process(
    mut file: NamedTempFile,
    bytes: u64,
    builder: &mut ExternalBuilder,
) -> Result<Processed, Error> {
    let directory = builder.directory.path();
    file.as_file_mut()
        .seek(SeekFrom::Start(0))
        .map_err(temporary("rewind partition"))?;
    let source = file
        .reopen()
        .map_err(temporary("open partition comparison"))?;
    let mut input = BufReader::new(file.as_file_mut());
    let values = NamedTempFile::new_in(directory).map_err(temporary("create partition values"))?;
    let mut output = BufWriter::new(
        values
            .reopen()
            .map_err(temporary("open partition values"))?,
    );
    let comparison = values
        .reopen()
        .map_err(temporary("open partition comparison"))?;
    let mut references = Spill::new(directory)?;
    let mut table = DiskTable::with_references();
    let mut keys = HashMap::<Vec<u8>, Span>::new();
    let mut charged = 0usize;
    let mut end = 0u64;
    let mut position = 0u64;
    let mut unique = 0u32;
    let mut scratch = [0; 8192];

    while let Some(header) = read_header::<20>(&mut input)? {
        let id = crate::layout::u32_at(&header, 0)?;
        let length = u64_at(&header, 4)?;
        let hash = u64_at(&header, 12)?;
        position = position
            .checked_add(20)
            .ok_or(Error::Limit("partition bytes"))?;
        let next = position
            .checked_add(length)
            .filter(|&end| end <= bytes)
            .ok_or(Error::Invalid("partition payload span"))?;
        let small = usize::try_from(length)
            .ok()
            .and_then(|length| scratch.get_mut(..length));
        let existing = if let Some(payload) = small {
            input
                .read_exact(payload)
                .map_err(temporary("read partition payload"))?;
            match keys.get(payload as &[u8]).copied() {
                Some(span) => Some(span),
                None => table
                    .find_reference(hash, payload, &mut output, &comparison, end)?
                    .map(|(offset, length, ordinal)| Span {
                        offset,
                        length,
                        ordinal,
                    }),
            }
        } else {
            table
                .find_file_reference(
                    hash,
                    (&source, position, length),
                    &mut output,
                    (&comparison, end),
                )?
                .map(|(offset, length, ordinal)| Span {
                    offset,
                    length,
                    ordinal,
                })
        };
        let span = if let Some(span) = existing {
            if length > scratch.len() as u64 {
                input
                    .seek(SeekFrom::Start(next))
                    .map_err(temporary("skip partition payload"))?;
            }
            span
        } else {
            let offset = end
                .checked_add(12)
                .ok_or(Error::Limit("partition values"))?;
            let span = Span {
                offset,
                length,
                ordinal: unique,
            };
            let value_end = offset
                .checked_add(length)
                .ok_or(Error::Limit("partition values"))?;
            builder.budget.reserve(12 + length)?;
            output
                .write_all(&id.to_le_bytes())
                .map_err(temporary("write partition values"))?;
            output
                .write_all(&length.to_le_bytes())
                .map_err(temporary("write partition values"))?;
            if length <= scratch.len() as u64 {
                let payload = scratch
                    .get(..length as usize)
                    .ok_or(Error::Invalid("partition payload buffer"))?;
                output
                    .write_all(payload)
                    .map_err(temporary("write partition values"))?;
                let charge = payload
                    .len()
                    .checked_add(64)
                    .ok_or(Error::Limit("payload cache bytes"))?;
                if charge <= builder.options.payload_cache_bytes.saturating_sub(charged) {
                    keys.try_reserve(1)?;
                    let mut key = Vec::new();
                    key.try_reserve_exact(payload.len())?;
                    key.extend_from_slice(payload);
                    keys.insert(key, span);
                    charged += charge;
                } else {
                    table.insert_reference(
                        hash,
                        (offset, length, unique),
                        directory,
                        &mut builder.budget,
                    )?;
                }
            } else {
                copy_bytes(&mut input, &mut output, length)?;
                table.insert_reference(
                    hash,
                    (offset, length, unique),
                    directory,
                    &mut builder.budget,
                )?;
            }
            unique = unique
                .checked_add(1)
                .ok_or(Error::Limit("payload values"))?;
            end = value_end;
            span
        };
        references.reserve(8, &mut builder.budget)?;
        references
            .output
            .write_all(&id.to_le_bytes())
            .map_err(temporary("write partition reference"))?;
        references
            .output
            .write_all(&span.ordinal.to_le_bytes())
            .map_err(temporary("write partition reference"))?;
        position = next;
    }
    if position != bytes {
        return Err(Error::Invalid("partition length"));
    }
    output
        .flush()
        .map_err(temporary("flush partition values"))?;
    drop(output);
    table.close(&mut builder.budget)?;
    drop(input);
    drop(source);
    file.close().map_err(temporary("remove partition"))?;
    builder.budget.release(bytes)?;
    builder.cache_bytes = builder.cache_bytes.max(charged);
    let (references, reference_bytes) = references.finish()?;
    Ok(Processed {
        values,
        value_bytes: end,
        references,
        reference_bytes,
        spans: Spill::new(directory)?,
    })
}

fn merge_values(parts: &mut [Processed], builder: &mut ExternalBuilder) -> Result<(), Error> {
    let mut inputs = Vec::new();
    inputs.try_reserve_exact(parts.len())?;
    let mut heap = BinaryHeap::new();
    heap.try_reserve(parts.len())?;
    for (index, part) in parts.iter_mut().enumerate() {
        part.values
            .as_file_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind partition values"))?;
        let mut input = BufReader::new(part.values.as_file_mut());
        if let Some(header) = read_header::<12>(&mut input)? {
            heap.push(Reverse((
                crate::layout::u32_at(&header, 0)?,
                index,
                u64_at(&header, 4)?,
            )));
        }
        inputs.push((input, &mut part.spans));
    }
    let mut ordinal = 0u32;
    while let Some(Reverse((_, index, length))) = heap.pop() {
        let (input, spans) = inputs
            .get_mut(index)
            .ok_or(Error::Invalid("partition merge"))?;
        let end = builder
            .payload_bytes
            .checked_add(usize::try_from(length).map_err(|_| Error::Limit("payload bytes"))?)
            .ok_or(Error::Limit("payload bytes"))?;
        Layout::new(builder.counts, 0, end, builder.limits)?;
        let span = Span {
            offset: builder.payload_bytes as u64,
            length,
            ordinal,
        };
        builder
            .budget
            .reserve(length + if builder.references.is_some() { 16 } else { 0 })?;
        copy_bytes(input, &mut builder.payloads, length)?;
        if let Some(references) = &mut builder.references {
            references.push((span.offset, length, ordinal), true)?;
        }
        spans.reserve(20, &mut builder.budget)?;
        span.write(&mut spans.output)?;
        builder.payload_bytes = end;
        ordinal = ordinal
            .checked_add(1)
            .ok_or(Error::Limit("payload values"))?;
        if let Some(header) = read_header::<12>(input)? {
            heap.push(Reverse((
                crate::layout::u32_at(&header, 0)?,
                index,
                u64_at(&header, 4)?,
            )));
        }
    }
    Ok(())
}

fn resolve(part: Processed, builder: &mut ExternalBuilder) -> Result<(NamedTempFile, u64), Error> {
    part.values
        .close()
        .map_err(temporary("remove partition values"))?;
    builder.budget.release(part.value_bytes)?;
    let (spans, span_bytes) = part.spans.finish()?;
    let count = span_bytes / 20;
    let mut cached = Vec::new();
    if count <= (builder.options.payload_cache_bytes / size_of::<Span>()) as u64 {
        cached.try_reserve_exact(count as usize)?;
        if cached.capacity() > builder.options.payload_cache_bytes / size_of::<Span>() {
            return Err(Error::Limit("partition span cache bytes"));
        }
        let mut reader = BufReader::new(spans.reopen().map_err(temporary("open partition spans"))?);
        for _ in 0..count {
            cached.push(Span::decode(
                &read_header::<20>(&mut reader)?.ok_or(Error::Invalid("partition span count"))?,
            )?);
        }
    }
    let span_reader = spans.reopen().map_err(temporary("open partition spans"))?;
    let mut reference_file = part.references;
    reference_file
        .as_file_mut()
        .seek(SeekFrom::Start(0))
        .map_err(temporary("rewind partition references"))?;
    let mut input = BufReader::new(reference_file.as_file_mut());
    let mut output = Spill::new(builder.directory.path())?;
    while let Some(header) = read_header::<8>(&mut input)? {
        let id = crate::layout::u32_at(&header, 0)?;
        let local = crate::layout::u32_at(&header, 4)? as usize;
        let span = if let Some(&span) = cached.get(local) {
            span
        } else {
            if local as u64 >= count {
                return Err(Error::Invalid("partition reference"));
            }
            let mut bytes = [0; 20];
            positioned::read_exact(&span_reader, &mut bytes, local as u64 * 20)
                .map_err(temporary("read partition span"))?;
            Span::decode(&bytes)?
        };
        output.reserve(24, &mut builder.budget)?;
        output
            .output
            .write_all(&id.to_le_bytes())
            .map_err(temporary("write resolved reference"))?;
        span.write(&mut output.output)?;
    }
    drop(input);
    reference_file
        .close()
        .map_err(temporary("remove partition references"))?;
    builder.budget.release(part.reference_bytes)?;
    drop(span_reader);
    spans.close().map_err(temporary("remove partition spans"))?;
    builder.budget.release(span_bytes)?;
    output.finish()
}

fn merge_references(
    mut files: Vec<(NamedTempFile, u64)>,
    builder: &mut ExternalBuilder,
) -> Result<(), Error> {
    let mut readers = Vec::new();
    readers.try_reserve_exact(files.len())?;
    let mut heap = BinaryHeap::new();
    heap.try_reserve(files.len())?;
    for (index, (file, _)) in files.iter_mut().enumerate() {
        file.as_file_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind resolved references"))?;
        let mut reader = BufReader::new(file.as_file_mut());
        if let Some(header) = read_header::<24>(&mut reader)? {
            heap.push(Reverse((crate::layout::u32_at(&header, 0)?, index, header)));
        }
        readers.push(reader);
    }
    let mut count = 0usize;
    while let Some(Reverse((id, index, header))) = heap.pop() {
        if id as usize != count {
            return Err(Error::Invalid("partition insertion IDs"));
        }
        let span = Span::decode(
            header
                .get(4..)
                .ok_or(Error::Invalid("resolved reference"))?,
        )?;
        let compact = builder.references.is_some();
        builder.budget.reserve(
            16 + if compact && span.length >= u64::from(u32::MAX) {
                8
            } else {
                0
            },
        )?;
        write_slot(
            &mut builder.slots,
            (span.offset, span.length, span.ordinal),
            compact,
        )?;
        count += 1;
        let input = readers
            .get_mut(index)
            .ok_or(Error::Invalid("partition reference merge"))?;
        if let Some(header) = read_header::<24>(input)? {
            heap.push(Reverse((crate::layout::u32_at(&header, 0)?, index, header)));
        }
    }
    if count != builder.records {
        return Err(Error::Invalid("partition record count"));
    }
    drop(readers);
    for (file, bytes) in files {
        file.close()
            .map_err(temporary("remove resolved references"))?;
        builder.budget.release(bytes)?;
    }
    Ok(())
}

fn read_header<const N: usize>(input: &mut impl Read) -> Result<Option<[u8; N]>, Error> {
    let mut header = [0; N];
    let first = header
        .get_mut(..1)
        .ok_or(Error::Invalid("partition header"))?;
    loop {
        match input.read(first) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(temporary("read partition header")(error)),
        }
    }
    input
        .read_exact(
            header
                .get_mut(1..)
                .ok_or(Error::Invalid("partition header"))?,
        )
        .map_err(temporary("read partition header"))?;
    Ok(Some(header))
}

fn copy_bytes(input: &mut impl Read, output: &mut impl Write, mut count: u64) -> Result<(), Error> {
    let mut bytes = [0; 8192];
    while count > 0 {
        let size = count.min(bytes.len() as u64) as usize;
        let chunk = bytes
            .get_mut(..size)
            .ok_or(Error::Invalid("partition copy buffer"))?;
        input
            .read_exact(chunk)
            .map_err(temporary("read partition payload"))?;
        output
            .write_all(chunk)
            .map_err(temporary("write partition payload"))?;
        count -= size as u64;
    }
    Ok(())
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let word = crate::layout::slice(bytes, offset, 8)?;
    Ok(u64::from_le_bytes(
        word.try_into()
            .map_err(|_| Error::Invalid("partition word"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Builder, ExternalOptions, Limits, Target};
    use std::io::Cursor;

    #[test]
    fn full_hash_collisions_and_large_last_byte_differences_preserve_first_seen_order() {
        let directory = tempfile::tempdir().unwrap();
        let mut actual = ExternalBuilder::new_partitioned(
            directory.path(),
            Limits::default(),
            ExternalOptions {
                run_records: 3,
                payload_cache_bytes: 0,
                temporary_bytes: 1 << 24,
            },
            4,
        )
        .unwrap();
        actual.partitions.as_mut().unwrap().forced_hash = Some(7);
        let mut expected = Builder::new(Limits::default());
        for id in 0..128u32 {
            let key = (id % 17) as u8;
            let mut payload = vec![0x5a; if key & 1 == 0 { 16_385 } else { 65 }];
            *payload.last_mut().unwrap() = key;
            let target = Target::Asn(id + 1);
            actual.push(target, &payload).unwrap();
            expected.push(target, &payload).unwrap();
        }
        let mut output = Cursor::new(Vec::new());
        actual.write_to(&mut output, b"collision").unwrap();
        assert_eq!(
            output.into_inner(),
            expected.into_bytes(b"collision").unwrap()
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn truncated_headers_and_payloads_keep_the_io_cause() {
        assert!(
            matches!(read_header::<20>(&mut Cursor::new(b"x")), Err(Error::TemporaryIo { operation: "read partition header", error }) if error.kind() == io::ErrorKind::UnexpectedEof)
        );
        assert!(
            matches!(copy_bytes(&mut Cursor::new(b"x"), &mut Vec::new(), 2), Err(Error::TemporaryIo { operation: "read partition payload", error }) if error.kind() == io::ErrorKind::UnexpectedEof)
        );
    }
}
