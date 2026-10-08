//! Optional sharing of byte values across records. Schemas and scalar encoding remain caller-owned.

use std::{collections::hash_map::RandomState, fmt, hash::BuildHasher, io::Write};

use hashbrown::HashTable;

use crate::{Error, Limits, layout::u32_at};

const MAGIC: &[u8; 8] = b"NIVALUES";
const HEADER: usize = 12;

/// Build a shared value pool without retaining a second copy of each value.
pub struct ValuePoolBuilder {
    limits: Limits,
    offsets: Vec<u32>,
    bytes: Vec<u8>,
    hashes: HashTable<(u64, u32)>,
    hash_state: RandomState,
}

impl ValuePoolBuilder {
    /// Bound unique values with `limits.records` and complete pool bytes with `limits.bytes`.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            offsets: Vec::new(),
            bytes: Vec::new(),
            hashes: HashTable::new(),
            hash_state: RandomState::new(),
        }
    }

    /// Return a stable zero-based ID, reusing identical byte values.
    ///
    /// Failed insertions leave existing IDs and bytes intact. Hash collisions are
    /// resolved by byte equality. IDs belong to this pool, not another release.
    ///
    /// # Errors
    /// Fails on configured limits, unrepresentable offsets, or allocation failure.
    pub fn intern(&mut self, value: &[u8]) -> Result<u32, Error> {
        let hash = self.hash_state.hash_one(value);

        for &(_, id) in self.hashes.iter_hash(hash) {
            let start = *self
                .offsets
                .get(id as usize)
                .ok_or(Error::Invalid("value ID"))? as usize;
            let end = self
                .offsets
                .get(id as usize + 1)
                .map_or(self.bytes.len(), |v| *v as usize);

            if self.bytes.get(start..end) == Some(value) {
                return Ok(id);
            }
        }

        let count = self
            .offsets
            .len()
            .checked_add(1)
            .ok_or(Error::Limit("shared values"))?;
        let size = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(Error::Limit("shared value bytes"))?;
        check_size(count, size, self.limits)?;

        let id = u32::try_from(self.offsets.len()).map_err(|_| Error::Limit("shared values"))?;
        let offset =
            u32::try_from(self.bytes.len()).map_err(|_| Error::Limit("shared value bytes"))?;

        self.offsets.try_reserve(1)?;
        self.bytes.try_reserve(value.len())?;
        self.hashes.try_reserve(1, |row| row.0)?;

        self.offsets.push(offset);
        self.bytes.extend_from_slice(value);
        self.hashes.insert_unique(hash, (hash, id), |row| row.0);

        Ok(id)
    }

    /// Number of unique values.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Whether no values have been inserted.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Encode the pool into owned bytes.
    ///
    /// # Errors
    /// Fails on limits, allocation, or output failure.
    pub fn into_bytes(self) -> Result<Vec<u8>, Error> {
        let size = self.encoded_length()?;
        let mut output = Vec::new();
        output.try_reserve_exact(size)?;
        self.write_to(&mut output)?;

        Ok(output)
    }

    /// Write offsets followed by value bytes. Failure may leave partial output.
    ///
    /// # Errors
    /// Fails on limits or output failure. Publication and synchronization belong to the caller.
    pub fn write_to(self, mut output: impl Write) -> Result<(), Error> {
        self.encoded_length()?;
        drop(self.hashes);

        output.write_all(MAGIC)?;
        output.write_all(&(self.offsets.len() as u32).to_le_bytes())?;

        for offset in self.offsets {
            output.write_all(&offset.to_le_bytes())?;
        }

        output.write_all(&(self.bytes.len() as u32).to_le_bytes())?;
        output.write_all(&self.bytes)?;
        output.flush()?;

        Ok(())
    }

    pub(crate) fn encoded_length(&self) -> Result<usize, Error> {
        check_size(self.offsets.len(), self.bytes.len(), self.limits)
    }
}

impl fmt::Debug for ValuePoolBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValuePoolBuilder")
            .field("values", &self.len())
            .field("value_bytes", &self.bytes.len())
            .field("limits", &self.limits)
            .finish()
    }
}

/// Validated shared values borrowing immutable caller-owned bytes.
#[derive(Clone, Copy)]
pub struct ValuePool<'a> {
    offsets: &'a [[u8; 4]],
    body: &'a [u8],
}

impl<'a> ValuePool<'a> {
    /// Validate offsets, exact length, and resource limits without allocating.
    ///
    /// Value bodies remain opaque. A pool may live in index metadata or a separate
    /// mapped file. Replace records and their pool together so IDs stay consistent.
    ///
    /// # Errors
    /// Fails on malformed bytes or configured limits.
    pub fn open(bytes: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if bytes.len() > limits.bytes {
            return Err(Error::Limit("shared value bytes"));
        }

        if bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(Error::Invalid("value pool magic"));
        }

