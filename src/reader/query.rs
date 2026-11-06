use std::{fmt, net::IpAddr, ops::ControlFlow};

use crate::{
    Error, Match, Reader, SharedAlias, Target,
    layout::{Encoding, Section, slice},
    packed,
    target::{Entry, number},
};

use super::asn_key;

/// One query in a borrowed, query-major batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Query {
    /// All assertions containing an address.
    Ip(IpAddr),
    /// All assertions for an exact ASN; zero is a miss.
    Asn(u32),
    /// All IP assertions intersecting this inclusive address/network/range.
    /// Invalid targets, including ASN targets, fail when this query is reached.
    Intersecting(Target),
}

/// A batch failure, after delivering the exact prefix preceding this failure.
#[derive(Debug)]
pub struct BatchError<E> {
    /// Zero-based input query position; earlier queries completed successfully.
    pub query_index: usize,
    /// Original reader or visitor failure; no later callback is invoked.
    pub error: E,
}

impl<E: fmt::Display> fmt::Display for BatchError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "batch query {} failed: {}", self.query_index, self.error)
    }
}

impl<E: std::error::Error + 'static> std::error::Error for BatchError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl<B: AsRef<[u8]>> Reader<B> {
    /// Visit queries in input order and each query's normal result order.
    ///
    /// Duplicated queries execute independently. Misses emit nothing. Results borrow
    /// this reader, not the input slice; the caller owns output storage. No result
    /// buffers, sorting, threads or speculative future queries are used. A callback
    /// may retain matches for the reader's lifetime. An empty batch does no work.
    ///
    /// # Errors
    /// Returns the failing query position and original error. Earlier queries and
    /// callbacks in that query have already run; side effects are not rolled back.
    pub fn visit_batch<'a, E: From<Error>>(
        &'a self,
        queries: &[Query],
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), BatchError<E>> {
        for (query_index, &query) in queries.iter().enumerate() {
            let result = match query {
                Query::Ip(address) => self.visit_ip(address, |row| emit(query_index, row)),
                Query::Asn(number) => self.visit_asn(number, |row| emit(query_index, row)),
                Query::Intersecting(target) => {
                    self.visit_intersecting(target, |row| emit(query_index, row))
                }
            };
            result.map_err(|error| BatchError { query_index, error })?;
        }
        Ok(())
    }

    /// Visit an IP until the visitor returns `Break(value)` or results end.
    ///
    /// `Continue(())` means every match was visited. Breaking is successful and
    /// never visits later matches. Ordering and borrowing match [`Self::visit_ip`].
    ///
    /// # Errors
    /// Preserves reader and visitor errors separately from successful termination.
    pub fn try_visit_ip<'a, T, E: From<Error>>(
        &'a self,
        address: IpAddr,
        mut emit: impl FnMut(Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        finish(self.visit_ip(address, |row| step(emit(row))))
    }

    /// Visit an exact ASN with the early-stop contract of [`Self::try_visit_ip`].
    ///
    /// # Errors
    /// Preserves reader and visitor errors; zero is an ordinary miss.
    pub fn try_visit_asn<'a, T, E: From<Error>>(
        &'a self,
        number: u32,
        mut emit: impl FnMut(Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        finish(self.visit_asn(number, |row| step(emit(row))))
    }

    /// Visit all assertions with explicit, successful early termination.
    ///
    /// # Errors
    /// Preserves reader and visitor errors, following [`Self::visit_all`] ordering.
    pub fn try_visit_all<'a, T, E: From<Error>>(
        &'a self,
        mut emit: impl FnMut(Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        finish(self.visit_all(|row| step(emit(row))))
    }

    /// Visit a batch until a successful break, returning its query position/value.
    ///
    /// # Errors
    /// Uses [`Self::visit_batch`]'s exact-prefix error contract. A successful break
    /// does not process subsequent results or queries.
    pub fn try_visit_batch<'a, T, E: From<Error>>(
        &'a self,
        queries: &[Query],
        mut emit: impl FnMut(usize, Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<(usize, T)>, BatchError<E>> {
        match self.visit_batch(queries, |index, row| step(emit(index, row))) {
            Ok(()) => Ok(ControlFlow::Continue(())),
            Err(BatchError {
                query_index,
                error: Stop::Break(value),
            }) => Ok(ControlFlow::Break((query_index, value))),
            Err(BatchError {
                query_index,
                error: Stop::Error(error),
            }) => Err(BatchError { query_index, error }),
        }
    }

    /// Test membership, stopping at the first IP match without resolving payloads.
    ///
    /// # Errors
    /// Returns errors encountered in the index search. Payload bytes and references
    /// are validated when opening the reader, and are not accessed by this query.
    pub fn contains_ip(&self, address: IpAddr) -> Result<bool, Error> {
        let address = if let IpAddr::V6(ip) = address
            && let Some(window) = self
                .aliases
                .as_ref()
                .and_then(|aliases| aliases.lookup(ip.into()))
        {
            window.source_address(ip.into()).into()
        } else {
            address
        };

        let ipv6 = address.is_ipv6();
        let query = number(address);
        let (first, last) = if ipv6 { self.bounds[1] } else { self.bounds[0] };
        if query < first || query > last {
            return Ok(false);
        }

        let section = if ipv6 { self.layout.v6 } else { self.layout.v4 };
        let bytes = self.bytes.as_ref();
        if section.encoding == Encoding::Packed {
            if let Some(entry) = self.lookup_packed_ip(bytes, section, query)? {
                entry.validate(ipv6)?;
                return Ok(true);
            }
            return Ok(false);
        }

        contains_interval(section, bytes, 0, section.count, query)
    }

    /// Test exact ASN membership; zero is a miss. Stops at the first match.
    ///
    /// # Errors
    /// Returns errors encountered in the ASN index search. Does not read payloads
    /// or their references.
    pub fn contains_asn(&self, number: u32) -> Result<bool, Error> {
        let rows = self.asn_rows(self.bytes.as_ref())?;
        let first = rows.partition_point(|row| asn_key(row) < number);
        Ok(rows.get(first).is_some_and(|row| asn_key(row) == number))
    }

    /// Visit every IP assertion intersecting an inclusive address/network/range.
    ///
    /// Returns whole original targets, not clipped results. Only the same address
    /// family is searched; IPv4 and mapped IPv6 remain distinct. Duplicates are
    /// preserved, ordered by `(start, end, ID)` as in [`Self::visit_all`]. Alias
    /// windows participate in that order without materializing projected tables.
    /// ASN targets and malformed ranges/networks are invalid, not empty results.
    ///
    /// # Errors
    /// Validates the query before emitting; thereafter stops on the first reader
    /// or visitor error. Partial delivery is not rolled back.
    pub fn visit_intersecting<'a, E: From<Error>>(
        &'a self,
        target: Target,
        mut emit: impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let (query, ipv6) = target.entry(0)?;
        let section = if ipv6 { self.layout.v6 } else { self.layout.v4 };
        let mut windows = self
            .aliases
            .as_ref()
            .into_iter()
            .flat_map(|aliases| aliases.windows.iter())
            .peekable();

        self.intersect_physical(section, query.start, query.end, ipv6, &mut |row| {
            if ipv6 {
                let (entry, _) = row.target.entry(0)?;
                while windows
                    .peek()
                    .is_some_and(|window| window.end() < entry.start)
                {
                    if let Some(&window) = windows.next() {
                        self.intersect_alias(window, query.start, query.end, &mut emit)?;
                    }
                }
            }
            emit(row)
        })?;

        if ipv6 {
            for &window in windows {
                self.intersect_alias(window, query.start, query.end, &mut emit)?;
            }
        }

        Ok(())
    }

    /// Intersect IP targets with the early-stop contract of [`Self::try_visit_ip`].
    ///
    /// # Errors
    /// Rejects malformed/non-IP targets and preserves reader/visitor failures.
    pub fn try_visit_intersecting<'a, T, E: From<Error>>(
        &'a self,
        target: Target,
        mut emit: impl FnMut(Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        finish(self.visit_intersecting(target, |row| step(emit(row))))
    }

    fn intersect_alias<'a, E: From<Error>>(
        &'a self,
        window: SharedAlias,
        first: u128,
        last: u128,
        emit: &mut impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if first > window.end() || last < window.start() {
            return Ok(());
        }

        self.intersect_entries(
            self.layout.v4,
            0,
            self.layout.v4.count,
            number(window.source_address(first.max(window.start())).into()),
            number(window.source_address(last.min(window.end())).into()),
            false,
            &mut |entry| {
                emit(window.matched(Match {
                    id: entry.id,
                    target: entry.target(false)?,
                    payload: self.layout.payload(self.bytes.as_ref(), entry.id)?,
                })?)
            },
        )
    }

    fn intersect_physical<'a, E: From<Error>>(
        &'a self,
        section: Section,
        first: u128,
        last: u128,
        ipv6: bool,
        emit: &mut impl FnMut(Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.intersect_entries(section, 0, section.count, first, last, ipv6, &mut |entry| {
            emit(self.physical_match(entry, ipv6)?)
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "bounded interval traversal carries explicit query and subtree bounds"
    )]
    fn intersect_entries<E: From<Error>>(
        &self,
        section: Section,
        low: usize,
        high: usize,
        first: u128,
        last: u128,
        ipv6: bool,
        emit: &mut impl FnMut(Entry) -> Result<(), E>,
    ) -> Result<(), E> {
        if section.encoding == Encoding::Packed {
            return packed::visit_intersecting(
                slice(self.bytes.as_ref(), section.offset, section.length)?,
                section.count,
                first,
                last,
                ipv6,
                self.search,
                emit,
            );
        }

        if low == high {
            return Ok(());
        }

        let bytes = self.bytes.as_ref();
        let mid = low + (high - low) / 2;
        if section.maximum(bytes, mid)? < first || section.start(bytes, low)? > last {
            return Ok(());
        }

        self.intersect_entries(section, low, mid, first, last, ipv6, emit)?;

        if section.start(bytes, mid)? > last {
            return Ok(());
        }

        let entry = section.entry(bytes, mid)?;
        if entry.end >= first {
            emit(entry)?;
        }

        self.intersect_entries(section, mid + 1, high, first, last, ipv6, emit)
    }
}

