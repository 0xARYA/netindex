//! Incremental conversion from MMDB networks into caller-defined payloads.
//!
//! Input must be random-access bytes or a caller-owned immutable mapping. Each
//! network is delivered separately; no collection of decoded records is retained.
//! Use the callback to select fields, skip records, and push into [`crate::Builder`]
//! or `ExternalBuilder` (with `external-sort`).

pub use maxminddb::{LookupResult, MaxMindDbError, Reader, WithinOptions};

use crate::Target;

/// Validate an MMDB and visit its networks in native traversal order.
///
/// The callback receives a canonical network target and its borrowed native record.
/// Payload encoding and builder limits belong to the callback. To preserve native
/// IPv6 alias lookups, pass [`WithinOptions::include_aliased_networks`]. Default
/// options omit aliases and networks without data, but retain empty values.
///
/// Validation precedes callbacks. A later iteration or callback failure stops
/// delivery; earlier pushes are not rolled back. Discard the partial build on error.
///
/// ```no_run
/// use netindex::{mmdb, Builder, Limits};
///
/// let source = mmdb::Reader::open_readfile("source.mmdb")?;
/// let mut builder = Builder::new(Limits::default());
/// let options = mmdb::WithinOptions::default().include_aliased_networks();
///
/// mmdb::visit_networks(&source, options, |target, record| {
///     if let Some(number) = record.decode_path::<u32>(&["autonomous_system_number".into()])? {
///         builder.push(target, &number.to_le_bytes())?;
///     }
///
///     Ok::<_, Box<dyn std::error::Error>>(())
/// })?;
///
/// let bytes = builder.into_bytes(b"application metadata")?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// Returns native validation, traversal, or network errors converted to `E`.
/// Callback errors retain their original type and cause.
pub fn visit_networks<'a, B: AsRef<[u8]>, E: From<MaxMindDbError>>(
    reader: &'a Reader<B>,
    options: WithinOptions,
    mut emit: impl FnMut(Target, LookupResult<'a, B>) -> Result<(), E>,
) -> Result<(), E> {
    reader.verify()?;

    for record in reader.networks(options)? {
        let record = record?;

        let network = record.network()?;

        let target = Target::Network {
            address: network.ip(),
            prefix: network.prefix(),
        };

        emit(target, record)?;
    }

    Ok(())
}
