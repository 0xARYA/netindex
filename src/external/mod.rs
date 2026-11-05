use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use tempfile::TempDir;

use crate::{
    Error, Limits, Target,
    layout::Layout,
    payload::{SlotWriter, Slots},
    target::Entry,
};
use dedup::DiskTable;
use runs::{Row, Sorter};

mod dedup;
mod encode;
mod positioned;
mod runs;

/// Working-set and temporary-disk limits for an external build.
///
/// These bound logical buffers, not allocator overhead, filesystem cache, or RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalOptions {
    /// Maximum target rows sorted in memory before spilling a run.
    pub run_records: usize,
    /// Maximum retained payload key bytes plus 64 bytes charged per cache entry.
    /// Uncached payloads use a disk-backed table with complete byte comparisons.
    /// Zero disables the memory cache, not deduplication.
    pub payload_cache_bytes: usize,
    /// Maximum live temporary-file bytes, including simultaneous merge inputs/output.
    /// Excludes the caller's output and filesystem metadata.
    pub temporary_bytes: u64,
}

impl Default for ExternalOptions {
    fn default() -> Self {
        Self {
            run_records: 16_384,
            payload_cache_bytes: 4 * 1024 * 1024,
            temporary_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Logical build sizes; excludes allocator overhead and filesystem cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalBuildStats {
    /// Assertions retained in the output.
    pub records: usize,
    /// Complete encoded artifact bytes.
    pub artifact_bytes: usize,
    /// Peak live temporary-file bytes charged during the build.
    pub temporary_peak_bytes: u64,
    /// Retained payload keys plus the configured per-entry charge.
    pub payload_cache_bytes: usize,
}

/// Disk-backed builder with bounded target buffers and complete payload deduplication.
///
/// Keeps every assertion and insertion-order ID. Binary merges open two input
/// runs at a time; only logarithmically many completed runs remain. Payloads outside
/// the memory cache are indexed on disk and compared in bounded chunks. Identical
/// payloads share the first-seen span, just as in [`crate::Builder`]. The disk table
/// uses random I/O and is charged to the temporary-byte limit, including both tables
/// during growth. Temporary files are owned by this builder and removed on drop.
/// Cleanup failures or process termination can leave files for the caller to remove.
///
/// # Example
///
/// ```
/// use std::io::{Read, Seek, SeekFrom};
///
/// use netindex::{ExternalBuilder, ExternalOptions, Limits, Reader, Target};
///
/// let directory = tempfile::tempdir()?;
/// let mut builder = ExternalBuilder::new(
///     directory.path(), Limits::default(), ExternalOptions::default(),
/// )?;
/// builder.push(Target::Asn(64512), b"listing")?;
///
/// let mut output = tempfile::tempfile()?;
/// let stats = builder.write_to(&mut output, b"release")?;
///
/// assert_eq!(stats.records, 1);
///
/// output.seek(SeekFrom::Start(0))?;
/// let mut bytes = Vec::new();
/// output.read_to_end(&mut bytes)?;
/// let reader = Reader::open(bytes, Limits::default())?;
///
/// assert_eq!(reader.lookup_asn(64512)?.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ExternalBuilder {
    sorter: Option<Sorter>,
    slots: BufWriter<File>,
    payloads: BufWriter<File>,
    payload_reader: File,
    cache: HashMap<Vec<u8>, (u64, u64, u32)>,
    references: Option<References>,
    cache_bytes: usize,
    disk_table: DiskTable,
    payload_bytes: usize,
    fixed_length: Option<usize>,
    counts: [usize; 3],
    records: usize,
    limits: Limits,
    options: ExternalOptions,
    budget: DiskBudget,
    failed: bool,
    // Close file handles before removing the directory on Windows.
    directory: TempDir,
}

impl ExternalBuilder {
    /// Create private temporary files beneath an existing caller-selected directory.
    /// Chooses compact shared-value references when smaller, emitting format 4;
    /// otherwise retains format 3. Use [`Self::new_legacy`] for 2.x readers.
    /// Temporary IDs, directory spans, and enlarged deduplication cells are
    /// charged to `temporary_bytes`; no distinct-value directory is held in memory.
    ///
    /// # Errors
    /// Fails on invalid run size, failed memory reservations, or temporary-file
    /// creation errors.
    pub fn new(directory: &Path, limits: Limits, options: ExternalOptions) -> Result<Self, Error> {
        Self::create(directory, limits, options, true)
    }

    /// Build format-3 output readable by netindex 2.x.
    /// Uses the same sorting and complete payload deduplication as [`Self::new`].
    ///
    /// # Errors
    /// Fails on invalid options, failed allocations, or temporary-file errors.
    pub fn new_legacy(
        directory: &Path,
        limits: Limits,
        options: ExternalOptions,
    ) -> Result<Self, Error> {
        Self::create(directory, limits, options, false)
    }

    fn create(
        directory: &Path,
        limits: Limits,
        options: ExternalOptions,
        compact: bool,
    ) -> Result<Self, Error> {
        if options.run_records == 0 {
            return Err(Error::Invalid("external run size"));
        }

        let sorter = Sorter::new(options.run_records.min(limits.records.max(1)))?;
        let directory = tempfile::Builder::new()
            .prefix("netindex-")
            .tempdir_in(directory)
            .map_err(temporary("create build directory"))?;
        let slots = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.path().join("slots"))
            .map_err(temporary("create payload slots"))?;
        let payloads = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.path().join("payloads"))
            .map_err(temporary("create payload pool"))?;
        let payload_reader = File::open(directory.path().join("payloads"))
            .map_err(temporary("open payload comparison handle"))?;

        let references = if compact {
            Some(References::new(directory.path())?)
        } else {
            None
        };

        Ok(Self {
            references,
            directory,
            sorter: Some(sorter),
            slots: BufWriter::new(slots),
            payloads: BufWriter::new(payloads),
            payload_reader,
            cache: HashMap::new(),
            cache_bytes: 0,
            disk_table: if compact {
                DiskTable::with_references()
            } else {
                DiskTable::new()
            },
            payload_bytes: 0,
            fixed_length: None,
            counts: [0; 3],
            records: 0,
            limits,
            options,
            budget: DiskBudget {
                used: 0,
                peak: 0,
                limit: options.temporary_bytes,
            },
            failed: false,
        })
    }

