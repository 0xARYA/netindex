use std::{fmt, net::IpAddr};

use crate::{
    layout::{slice, u32_at, Encoding, Layout, Section, HEADER},
    packed,
    search::Search,
    target::number,
    Coverage, Error, Limits, Target,
};

/// One original assertion and a borrowed opaque payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Match<'a> {
    /// Insertion-order record ID, unique within this component.
    pub id: u32,
    /// Original address, network, inclusive range, or exact ASN.
    pub target: Target,
    /// Payload bytes owned by the reader's backing storage.
    pub payload: &'a [u8],
}

/// Validated reader over owned bytes, a borrowed slice, or caller-owned mapping.
///
/// The backing must remain unchanged after opening. Mapped-file safety belongs to
/// the mapping's owner; this crate does not map or mutate files. Visitor methods
/// allocate no output buffer; collected lookups allocate a result vector. Payload
/// bytes are never reinterpreted.
/// Readers are `Send` and `Sync` when their backing storage is, and may be shared
/// between threads without an internal lock.
/// See [reading examples](crate#reading) for lookup and application decoding.
pub struct Reader<B: AsRef<[u8]>> {
    bytes: B,
    layout: Layout,
    search: Search,
}

impl<B: AsRef<[u8]>> Reader<B> {
    /// Validate the entire file before exposing lookups.
    ///
    /// Checks target bounds, sorting, IDs, interval augmentation, contiguous
    /// payload spans, lengths, reserved fields, and version. Payload contents are opaque.
    ///
    /// # Errors
    /// Returns malformed/unsupported files, limits, or validation-memory failures.
    pub fn open(bytes: B, limits: Limits) -> Result<Self, Error> {
        let data = bytes.as_ref();
        let layout = Layout::read(data, limits)?;

        validate(data, layout)?;

        Ok(Self {
            bytes,
            layout,
            search: Search::detect(),
        })
    }

    /// Number of assertions, including duplicates.
    pub fn len(&self) -> usize {
        self.layout.records
    }

    /// Whether the component contains no assertions.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the component's uninterpreted metadata.
    ///
    /// # Errors
    /// Returns a bounds failure if backing storage changed after opening.
    pub fn metadata(&self) -> Result<&[u8], Error> {
        slice(self.bytes.as_ref(), HEADER, self.layout.metadata)
    }

    /// Derive conservative coverage once when assembling a set of readers.
    ///
    /// Scans IP targets without decoding payloads or expanding addresses. This
    /// allocates no heap memory and does not change the reader or file.
    ///
    /// # Errors
    /// Returns invalid backing data if it changed after opening.
    pub fn coverage(&self) -> Result<Coverage, Error> {
        let mut coverage = Coverage::empty();
        let bytes = self.bytes.as_ref();

        for (section, ipv6) in [(self.layout.v4, false), (self.layout.v6, true)] {
            for position in 0..section.count {
                let entry = section.entry(bytes, position)?;
                entry.target(ipv6)?;

                coverage.insert(entry.start, entry.end, ipv6);
            }
        }

        Ok(coverage)
    }

    /// Collect every assertion containing an IP, including overlaps and duplicates.
    ///
    /// Allocates a result vector; payloads still borrow the reader's backing.
    /// A miss returns an empty vector. Use [`Self::visit_ip`] to avoid result allocation.
    ///
    /// # Errors
    /// Returns invalid backing data or a result-vector memory reservation failure.
    pub fn lookup_ip(&self, address: IpAddr) -> Result<Vec<Match<'_>>, Error> {
        let mut matches = Vec::new();

        self.visit_ip(address, |matched| {
            matches.try_reserve(1)?;
            matches.push(matched);

            Ok::<_, Error>(())
        })?;

