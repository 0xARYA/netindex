//! Original-input oracles for batches, termination, intersections and inspection.
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ops::ControlFlow,
};

use netindex::{
    Builder, Error, Inspection, Limits, Match, Query, Reader, SectionEncoding, SharedAlias, Target,
};

fn bounds(target: Target) -> Option<(bool, u128, u128)> {
    let value = |ip: IpAddr| match ip {
        IpAddr::V4(ip) => u128::from(u32::from(ip)),
        IpAddr::V6(ip) => u128::from(ip),
    };
    match target {
        Target::Address(ip) => Some((ip.is_ipv6(), value(ip), value(ip))),
        Target::Network { address, prefix } => {
            let width = if address.is_ipv6() { 128 } else { 32 };
            let host = u128::MAX
                .checked_shr(128 - (width - u32::from(prefix)))
                .unwrap_or(0);
            Some((address.is_ipv6(), value(address), value(address) | host))
        }
        Target::Range { start, end } => Some((start.is_ipv6(), value(start), value(end))),
        Target::Asn(_) => None,
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "independent test oracle fails on invalid fixture construction"
)]
fn expected(targets: &[(u32, Target, Vec<u8>)], query: Query) -> Vec<(u32, Target, Vec<u8>)> {
    let mut result: Vec<_> = targets
        .iter()
        .filter(|(_, target, _)| match query {
            Query::Asn(key) => *target == Target::Asn(key),
            Query::Ip(ip) => bounds(*target).is_some_and(|(v6, start, end)| {
                let (_, q, _) = bounds(Target::Address(ip)).unwrap();
                v6 == ip.is_ipv6() && start <= q && q <= end
            }),
            Query::Intersecting(query) => match (bounds(*target), bounds(query)) {
                (Some((v6, start, end)), Some((qv6, first, last))) => {
                    v6 == qv6 && start <= last && end >= first
                }
                _ => false,
            },
        })
        .cloned()
        .collect();
    result.sort_by_key(|(id, target, _)| match bounds(*target) {
        Some((_, start, end)) => (start, end, *id),
        None => (0, 0, *id),
    });
    result
}

fn owned(row: Match<'_>) -> (u32, Target, Vec<u8>) {
    (row.id, row.target, row.payload.to_vec())
}

