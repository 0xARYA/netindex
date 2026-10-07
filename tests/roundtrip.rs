//! File validation, overlap semantics, and writer/consumer failure contracts.

#![cfg(test)]

use std::{
    io::{self, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use netindex::{Builder, Error, Limits, Reader, Target};

#[test]
fn independent_consecutive_id_fixture_preserves_targets_and_rejects_invalid_bases() {
    let bytes = include_bytes!("fixtures/consecutive-ids.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();

    assert_eq!(reader.len(), 3);
    for (address, id) in [("10.0.0.1", 1), ("10.0.0.2", 2)] {
        let rows = reader.lookup_ip(address.parse().unwrap()).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].target, Target::Address(address.parse().unwrap()));
        assert_eq!(rows[0].payload, b"x");
    }
    assert_eq!(reader.lookup_asn(7).unwrap()[0].id, 0);
    assert!(reader
        .lookup_ip("10.0.0.3".parse().unwrap())
        .unwrap()
        .is_empty());

    for base in [0u32, 2, u32::MAX] {
        let mut invalid = bytes.to_vec();
        invalid[108..112].copy_from_slice(&base.to_le_bytes());

        assert!(Reader::open(invalid, Limits::default()).is_err());
    }
}

#[test]
fn convenience_build_and_lookup_preserve_borrowed_payloads_and_every_match() {
    let targets = [
        Target::Network {
            address: "203.0.113.0".parse().unwrap(),
            prefix: 24,
        },
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Asn(64512),
        Target::Asn(64512),
        Target::Network {
            address: "2001:db8::".parse().unwrap(),
            prefix: 32,
        },
    ];
    let mut builder = Builder::new(Limits::default());
    for target in targets {
        builder.push(target, b"evidence").unwrap();
    }

    let bytes = builder.into_bytes(b"release").unwrap();
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();

    for (address, ids) in [("203.0.113.42", vec![0, 1, 2]), ("2001:db8::1", vec![5])] {
        let matches = reader.lookup_ip(address.parse().unwrap()).unwrap();

        assert_eq!(
            matches.iter().map(|matched| matched.id).collect::<Vec<_>>(),
            ids
        );

        for matched in matches {
            assert_eq!(matched.payload, b"evidence");
            assert!(bytes.as_ptr_range().contains(&matched.payload.as_ptr()));
        }
    }

    let matches = reader.lookup_asn(64512).unwrap();

    assert_eq!(
        matches.iter().map(|matched| matched.id).collect::<Vec<_>>(),
        [3, 4]
    );
    assert!(matches.iter().all(|matched| matched.payload == b"evidence"));

    assert!(reader.lookup_asn(0).unwrap().is_empty());
    assert!(reader.lookup_asn(64513).unwrap().is_empty());
    assert!(reader
        .lookup_ip("192.0.2.1".parse().unwrap())
        .unwrap()
        .is_empty());

    let mut builder = Builder::new(Limits::default());
    for target in targets {
        builder.push(target, b"evidence").unwrap();
    }

    let mut streamed = Vec::new();
    builder.write_to(&mut streamed, b"release").unwrap();

    assert_eq!(bytes, streamed);

    let builder = Builder::new(Limits {
        records: 1,
        bytes: 0,
    });

    assert!(matches!(
        builder.into_bytes(b"release"),
        Err(Error::Limit(_))
    ));
}

#[test]
fn overlaps_duplicates_ranges_and_asns_preserve_original_targets_and_payloads() {
    let targets = vec![
        Target::Network {
            address: "0.0.0.0".parse().unwrap(),
            prefix: 0,
        },
        Target::Range {
            start: "203.0.113.1".parse().unwrap(),
            end: "203.0.113.127".parse().unwrap(),
        },
        Target::Network {
            address: "203.0.113.0".parse().unwrap(),
            prefix: 24,
        },
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Network {
            address: "::".parse().unwrap(),
            prefix: 0,
        },
        Target::Range {
            start: "2001:db8::1".parse().unwrap(),
            end: "2001:db8::ffff".parse().unwrap(),
        },
        Target::Address("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()),
        Target::Asn(64500),
        Target::Asn(64500),
        Target::Asn(u32::MAX),
    ];
    let bytes = encode(&targets, b"source release");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();

    assert_eq!(reader.len(), targets.len());
    assert_eq!(reader.metadata().unwrap(), b"source release");

    for address in [
        "0.0.0.0",
        "203.0.113.0",
        "203.0.113.1",
        "203.0.113.42",
        "203.0.113.127",
        "203.0.113.128",
        "255.255.255.255",
        "::",
        "2001:db8::1",
        "2001:db8::ffff",
        "2001:db8::1:0",
        "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
    ] {
        assert_ip(&reader, &targets, address.parse().unwrap());
    }

    for number in [0, 64500, 64501, u32::MAX] {
        let mut ids = Vec::new();
        reader
            .visit_asn(number, |matched| {
                ids.push(matched.id as usize);
                Ok::<_, Error>(())
            })
            .unwrap();

        let expected: Vec<_> = targets
            .iter()
            .enumerate()
            .filter_map(|(id, target)| (*target == Target::Asn(number)).then_some(id))
            .collect();

        assert_eq!(ids, expected);
    }
}

#[test]
fn interval_search_matches_a_linear_oracle_for_unsorted_generated_ranges() {
    let mut state = 17u64;
    let mut targets = Vec::new();
    for _ in 0..300 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let start = (state >> 32) as u32 % 512;
        let end = start + (state as u32 % 128);
        targets.push(Target::Range {
            start: Ipv4Addr::from(start).into(),
            end: Ipv4Addr::from(end).into(),
        });
        targets.push(Target::Range {
            start: Ipv6Addr::from(u128::from(start)).into(),
            end: Ipv6Addr::from(u128::from(end)).into(),
        });
    }

    let reader = Reader::open(encode(&targets, b""), Limits::default()).unwrap();

    for value in 0..700u32 {
        assert_ip(&reader, &targets, Ipv4Addr::from(value).into());
        assert_ip(&reader, &targets, Ipv6Addr::from(u128::from(value)).into());
    }
}

#[test]
fn empty_components_and_rejected_pushes_do_not_create_phantom_records() {
    let reader = Reader::open(encode(&[], b""), Limits::default()).unwrap();

    assert!(reader.is_empty());
    assert_ip(&reader, &[], "203.0.113.42".parse().unwrap());

    let mut builder = Builder::new(Limits {
        records: 1,
        ..Limits::default()
    });
    for target in [
        Target::Asn(0),
        Target::Network {
            address: "203.0.113.1".parse().unwrap(),
            prefix: 24,
        },
        Target::Network {
            address: "::".parse().unwrap(),
            prefix: 129,
        },
        Target::Range {
            start: "::".parse().unwrap(),
            end: "0.0.0.0".parse().unwrap(),
        },
        Target::Range {
            start: "203.0.113.2".parse().unwrap(),
            end: "203.0.113.1".parse().unwrap(),
        },
    ] {
        assert!(builder.push(target, b"invalid").is_err());
    }

    builder.push(Target::Asn(1), b"accepted").unwrap();
    assert!(matches!(
        builder.push(Target::Asn(2), b""),
        Err(Error::Limit("records"))
    ));

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"metadata").unwrap();
    let reader = Reader::open(bytes, Limits::default()).unwrap();

    assert_eq!(reader.len(), 1);

    reader
        .visit_asn(1, |matched| {
            assert_eq!(matched.payload, b"accepted");
            Ok::<_, Error>(())
        })
        .unwrap();
}

