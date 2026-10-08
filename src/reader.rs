use std::{fmt, net::IpAddr};

use crate::{
    Coverage, Error, Limits, Target,
    layout::{Encoding, HEADER, Layout, Section, slice, u32_at},
    packed,
    routing::BroadRoutes,
    search::Search,
    target::{Entry, number},
};

#[cfg(feature = "mmap")]
#[path = "mmap.rs"]
mod mmap;

/// A matching assertion with its original target and borrowed payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Match<'a> {
    /// Insertion-order record ID, unique within this index.
    pub id: u32,
    /// Original address, network, inclusive range, or exact ASN.
    pub target: Target,
    /// Payload bytes owned by the reader's backing storage.
    pub payload: &'a [u8],
}

/// Validated reader over owned bytes, a borrowed slice, or caller-owned mapping.
///
/// Keep the backing unchanged after opening. The caller owns mapping safety.
/// Visitors allocate no output buffer; collected lookups allocate a result vector.
/// Payload bytes remain opaque.
/// Readers are `Send` and `Sync` when their backing storage is, and may be shared
/// between threads without an internal lock.
/// See [reading examples](crate#reading) for lookup and application decoding.
pub struct Reader<B: AsRef<[u8]>> {
    bytes: B,
    layout: Layout,
    search: Search,
    bounds: [(u128, u128); 2],
    ipv4_directory: Option<packed::Directory>,
    coverage: Coverage,
    broad_routes: Option<BroadRoutes>,
}

impl<B: AsRef<[u8]>> Reader<B> {
    /// Validate the entire file before exposing lookups.
    ///
    /// Checks target bounds, sorting, IDs, interval augmentation, contiguous
    /// payload spans, lengths, reserved fields, and version. Payload contents are opaque.
    /// Caches address bounds and builds a small directory for large packed IPv4 sections.
    ///
    /// # Errors
    /// Fails on malformed or unsupported files, exceeded limits, or failed
    /// validation-memory reservations.
    pub fn open(bytes: B, limits: Limits) -> Result<Self, Error> {
        Self::open_inner(bytes, limits, false)
    }

    /// Validate and cache direct routes for broad, disjoint packed networks.
    ///
    /// Uses up to 32 KiB per eligible address family. Routes are collected during
    /// validation; other targets retain the ordinary lookup path. This can improve
    /// broad-network queries but adds a check to other queries; measure your workload.
    ///
    /// # Errors
    /// Returns the same validation errors as [`Self::open`], or a route allocation error.
    pub fn open_with_broad_routes(bytes: B, limits: Limits) -> Result<Self, Error> {
        Self::open_inner(bytes, limits, true)
    }

    /// Number of assertions, including duplicates.
    pub fn len(&self) -> usize {
        self.layout.records
    }

    /// Whether the index contains no assertions.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the index's opaque metadata.
    ///
    /// # Errors
    /// Fails if changed backing storage makes the metadata bounds invalid.
    pub fn metadata(&self) -> Result<&[u8], Error> {
        slice(self.bytes.as_ref(), HEADER, self.layout.metadata)
    }

    /// Return conservative coverage cached during validation.
    ///
    /// Copies a 1 KiB summary without rescanning targets or decoding payloads.
    pub fn coverage(&self) -> Coverage {
        self.coverage.clone()
    }

    /// Collect every assertion containing an IP, including overlaps and duplicates.
    ///
    /// Allocates a result vector; payloads still borrow the reader's backing.
    /// A miss returns an empty vector. Use [`Self::visit_ip`] to avoid result allocation.
    ///
    /// # Errors
    /// Fails on invalid backing data or a failed result-vector reservation.
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
    /// Fails on invalid backing data or a failed result-vector reservation.
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
    /// Stops on invalid backing data or a visitor error, preserving its type.
    pub fn visit_ip<'a, E: From<Error>>(
        &'a self,
        address: IpAddr,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let ipv6 = address.is_ipv6();
        let query = number(address);
        let (start, end) = if ipv6 { self.bounds[1] } else { self.bounds[0] };