        let count = u32_at(bytes, 8)? as usize;
        if count > limits.records {
            return Err(Error::Limit("shared values"));
        }

        let data = count
            .checked_add(1)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_add(HEADER))
            .ok_or(Error::Invalid("value offsets"))?;
        let body = bytes.get(data..).ok_or(Error::Invalid("value offsets"))?;

        if u32_at(bytes, HEADER)? != 0 || u32_at(bytes, data - 4)? as usize != body.len() {
            return Err(Error::Invalid("value pool length"));
        }

        let mut previous = 0;
        for id in 0..count {
            let next = u32_at(bytes, HEADER + (id + 1) * 4)? as usize;
            if next < previous || next > body.len() {
                return Err(Error::Invalid("value offset order"));
            }

            previous = next;
        }

        let directory = bytes
            .get(HEADER..data)
            .ok_or(Error::Invalid("value offsets"))?;
        let (offsets, _) = directory.as_chunks::<4>();

        Ok(Self { offsets, body })
    }

    /// Number of values in the pool.
    pub fn len(self) -> usize {
        self.offsets.len() - 1
    }

    /// Whether the pool contains no values.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Borrow a value by ID, without allocation or schema decoding.
    ///
    /// # Errors
    /// Fails when the ID is outside this pool.
    #[inline]
    pub fn get(self, id: u32) -> Result<&'a [u8], Error> {
        let id = id as usize;
        let &[start, end] = self
            .offsets
            .get(id..)
            .and_then(|offsets| offsets.first_chunk::<2>())
            .ok_or(Error::Invalid("value ID"))?;

        self.body
            .get(u32::from_le_bytes(start) as usize..u32::from_le_bytes(end) as usize)
            .ok_or(Error::Invalid("truncated section"))
    }
}

impl fmt::Debug for ValuePool<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValuePool")
            .field("values", &self.len())
            .field(
                "pool_bytes",
                &(HEADER + self.offsets.len() * 4 + self.body.len()),
            )
            .finish()
    }
}

/// Shared strings validated once over immutable bytes, without a decoded-string cache.
#[derive(Debug, Clone, Copy)]
pub struct StringPool<'a> {
    values: ValuePool<'a>,
}

impl<'a> StringPool<'a> {
    /// Validate the value directory and every string's UTF-8 before serving.
    ///
    /// # Errors
    /// Fails on malformed data, exceeded limits, or invalid UTF-8.
    pub fn open(bytes: &'a [u8], limits: Limits) -> Result<Self, Error> {
        let values = ValuePool::open(bytes, limits)?;

        for id in 0..values.len() {
            std::str::from_utf8(values.get(id as u32)?)?;
        }

        Ok(Self { values })
    }

    /// Number of shared strings.
    pub fn len(self) -> usize {
        self.values.len()
    }

    /// Whether there are no strings.
    pub fn is_empty(self) -> bool {
        self.values.is_empty()
    }

    /// Borrow a string by ID without copying or repeating UTF-8 validation.
    ///
    /// # Errors
    /// Fails if the ID is outside this pool.
    #[inline]
    pub fn get(self, id: u32) -> Result<&'a str, Error> {
        let bytes = self.values.get(id)?;

        // SAFETY: `open` validates every value as UTF-8. Fields are private and
        // the backing slice remains immutable for the lifetime of this pool.
        Ok(unsafe { std::str::from_utf8_unchecked(bytes) })
    }
}

fn check_size(count: usize, bytes: usize, limits: Limits) -> Result<usize, Error> {
    if count > limits.records || count > u32::MAX as usize {
        return Err(Error::Limit("shared values"));
    }

    let size = count
        .checked_add(1)
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(HEADER))
        .and_then(|v| v.checked_add(bytes))
        .ok_or(Error::Limit("shared value bytes"))?;

    if size > limits.bytes || bytes > u32::MAX as usize {
        return Err(Error::Limit("shared value bytes"));
    }

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_collisions_compare_value_bytes_before_reusing_ids() -> Result<(), Error> {
        let mut builder = ValuePoolBuilder::new(Limits::default());
        let first = builder.intern(b"first")?;
        let hash = builder.hash_state.hash_one(b"other");
        builder
            .hashes
            .insert_unique(hash, (hash, first), |row| row.0);

        let other = builder.intern(b"other")?;
        assert_ne!(first, other);
        assert_eq!(builder.intern(b"other")?, other);

        let bytes = builder.into_bytes()?;
        let pool = ValuePool::open(&bytes, Limits::default())?;

        assert_eq!(pool.get(first)?, b"first");
        assert_eq!(pool.get(other)?, b"other");

        Ok(())
    }
}
