//! Compare public queries with a linear scan of the original assertions.

use std::{cmp::Reverse, net::IpAddr, ops::ControlFlow};

use netindex::{Error, Inspection, Match, Query, Reader, Target};

type Assertion = (u32, Target, Vec<u8>);

pub(super) fn assert_inspection_totals(info: Inspection) {
    assert_eq!(
        info.header_bytes
            + info.metadata_bytes
            + info.ipv4.bytes
            + info.ipv6.bytes
            + info.asn.bytes
            + info.reference_bytes
            + info.payload_bytes,
        info.artifact_bytes
    );
    assert_eq!(
        info.ipv4.records + info.ipv6.records + info.asn.records,
        info.physical_records
    );
    assert!(info.ipv4.bitpacked_blocks <= info.ipv4.blocks);
    assert!(info.ipv6.bitpacked_blocks <= info.ipv6.blocks);
    assert_eq!(
        info.validation_id_bitmap_bytes,
        info.physical_records.div_ceil(64) * 8
    );
}

#[expect(
    clippy::unwrap_used,
    reason = "query errors are unexpected for valid fixture input"
)]
pub(super) fn assert_queries_match_original(
    reader: &Reader<impl AsRef<[u8]>>,
    targets: &[Assertion],
    queries: &[Query],
) {
    let expected_by_query: Vec<_> = queries
        .iter()
        .map(|&query| expected(targets, query))
        .collect();
    assert_inspection_totals(reader.inspect().unwrap());

    let mut actual = Vec::new();
    reader
        .visit_batch(queries, |index, row| {
            actual.push((index, owned_match(row)));
            Ok::<_, Error>(())
        })
        .unwrap();
    let expected_batch = expected_by_query
        .iter()
        .enumerate()
        .flat_map(|(index, rows)| rows.iter().map(move |row| (index, row)));
    assert_eq!(
        actual.len(),
        expected_by_query.iter().map(Vec::len).sum::<usize>()
    );
    for ((index, actual), (expected_index, expected)) in actual.iter().zip(expected_batch) {
        assert_eq!((*index, actual), (expected_index, expected), "batch visit");
    }

    for (index, (&query, want)) in queries.iter().zip(&expected_by_query).enumerate() {
        let mut rows = Vec::new();
        match query {
            Query::Ip(ip) => {
                reader
                    .visit_ip(ip, |row| {
                        rows.push(owned_match(row));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(reader.contains_ip(ip).unwrap(), !want.is_empty());
                let winner = want
                    .iter()
                    .filter_map(|row| prefix_rank(row.1).map(|rank| (rank, row)))
                    .min_by_key(|(rank, row)| (Reverse(*rank), row.0))
                    .map(|(_, row)| row);
                assert_eq!(
                    reader.longest_prefix(ip).unwrap().map(owned_match).as_ref(),
                    winner
                );
            }
            Query::Asn(key) => {
                reader
                    .visit_asn(key, |row| {
                        rows.push(owned_match(row));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(reader.contains_asn(key).unwrap(), !want.is_empty());
            }
            Query::Intersecting(target) => reader
                .visit_intersecting(target, |row| {
                    rows.push(owned_match(row));
                    Ok::<_, Error>(())
                })
                .unwrap(),
        }
        assert_eq!(&rows, want, "query {index}: {query:?}");

        let stop = reader
            .try_visit_batch(&[query], |_, row| Ok::<_, Error>(ControlFlow::Break(row)))
            .unwrap();
        let first = match stop {
            ControlFlow::Continue(()) => None,
            ControlFlow::Break((position, row)) => {
                assert_eq!(position, 0, "single-query batch position");
                Some(owned_match(row))
            }
        };
        assert_eq!(first.as_ref(), want.first(), "query {index}: {query:?}");
    }
}

fn ip_number(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(ip) => u128::from(u32::from(ip)),
        IpAddr::V6(ip) => u128::from(ip),
    }
}

pub(super) fn ip_bounds(target: Target) -> Option<(bool, u128, u128)> {
    match target {
        Target::Address(ip) => Some((ip.is_ipv6(), ip_number(ip), ip_number(ip))),
        Target::Network { address, prefix } => {
            let width = if address.is_ipv6() { 128 } else { 32 };
            let host = u128::MAX
                .checked_shr(128 - (width - u32::from(prefix)))
                .unwrap_or(0);
            let start = ip_number(address);
            Some((address.is_ipv6(), start, start | host))
        }
        Target::Range { start, end } => Some((start.is_ipv6(), ip_number(start), ip_number(end))),
        Target::Asn(_) => None,
    }
}

fn expected(targets: &[Assertion], query: Query) -> Vec<Assertion> {
    let mut result: Vec<_> = targets
        .iter()
        .filter(|(_, target, _)| match query {
            Query::Asn(key) => *target == Target::Asn(key),
            Query::Ip(ip) => ip_bounds(*target).is_some_and(|(v6, start, end)| {
                let q = ip_number(ip);
                v6 == ip.is_ipv6() && start <= q && q <= end
            }),
            Query::Intersecting(query) => match (ip_bounds(*target), ip_bounds(query)) {
                (Some((v6, start, end)), Some((qv6, first, last))) => {
                    v6 == qv6 && start <= last && end >= first
                }
                _ => false,
            },
        })
        .cloned()
        .collect();

    result.sort_by_key(|(id, target, _)| match ip_bounds(*target) {
        Some((_, start, end)) => (start, end, *id),
        None => (0, 0, *id),
    });
    result
}

fn prefix_rank(target: Target) -> Option<u8> {
    match target {
        Target::Network { prefix, .. } => Some(prefix),
        Target::Address(ip) => Some(if ip.is_ipv6() { 128 } else { 32 }),
        _ => None,
    }
}

pub(super) fn owned_match(row: Match<'_>) -> Assertion {
    (row.id, row.target, row.payload.to_vec())
}