    /// Append an unsorted assertion. Payload bytes are copied to disk or a bounded cache.
    ///
    /// Any failed push invalidates the builder: subsequent pushes and finalization
    /// return an error. This prevents publication after partial temporary-file I/O.
    ///
    /// # Errors
    /// Fails on invalid targets, exceeded limits, failed memory reservations, or
    /// temporary-file I/O errors.
    pub fn push(&mut self, target: Target, payload: &[u8]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("failed external builder"));
        }

        let result = self.append(target, payload);
        if result.is_err() {
            self.failed = true;
        }

        result
    }

    /// Merge runs and stream an index into a seekable output.
    ///
    /// Output must start at position zero in an empty file. Sorting and layout
    /// checks precede output. Later I/O failure may leave partial bytes; the caller
    /// must discard them. Does not synchronize, validate payloads, or publish.
    ///
    /// # Errors
    /// Fails on invalid builder state or output position, exceeded limits, or
    /// temporary-file or output I/O errors.
    pub fn write_to(
        mut self,
        output: impl Write + Seek,
        metadata: &[u8],
    ) -> Result<ExternalBuildStats, Error> {
        if self.failed {
            return Err(Error::Invalid("failed external builder"));
        }

        self.cache = HashMap::new();
        self.disk_table.close(&mut self.budget)?;
        self.slots
            .flush()
            .map_err(temporary("flush payload slots"))?;
        self.payloads
            .flush()
            .map_err(temporary("flush payload pool"))?;

        let sorter = self
            .sorter
            .take()
            .ok_or(Error::Invalid("finished external builder"))?;
        let mut run = sorter.finish(self.directory.path(), &mut self.budget)?;

        let packed = encode::sizes(run.file(), self.counts)?;
        let slot_encoding = Slots::select(self.records, self.payload_bytes, self.fixed_length)?;
        let slot_encoding = match &self.references {
            Some(references) => slot_encoding.indexed(
                self.records,
                self.payload_bytes,
                references.values as usize,
            )?,
            None => slot_encoding,
        };
        let layout = Layout::with_packed(
            self.counts,
            metadata.len(),
            self.payload_bytes,
            self.limits,
            packed,
            slot_encoding,
        )?;

        let mut output = BufWriter::new(output);
        if output.stream_position()? != 0 || output.seek(SeekFrom::End(0))? != 0 {
            return Err(Error::Invalid("nonempty external output"));
        }

        layout.write_header(&mut output)?;
        output.write_all(metadata)?;
        encode::write_sections(run.file(), &mut output, self.counts, packed)?;

        if let Slots::Indexed { values, width } = slot_encoding {
            self.references
                .as_mut()
                .ok_or(Error::Invalid("payload reference files"))?
                .write(&mut output, values, width)?;
        }
        self.slots
            .get_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind payload slots"))?;
        let mut slots = BufReader::new(self.slots.get_mut());
        if let Slots::Indexed { width, .. } = slot_encoding {
            for _ in 0..self.records {
                let (_, _, ordinal) = read_slot(&mut slots, true)?;
                crate::payload::write_ordinal(&mut output, ordinal, width)?;
            }
        } else {
            let mut encoded = SlotWriter::new(&mut output, slot_encoding)?;
            for _ in 0..self.records {
                let (offset, length, _) = read_slot(&mut slots, self.references.is_some())?;
                encoded.push(offset, length)?;
            }
        }

        self.payloads
            .get_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind payload pool"))?;
        copy_payloads(self.payloads.get_mut(), &mut output, self.payload_bytes)?;

        output.flush()?;

        Ok(ExternalBuildStats {
            records: self.records,
            artifact_bytes: layout.bytes,
            temporary_peak_bytes: self.budget.peak,
            payload_cache_bytes: self.cache_bytes,
        })
    }

    fn append(&mut self, target: Target, payload: &[u8]) -> Result<(), Error> {
        let id = u32::try_from(self.records).map_err(|_| Error::Limit("records"))?;

        let row = match target {
            Target::Asn(0) => return Err(Error::Invalid("ASN zero")),
            Target::Asn(number) => Row::new(
                Entry {
                    start: u128::from(number),
                    end: u128::from(number),
                    maximum: u128::from(number),
                    id,
                    prefix: 0,
                    kind: 0,
                },
                2,
            ),
            target => {
                let (entry, ipv6) = target.entry(id)?;
                Row::new(entry, u8::from(ipv6))
            }
        };

        let mut counts = self.counts;
        let count = counts
            .get_mut(usize::from(row.section))
            .ok_or(Error::Invalid("temporary section"))?;
        *count = count.checked_add(1).ok_or(Error::Limit("records"))?;

        let hash = self.disk_table.hash(payload);
        let existing = match self.cache.get(payload).copied() {
            Some(span) => Some(span),
            None => self.disk_table.find_reference(
                hash,
                payload,
                &mut self.payloads,
                &self.payload_reader,
                self.payload_bytes as u64,
            )?,
        };
        let added = if existing.is_some() { 0 } else { payload.len() };
        let payload_bytes = self
            .payload_bytes
            .checked_add(added)
            .ok_or(Error::Limit("payload bytes"))?;
        Layout::new(counts, 0, payload_bytes, self.limits)?;

        let reference_bytes = if self.references.is_some() {
            (if existing.is_none() { 16 } else { 0 })
                + if payload.len() >= u32::MAX as usize {
                    8
                } else {
                    0
                }
        } else {
            0
        };
        self.budget.reserve(16 + added as u64 + reference_bytes)?;

        let charge = payload
            .len()
            .checked_add(64)
            .ok_or(Error::Limit("payload cache bytes"))?;
        let cache_key = if existing.is_none()
            && charge
                <= self
                    .options
                    .payload_cache_bytes
                    .saturating_sub(self.cache_bytes)
        {
            self.cache.try_reserve(1)?;

            let mut key = Vec::new();
            key.try_reserve_exact(payload.len())?;
            key.extend_from_slice(payload);
            Some(key)
        } else {
            None
        };

        self.sorter
            .as_mut()
            .ok_or(Error::Invalid("finished external builder"))?
            .push(row, self.directory.path(), &mut self.budget)?;

        let ordinal = self
            .references
            .as_ref()
            .map_or(0, |references| references.values);
        let span = existing.unwrap_or((self.payload_bytes as u64, payload.len() as u64, ordinal));
        if existing.is_none() {
            if cache_key.is_none() {
                self.disk_table.insert_reference(
                    hash,
                    span,
                    self.directory.path(),
                    &mut self.budget,
                )?;
            }
            self.payloads
                .write_all(payload)
                .map_err(temporary("write payload pool"))?;
        }

        write_slot(&mut self.slots, span, self.references.is_some())?;
        if let Some(references) = &mut self.references {
            references.push(span, existing.is_none())?;
        }

        if let Some(key) = cache_key {
            self.cache.insert(key, span);
            self.cache_bytes += charge;
        }

        self.fixed_length = if self.records == 0 {
            Some(payload.len())
        } else {
            self.fixed_length.filter(|&length| length == payload.len())
        };
        self.counts = counts;
        self.payload_bytes = payload_bytes;
        self.records += 1;

        Ok(())
    }
}

