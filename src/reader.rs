use std::{fmt, net::IpAddr};

use crate::{
    Coverage, Error, Limits, SharedAlias, Target,
    aliases::Aliases,
    broad_routes::BroadRoutes,
    layout::{Encoding, Layout, Section, slice, u32_at},
    packed,
    search::Search,
    target::{Entry, number},
};

pub use inspection::{Inspection, PayloadReferences, SectionEncoding, SectionInspection};
pub use query::{BatchError, Query};

mod inspection;
#[cfg(feature = "mmap")]
mod mmap;
mod prefix;
mod query;
mod rewrite;
mod validation;

/// A matching assertion with its target and borrowed payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Match<'a> {
    /// Insertion-order record ID, unique within this index.
    pub id: u32,
    /// Original address, network, inclusive range, or exact ASN. Shared IPv4
    /// aliases return the projected IPv6 target.
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
///
/// Use [`Self::lookup_ip`] to collect all matches, [`Self::contains_ip`] for
/// membership, and [`Self::longest_prefix`] for a single most specific match.
/// Use [`Self::visit_ip`] or [`Self::try_visit_ip`] when the caller owns output
/// storage or only needs to process some results. See the
/// [query guide](crate#reading) for ASN, intersection, and batch queries.
pub struct Reader<B: AsRef<[u8]>> {
    bytes: B,
    layout: Layout,
    search: Search,
    bounds: [(u128, u128); 2],
    ipv4_directory: Option<packed::Directory>,
    coverage: Coverage,
    broad_routes: Option<BroadRoutes>,
    aliases: Option<Aliases>,
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
        self.aliases
            .as_ref()
            .map_or(self.layout.records, |aliases| aliases.records)
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
        slice(
            self.bytes.as_ref(),
            self.layout.metadata_offset(),
            self.layout.metadata,
        )
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
        if let Some(aliases) = &self.aliases {
            if let IpAddr::V6(ip) = address
                && let Some(window) = aliases.lookup(u128::from(ip))
            {
                return self
                    .visit_physical_ip(window.source_address(u128::from(ip)).into(), |row| {
                        emit(window.matched(row)?)
                    });
            }
            return self.visit_physical_ip(address, |mut row| {
                row.id = aliases.logical_id(row.id);
                emit(row)
            });
        }
        self.visit_physical_ip(address, emit)
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
        let rows = self.asn_rows(bytes)?;
        let first = rows.partition_point(|row| asn_key(row) < number);
        let matches = rows
            .get(first..)
            .ok_or(Error::Invalid("ASN index position"))?;

        for row in matches {
            if asn_key(row) != number {
                break;
            }

            let id = u32_at(row, 4)?;

            emit(Match {
                id: self.logical_id(id),
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

        self.layout
            .v4
            .visit_entries(bytes, |entry| emit(self.physical_match(entry, false)?))?;

        let mut windows = self
            .aliases
            .as_ref()
            .into_iter()
            .flat_map(|aliases| aliases.windows.iter())
            .peekable();

        self.layout.v6.visit_entries(bytes, |entry| {
            while windows
                .peek()
                .is_some_and(|window| window.end() < entry.start)
            {
                if let Some(&window) = windows.next() {
                    self.visit_alias(window, &mut emit)?;
                }
            }
            emit(self.physical_match(entry, true)?)
        })?;

        for &window in windows {
            self.visit_alias(window, &mut emit)?;
        }

        for position in 0..self.layout.asns.count {
            let row = self.layout.asns.row(bytes, position)?;
            let id = u32_at(row, 4)?;

            emit(Match {
                id: self.logical_id(id),
                target: Target::Asn(u32_at(row, 0)?),
                payload: self.layout.payload(bytes, id)?,
            })?;
        }

        Ok(())
    }

    fn visit_physical_ip<'a, E: From<Error>>(
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

        if section.encoding == Encoding::Packed {
            let bytes = self.bytes.as_ref();
            if let Some(entry) = self.lookup_packed_ip(bytes, section, query)? {
                emit(Match {
                    id: entry.id,
                    target: entry.target(ipv6)?,
                    payload: self.layout.payload(bytes, entry.id)?,
                })?;
            }
            return Ok(());
        }

        self.visit_interval(section, 0, section.count, query, ipv6, &mut emit)
    }

    fn lookup_packed_ip(
        &self,
        bytes: &[u8],
        section: Section,
        query: u128,
    ) -> Result<Option<Entry>, Error> {
        if let Some(routes) = &self.broad_routes
            && let Some(entry) = routes.lookup(query, section.ipv6)?
        {
            return Ok(Some(entry));
        }

        let data = slice(bytes, section.offset, section.length)?;
        packed::lookup(
            data,
            section.count,
            query,
            section.ipv6,
            self.search,
            if section.ipv6 {
                None
            } else {
                self.ipv4_directory.as_ref()
            },
        )
    }

    fn asn_rows<'a>(&self, bytes: &'a [u8]) -> Result<&'a [[u8; 8]], Error> {
        let section = self.layout.asns;
        let (rows, remainder) = slice(bytes, section.offset, section.length)?.as_chunks::<8>();
        if !remainder.is_empty() || rows.len() != section.count {
            return Err(Error::Invalid("ASN section length"));
        }
        Ok(rows)
    }

    fn logical_id(&self, id: u32) -> u32 {
        self.aliases
            .as_ref()
            .map_or(id, |aliases| aliases.logical_id(id))
    }

    fn physical_match(&self, entry: Entry, ipv6: bool) -> Result<Match<'_>, Error> {
        Ok(Match {
            id: self.logical_id(entry.id),
            target: entry.target(ipv6)?,
            payload: self.layout.payload(self.bytes.as_ref(), entry.id)?,
        })
    }

    fn visit_alias<'a, E: From<Error>>(
        &'a self,
        window: SharedAlias,
        emit: &mut impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let bytes = self.bytes.as_ref();
        self.layout.v4.visit_entries(bytes, |entry| {
            emit(window.matched(Match {
                id: entry.id,
                target: entry.target(false)?,
                payload: self.layout.payload(bytes, entry.id)?,
            })?)
        })
    }

    fn visit_interval<'a, E: From<Error>>(
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

        self.visit_interval(section, low, mid, query, ipv6, emit)?;

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

        self.visit_interval(section, mid + 1, high, query, ipv6, emit)?;

        Ok(())
    }
}

impl<B: AsRef<[u8]>> fmt::Debug for Reader<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Reader")
            .field("records", &self.len())
            .field("artifact_bytes", &self.layout.bytes)
            .finish()
    }
}

fn asn_key(row: &[u8; 8]) -> u32 {
    let [a, b, c, d, _, _, _, _] = *row;
    u32::from_le_bytes([a, b, c, d])
}