#[test]
fn byte_limits_writer_failures_and_visitor_errors_are_observed() {
    let mut builder = Builder::new(Limits {
        bytes: 80,
        ..Limits::default()
    });

    assert!(matches!(
        builder.push(Target::Asn(1), b""),
        Err(Error::Limit("artifact bytes"))
    ));

    let mut output = Vec::new();

    assert!(matches!(
        builder.write_to(&mut output, b"x"),
        Err(Error::Limit("artifact bytes"))
    ));
    assert!(output.is_empty());

    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Asn(1), b"payload").unwrap();

    assert!(
        matches!(builder.write_to(FailedWriter, b""), Err(Error::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe)
    );

    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Asn(1), b"payload").unwrap();

    assert!(
        matches!(builder.write_to(FailedFlush, b""), Err(Error::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );

    let reader = Reader::open(
        encode(&[Target::Asn(1), Target::Asn(1)], b""),
        Limits::default(),
    )
    .unwrap();
    let mut delivered = 0;
    let error = reader
        .visit_asn(1, |_| {
            delivered += 1;
            Err(Error::Invalid("consumer stopped"))
        })
        .unwrap_err();

    assert!(matches!(error, Error::Invalid("consumer stopped")));
    assert_eq!(delivered, 1);
}

#[test]
fn identical_and_empty_payloads_share_bytes_without_merging_assertions() {
    let mut builder = Builder::new(Limits::default());
    for (number, payload) in [
        (1, &b""[..]),
        (2, &b"shared"[..]),
        (3, &b""[..]),
        (4, &b"shared"[..]),
    ] {
        builder.push(Target::Asn(number), payload).unwrap();
    }

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"").unwrap();

    assert_eq!(bytes.len(), 80 + 4 * 8 + 4 * 8 + 6);

    let reader = Reader::open(bytes, Limits::default()).unwrap();

    let mut payloads = Vec::new();
    reader
        .visit_all(|matched| {
            payloads.push(matched.payload);
            Ok::<_, Error>(())
        })
        .unwrap();

    assert_eq!(payloads, [b"".as_slice(), b"shared", b"", b"shared"]);
    assert_eq!(payloads[1].as_ptr(), payloads[3].as_ptr());
    assert_eq!(reader.len(), 4);

    let expected = [b"first".as_slice(), b"", b"other", b""];
    let mut builder = Builder::new(Limits::default());
    for (id, payload) in expected.iter().enumerate() {
        builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
    }

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
    assert_eq!(payloads, expected);
    assert_eq!(payloads[1].as_ptr(), payloads[3].as_ptr());
}

