use std::{net::IpAddr, ops::ControlFlow};

use crate::{
    Error, Match, Reader,
    layout::{Encoding, Section},
    target::{Entry, number},
};

impl<B: AsRef<[u8]>> Reader<B> {
    /// Return the most specific matching address or CIDR in the query family.
    ///
    /// Addresses count as host prefixes. Arbitrary range assertions are ignored,
    /// even when their bounds happen to describe a CIDR. Equal prefixes choose
    /// the lowest logical assertion ID. Shared aliases return the projected IPv6
    /// target and logical ID; the match borrows the original payload.
    /// Returns `None` if no address or CIDR matches. Use [`Self::lookup_ip`]
    /// to include ranges and preserve all matches.
    ///
    /// # Examples
    ///
    /// ```
    /// use netindex::{Builder, Limits, Reader, Target};
    ///
    /// let limits = Limits::default();
    /// let mut builder = Builder::new(limits);
    /// let address = "203.0.113.42".parse()?;
    /// builder.push(Target::Network { address: "203.0.113.0".parse()?, prefix: 24 }, b"network")?;
    /// builder.push(Target::Address(address), b"host")?;
    /// let reader = Reader::open(builder.into_bytes(b"")?, limits)?;
    ///
    /// assert_eq!(reader.lookup_ip(address)?.len(), 2);
    /// assert_eq!(reader.longest_prefix(address)?.map(|row| row.payload), Some(&b"host"[..]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// Returns errors encountered while reading the search path or the winner.
    pub fn longest_prefix(&self, address: IpAddr) -> Result<Option<Match<'_>>, Error> {
        let window = match address {
            IpAddr::V6(ip) => self.aliases.as_ref().and_then(|a| a.lookup(u128::from(ip))),
            IpAddr::V4(_) => None,
        };

        let source = window.map_or(address, |window| {
            window.source_address(number(address)).into()
        });
        let ipv6 = source.is_ipv6();
        let section = if ipv6 { self.layout.v6 } else { self.layout.v4 };
        if section.encoding == Encoding::Packed {
            // Packed sections are disjoint and contain only networks and addresses.
            return match self
                .try_visit_ip(address, |row| Ok::<_, Error>(ControlFlow::Break(row)))?
            {
                ControlFlow::Break(row) => Ok(Some(row)),
                ControlFlow::Continue(()) => Ok(None),
            };
        }

        let query = number(source);
        let (first, last) = if ipv6 { self.bounds[1] } else { self.bounds[0] };
        if query < first || query > last {
            return Ok(None);
        }

        let bytes = self.bytes.as_ref();
        let mut best = None;
        seek_interval_prefix(section, bytes, 0, section.count, query, &mut best)?;

        let Some(entry) = best else { return Ok(None) };
        let row = Match {
            id: entry.id,
            target: entry.target(ipv6)?,
            payload: self.layout.payload(bytes, entry.id)?,
        };

        if let Some(window) = window {
            Ok(Some(window.matched(row)?))
        } else {
            Ok(Some(Match {
                id: self.logical_id(row.id),
                ..row
            }))
        }
    }
}

fn seek_interval_prefix(
    section: Section,
    bytes: &[u8],
    low: usize,
    high: usize,
    query: u128,
    best: &mut Option<Entry>,
) -> Result<(), Error> {
    if low == high {
        return Ok(());
    }

    let mid = low + (high - low) / 2;
    if section.maximum(bytes, mid)? < query {
        return Ok(());
    }

    let entry = section.entry(bytes, mid)?;
    if entry.start > query {
        return seek_interval_prefix(section, bytes, low, mid, query, best);
    }

    seek_interval_prefix(section, bytes, mid + 1, high, query, best)?;

    // Containing CIDRs are nested. A prefix starting earlier than the winner
    // cannot be as specific; equal starts still require end and ID comparison.
    if best.is_some_and(|winner| entry.start < winner.start) {
        return Ok(());
    }

    if entry.kind != 2
        && entry.end >= query
        && best.is_none_or(|winner| {
            entry.prefix > winner.prefix || (entry.prefix == winner.prefix && entry.id < winner.id)
        })
    {
        *best = Some(entry);
    }

    seek_interval_prefix(section, bytes, low, mid, query, best)
}
