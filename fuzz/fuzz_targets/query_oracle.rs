use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ops::ControlFlow,
};

use netindex::{Error, Query, Reader, Target};

fn bounds(target: Target) -> Option<(bool, u128, u128)> {
    let number = |ip| match ip {
        IpAddr::V4(v) => u128::from(u32::from(v)),
        IpAddr::V6(v) => u128::from(v),
    };
    match target {
        Target::Address(v) => Some((v.is_ipv6(), number(v), number(v))),
        Target::Range { start, end } => Some((start.is_ipv6(), number(start), number(end))),
        Target::Network { address, prefix } => {
            let width = if address.is_ipv6() { 128 } else { 32 };
            Some((
                address.is_ipv6(),
                number(address),
                number(address)
                    | u128::MAX
                        .checked_shr(128 - u32::from(width - prefix))
                        .unwrap_or(0),
            ))
        }
        Target::Asn(_) => None,
    }
}

pub fn check<B: AsRef<[u8]>>(reader: &Reader<B>, original: &[(u32, Target, &[u8])]) {
    let info = reader.inspect().unwrap();
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
    let mut queries = vec![
        Query::Ip(Ipv4Addr::UNSPECIFIED.into()),
        Query::Ip(Ipv6Addr::UNSPECIFIED.into()),
        Query::Asn(0),
        Query::Intersecting(Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        }),
        Query::Intersecting(Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        }),
    ];
    for &(_, target, _) in original.iter().take(12) {
        match target {
            Target::Asn(k) => queries.push(Query::Asn(k)),
            _ => {
                queries.push(Query::Intersecting(target));
                let (v6, start, end) = bounds(target).unwrap();
                let ip = |v| {
                    if v6 {
                        IpAddr::from(Ipv6Addr::from(v))
                    } else {
                        IpAddr::from(Ipv4Addr::from(v as u32))
                    }
                };
                queries.push(Query::Ip(ip(end)));
                queries.push(Query::Intersecting(Target::Range {
                    start: ip(start.saturating_sub(1)),
                    end: ip(end),
                }));
            }
        }
    }
    let mut expected = Vec::new();
    for (position, query) in queries.iter().enumerate() {
        let mut rows: Vec<_> = original
            .iter()
            .copied()
            .filter(|(_, target, _)| match *query {
                Query::Asn(k) => *target == Target::Asn(k),
                Query::Ip(ip) => match (bounds(*target), bounds(Target::Address(ip))) {
                    (Some((v, a, b)), Some((w, q, _))) => v == w && a <= q && q <= b,
                    _ => false,
                },
                Query::Intersecting(q) => match (bounds(*target), bounds(q)) {
                    (Some((v, a, b)), Some((w, c, d))) => v == w && a <= d && b >= c,
                    _ => false,
                },
            })
            .collect();
        rows.sort_unstable_by_key(|(id, target, _)| {
            let (_, start, end) = bounds(*target).unwrap_or((false, 0, 0));
            (start, end, *id)
        });
        match *query {
            Query::Ip(ip) => assert_eq!(reader.contains_ip(ip).unwrap(), !rows.is_empty()),
            Query::Asn(k) => assert_eq!(reader.contains_asn(k).unwrap(), !rows.is_empty()),
            Query::Intersecting(q) => {
                let mut actual = Vec::new();
                reader
                    .visit_intersecting(q, |row| {
                        actual.push((row.id, row.target, row.payload));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(actual, rows);
            }
        }
        expected.extend(rows.into_iter().map(|row| (position, row)));
    }
    let mut actual = Vec::new();
    reader
        .visit_batch(&queries, |q, row| {
            actual.push((q, (row.id, row.target, row.payload)));
            Ok::<_, Error>(())
        })
        .unwrap();
    assert_eq!(actual, expected);
    let mut calls = 0;
    let stopped = reader
        .try_visit_batch(&queries, |q, row| {
            calls += 1;
            Ok::<_, Error>(ControlFlow::Break((q, row.id)))
        })
        .unwrap();
    match expected.first() {
        None => {
            assert_eq!(calls, 0);
            assert!(stopped.is_continue());
        }
        Some((q, (id, _, _))) => {
            assert_eq!(calls, 1);
            assert_eq!(stopped, ControlFlow::Break((*q, (*q, *id))));
        }
    }
}
