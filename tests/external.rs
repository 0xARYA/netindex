//! External builds preserve the current bytes and assertion identity under spilling.

#![cfg(all(test, feature = "external-sort"))]

use std::{
    io::{self, Cursor, Seek, SeekFrom, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use netindex::{Builder, Error, ExternalBuilder, ExternalOptions, Limits, Reader, Target};

#[test]
fn spilled_unsorted_runs_match_the_in_memory_writer_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    let limits = Limits::default();
    let scenarios = [
        (0..1031u32)
            .rev()
            .map(|id| Target::Network {
                address: Ipv4Addr::from(id * 256).into(),
                prefix: 24,
            })
            .collect::<Vec<_>>(),
        (0..1031u128)
            .rev()
            .map(|id| Target::Network {
                address: Ipv6Addr::from(id << 64).into(),
                prefix: 64,
            })
            .collect(),
        (0..1031u32)
            .rev()
            .flat_map(|id| {
                [
                    Target::Range {
                        start: Ipv4Addr::from(id * 7).into(),
                        end: Ipv4Addr::from(id * 7 + 999).into(),
                    },
                    Target::Address(Ipv6Addr::from(u128::MAX - u128::from(id)).into()),
                    Target::Asn(id + 1),
                    Target::Asn(id + 1),
                ]
            })
            .collect(),
        Vec::new(),
    ];

    for targets in scenarios {
        for run_records in [1, 7, 256, 2048] {
            let mut expected = Builder::new(limits);
            let mut actual = ExternalBuilder::new(
                directory.path(),
                limits,
                ExternalOptions {
                    run_records,
                    ..Default::default()
                },
            )
            .unwrap();
            for (id, &target) in targets.iter().enumerate() {
                let payload = (id % 17).to_le_bytes();
                expected.push(target, &payload).unwrap();
                actual.push(target, &payload).unwrap();
            }

            let mut expected_bytes = Vec::new();
            expected
                .write_to(&mut expected_bytes, b"same metadata")
                .unwrap();
            let mut actual_bytes = Cursor::new(Vec::new());
            actual
                .write_to(&mut actual_bytes, b"same metadata")
                .unwrap();

            assert_eq!(actual_bytes.get_ref(), &expected_bytes);

            let reader = Reader::open(actual_bytes.into_inner(), limits).unwrap();

            assert_eq!(reader.len(), targets.len());

            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }
}

#[test]
fn a_full_or_disabled_payload_cache_preserves_every_assertion() {
    let directory = tempfile::tempdir().unwrap();

    for payload_cache_bytes in [0, 64, 65, 4096] {
        let mut builder = ExternalBuilder::new(
            directory.path(),
            Limits::default(),
            ExternalOptions {
                run_records: 1,
                payload_cache_bytes,
                ..Default::default()
            },
        )
        .unwrap();
        let address: IpAddr = "203.0.113.42".parse().unwrap();
        for payload in [b"a".as_slice(), b"b", b"a", b"", b"", b"b"] {
            builder.push(Target::Address(address), payload).unwrap();
        }

        let mut bytes = Cursor::new(Vec::new());
        builder.write_to(&mut bytes, b"").unwrap();
        let reader = Reader::open(bytes.into_inner(), Limits::default()).unwrap();

        let mut matches = Vec::new();
        reader
            .visit_ip(address, |matched| {
                matches.push((matched.id, matched.payload));
                Ok::<_, Error>(())
            })
            .unwrap();

        assert_eq!(
            matches,
            [
                (0, b"a".as_slice()),
                (1, b"b"),
                (2, b"a"),
                (3, b""),
                (4, b""),
                (5, b"b")
            ]
        );
    }

    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn limits_invalid_inputs_and_output_failures_remove_temporary_files() {
    let directory = tempfile::tempdir().unwrap();

    assert!(matches!(
        ExternalBuilder::new(
            directory.path(),
            Limits::default(),
            ExternalOptions {
                run_records: 0,
                ..Default::default()
            }
        ),
        Err(Error::Invalid("external run size"))
    ));

    let mut builder = ExternalBuilder::new(
        directory.path(),
        Limits::default(),
        ExternalOptions {
            temporary_bytes: 0,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(matches!(
        builder.push(Target::Asn(1), b""),
        Err(Error::Limit("temporary bytes"))
    ));
    assert!(matches!(
        builder.push(Target::Asn(1), b""),
        Err(Error::Invalid("failed external builder"))
    ));
    assert!(matches!(
        builder.write_to(Cursor::new(Vec::new()), b""),
        Err(Error::Invalid("failed external builder"))
    ));

    let mut builder = ExternalBuilder::new(
        directory.path(),
        Limits::default(),
        ExternalOptions {
            run_records: 1,
            temporary_bytes: 150,
            ..Default::default()
        },
    )
    .unwrap();
    builder.push(Target::Asn(1), b"").unwrap();
    builder.push(Target::Asn(2), b"").unwrap();

    assert!(matches!(
        builder.push(Target::Asn(3), b""),
        Err(Error::Limit("temporary bytes"))
    ));

    drop(builder);

    let mut builder = ExternalBuilder::new(
        directory.path(),
        Limits {
            records: 1,
            bytes: 128,
        },
        Default::default(),
    )
    .unwrap();
    builder.push(Target::Asn(1), b"").unwrap();

    assert!(matches!(
        builder.push(Target::Asn(2), b""),
        Err(Error::Limit("records"))
    ));

    drop(builder);

    let builder = ExternalBuilder::new(
        directory.path(),
        Limits {
            records: 0,
            bytes: 80,
        },
        Default::default(),
    )
    .unwrap();
    let mut output = Cursor::new(Vec::new());

    assert!(matches!(
        builder.write_to(&mut output, b"too large"),
        Err(Error::Limit("artifact bytes"))
    ));
    assert!(output.get_ref().is_empty());

    let builder =
        ExternalBuilder::new(directory.path(), Limits::default(), Default::default()).unwrap();

    assert!(matches!(
        builder.write_to(FailOnWrite, b""),
        Err(Error::Io(_))
    ));

    let builder =
        ExternalBuilder::new(directory.path(), Limits::default(), Default::default()).unwrap();
    let mut output = Cursor::new(b"existing".to_vec());

    assert!(matches!(
        builder.write_to(&mut output, b""),
        Err(Error::Invalid("nonempty external output"))
    ));
    assert_eq!(output.into_inner(), b"existing");

    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn invalid_targets_poison_the_external_builder_without_publication() {
    let directory = tempfile::tempdir().unwrap();

    for target in [
        Target::Asn(0),
        Target::Network {
            address: "203.0.113.42".parse().unwrap(),
            prefix: 24,
        },
        Target::Network {
            address: "0.0.0.0".parse().unwrap(),
            prefix: 33,
        },
        Target::Range {
            start: "203.0.113.42".parse().unwrap(),
            end: "203.0.113.1".parse().unwrap(),
        },
        Target::Range {
            start: "0.0.0.0".parse().unwrap(),
            end: "::".parse().unwrap(),
        },
    ] {
        let mut builder =
            ExternalBuilder::new(directory.path(), Limits::default(), Default::default()).unwrap();

        assert!(matches!(builder.push(target, b""), Err(Error::Invalid(_))));

        let mut output = Cursor::new(Vec::new());
        assert!(matches!(
            builder.write_to(&mut output, b""),
            Err(Error::Invalid("failed external builder"))
        ));
        assert!(output.get_ref().is_empty());

        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

struct FailOnWrite;

impl Write for FailOnWrite {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::WriteZero))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for FailOnWrite {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Ok(0)
    }
}
