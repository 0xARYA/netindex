use std::{
    collections::hash_map::RandomState,
    fmt,
    hash::BuildHasher,
    io::{self, BufWriter, Cursor, Write},
};

use hashbrown::HashTable;

use crate::{
    layout::{slice, Layout},
    packed,
    payload::{SlotWriter, Slots},
    target::Entry,
    Error, Limits, Target,
};

/// In-memory builder for one immutable index.
///
/// Buffers unsorted assertions and opaque payloads. Sorting preserves targets;
/// identical payloads share bytes. See the [quick start](crate#quick-start).
pub struct Builder {
    limits: Limits,
    v4: Vec<Entry<u32>>,
    v6: Vec<Entry>,
    asns: Vec<(u32, u32)>,
    slots: Vec<(u64, u64)>,
    payloads: Vec<u8>,
    hash_state: RandomState,
    hashes: HashTable<(u64, (u64, u64))>,
}

impl Builder {
    /// Start an empty index with explicit limits.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            v4: Vec::new(),
            v6: Vec::new(),
            asns: Vec::new(),
            slots: Vec::new(),
            payloads: Vec::new(),
            hash_state: RandomState::new(),
            hashes: HashTable::new(),
        }
    }

    /// Append one assertion and its opaque payload.
    ///
    /// On failure no assertion is added. IDs follow insertion order.
    ///
    /// # Errors
    /// Fails on invalid targets, exceeded limits, or failed memory reservations.
    pub fn push(&mut self, target: Target, payload: &[u8]) -> Result<(), Error> {
        let hash = self.hash_state.hash_one(payload);
        self.push_hashed(target, payload, hash)
    }

    /// Consume the builder and return encoded bytes.
    /// Metadata remains opaque. Use [`Self::write_to`] for file output.
    ///
    /// # Errors
    /// Fails if metadata or file size exceeds limits, or output allocation fails
    /// through [`Error::Io`].
    pub fn into_bytes(self, metadata: &[u8]) -> Result<Vec<u8>, Error> {
        let mut output = MemoryWriter(Vec::new());
        self.write_to(&mut output, metadata)?;

        Ok(output.0)
    }

    /// Sort and encode the index into the caller's writer.
    ///
    /// Metadata is opaque and stored once. Output is buffered and flushed. Failure
    /// may leave partial output; the caller must stage and synchronize before publishing.
    ///
    /// # Errors
    /// Fails if metadata or file size exceeds limits, or writing or flushing fails.
    pub fn write_to(mut self, output: impl Write, metadata: &[u8]) -> Result<(), Error> {
        drop(self.hashes);

        self.v4
            .sort_unstable_by_key(|entry| (entry.start, entry.end, entry.id));
        self.v6
            .sort_unstable_by_key(|entry| (entry.start, entry.end, entry.id));
        self.asns.sort_unstable();

        let packed_v4 = packed::size(&self.v4).filter(|&size| size < self.v4.len() * 20);
        let packed_v6 = packed::size(&self.v6).filter(|&size| size < self.v6.len() * 56);

        let length = self
            .slots
            .first()
            .map(|&(_, length)| length as usize)
            .filter(|&length| self.slots.iter().all(|&(_, value)| value == length as u64));
        let slot_encoding = Slots::select(self.slots.len(), self.payloads.len(), length)?;
        let layout = Layout::with_packed(
            [self.v4.len(), self.v6.len(), self.asns.len()],
            metadata.len(),
            self.payloads.len(),
            self.limits,
            [packed_v4, packed_v6],
            slot_encoding,
        )?;

        if packed_v4.is_none() {
            augment(&mut self.v4);
        }

        if packed_v6.is_none() {
            augment(&mut self.v6);
        }

        let mut output = BufWriter::new(output);

        layout.write_header(&mut output)?;
        output.write_all(metadata)?;

        if packed_v4.is_some() {
            packed::write(&self.v4, &mut output)?;
        } else {
            for entry in &self.v4 {
                write_entry(&mut output, &entry.map(u128::from), false)?;
            }
        }

        if packed_v6.is_some() {
            packed::write(&self.v6, &mut output)?;
        } else {
            for entry in &self.v6 {
                write_entry(&mut output, entry, true)?;
            }
        }

        for &(number, id) in &self.asns {
            output.write_all(&number.to_le_bytes())?;
            output.write_all(&id.to_le_bytes())?;
        }

        let mut slots = SlotWriter::new(&mut output, slot_encoding)?;
        for &(offset, length) in &self.slots {
            slots.push(offset, length)?;
        }

        output.write_all(&self.payloads)?;
        output.flush()?;

        Ok(())
    }

    fn push_hashed(&mut self, target: Target, payload: &[u8], hash: u64) -> Result<(), Error> {
        let id = u32::try_from(self.slots.len()).map_err(|_| Error::Limit("records"))?;

        let entry = match target {
            Target::Asn(0) => return Err(Error::Invalid("ASN zero")),
            Target::Asn(_) => None,
            target => Some(target.entry(id)?),
        };

        let mut counts = [self.v4.len(), self.v6.len(), self.asns.len()];
        let index = match entry {
            Some((_, false)) => 0,
            Some((_, true)) => 1,
            None => 2,
        };
        let count = counts.get_mut(index).ok_or(Error::Invalid("section"))?;
        *count = count.checked_add(1).ok_or(Error::Limit("records"))?;

        let mut existing = None;
        for &(_, (offset, length)) in self.hashes.iter_hash(hash) {
            if slice(&self.payloads, offset as usize, length as usize)? == payload {
                existing = Some((offset, length));
                break;
            }
        }

        let added = if existing.is_some() { 0 } else { payload.len() };
        let payload_bytes = self
            .payloads
            .len()
            .checked_add(added)
            .ok_or(Error::Limit("payload bytes"))?;
        Layout::new(counts, 0, payload_bytes, self.limits)?;

        self.slots.try_reserve(1)?;
        self.payloads.try_reserve(added)?;

        if existing.is_none() {
            self.hashes.try_reserve(1, |row| row.0)?;
        }

        match entry {
            Some((_, false)) => self.v4.try_reserve(1)?,
            Some((_, true)) => self.v6.try_reserve(1)?,
            None => self.asns.try_reserve(1)?,
        }

        match entry {
            Some((entry, false)) => self.v4.push(entry.map(|value| value as u32)),
            Some((entry, true)) => self.v6.push(entry),
            None => {
                let Target::Asn(number) = target else {
                    return Err(Error::Invalid("ASN target"));
                };

                self.asns.push((number, id));
            }
        }

        let span = match existing {
            Some(span) => span,
            None => {
                let span = (self.payloads.len() as u64, payload.len() as u64);
                self.hashes.insert_unique(hash, (hash, span), |row| row.0);

                self.payloads.extend_from_slice(payload);
                span
            }
        };

        self.slots.push(span);

        Ok(())
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Builder")
            .field("records", &self.slots.len())
            .field("payload_bytes", &self.payloads.len())
            .field("limits", &self.limits)
            .finish()
    }
}