fn inspect_totals(info: Inspection) {
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
    clippy::indexing_slicing,
    clippy::panic,
    reason = "independent oracle assertions fail loudly on any contract mismatch"
)]
fn verify(
    reader: &Reader<impl AsRef<[u8]>>,
    targets: &[(u32, Target, Vec<u8>)],
    queries: &[Query],
) {
    inspect_totals(reader.inspect().unwrap());
    let mut actual = Vec::new();
    reader
        .visit_batch(queries, |index, row| {
            actual.push((index, owned(row)));
            Ok::<_, Error>(())
        })
        .unwrap();
    let wanted: Vec<_> = queries
        .iter()
        .enumerate()
        .flat_map(|(index, &query)| {
            expected(targets, query)
                .into_iter()
                .map(move |row| (index, row))
        })
        .collect();
    assert_eq!(actual, wanted);
    for (index, &query) in queries.iter().enumerate() {
        let want = expected(targets, query);
        let mut rows = Vec::new();
        match query {
            Query::Ip(ip) => {
                reader
                    .visit_ip(ip, |row| {
                        rows.push(owned(row));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(reader.contains_ip(ip).unwrap(), !want.is_empty());
            }
            Query::Asn(key) => {
                reader
                    .visit_asn(key, |row| {
                        rows.push(owned(row));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(reader.contains_asn(key).unwrap(), !want.is_empty());
            }
            Query::Intersecting(target) => reader
                .visit_intersecting(target, |row| {
                    rows.push(owned(row));
                    Ok::<_, Error>(())
                })
                .unwrap(),
        }
        assert_eq!(rows, want, "query {index}: {query:?}");
        let stop = reader
            .try_visit_batch(&[query], |_, row| Ok::<_, Error>(ControlFlow::Break(row)))
            .unwrap();
        match stop {
            ControlFlow::Continue(()) => assert!(want.is_empty()),
            ControlFlow::Break((0, row)) => assert_eq!(owned(row), want[0]),
            _ => panic!("single-query batch position"),
        }
    }
}

#[test]
fn generated_original_assertion_oracle_covers_layouts_boundaries_and_tails() {
    for (ipv6, overlapping) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut builder = Builder::new(Limits::default());
        let mut targets = Vec::new();
        let ip = |value| {
            if ipv6 {
                IpAddr::V6(Ipv6Addr::from(value))
            } else {
                IpAddr::V4(Ipv4Addr::from(value as u32))
            }
        };
        let mut queries = vec![
            Query::Ip(ip(0)),
            Query::Ip(ip(if ipv6 {
                u128::MAX
            } else {
                u128::from(u32::MAX)
            })),
        ];
        for i in 0..529u32 {
            let start = if overlapping {
                u128::from(i / 8) * 32
            } else {
                u128::from(i) * 17
            };
            let start = if ipv6 {
                start * ((1u128 << 80) + 1)
            } else {
                start
            };
            let target = if overlapping {
                Target::Range {
                    start: ip(start),
                    end: ip(start + 128),
                }
            } else {
                Target::Address(ip(start))
            };
            let payload = vec![i as u8; (i as usize % 7) + 1];
            builder.push(target, &payload).unwrap();
            targets.push((i, target, payload));
            queries.extend([
                Query::Ip(ip(start)),
                Query::Intersecting(Target::Range {
                    start: ip(start.saturating_sub(1)),
                    end: ip(start + 20),
                }),
            ]);
        }
        for id in 529..545 {
            let t = Target::Asn(id % 3 + 1);
            let p = vec![id as u8];
            builder.push(t, &p).unwrap();
            targets.push((id, t, p));
        }
        queries.extend([
            Query::Asn(0),
            Query::Asn(1),
            Query::Asn(u32::MAX),
            Query::Intersecting(Target::Network {
                address: ip(0),
                prefix: 0,
            }),
        ]);
        for legacy in [false, true] {
            let mut builder = Builder::new(Limits::default());
            for (_, target, payload) in &targets {
                builder.push(*target, payload).unwrap();
            }
            let mut bytes = Vec::new();
            if legacy {
                builder.write_legacy_to(&mut bytes, b"oracle").unwrap();
            } else {
                builder.write_to(&mut bytes, b"oracle").unwrap();
            }
            let reader = Reader::open(bytes, Limits::default()).unwrap();
            assert_eq!(
                reader.inspect().unwrap().ipv4.encoding == SectionEncoding::Packed,
                !ipv6 && !overlapping
            );
            verify(&reader, &targets, &queries);
            let mut compact = Vec::new();
            reader.write_bitpacked_to(&mut compact).unwrap();
            verify(
                &Reader::open(compact, Limits::default()).unwrap(),
                &targets,
                &queries,
            );
        }
    }
}

#[test]
fn aliases_intersections_preserve_projected_ids_whole_targets_and_order() {
    for prefix in [0, 64, 80, 96] {
        let mut builder = Builder::new(Limits::default());
        let mut targets = Vec::new();
        for i in 0..529u32 {
            let target = Target::Network {
                address: Ipv4Addr::from(i * 256).into(),
                prefix: 24,
            };
            let p = vec![i as u8; 2];
            builder.push(target, &p).unwrap();
            targets.push((i, target, p));
        }
        let mut windows = vec![SharedAlias {
            address: Ipv6Addr::UNSPECIFIED,
            prefix,
            first_id: 529,
            interleaved: false,
        }];
        if prefix != 0 {
            let t = Target::Address(Ipv6Addr::from(u128::MAX).into());
            builder.push(t, b"native").unwrap();
            targets.push((529, t, b"native".to_vec()));
            windows[0].first_id = 530;
            windows.push(SharedAlias {
                address: Ipv6Addr::from(1u128 << 127),
                prefix,
                first_id: 1059,
                interleaved: false,
            });
        }
        for &window in &windows {
            for i in 0..529u32 {
                targets.push((
                    window.first_id + i,
                    window.project(targets[i as usize].1).unwrap(),
                    targets[i as usize].2.clone(),
                ));
            }
        }
        let base = Reader::open(
            builder.into_bytes(b"alias query").unwrap(),
            Limits::default(),
        )
        .unwrap();
        let mut envelope = Vec::new();
        base.write_shared_aliases(&windows, &mut envelope, Limits::default())
            .unwrap();
        let mut queries = vec![Query::Intersecting(Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        })];
        for (_, target, _) in &targets {
            let Some((v6, start, end)) = bounds(*target) else {
                continue;
            };
            let ip = |v| {
                if v6 {
                    IpAddr::from(Ipv6Addr::from(v))
                } else {
                    IpAddr::from(Ipv4Addr::from(v as u32))
                }
            };
            queries.extend([
                Query::Intersecting(Target::Range {
                    start: ip(start),
                    end: ip(end),
                }),
                Query::Ip(ip(end)),
                Query::Intersecting(Target::Address(ip(end.saturating_add(1).min(if v6 {
                    u128::MAX
                } else {
                    u128::from(u32::MAX)
                })))),
            ]);
        }
        let reader = Reader::open(envelope, Limits::default()).unwrap();
        verify(&reader, &targets, &queries);
        assert_eq!(reader.inspect().unwrap().alias_windows, windows.len());
        let mut compact = Vec::new();
        reader.write_bitpacked_to(&mut compact).unwrap();
        verify(
            &Reader::open(compact, Limits::default()).unwrap(),
            &targets,
            &queries,
        );
    }
}

#[derive(Debug)]
enum Failure {
    Reader(Error),
    User(u32),
}
impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Self::Reader(e)
    }
}

