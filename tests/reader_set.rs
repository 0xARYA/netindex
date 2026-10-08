//! Source identity, complete membership, replacement and error propagation.

#![cfg(test)]

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use netindex::{Builder, Error, Limits, Reader, ReaderSet, Target};

#[test]
fn routed_visits_match_independent_ranges_and_keep_every_source_and_id() {
    let targets = [
        vec![
            Target::Range {
                start: Ipv4Addr::from(19).into(),
                end: Ipv4Addr::from(41).into(),
            },
            Target::Address(Ipv4Addr::from(23).into()),
            Target::Address(Ipv4Addr::from(23).into()),
        ],
        vec![
            Target::Network {
                address: Ipv4Addr::UNSPECIFIED.into(),
                prefix: 0,
            },
            Target::Asn(64512),
            Target::Asn(64512),
        ],
        vec![Target::Range {
            start: Ipv6Addr::from(19u128).into(),
            end: Ipv6Addr::from(41u128).into(),
        }],
        vec![],
    ];
    let readers: Vec<_> = targets
        .iter()
        .map(|rows| reader(rows, b"evidence"))
        .collect();

    for set in [
        ReaderSet::new(readers.clone()).unwrap(),
        ReaderSet::with_fine_routing(readers.clone()).unwrap(),
    ] {
        for address in (0..64)
            .map(|n| IpAddr::from(Ipv4Addr::from(n)))
            .chain((0..64).map(|n| IpAddr::from(Ipv6Addr::from(n as u128))))
        {
            let mut expected = Vec::new();
            for (source, rows) in targets.iter().enumerate() {
                for (id, &target) in rows.iter().enumerate() {
                    let matches = match target {
                        Target::Address(value) => value == address,
                        Target::Range { start, end } => {
                            start.is_ipv6() == address.is_ipv6()
                                && start <= address
                                && address <= end
                        }
                        Target::Network {
                            address: base,
                            prefix: 0,
                        } => base.is_ipv6() == address.is_ipv6(),
                        _ => false,
                    };
                    if matches {
                        expected.push((source, id as u32, target));
                    }
                }
            }

            let mut actual = Vec::new();
            set.visit_ip(address, |source, matched| {
                actual.push((source, matched.id, matched.target));
                assert_eq!(matched.payload, b"evidence");
                Ok::<_, Error>(())
            })
            .unwrap();

            expected.sort_by_key(|&(source, id, _)| (source, id));
            actual.sort_by_key(|&(source, id, _)| (source, id));

            assert_eq!(actual, expected);
        }

        let mut asns = Vec::new();
        set.visit_asn(64512, |source, row| {
            asns.push((source, row.id));
            Ok::<_, Error>(())
        })
        .unwrap();

        assert_eq!(asns, [(1, 1), (1, 2)]);
        assert_eq!(set.len(), 4);
        assert!(Arc::ptr_eq(set.reader(0).unwrap(), &readers[0]));
        assert!(set.reader(4).is_none());
    }
}

#[test]
fn replacement_keeps_old_snapshots_borrowed_payloads_and_unchanged_readers() {
    let first: IpAddr = "203.0.113.1".parse().unwrap();
    let second: IpAddr = "198.51.100.2".parse().unwrap();
    let unchanged = reader(&[Target::Address(first)], b"unchanged");

    for fine in [false, true] {
        let readers = vec![
            reader(&[Target::Address(first)], b"old"),
            Arc::clone(&unchanged),
        ];
        let old = if fine {
            ReaderSet::with_fine_routing(readers)
        } else {
            ReaderSet::new(readers)
        }
        .unwrap();
        let mut borrowed = Vec::new();
        old.visit_ip(first, |_, row| {
            borrowed.push(row.payload);
            Ok::<_, Error>(())
        })
        .unwrap();

        let new = old
            .replaced(0, reader(&[Target::Address(second)], b"new"))
            .unwrap();

        assert!(Arc::ptr_eq(new.reader(1).unwrap(), &unchanged));
        assert_eq!(borrowed, [b"old".as_slice(), b"unchanged".as_slice()]);

        let mut matches = Vec::new();
        new.visit_ip(first, |source, _| {
            matches.push(source);
            Ok::<_, Error>(())
        })
        .unwrap();

        assert_eq!(matches, [1]);

        matches.clear();
        new.visit_ip(second, |source, row| {
            matches.push(source);
            assert_eq!(row.payload, b"new");
            Ok::<_, Error>(())
        })
        .unwrap();

        assert_eq!(matches, [0]);
        assert!(matches!(
            old.replaced(2, Arc::clone(&unchanged)),
            Err(Error::Invalid("reader position"))
        ));
    }
}