struct MemoryWriter(Vec<u8>);

impl Write for MemoryWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .try_reserve(bytes.len())
            .map_err(|error| io::Error::new(io::ErrorKind::OutOfMemory, error))?;

        self.0.extend_from_slice(bytes);

        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn augment<N: Copy + Ord + Default>(entries: &mut [Entry<N>]) -> N {
    let mid = entries.len() / 2;
    let (left, rest) = entries.split_at_mut(mid);

    let Some((entry, right)) = rest.split_first_mut() else {
        return N::default();
    };

    entry.maximum = entry.end.max(augment(left)).max(augment(right));

    entry.maximum
}

pub(crate) fn write_entry(output: &mut impl Write, entry: &Entry, ipv6: bool) -> Result<(), Error> {
    let mut bytes = [0; 56];
    let mut row = Cursor::new(bytes.as_mut_slice());

    for value in [entry.start, entry.end, entry.maximum] {
        if ipv6 {
            row.write_all(&value.to_le_bytes())?;
        } else {
            row.write_all(&(value as u32).to_le_bytes())?;
        }
    }

    row.write_all(&entry.id.to_le_bytes())?;
    row.write_all(&[entry.prefix, entry.kind, 0, 0])?;

    output.write_all(slice(&bytes, 0, if ipv6 { 56 } else { 20 })?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Reader;

    use super::*;

    #[test]
    fn hash_collisions_compare_bytes_before_reusing_payloads() {
        let mut builder = Builder::new(Limits::default());
        builder.push_hashed(Target::Asn(1), b"first", 0).unwrap();
        builder.push_hashed(Target::Asn(2), b"other", 0).unwrap();
        builder.push_hashed(Target::Asn(3), b"first", 0).unwrap();
        builder.push_hashed(Target::Asn(4), b"other", 0).unwrap();

        assert_eq!(builder.payloads, b"firstother");

        let mut bytes = Vec::new();
        builder.write_to(&mut bytes, b"").unwrap();

        let reader = Reader::open(bytes, Limits::default()).unwrap();
        let mut payloads = Vec::new();

        reader
            .visit_all(|matched| {
                payloads.push(matched.payload);
                Ok::<_, Error>(())
            })
            .unwrap();

        assert_eq!(payloads, [b"first", b"other", b"first", b"other"]);
    }
}
