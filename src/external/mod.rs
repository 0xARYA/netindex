use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use tempfile::TempDir;

use crate::{
    layout::Layout,
    payload::{SlotWriter, Slots},
    target::Entry,
    Error, Limits, Target,
};
use runs::{Row, Sorter};

mod encode;
mod runs;

/// Working-set and temporary-disk limits for an external build.
///
/// These bound logical buffers, not allocator overhead, filesystem cache, or RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalOptions {
    /// Maximum target rows sorted in memory before spilling a run.
    pub run_records: usize,
    /// Maximum retained payload key bytes plus 64 bytes charged per cache entry.
    /// Once full, uncached payloads are written separately, even if identical.
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

/// Disk-backed builder with bounded target buffers and a payload cache.
///
/// Keeps every assertion and insertion-order ID. Binary merges open two input
/// runs at a time; only logarithmically many completed runs remain. Cache misses
/// can produce a larger artifact than [`crate::Builder`], without changing lookup
/// results. Temporary files are owned by this builder and removed on drop.
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
    cache: HashMap<Vec<u8>, (u64, u64)>,
    cache_bytes: usize,
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
    ///
    /// # Errors
    /// Fails on invalid run size, failed memory reservations, or temporary-file
    /// creation errors.
    pub fn new(directory: &Path, limits: Limits, options: ExternalOptions) -> Result<Self, Error> {
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

        Ok(Self {
            directory,
            sorter: Some(sorter),
            slots: BufWriter::new(slots),
            payloads: BufWriter::new(payloads),
            cache: HashMap::new(),
            cache_bytes: 0,
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

        self.slots
            .get_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind payload slots"))?;

        let mut slots = BufReader::new(self.slots.get_mut());
        let mut encoded = SlotWriter::new(&mut output, slot_encoding)?;

        for _ in 0..self.records {
            let mut bytes = [0; 16];
            slots
                .read_exact(&mut bytes)
                .map_err(temporary("read payload slots"))?;

            encoded.push(
                crate::layout::usize_at(&bytes, 0)? as u64,
                crate::layout::usize_at(&bytes, 8)? as u64,
            )?;
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

        let existing = self.cache.get(payload).copied();
        let added = if existing.is_some() { 0 } else { payload.len() };
        let payload_bytes = self
            .payload_bytes
            .checked_add(added)
            .ok_or(Error::Limit("payload bytes"))?;
        Layout::new(counts, 0, payload_bytes, self.limits)?;

        self.budget.reserve(16 + added as u64)?;

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

        let span = existing.unwrap_or((self.payload_bytes as u64, payload.len() as u64));
        if existing.is_none() {
            self.payloads
                .write_all(payload)
                .map_err(temporary("write payload pool"))?;
        }

        self.slots
            .write_all(&span.0.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
        self.slots
            .write_all(&span.1.to_le_bytes())
            .map_err(temporary("write payload slots"))?;

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
