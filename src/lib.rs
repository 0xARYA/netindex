#![doc = include_str!("../README.md")]

use std::{collections::TryReserveError, io};

pub use builder::Builder;
pub use coverage::{Coverage, CoverageRouter};
#[cfg(feature = "external-sort")]
pub use external::{ExternalBuildStats, ExternalBuilder, ExternalOptions};
pub use reader::{Match, Reader};
pub use reader_set::ReaderSet;
pub use target::Target;

mod builder;
mod coverage;
#[cfg(feature = "external-sort")]
mod external;
mod layout;
#[cfg(feature = "mmdb")]
pub mod mmdb;
mod packed;
mod payload;
mod reader;
mod reader_set;
mod search;
mod target;
#[cfg(feature = "shared-values")]
pub mod values;

/// A validated file-backed reader; requires the mmap feature.
#[cfg(feature = "mmap")]
pub type MappedReader = Reader<memmap2::Mmap>;

/// Failures in index and shared-value operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A configured resource ceiling was exceeded.
    #[error("resource limit exceeded: {0}")]
    Limit(&'static str),
    /// An input target or encoded field violates the format.
    #[error("invalid {0}")]
    Invalid(&'static str),
    /// A typed string pool contains invalid UTF-8.
    #[cfg(feature = "shared-values")]
    #[error("invalid shared string UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
    /// An on-disk version is not supported.
    #[error("unsupported index version {0}")]
    Version(u32),
    /// A memory reservation failed.
    #[error("failed to reserve index memory")]
    Allocation(#[from] TryReserveError),
    /// Reserving an index or shared-value deduplication table failed.
    #[error("failed to reserve deduplication table memory")]
    HashAllocation(#[from] hashbrown::TryReserveError),
    /// Reading file metadata or creating a read-only mapping failed.
    #[cfg(feature = "mmap")]
    #[error("failed to {operation} index file")]
    MappingIo {
        /// Filesystem operation that failed.
        operation: &'static str,
        /// Underlying filesystem cause.
        #[source]
        error: io::Error,
    },
    /// Writing the caller's encoded artifact failed.
    #[error("failed to write artifact")]
    Io(#[from] io::Error),
    /// Creating, reading, writing, or cleaning up temporary build files failed.
    #[cfg(feature = "external-sort")]
    #[error("failed to {operation} temporary index data")]
    TemporaryIo {
        /// Temporary-file operation that failed.
        operation: &'static str,
        /// Underlying filesystem cause.
        #[source]
        error: io::Error,
    },
}

/// Resource limits for building and opening an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Limits {
    /// Maximum assertions (including duplicates), or unique values in a value pool.
    pub records: usize,
    /// Maximum complete artifact bytes, including metadata and payloads.
    pub bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 1_000_000,
            bytes: 128 * 1024 * 1024,
        }
    }
}