        Ok(matches)
    }

    /// Collect every assertion for an exact ASN key, including duplicates.
    ///
    /// Allocates a result vector; payloads still borrow the reader's backing.
    /// Zero or a missing ASN returns an empty vector. Use [`Self::visit_asn`] to
    /// avoid result allocation.
    ///
    /// # Errors
    /// Returns invalid backing data or a result-vector memory reservation failure.
    pub fn lookup_asn(&self, number: u32) -> Result<Vec<Match<'_>>, Error> {
        let mut matches = Vec::new();

        self.visit_asn(number, |matched| {
            matches.try_reserve(1)?;
            matches.push(matched);

            Ok::<_, Error>(())
        })?;

        Ok(matches)
    }

    /// Visit every assertion containing an IP, in sorted target order.
    ///
    /// ASN assertions are not inferred from an IP. Duplicate records are delivered
    /// separately. The visitor is never called for ordinary misses.
    ///
    /// # Errors
    /// Returns invalid backing data or the visitor's error, stopping on failure.
    pub fn visit_ip<'a, E: From<Error>>(
        &'a self,
        address: IpAddr,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let section = if address.is_ipv6() {
            self.layout.v6
        } else {
            self.layout.v4
        };

        if section.encoding == Encoding::Packed {
            let bytes = self.bytes.as_ref();
            let data = slice(bytes, section.offset, section.length)?;

            if let Some(position) =
                packed::predecessor(data, section.count, number(address), self.search)?
            {
                let entry = section.entry(bytes, position)?;
                if entry.start <= number(address) && number(address) <= entry.end {
                    emit(Match {
                        id: entry.id,
                        target: entry.target(address.is_ipv6())?,
                        payload: self.layout.payload(bytes, entry.id)?,
                    })?;
                }
            }

            return Ok(());
        }

        self.visit(
            section,
            0,
            section.count,
            number(address),
            address.is_ipv6(),
            &mut emit,
        )
    }

    /// Visit every assertion for an exact ASN key, including duplicates.
    ///
    /// # Errors
    /// Returns invalid backing data or the visitor's error; zero has no matches.
    pub fn visit_asn<'a, E: From<Error>>(
        &'a self,
        number: u32,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let bytes = self.bytes.as_ref();
        let section = self.layout.asns;
        let (mut low, mut high) = (0, section.count);

        while low < high {
            let mid = low + (high - low) / 2;
            if u32_at(section.row(bytes, mid)?, 0)? < number {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        while low < section.count {
            let row = section.row(bytes, low)?;
            if u32_at(row, 0)? != number {
                break;
            }

            let id = u32_at(row, 4)?;

            emit(Match {
                id,
                target: Target::Asn(number),
                payload: self.layout.payload(bytes, id)?,
            })?;
            low += 1;
        }

        Ok(())
    }

    /// Visit all assertions, grouped by IPv4, IPv6, then ASN and sorted per section.
    ///
    /// # Errors
    /// Returns invalid backing data or the visitor's error, stopping on failure.
    pub fn visit_all<'a, E: From<Error>>(
        &'a self,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let bytes = self.bytes.as_ref();

        for (section, ipv6) in [(self.layout.v4, false), (self.layout.v6, true)] {
            for position in 0..section.count {
                let entry = section.entry(bytes, position)?;

                emit(Match {
                    id: entry.id,
                    target: entry.target(ipv6)?,
                    payload: self.layout.payload(bytes, entry.id)?,
                })?;
            }
        }

        for position in 0..self.layout.asns.count {
            let row = self.layout.asns.row(bytes, position)?;
            let id = u32_at(row, 4)?;

            emit(Match {
                id,
                target: Target::Asn(u32_at(row, 0)?),
                payload: self.layout.payload(bytes, id)?,
            })?;
        }

        Ok(())
    }

    fn visit<'a, E: From<Error>>(
        &'a self,
        section: Section,
        low: usize,
        high: usize,
        query: u128,
        ipv6: bool,
        emit: &mut impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if low == high {
            return Ok(());
        }

        let bytes = self.bytes.as_ref();
        let mid = low + (high - low) / 2;
        if section.maximum(bytes, mid)? < query || section.start(bytes, low)? > query {
            return Ok(());
        }

        self.visit(section, low, mid, query, ipv6, emit)?;

        if section.start(bytes, mid)? > query {
            return Ok(());
        }

        let entry = section.entry(bytes, mid)?;
        if query <= entry.end {
            emit(Match {
                id: entry.id,
                target: entry.target(ipv6)?,
                payload: self.layout.payload(bytes, entry.id)?,
            })?;
        }

        self.visit(section, mid + 1, high, query, ipv6, emit)?;

        Ok(())
    }
}

impl<B: AsRef<[u8]>> fmt::Debug for Reader<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Reader")
            .field("records", &self.layout.records)
            .field("artifact_bytes", &self.layout.bytes)
            .finish()
    }
}