#[test]
fn packed_blocks_preserve_targets_at_fences_gaps_and_address_extremes() {
    let mut targets = Vec::new();
    for id in (0..777u32).rev() {
        targets.push(Target::Network {
            address: Ipv4Addr::from(id * 512).into(),
            prefix: 24,
        });
        targets.push(Target::Network {
            address: Ipv6Addr::from(u128::from(id) << 80).into(),
            prefix: 88,
        });
    }

    let bytes = encode(&targets, b"");

    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 11);

    let reader = Reader::open(bytes, Limits::default()).unwrap();

    for id in [0, 1, 254, 255, 256, 257, 511, 512, 775, 776, 777] {
        for suffix in [0u32, 255, 256, 511] {
            assert_ip(&reader, &targets, Ipv4Addr::from(id * 512 + suffix).into());
            assert_ip(
                &reader,
                &targets,
                Ipv6Addr::from((u128::from(id) << 80) + (u128::from(suffix) << 40)).into(),
            );
        }
    }

    assert_ip(&reader, &targets, Ipv4Addr::BROADCAST.into());
    assert_ip(&reader, &targets, Ipv6Addr::from(u128::MAX).into());

    for address in [
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    ] {
        let targets = [Target::Network { address, prefix: 0 }];
        let reader = Reader::open(encode(&targets, b""), Limits::default()).unwrap();
        let query = if address.is_ipv6() {
            Ipv6Addr::from(u128::MAX).into()
        } else {
            Ipv4Addr::BROADCAST.into()
        };

        assert_ip(&reader, &targets, query);
    }
}