#[test]
fn breaks_errors_and_lifetimes_have_exact_non_speculative_prefixes() {
    let mut builder = Builder::new(Limits::default());
    for _ in 0..8 {
        builder
            .push(
                Target::Network {
                    address: Ipv4Addr::UNSPECIFIED.into(),
                    prefix: 0,
                },
                b"borrowed",
            )
            .unwrap();
    }
    builder.push(Target::Asn(1), b"asn").unwrap();
    let reader = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();
    let query = Query::Ip(Ipv4Addr::LOCALHOST.into());
    let invalid = Query::Intersecting(Target::Network {
        address: Ipv4Addr::LOCALHOST.into(),
        prefix: 8,
    });
    let mut calls = Vec::new();
    let error = reader
        .visit_batch(&[query, query, invalid], |q, row| {
            calls.push((q, row.id));
            if q == 1 && row.id == 2 {
                Err(Failure::User(123))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.query_index, 1);
    assert!(matches!(error.error, Failure::User(123)));
    assert_eq!(calls.len(), 11);
    let error = reader
        .visit_batch(&[query, invalid], |_, _| Ok::<_, Failure>(()))
        .unwrap_err();
    assert_eq!(error.query_index, 1);
    assert!(matches!(error.error, Failure::Reader(Error::Invalid(_))));
    let stopped = reader
        .try_visit_batch(&[query, invalid], |_, row| {
            Ok::<_, Failure>(ControlFlow::Break(row))
        })
        .unwrap();
    assert!(matches!(
        stopped,
        ControlFlow::Break((0, Match { id: 0, .. }))
    ));
    assert!(matches!(
        reader
            .try_visit_asn(1, |row| Ok::<_, Error>(ControlFlow::Break(row.id)))
            .unwrap(),
        ControlFlow::Break(8)
    ));
    let mut calls = 0;
    reader
        .visit_batch(&[], |_, _| {
            calls += 1;
            Ok::<_, Error>(())
        })
        .unwrap();
    assert_eq!(calls, 0);
    let retained = {
        let queries = vec![query];
        let mut result = Vec::new();
        reader
            .visit_batch(&queries, |_, row| {
                result.push(row);
                Ok::<_, Error>(())
            })
            .unwrap();
        result
    };
    assert!(retained.iter().all(|row| row.payload == b"borrowed"));
    let pointer = reader.lookup_ip(Ipv4Addr::LOCALHOST.into()).unwrap()[0]
        .payload
        .as_ptr();
    assert!(retained.iter().all(|row| row.payload.as_ptr() == pointer));
    for target in [
        Target::Asn(1),
        Target::Range {
            start: Ipv4Addr::LOCALHOST.into(),
            end: Ipv6Addr::LOCALHOST.into(),
        },
        Target::Range {
            start: Ipv4Addr::BROADCAST.into(),
            end: Ipv4Addr::UNSPECIFIED.into(),
        },
    ] {
        assert!(
            reader
                .visit_intersecting(target, |_| -> Result<(), Error> {
                    panic!("invalid query emitted")
                })
                .is_err()
        );
    }
}

#[test]
fn independent_format_fixtures_inspection_and_queries_agree() {
    for bytes in [
        include_bytes!("fixtures/bitpacked-ipv4.nidx").as_slice(),
        include_bytes!("fixtures/bitpacked-ipv6.nidx").as_slice(),
    ] {
        let reader = Reader::open(bytes, Limits::default()).unwrap();
        let info = reader.inspect().unwrap();
        inspect_totals(info);
        assert_eq!(info.format, 6);
        assert_eq!(info.ipv4.bitpacked_blocks + info.ipv6.bitpacked_blocks, 1);
        let mut targets = Vec::new();
        reader
            .visit_all(|row| {
                targets.push(owned(row));
                Ok::<_, Error>(())
            })
            .unwrap();
        let queries = [
            Query::Intersecting(Target::Network {
                address: Ipv4Addr::UNSPECIFIED.into(),
                prefix: 0,
            }),
            Query::Intersecting(Target::Network {
                address: Ipv6Addr::UNSPECIFIED.into(),
                prefix: 0,
            }),
        ];
        verify(&reader, &targets, &queries);
    }
    let empty = Reader::open(
        Builder::new(Limits::default()).into_bytes(b"").unwrap(),
        Limits::default(),
    )
    .unwrap();
    inspect_totals(empty.inspect().unwrap());
    assert!(!empty.contains_asn(0).unwrap());
}

#[test]
fn reader_sets_preserve_query_source_order_and_early_stop() {
    use netindex::ReaderSet;
    use std::sync::Arc;
    let mut builder = Builder::new(Limits::default());
    builder
        .push(Target::Address(Ipv4Addr::LOCALHOST.into()), b"ip")
        .unwrap();
    builder.push(Target::Asn(1), b"asn").unwrap();
    let reader =
        Arc::new(Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap());
    let queries = [
        Query::Ip(Ipv4Addr::LOCALHOST.into()),
        Query::Asn(1),
        Query::Intersecting(Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        }),
    ];
    for fine in [false, true] {
        let readers = vec![reader.clone(), reader.clone()];
        let set = if fine {
            ReaderSet::with_fine_routing(readers)
        } else {
            ReaderSet::new(readers)
        }
        .unwrap();
        let mut rows = Vec::new();
        set.visit_batch(&queries, |q, source, row| {
            rows.push((q, source, row.id));
            Ok::<_, Error>(())
        })
        .unwrap();
        assert_eq!(
            rows,
            vec![
                (0, 0, 0),
                (0, 1, 0),
                (1, 0, 1),
                (1, 1, 1),
                (2, 0, 0),
                (2, 1, 0)
            ]
        );
        assert!(set.contains_ip(Ipv4Addr::LOCALHOST.into()).unwrap());
        assert!(set.contains_asn(1).unwrap());
        assert!(!set.contains_asn(0).unwrap());
        assert!(matches!(
            set.try_visit_batch(&queries, |q, source, row| Ok::<_, Error>(
                ControlFlow::Break((q, source, row.id))
            ))
            .unwrap(),
            ControlFlow::Break((0, (0, 0, 0)))
        ));
        let mut calls = 0;
        let error = set
            .visit_batch(&queries, |_, source, _| {
                calls += 1;
                if source == 1 {
                    Err(Failure::User(789))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(error.query_index, 0);
        assert!(matches!(error.error, Failure::User(789)));
        assert_eq!(calls, 2);
    }
    let empty = ReaderSet::<Vec<u8>>::new(vec![]).unwrap();
    assert!(
        empty
            .visit_intersecting(Target::Asn(1), |_, _| Ok::<_, Error>(()))
            .is_err()
    );
    assert!(!empty.contains_ip(Ipv4Addr::LOCALHOST.into()).unwrap());
}

#[test]
fn inspection_reports_truncated_backing_as_an_error() {
    use std::{cell::Cell, rc::Rc};
    struct Backing {
        short: Rc<Cell<bool>>,
        bytes: Vec<u8>,
    }
    impl AsRef<[u8]> for Backing {
        fn as_ref(&self) -> &[u8] {
            if self.short.get() {
                &self.bytes[..80]
            } else {
                &self.bytes
            }
        }
    }
    let flag = Rc::new(Cell::new(false));
    let mut builder = Builder::new(Limits::default());
    builder
        .push(Target::Address(Ipv4Addr::LOCALHOST.into()), b"payload")
        .unwrap();
    let reader = Reader::open(
        Backing {
            short: flag.clone(),
            bytes: builder.into_bytes(b"").unwrap(),
        },
        Limits::default(),
    )
    .unwrap();
    flag.set(true);
    assert!(matches!(
        reader.inspect(),
        Err(Error::Invalid("inspection backing length"))
    ));
}

#[test]
fn interleaved_alias_ids_and_native_window_merge_follow_original_oracle() {
    let mut builder = Builder::new(Limits::default());
    let mut targets = Vec::new();
    for i in 0..17u32 {
        let source = Target::Network {
            address: Ipv4Addr::from(i * 17).into(),
            prefix: 32,
        };
        let payload = vec![i as u8];
        builder.push(source, &payload).unwrap();
        targets.push((2 * i, source, payload.clone()));
        targets.push((
            2 * i + 1,
            Target::Network {
                address: Ipv6Addr::from((1u128 << 112) + u128::from(i * 17)).into(),
                prefix: 128,
            },
            payload,
        ));
    }
    for (id, address) in [(34, Ipv6Addr::UNSPECIFIED), (35, Ipv6Addr::from(u128::MAX))] {
        let target = Target::Address(address.into());
        builder.push(target, b"native").unwrap();
        targets.push((id, target, b"native".to_vec()));
    }
    let base = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();
    let mut bytes = Vec::new();
    base.write_shared_aliases(
        &[SharedAlias {
            address: Ipv6Addr::from(1u128 << 112),
            prefix: 96,
            first_id: 1,
            interleaved: true,
        }],
        &mut bytes,
        Limits::default(),
    )
    .unwrap();
    let reader = Reader::open(bytes, Limits::default()).unwrap();
    let queries = [
        Query::Intersecting(Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        }),
        Query::Intersecting(Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        }),
        Query::Ip(Ipv6Addr::from((1u128 << 112) + 17).into()),
    ];
    verify(&reader, &targets, &queries);
    let mut calls = 0;
    let first = reader
        .try_visit_all(|row| {
            calls += 1;
            Ok::<_, Error>(ControlFlow::Break(row.id))
        })
        .unwrap();
    assert_eq!(first, ControlFlow::Break(0));
    assert_eq!(calls, 1);
    let first = reader
        .try_visit_intersecting(
            Target::Network {
                address: Ipv6Addr::UNSPECIFIED.into(),
                prefix: 0,
            },
            |row| Ok::<_, Error>(ControlFlow::Break(row.id)),
        )
        .unwrap();
    assert_eq!(first, ControlFlow::Break(34));
    let error = reader
        .try_visit_ip(Ipv4Addr::UNSPECIFIED.into(), |_| {
            Err::<ControlFlow<()>, _>(Failure::User(456))
        })
        .unwrap_err();
    assert!(matches!(error, Failure::User(456)));
}

#[test]
fn broad_route_readers_preserve_intersections_and_report_table_capacity() {
    let mut builder = Builder::new(Limits::default());
    for i in 0..4096u32 {
        builder
            .push(
                Target::Network {
                    address: Ipv4Addr::from(i << 20).into(),
                    prefix: 12,
                },
                b"",
            )
            .unwrap();
    }
    let reader =
        Reader::open_with_broad_routes(builder.into_bytes(b"").unwrap(), Limits::default())
            .unwrap();
    let info = reader.inspect().unwrap();
    inspect_totals(info);
    assert!(info.broad_route_heap_bytes >= 4096 * 8);
    assert!(info.reader_struct_bytes > 0);
    let mut hits = Vec::new();
    reader
        .visit_intersecting(
            Target::Network {
                address: Ipv4Addr::new(128, 0, 0, 0).into(),
                prefix: 30,
            },
            |row| {
                hits.push(row.id);
                Ok::<_, Error>(())
            },
        )
        .unwrap();
    assert_eq!(hits, vec![2048]);
    assert!(reader.contains_ip(Ipv4Addr::BROADCAST.into()).unwrap());
    let mut count = 0;
    assert!(
        reader
            .try_visit_all(|_| {
                count += 1;
                Ok::<ControlFlow<()>, Error>(ControlFlow::Continue(()))
            })
            .unwrap()
            .is_continue()
    );
    assert_eq!(count, 4096);
}