        if query < start || query > end {
            return Ok(());
        }

        let section = if ipv6 { self.layout.v6 } else { self.layout.v4 };

        if let Some(routes) = &self.broad_routes
            && let Some(entry) = routes.lookup(query, ipv6)?
        {
            emit(Match {
                id: entry.id,
                target: entry.target(ipv6)?,
                payload: self.layout.payload(self.bytes.as_ref(), entry.id)?,
            })?;

            return Ok(());
        }

        if section.encoding == Encoding::Packed {
            let bytes = self.bytes.as_ref();
            let data = slice(bytes, section.offset, section.length)?;

            if let Some(entry) = packed::lookup(
                data,
                section.count,
                query,
                ipv6,
                self.search,
                if ipv6 {
                    None
                } else {
                    self.ipv4_directory.as_ref()
                },
            )? {
                emit(Match {
                    id: entry.id,
                    target: entry.target(ipv6)?,
                    payload: self.layout.payload(bytes, entry.id)?,
                })?;
            }

            return Ok(());
        }

        self.visit(section, 0, section.count, query, ipv6, &mut emit)
    }

    /// Visit every assertion for an exact ASN key, including duplicates.
    ///
    /// Zero or a missing key does not call the visitor.
    ///
    /// # Errors
    /// Stops on invalid backing data or a visitor error, preserving its type.
    pub fn visit_asn<'a, E: From<Error>>(
        &'a self,
        number: u32,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let bytes = self.bytes.as_ref();
        let section = self.layout.asns;
        let (rows, remainder) = slice(bytes, section.offset, section.length)?.as_chunks::<8>();

        if !remainder.is_empty() || rows.len() != section.count {
            return Err(Error::Invalid("ASN section length").into());
        }

        let key = |row: &[u8; 8]| {
            let [a, b, c, d, _, _, _, _] = *row;
            u32::from_le_bytes([a, b, c, d])
        };

        let first = rows.partition_point(|row| key(row) < number);
        let matches = rows
            .get(first..)
            .ok_or(Error::Invalid("ASN index position"))?;

        for row in matches {
            if key(row) != number {
                break;
            }

            let id = u32_at(row, 4)?;

            emit(Match {
                id,
                target: Target::Asn(number),
                payload: self.layout.payload(bytes, id)?,
            })?;
        }

        Ok(())
    }

    /// Visit all assertions, grouped by IPv4, IPv6, then ASN and sorted per section.
    ///
    /// # Errors
    /// Stops on invalid backing data or a visitor error, preserving its type.
    pub fn visit_all<'a, E: From<Error>>(
        &'a self,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let bytes = self.bytes.as_ref();

        for (section, ipv6) in [(self.layout.v4, false), (self.layout.v6, true)] {
            let mut visit = |entry: Entry| -> Result<(), E> {
                emit(Match {
                    id: entry.id,
                    target: entry.target(ipv6)?,
                    payload: self.layout.payload(bytes, entry.id)?,
                })?;

                Ok(())
            };

            if section.encoding == Encoding::Packed {
                packed::visit_entries(
                    slice(bytes, section.offset, section.length)?,
                    section.count,
                    ipv6,
                    &mut visit,
                )?;
            } else {
                for position in 0..section.count {
                    visit(section.entry(bytes, position)?)?;
                }
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

    fn open_inner(bytes: B, limits: Limits, broad_routes: bool) -> Result<Self, Error> {
        let data = bytes.as_ref();
        let layout = Layout::read(data, limits)?;

        let (coverage, broad_routes) = validate(data, layout, broad_routes)?;

        let bounds = [ip_bounds(data, layout.v4)?, ip_bounds(data, layout.v6)?];
        let ipv4_directory = ip_directory(data, layout.v4, bounds[0])?;

        Ok(Self {
            bytes,
            layout,
            search: Search::detect(),
            bounds,
            ipv4_directory,
            coverage,
            broad_routes,
        })
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

fn ip_bounds(bytes: &[u8], section: Section) -> Result<(u128, u128), Error> {
    if section.count == 0 {
        return Ok((0, 0));
    }

    let start = section.entry(bytes, 0)?.start;
    let end = if section.encoding == Encoding::Packed {
        section.entry(bytes, section.count - 1)?.end
    } else {
        section.maximum(bytes, section.count / 2)?
    };

    Ok((start, end))
}

fn ip_directory(
    bytes: &[u8],
    section: Section,
    (start, end): (u128, u128),
) -> Result<Option<packed::Directory>, Error> {
    if section.encoding != Encoding::Packed {
        return Ok(None);
    }

    packed::Directory::build(
        slice(bytes, section.offset, section.length)?,
        section.count,
        start,
        end,
    )
}

fn validate(
    bytes: &[u8],
    layout: Layout,
    broad_routes: bool,
) -> Result<(Coverage, Option<BroadRoutes>), Error> {
    let mut seen = Vec::new();
    let words = layout.records.div_ceil(64);
    seen.try_reserve_exact(words)?;
    seen.resize(words, 0u64);
    let mut coverage = Coverage::empty();
    let mut routes = broad_routes.then(BroadRoutes::empty);

    for (section, ipv6) in [(layout.v4, false), (layout.v6, true)] {
        if section.encoding == Encoding::Packed {
            packed::validate(slice(bytes, section.offset, section.length)?, section.count)?;
        }

        let mut previous = None;
        let mut previous_end = None;

        let mut check = |entry: Entry| -> Result<(), Error> {
            entry.validate(ipv6)?;

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

            coverage.insert(entry.start, entry.end, ipv6);
            if section.encoding == Encoding::Packed
                && let Some(routes) = &mut routes
            {
                routes.insert(entry, ipv6)?;
            }

            previous = Some(key);
            previous_end = Some(entry.end);

            Ok(())
        };

        if section.encoding == Encoding::Packed {
            packed::visit_entries(
                slice(bytes, section.offset, section.length)?,
                section.count,
                ipv6,
                &mut check,
            )?;
        } else {
            for position in 0..section.count {
                check(section.entry(bytes, position)?)?;
            }
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

    drop(seen);

    if matches!(layout.slot_encoding, crate::payload::Slots::Fixed { .. }) {
        let mut end = 0;

        for id in 0..layout.records {
            let (start, length) = layout.span(bytes, id as u32)?;
            if start > end {
                return Err(Error::Invalid("payload reference order"));
            }

            slice(bytes, crate::layout::add(layout.payloads, start)?, length)?;

            if start == end {
                end = end
                    .checked_add(length)
                    .ok_or(Error::Limit("payload bytes"))?;
            }
        }

        if end != layout.bytes - layout.payloads {
            return Err(Error::Invalid("payload length"));
        }

        return Ok((coverage, routes.filter(|routes| !routes.is_empty())));
    }

    // First-seen contiguous spans stay sorted, including empty spans before new bytes.
    let mut offset = 0usize;
    let mut spans = Vec::new();
    let mut previous_span = None;

    for id in 0..layout.records {
        let (start, length) = layout.span(bytes, id as u32)?;
        if previous_span == Some((start, length)) {
            continue;
        }

        if (start < offset || length == 0) && spans.binary_search(&(start, length)).is_ok() {
            previous_span = Some((start, length));
            continue;
        }

        if start != offset {
            return Err(Error::Invalid("payload offset"));
        }

        slice(bytes, crate::layout::add(layout.payloads, offset)?, length)?;

        spans.try_reserve(1)?;
        spans.push((start, length));
        previous_span = Some((start, length));
        offset = offset
            .checked_add(length)
            .ok_or(Error::Limit("payload bytes"))?;
    }

    if offset != layout.bytes - layout.payloads {
        return Err(Error::Invalid("payload length"));
    }

    Ok((coverage, routes.filter(|routes| !routes.is_empty())))
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