#[test]
fn packed_wide_deltas_and_partial_blocks_match_linear_queries() {
    let mut state = 23u128;
    let mut targets = Vec::new();
    for _ in 0..513 {
        state = state
            .wrapping_mul(0x2360_ed05_1fc6_5da4_4385_df64_9fcc_f645)
            .wrapping_add(1);
        targets.push(Target::Address(Ipv6Addr::from(state).into()));
    }

    targets.push(Target::Address(Ipv6Addr::UNSPECIFIED.into()));
    targets.push(Target::Address(Ipv6Addr::from(u128::MAX).into()));
    let reader = Reader::open(encode(&targets, b""), Limits::default()).unwrap();

    for target in &targets {
        let Target::Address(address) = *target else {
            continue;
        };

        assert_ip(&reader, &targets, address);

        let IpAddr::V6(address) = address else {
            continue;
        };

        assert_ip(
            &reader,
            &targets,
            Ipv6Addr::from(u128::from(address).wrapping_add(1)).into(),
        );
    }
}

#[test]
fn independently_encoded_interval_fixture_preserves_targets_and_rejects_other_versions() {
    let targets = [
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Address("203.0.113.43".parse().unwrap()),
    ];
    let reader = Reader::open(
        include_bytes!("fixtures/interval.nidx").as_slice(),
        Limits::default(),
    )
    .unwrap();

    assert_eq!(reader.len(), 2);
    assert_ip(&reader, &targets, "203.0.113.42".parse().unwrap());
    assert_ip(&reader, &targets, "203.0.113.43".parse().unwrap());
    assert_ip(&reader, &targets, "203.0.113.44".parse().unwrap());

    let targets: Vec<_> = (0..100u32)
        .map(|id| Target::Address(Ipv4Addr::from(id).into()))
        .collect();
    let mut bytes = encode(&targets, b"");
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());

    assert!(matches!(
        Reader::open(bytes, Limits::default()),
        Err(Error::Version(1))
    ));
}

#[test]
fn independently_encoded_packed_fixture_preserves_ids_and_gaps() {
    let reader = Reader::open(
        include_bytes!("fixtures/packed-ipv4.nidx").as_slice(),
        Limits::default(),
    )
    .unwrap();

    assert_eq!(reader.len(), 40);

    for query in 0..82u32 {
        let mut ids = Vec::new();
        reader
            .visit_ip(Ipv4Addr::from(query).into(), |matched| {
                assert_eq!(
                    matched.target,
                    Target::Address(Ipv4Addr::from(query).into())
                );
                assert_eq!(matched.payload, b"P");
                ids.push(matched.id);
                Ok::<_, Error>(())
            })
            .unwrap();

        let expected: Vec<_> = (query < 80 && query % 2 == 0)
            .then_some(query / 2)
            .into_iter()
            .collect();

        assert_eq!(ids, expected);
    }
}

#[test]
fn fixed_ipv6_and_mixed_fixtures_preserve_family_and_overlap_semantics() {
    let reader = Reader::open(
        include_bytes!("fixtures/packed-ipv6.nidx").as_slice(),
        Limits::default(),
    )
    .unwrap();

    for query in [0, 1, 1u128 << 80, (39u128 << 80) + 1, u128::MAX] {
        let mut ids = Vec::new();
        reader
            .visit_ip(Ipv6Addr::from(query).into(), |matched| {
                assert_eq!(
                    matched.target,
                    Target::Address(Ipv6Addr::from(query).into())
                );
                assert_eq!(matched.payload, b"P");
                ids.push(matched.id);
                Ok::<_, Error>(())
            })
            .unwrap();

        let expected: Vec<_> = (query == 0 || query == 1u128 << 80)
            .then_some((query >> 80) as u32)
            .into_iter()
            .collect();

        assert_eq!(ids, expected);
    }

    let reader = Reader::open(
        include_bytes!("fixtures/mixed.nidx").as_slice(),
        Limits::default(),
    )
    .unwrap();

    for (query, expected) in [
        (IpAddr::from(Ipv4Addr::from(42)), vec![0, 1]),
        (IpAddr::from(Ipv4Addr::from(43)), vec![0]),
        (IpAddr::from(Ipv6Addr::from(u128::MAX)), vec![2, 3]),
        (IpAddr::from(Ipv6Addr::UNSPECIFIED), vec![2]),
    ] {
        let mut ids = Vec::new();
        reader
            .visit_ip(query, |matched| {
                assert_eq!(matched.payload, b"P");
                ids.push(matched.id);
                Ok::<_, Error>(())
            })
            .unwrap();

        assert_eq!(ids, expected);
    }

    let mut ids = Vec::new();
    reader
        .visit_asn(42, |matched| {
            assert_eq!(matched.payload, b"P");
            ids.push(matched.id);
            Ok::<_, Error>(())
        })
        .unwrap();

    assert_eq!(ids, [4]);
}