#[test]
fn empty_sets_limits_and_visitor_errors_are_explicit() {
    let empty = ReaderSet::<Vec<u8>>::new(vec![]).unwrap();
    assert!(empty.is_empty());

    let address: IpAddr = "203.0.113.42".parse().unwrap();
    let shared = reader(&[Target::Address(address), Target::Asn(64512)], b"value");
    assert!(matches!(
        ReaderSet::new(vec![Arc::clone(&shared); 65]),
        Err(Error::Limit("routed readers"))
    ));
    assert!(matches!(
        ReaderSet::with_fine_routing(vec![Arc::clone(&shared); 65]),
        Err(Error::Limit("routed readers"))
    ));

    for set in [
        ReaderSet::new(vec![Arc::clone(&shared); 64]).unwrap(),
        ReaderSet::with_fine_routing(vec![shared; 64]).unwrap(),
    ] {
        let mut sources = Vec::new();
        set.visit_ip(address, |source, _| {
            sources.push(source);
            Ok::<_, Error>(())
        })
        .unwrap();

        assert_eq!(sources, (0..64).collect::<Vec<_>>());

        #[derive(Debug)]
        enum VisitError {
            Index,
            Stop,
        }
        impl From<Error> for VisitError {
            fn from(_: Error) -> Self {
                Self::Index
            }
        }

        let mut calls = 0;
        let result = set.visit_ip(address, |_, _| {
            calls += 1;
            Err(VisitError::Stop)
        });

        assert!(matches!(result, Err(VisitError::Stop)));
        assert_eq!(calls, 1);

        calls = 0;
        let result = set.visit_asn(64512, |_, _| {
            calls += 1;
            Err(VisitError::Stop)
        });

        assert!(matches!(result, Err(VisitError::Stop)));
        assert_eq!(calls, 1);
    }
}

#[test]
fn fine_routing_preserves_cross_bucket_ranges_and_address_extremes() {
    let start = (63u128 << 112) + 7;
    let end = (129u128 << 112) + 3;
    let set = ReaderSet::with_fine_routing(vec![
        reader(
            &[Target::Network {
                address: Ipv4Addr::UNSPECIFIED.into(),
                prefix: 0,
            }],
            b"all-v4",
        ),
        reader(
            &[Target::Range {
                start: Ipv6Addr::from(start).into(),
                end: Ipv6Addr::from(end).into(),
            }],
            b"range",
        ),
        reader(
            &[
                Target::Address(Ipv4Addr::BROADCAST.into()),
                Target::Address(Ipv6Addr::from(u128::MAX).into()),
            ],
            b"last",
        ),
    ])
    .unwrap();

    for number in [0, start - 1, start, end, end + 1, u128::MAX] {
        let mut matches = Vec::new();
        set.visit_ip(Ipv6Addr::from(number).into(), |source, row| {
            matches.push((source, row.id));
            Ok::<_, Error>(())
        })
        .unwrap();

        let expected = if (start..=end).contains(&number) {
            vec![(1, 0)]
        } else if number == u128::MAX {
            vec![(2, 1)]
        } else {
            vec![]
        };

        assert_eq!(matches, expected);
    }

    for number in [0, 65535, 65536, u32::MAX] {
        let mut matches = Vec::new();
        set.visit_ip(Ipv4Addr::from(number).into(), |source, row| {
            matches.push((source, row.id));
            Ok::<_, Error>(())
        })
        .unwrap();

        let mut expected = vec![(0, 0)];
        if number == u32::MAX {
            expected.push((2, 0));
        }

        assert_eq!(matches, expected);
    }

    assert!(
        ReaderSet::<Vec<u8>>::with_fine_routing(vec![])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn replacement_updates_family_coverage_and_exact_asn_memberships() {
    let v4: IpAddr = "203.0.113.42".parse().unwrap();
    let v6: IpAddr = "2001:db8::42".parse().unwrap();
    let initial = reader(&[Target::Address(v4)], b"v4");

    for old in [
        ReaderSet::new(vec![Arc::clone(&initial)]).unwrap(),
        ReaderSet::with_fine_routing(vec![Arc::clone(&initial)]).unwrap(),
    ] {
        let ipv6 = old
            .replaced(0, reader(&[Target::Address(v6)], b"v6"))
            .unwrap();
        let asn_only = ipv6
            .replaced(0, reader(&[Target::Asn(64512)], b"asn"))
            .unwrap();

        for (set, address, expected) in [
            (&old, v4, vec![b"v4".as_slice()]),
            (&old, v6, vec![]),
            (&ipv6, v4, vec![]),
            (&ipv6, v6, vec![b"v6".as_slice()]),
            (&asn_only, v4, vec![]),
            (&asn_only, v6, vec![]),
        ] {
            let mut payloads = Vec::new();
            set.visit_ip(address, |source, row| {
                assert_eq!(source, 0);
                payloads.push(row.payload);
                Ok::<_, Error>(())
            })
            .unwrap();

            assert_eq!(payloads, expected);
        }

        for (set, expected) in [
            (&old, vec![]),
            (&ipv6, vec![]),
            (&asn_only, vec![b"asn".as_slice()]),
        ] {
            let mut payloads = Vec::new();
            set.visit_asn(64512, |source, row| {
                assert_eq!(source, 0);
                payloads.push(row.payload);
                Ok::<_, Error>(())
            })
            .unwrap();

            assert_eq!(payloads, expected);
        }
    }
}

fn reader(targets: &[Target], payload: &[u8]) -> Arc<Reader<Vec<u8>>> {
    let mut builder = Builder::new(Limits::default());
    for &target in targets {
        builder.push(target, payload).unwrap();
    }

    let bytes = builder.into_bytes(b"").unwrap();
    Arc::new(Reader::open(bytes, Limits::default()).unwrap())
}