fn write_slot(output: &mut impl Write, span: (u64, u64, u32), compact: bool) -> Result<(), Error> {
    output
        .write_all(&span.0.to_le_bytes())
        .map_err(temporary("write payload slots"))?;
    if compact {
        let length = u32::try_from(span.1).unwrap_or(u32::MAX);
        output
            .write_all(&length.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
        output
            .write_all(&span.2.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
        if length == u32::MAX {
            output
                .write_all(&span.1.to_le_bytes())
                .map_err(temporary("write payload slots"))?;
        }
    } else {
        output
            .write_all(&span.1.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
    }
    Ok(())
}

// Compact builds reuse the 16-byte slot spill for offset, u32 length and u32
// ordinal. The sentinel length escapes to u64 so wide payloads retain support.
fn read_slot(input: &mut impl Read, compact: bool) -> Result<(u64, u64, u32), Error> {
    let mut bytes = [0; 16];
    input
        .read_exact(&mut bytes)
        .map_err(temporary("read payload slots"))?;
    let offset = crate::layout::usize_at(&bytes, 0)? as u64;
    if !compact {
        return Ok((offset, crate::layout::usize_at(&bytes, 8)? as u64, 0));
    }
    let length = crate::layout::u32_at(&bytes, 8)?;
    let ordinal = crate::layout::u32_at(&bytes, 12)?;
    let length = if length == u32::MAX {
        let mut word = [0; 8];
        input
            .read_exact(&mut word)
            .map_err(temporary("read wide payload slot"))?;
        u64::from_le_bytes(word)
    } else {
        u64::from(length)
    };
    Ok((offset, length, ordinal))
}

struct References {
    directory: BufWriter<File>,
    values: u32,
}

impl References {
    fn new(directory: &Path) -> Result<Self, Error> {
        Ok(Self {
            directory: BufWriter::new(
                tempfile::tempfile_in(directory).map_err(temporary("create payload directory"))?,
            ),
            values: 0,
        })
    }

    fn push(&mut self, span: (u64, u64, u32), new: bool) -> Result<(), Error> {
        if new {
            self.directory
                .write_all(&span.0.to_le_bytes())
                .map_err(temporary("write payload directory"))?;
            self.directory
                .write_all(&span.1.to_le_bytes())
                .map_err(temporary("write payload directory"))?;
            self.values = self
                .values
                .checked_add(1)
                .ok_or(Error::Limit("payload values"))?;
        }
        Ok(())
    }

    fn write(&mut self, output: &mut impl Write, values: usize, width: u8) -> Result<(), Error> {
        self.directory
            .flush()
            .map_err(temporary("flush payload directory"))?;
        self.directory
            .get_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind payload directory"))?;
        output.write_all(&(values as u32).to_le_bytes())?;
        output.write_all(&[width, 0, 0, 0])?;
        let mut directory = BufReader::new(self.directory.get_mut());
        for _ in 0..values {
            let mut span = [0; 16];
            directory
                .read_exact(&mut span)
                .map_err(temporary("read payload directory"))?;
            crate::payload::write_directory(
                output,
                crate::layout::usize_at(&span, 0)? as u64,
                crate::layout::usize_at(&span, 8)? as u64,
            )?;
        }
        Ok(())
    }
}

impl fmt::Debug for ExternalBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalBuilder")
            .field("records", &self.records)
            .field("payload_bytes", &self.payload_bytes)
            .field("temporary_bytes", &self.budget.used)
            .field("options", &self.options)
            .field("failed", &self.failed)
            .finish()
    }
}

#[derive(Debug)]
struct DiskBudget {
    used: u64,
    peak: u64,
    limit: u64,
}

impl DiskBudget {
    fn reserve(&mut self, bytes: u64) -> Result<(), Error> {
        let used = self
            .used
            .checked_add(bytes)
            .ok_or(Error::Limit("temporary bytes"))?;
        if used > self.limit {
            return Err(Error::Limit("temporary bytes"));
        }

        self.used = used;
        self.peak = self.peak.max(used);

        Ok(())
    }

    fn release(&mut self, bytes: u64) -> Result<(), Error> {
        self.used = self
            .used
            .checked_sub(bytes)
            .ok_or(Error::Invalid("temporary byte accounting"))?;

        Ok(())
    }
}

fn copy_payloads(input: &mut File, output: &mut impl Write, bytes: usize) -> Result<(), Error> {
    let mut remaining = bytes;
    let mut buffer = [0; 8192];

    while remaining > 0 {
        let length = remaining.min(buffer.len());
        let chunk = buffer
            .get_mut(..length)
            .ok_or(Error::Invalid("payload copy buffer"))?;

        input
            .read_exact(chunk)
            .map_err(temporary("read payload pool"))?;
        output.write_all(chunk)?;

        remaining -= length;
    }

    Ok(())
}

fn temporary(operation: &'static str) -> impl FnOnce(std::io::Error) -> Error {
    move |error| Error::TemporaryIo { operation, error }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn failed_disk_payload_comparison_poisoning_prevents_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let mut builder = ExternalBuilder::new(
            directory.path(),
            Limits::default(),
            ExternalOptions {
                payload_cache_bytes: 0,
                ..Default::default()
            },
        )
        .unwrap();
        builder.push(Target::Asn(1), b"payload").unwrap();
        builder.payloads.flush().unwrap();
        builder.payloads = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .open(builder.directory.path().join("payloads"))
                .unwrap(),
        );
        builder.payload_reader = OpenOptions::new()
            .write(true)
            .open(builder.directory.path().join("payloads"))
            .unwrap();

        assert!(matches!(
            builder.push(Target::Asn(2), b"payload"),
            Err(Error::TemporaryIo {
                operation: "compare payload pool",
                ..
            })
        ));
        let mut output = Cursor::new(Vec::new());
        assert!(matches!(
            builder.write_to(&mut output, b""),
            Err(Error::Invalid("failed external builder"))
        ));
        assert!(output.get_ref().is_empty());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_temporary_payload_io_poisoning_prevents_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let mut builder =
            ExternalBuilder::new(directory.path(), Limits::default(), Default::default()).unwrap();
        builder.payloads =
            BufWriter::new(File::open(builder.directory.path().join("payloads")).unwrap());

        let error = builder.push(Target::Asn(1), &vec![0; 16_384]).unwrap_err();

        assert!(matches!(
            error,
            Error::TemporaryIo {
                operation: "write payload pool",
                ..
            }
        ));

        assert!(matches!(
            builder.write_to(Cursor::new(Vec::new()), b""),
            Err(Error::Invalid("failed external builder"))
        ));

        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    #[test]
    fn temporary_slots_preserve_wide_lengths_and_ids_with_exact_framing() {
        for length in [
            0,
            1,
            u64::from(u32::MAX) - 1,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
        ] {
            let mut bytes = Vec::new();
            write_slot(&mut bytes, (8, length, 513), true).unwrap();
            assert_eq!(
                bytes.len(),
                if length >= u64::from(u32::MAX) {
                    24
                } else {
                    16
                }
            );
            write_slot(&mut bytes, (12, 1, 9), true).unwrap();
            let mut input = bytes.as_slice();
            assert_eq!(read_slot(&mut input, true).unwrap(), (8, length, 513));
            assert_eq!(read_slot(&mut input, true).unwrap(), (12, 1, 9));
            assert!(input.is_empty());
            for end in 0..bytes.len() - 16 {
                assert!(read_slot(&mut &bytes[..end], true).is_err());
            }
        }
    }
}