fn validate(bytes: &[u8], layout: Layout) -> Result<(), Error> {
    let mut seen = Vec::new();
    let words = layout.records.div_ceil(64);
    seen.try_reserve_exact(words)?;
    seen.resize(words, 0u64);

    for (section, ipv6) in [(layout.v4, false), (layout.v6, true)] {
        if section.encoding == Encoding::Packed {
            packed::validate(slice(bytes, section.offset, section.length)?, section.count)?;
        }

        let mut previous = None;
        let mut previous_end = None;

        for position in 0..section.count {
            let entry = section.entry(bytes, position)?;
            entry.target(ipv6)?;

            let key = (entry.start, entry.end, entry.id);
            if previous.is_some_and(|previous| previous >= key) {
                return Err(Error::Invalid("IP entry order"));
            }

            if section.encoding == Encoding::Packed
                && previous_end.is_some_and(|end| end >= entry.start)
            {
                return Err(Error::Invalid("packed overlapping targets"));
            }

            mark(&mut seen, layout.records, entry.id)?;
            previous = Some(key);
            previous_end = Some(entry.end);
        }

        if section.encoding == Encoding::Interval {
            validate_maximum(bytes, section, 0, section.count)?;
        }
    }

    let mut previous = None;

    for position in 0..layout.asns.count {
        let row = layout.asns.row(bytes, position)?;
        let key = (u32_at(row, 0)?, u32_at(row, 4)?);
        if key.0 == 0 || previous.is_some_and(|previous| previous >= key) {
            return Err(Error::Invalid("ASN entry order or key"));
        }

        mark(&mut seen, layout.records, key.1)?;
        previous = Some(key);
    }

    // First-seen contiguous spans stay sorted, including empty spans before new bytes.
    let mut offset = 0usize;
    let mut spans = Vec::new();

    for id in 0..layout.records {
        let (start, length) = layout.span(bytes, id as u32)?;
        if (start < offset || length == 0) && spans.binary_search(&(start, length)).is_ok() {
            continue;
        }

        if start != offset {
            return Err(Error::Invalid("payload offset"));
        }

        slice(bytes, layout.payloads + offset, length)?;

        spans.try_reserve(1)?;
        spans.push((start, length));
        offset = offset
            .checked_add(length)
            .ok_or(Error::Limit("payload bytes"))?;
    }

    if offset != layout.bytes - layout.payloads {
        return Err(Error::Invalid("payload length"));
    }

    Ok(())
}

fn mark(seen: &mut [u64], records: usize, id: u32) -> Result<(), Error> {
    let id = id as usize;
    if id >= records {
        return Err(Error::Invalid("record ID"));
    }

    let seen = seen.get_mut(id / 64).ok_or(Error::Invalid("record ID"))?;
    let mask = 1u64 << (id % 64);
    if *seen & mask != 0 {
        return Err(Error::Invalid("duplicate record ID"));
    }

    *seen |= mask;

    Ok(())
}

fn validate_maximum(
    bytes: &[u8],
    section: Section,
    low: usize,
    high: usize,
) -> Result<u128, Error> {
    if low == high {
        return Ok(0);
    }

    let mid = low + (high - low) / 2;
    let expected = section
        .endpoint(bytes, mid)?
        .max(validate_maximum(bytes, section, low, mid)?)
        .max(validate_maximum(bytes, section, mid + 1, high)?);
    if expected != section.maximum(bytes, mid)? {
        return Err(Error::Invalid("interval maximum"));
    }

    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_marks_reject_duplicates_word_boundaries_and_padding_ids() {
        let mut seen = [0; 2];

        for id in [0, 63, 64] {
            mark(&mut seen, 65, id).unwrap();
        }

        for id in [0, 63, 64] {
            assert!(matches!(
                mark(&mut seen, 65, id),
                Err(Error::Invalid("duplicate record ID"))
            ));
        }

        for id in [65, 127, u32::MAX] {
            assert!(matches!(
                mark(&mut seen, 65, id),
                Err(Error::Invalid("record ID"))
            ));
        }

        assert!(matches!(
            mark(&mut [], 0, 0),
            Err(Error::Invalid("record ID"))
        ));
        assert!(matches!(
            mark(&mut [], 1, 0),
            Err(Error::Invalid("record ID"))
        ));
    }
}
