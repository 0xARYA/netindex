use std::fs::File;

use memmap2::{Mmap, MmapOptions};

use crate::{layout::HEADER, Error, Limits, Reader};

impl Reader<Mmap> {
    /// Map an entire immutable file read-only and validate the index.
    ///
    /// Checks the file length against `limits` before mapping. The returned reader
    /// owns its mapping and remains usable after the file handle is closed.
    /// No access advice or preloading is applied.
    ///
    /// # Safety
    /// The file must not be modified or truncated, by any process, until every
    /// reader and borrowed match using this mapping has been dropped. A read-only
    /// handle does not enforce this requirement. Publish replacements as separate
    /// files rather than overwriting a mapped file.
    ///
    /// # Errors
    /// Returns file metadata, mapping, resource-limit, or index-validation errors.
    pub unsafe fn map_file(file: &File, limits: Limits) -> Result<Self, Error> {
        let metadata = file.metadata().map_err(|error| Error::MappingIo {
            operation: "read metadata for",
            error,
        })?;
        let length = usize::try_from(metadata.len()).map_err(|_| Error::Limit("artifact bytes"))?;

        if length > limits.bytes {
            return Err(Error::Limit("artifact bytes"));
        }

        if length < HEADER {
            return Err(Error::Invalid("index header"));
        }

        // SAFETY: The caller guarantees that this file stays unchanged for the
        // mapping's lifetime. Its complete length was checked before mapping.
        let mapping = unsafe { MmapOptions::new().len(length).map(file) }.map_err(|error| {
            Error::MappingIo {
                operation: "map",
                error,
            }
        })?;

        Self::open(mapping, limits)
    }

    /// Borrow the mapping for platform-specific access advice.
    ///
    /// Hints do not replace validation or guarantee residency. The mapping has
    /// the same lifetime and immutable-file requirements as this reader.
    pub fn mapping(&self) -> &Mmap {
        &self.bytes
    }
}