#[test]
fn immutable_readers_can_be_shared_between_requests_without_locks() {
    let targets = [
        Target::Network {
            address: "203.0.113.0".parse().unwrap(),
            prefix: 24,
        },
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Network {
            address: "2001:db8::".parse().unwrap(),
            prefix: 32,
        },
    ];
    let reader = Reader::open(encode(&targets, b""), Limits::default()).unwrap();

    std::thread::scope(|scope| {
        for query in ["203.0.113.42", "203.0.114.42", "2001:db8::42", "::1"] {
            let reader = &reader;
            let targets = &targets;
            scope.spawn(move || assert_ip(reader, targets, query.parse().unwrap()));
        }
    });
}

#[test]
fn truncated_corrupt_and_unsupported_files_are_rejected_before_lookup() {
    let targets = [
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Address("203.0.113.43".parse().unwrap()),
    ];
    let bytes = encode(&targets, b"");

    for end in 0..bytes.len() {
        assert!(
            Reader::open(&bytes[..end], Limits::default()).is_err(),
            "truncation={end}"
        );
    }

    let mut version = bytes.clone();
    version[8..12].copy_from_slice(&3u32.to_le_bytes());

    assert!(matches!(
        Reader::open(version, Limits::default()),
        Err(Error::Version(3))
    ));

    for offset in [0, 16, 64, 80 + 8, 80 + 16, 80 + 18, 100 + 12, 120] {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;

        assert!(
            Reader::open(changed, Limits::default()).is_err(),
            "corruption={offset}"
        );
    }

    let mut trailing = bytes.clone();
    trailing.push(0);

    assert!(Reader::open(trailing, Limits::default()).is_err());

    assert!(matches!(
        Reader::open(
            bytes,
            Limits {
                records: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("records"))
    ));
}

#[test]
fn accepted_mutations_still_match_their_decoded_targets() {
    let targets: Vec<_> = (0..32u32)
        .map(|value| Target::Range {
            start: Ipv4Addr::from(value).into(),
            end: Ipv4Addr::from(value + 16).into(),
        })
        .collect();
    let bytes = encode(&targets, b"opaque metadata");

    for offset in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[offset] ^= 1 << (offset % 8);

        let Ok(reader) = Reader::open(changed, Limits::default()) else {
            continue;
        };

        let mut decoded = Vec::new();
        reader
            .visit_all(|matched| {
                decoded.push(matched);
                Ok::<_, Error>(())
            })
            .unwrap();

        for query in [0u32, 16, 32, 48, u32::MAX] {
            let address = IpAddr::from(Ipv4Addr::from(query));
            let mut actual = Vec::new();
            reader
                .visit_ip(address, |matched| {
                    actual.push(matched.id);
                    Ok::<_, Error>(())
                })
                .unwrap();

            let mut expected: Vec<_> = decoded
                .iter()
                .filter_map(|matched| contains(matched.target, address).then_some(matched.id))
                .collect();
            actual.sort_unstable();
            expected.sort_unstable();

            assert_eq!(actual, expected, "mutation={offset} query={query}");
        }
    }
}

