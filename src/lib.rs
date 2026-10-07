#![doc = include_str!("../README.md")]

use std::{collections::TryReserveError, io};

pub use builder::Builder;
pub use coverage::{Coverage, CoverageRouter};
#[cfg(feature = "external-sort")]
pub use external::{ExternalBuildStats, ExternalBuilder, ExternalOptions};
pub use reader::{Match, Reader};
pub use target::Target;

mod builder;
mod coverage;
#[cfg(feature = "external-sort")]
mod external;
mod layout;
#[cfg(feature = "mmdb")]
pub mod mmdb;
mod packed;
mod reader;
mod search;
mod target;

/// Failures in target validation, encoding, or opening an index.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A configured resource ceiling was exceeded.
    #[error("resource limit exceeded: {0}")]
    Limit(&'static str),
    /// An input target or encoded field violates the format.
    #[error("invalid {0}")]
    Invalid(&'static str),
    /// An on-disk version is not supported.
    #[error("unsupported index version {0}")]
    Version(u32),
    /// A memory reservation failed.
    #[error("failed to reserve index memory")]
    Allocation(#[from] TryReserveError),
    /// Reserving the payload deduplication table failed.
    #[error("failed to reserve payload index memory")]
    HashAllocation(#[from] hashbrown::TryReserveError),
    /// Writing the caller's output failed.
    #[error("failed to write index")]
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

/// Resource ceilings for building and opening a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Limits {
    /// Maximum assertions, including duplicate targets.
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
