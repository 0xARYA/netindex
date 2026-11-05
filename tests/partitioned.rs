//! Partitioned builds preserve every byte while restoring first-seen order.

#![cfg(all(test, feature = "external-sort"))]

use std::io::Cursor;

use netindex::{Builder, Error, ExternalBuilder, ExternalOptions, Limits, Reader, Target};

fn target(id: u32) -> Target {
    match id % 4 {
        0 => Target::Asn(id + 1),
        1 => Target::Address(std::net::Ipv4Addr::new(192, 0, 2, 1).into()),
        2 => Target::Network {
            address: std::net::Ipv4Addr::new(192, 0, 2, 0).into(),
            prefix: 24,
        },
        _ => Target::Range {
            start: std::net::Ipv6Addr::UNSPECIFIED.into(),
            end: std::net::Ipv6Addr::new(0xffff, 0, 0, 0, 0, 0, 0, 0).into(),
        },
    }
}

#[test]
fn partitions_restore_exact_bytes_with_disabled_full_and_large_value_caches() {
    let directory = tempfile::tempdir().unwrap();
    for partitions in [1, 4, 16] {
        for payload_cache_bytes in [0, 512, 4096] {
            let mut expected = Builder::new(Limits::default());
            let mut actual = ExternalBuilder::new_partitioned(
                directory.path(),
                Limits::default(),
                ExternalOptions {
                    run_records: 7,
                    payload_cache_bytes,
                    temporary_bytes: 1 << 26,
                },
                partitions,
            )
            .unwrap();
            for id in 0..600u32 {
                let key = id % 137;
                let mut payload = vec![
                    0x5a;
                    if key % 23 == 0 {
                        16_385
                    } else {
                        4 + key as usize % 19
                    }
                ];
                payload[..4].copy_from_slice(&key.to_le_bytes());
                if key == 1 {
                    payload.clear();
                }
                expected.push(target(id), &payload).unwrap();
                actual.push(target(id), &payload).unwrap();
            }
            let expected = expected.into_bytes(b"partitioned").unwrap();
            let mut output = Cursor::new(Vec::new());
            let stats = actual.write_to(&mut output, b"partitioned").unwrap();
            assert_eq!(output.get_ref(), &expected);
            assert!(stats.payload_cache_bytes <= payload_cache_bytes);
            assert_eq!(stats.records, 600);
            let actual = Reader::open(output.into_inner(), Limits::default()).unwrap();
            let expected = Reader::open(expected, Limits::default()).unwrap();
            assert_eq!(
                actual
                    .lookup_ip(std::net::Ipv4Addr::new(192, 0, 2, 1).into())
                    .unwrap(),
                expected
                    .lookup_ip(std::net::Ipv4Addr::new(192, 0, 2, 1).into())
                    .unwrap()
            );
        }
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn empty_partitioned_builds_match_and_invalid_counts_clean_up() {
    let directory = tempfile::tempdir().unwrap();
    for count in [0, 3, 128] {
        assert!(matches!(
            ExternalBuilder::new_partitioned(
                directory.path(),
                Limits::default(),
                ExternalOptions::default(),
                count
            ),
            Err(Error::Invalid("payload partition count"))
        ));
    }
    for count in [1, 64] {
        let builder = ExternalBuilder::new_partitioned(
            directory.path(),
            Limits::default(),
            ExternalOptions::default(),
            count,
        )
        .unwrap();
        let mut output = Cursor::new(Vec::new());
        builder.write_to(&mut output, b"empty").unwrap();
        assert_eq!(
            output.into_inner(),
            Builder::new(Limits::default())
                .into_bytes(b"empty")
                .unwrap()
        );
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn failed_staging_poisoning_and_deferred_limits_prevent_output() {
    let directory = tempfile::tempdir().unwrap();
    let mut builder = ExternalBuilder::new_partitioned(
        directory.path(),
        Limits::default(),
        ExternalOptions {
            temporary_bytes: 19,
            ..Default::default()
        },
        4,
    )
    .unwrap();
    assert!(matches!(
        builder.set_disk_table_cache(512),
        Err(Error::Invalid("partitioned table cache"))
    ));
    assert!(matches!(
        builder.push(Target::Asn(1), b""),
        Err(Error::Limit("temporary bytes"))
    ));
    assert!(matches!(
        builder.push(Target::Asn(1), b""),
        Err(Error::Invalid("failed external builder"))
    ));
    let mut output = Cursor::new(Vec::new());
    assert!(matches!(
        builder.write_to(&mut output, b""),
        Err(Error::Invalid("failed external builder"))
    ));
    assert!(output.get_ref().is_empty());

    let mut builder = ExternalBuilder::new_partitioned(
        directory.path(),
        Limits {
            records: 10,
            bytes: 180,
        },
        ExternalOptions::default(),
        4,
    )
    .unwrap();
    for id in 1..=4u32 {
        builder.push(Target::Asn(id), &[id as u8; 24]).unwrap();
    }
    assert!(matches!(
        builder.write_to(&mut output, b""),
        Err(Error::Limit("artifact bytes"))
    ));
    assert!(output.get_ref().is_empty());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