#[test]
fn packed_corruption_truncation_and_accepted_mutations_remain_safe() {
    let targets: Vec<_> = (0..40u32)
        .map(|id| Target::Address(Ipv4Addr::from(id * 2).into()))
        .collect();
    let bytes = encode(&targets, b"");

    assert_eq!(bytes[12], 9);

    for end in 0..bytes.len() {
        assert!(Reader::open(&bytes[..end], Limits::default()).is_err());
    }

    for offset in [80 + 16, 80 + 24, 80 + 25, 80 + 26, 80 + 28, 112] {
        let mut changed = bytes.clone();
        changed[offset] = 255;

        assert!(
            Reader::open(changed, Limits::default()).is_err(),
            "corruption={offset}"
        );
    }

    for offset in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[offset] ^= 1 << (offset % 8);

        let Ok(reader) = Reader::open(changed, Limits::default()) else {
            continue;
        };

        let mut decoded = Vec::new();
        reader
            .visit_all(|matched| {
                decoded.push(matched);
                Ok::<_, Error>(())
            })
            .unwrap();

        for query in [0u32, 1, 2, 38, 78, 79, 80, u32::MAX] {
            let address = Ipv4Addr::from(query).into();
            let mut actual = Vec::new();
            reader
                .visit_ip(address, |matched| {
                    actual.push(matched.id);
                    Ok::<_, Error>(())
                })
                .unwrap();

            let expected: Vec<_> = decoded
                .iter()
                .filter_map(|matched| contains(matched.target, address).then_some(matched.id))
                .collect();

            assert_eq!(actual, expected, "mutation={offset} query={query}");
        }
    }
}

#[test]
fn repeated_payload_runs_do_not_allow_unvalidated_spans() {
    let mut builder = Builder::new(Limits::default());
    for asn in 1..=4 {
        builder.push(Target::Asn(asn), b"same").unwrap();
    }
    builder.push(Target::Asn(5), b"other").unwrap();

    let bytes = builder.into_bytes(b"").unwrap();
    let reader = Reader::open(bytes.clone(), Limits::default()).unwrap();

    for asn in 1..=4 {
        assert_eq!(reader.lookup_asn(asn).unwrap()[0].payload, b"same");
    }

    // Eight-byte ASN rows precede compact payload slots in this fixture.
    let slots = 80 + 5 * 8;
    let mut invalid = bytes;
    for id in 1..4 {
        invalid[slots + id * 8..slots + id * 8 + 4].copy_from_slice(&1u32.to_le_bytes());
    }

    assert!(Reader::open(invalid, Limits::default()).is_err());
}

fn encode(targets: &[Target], metadata: &[u8]) -> Vec<u8> {
    let mut builder = Builder::new(Limits::default());
    for (id, target) in targets.iter().enumerate() {
        builder.push(*target, &(id as u32).to_le_bytes()).unwrap();
    }

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, metadata).unwrap();

    bytes
}

fn assert_ip<B: AsRef<[u8]>>(reader: &Reader<B>, targets: &[Target], query: IpAddr) {
    let mut actual = Vec::new();
    reader
        .visit_ip(query, |matched| {
            assert_eq!(matched.target, targets[matched.id as usize]);
            assert_eq!(matched.payload, matched.id.to_le_bytes());
            actual.push(matched.id as usize);
            Ok::<_, Error>(())
        })
        .unwrap();

    actual.sort_unstable();
    let expected: Vec<_> = targets
        .iter()
        .enumerate()
        .filter_map(|(id, target)| contains(*target, query).then_some(id))
        .collect();

    assert_eq!(actual, expected, "query={query}");
}

fn contains(target: Target, query: IpAddr) -> bool {
    match target {
        Target::Address(address) => address == query,
        Target::Asn(_) => false,
        Target::Range { start, end } => {
            start.is_ipv6() == query.is_ipv6() && start <= query && query <= end
        }
        Target::Network { address, prefix } => {
            if address.is_ipv6() != query.is_ipv6() {
                return false;
            }

            if prefix == 0 {
                return true;
            }

            match (address, query) {
                (IpAddr::V4(address), IpAddr::V4(query)) => {
                    u32::from(address) >> (32 - prefix) == u32::from(query) >> (32 - prefix)
                }
                (IpAddr::V6(address), IpAddr::V6(query)) => {
                    u128::from(address) >> (128 - prefix) == u128::from(query) >> (128 - prefix)
                }
                _ => false,
            }
        }
    }
}

struct FailedWriter;

impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "test writer failure",
        ))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FailedFlush;

impl Write for FailedFlush {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "test flush failure",
        ))
    }
}