fn contains_interval(
    section: Section,
    bytes: &[u8],
    low: usize,
    high: usize,
    query: u128,
) -> Result<bool, Error> {
    if low == high {
        return Ok(false);
    }

    let mid = low + (high - low) / 2;
    if section.maximum(bytes, mid)? < query || section.start(bytes, low)? > query {
        return Ok(false);
    }

    if contains_interval(section, bytes, low, mid, query)? {
        return Ok(true);
    }

    if section.start(bytes, mid)? > query {
        return Ok(false);
    }

    let entry = section.entry(bytes, mid)?;
    if entry.end >= query {
        entry.validate(section.ipv6)?;
        return Ok(true);
    }

    contains_interval(section, bytes, mid + 1, high, query)
}

enum Stop<E, T> {
    Error(E),
    Break(T),
}

impl<E: From<Error>, T> From<Error> for Stop<E, T> {
    fn from(error: Error) -> Self {
        Self::Error(error.into())
    }
}

fn step<E, T>(result: Result<ControlFlow<T>, E>) -> Result<(), Stop<E, T>> {
    match result {
        Ok(ControlFlow::Continue(())) => Ok(()),
        Ok(ControlFlow::Break(value)) => Err(Stop::Break(value)),
        Err(error) => Err(Stop::Error(error)),
    }
}

fn finish<E, T>(result: Result<(), Stop<E, T>>) -> Result<ControlFlow<T>, E> {
    match result {
        Ok(()) => Ok(ControlFlow::Continue(())),
        Err(Stop::Break(value)) => Ok(ControlFlow::Break(value)),
        Err(Stop::Error(error)) => Err(error),
    }
}
