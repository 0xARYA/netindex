//! Strided IDs preserve insertion identity without explicit per-row ID columns.

use std::net::{Ipv4Addr, Ipv6Addr};

use netindex::{Builder, Error, Limits, Reader, Target};

#[test]
fn independent_stride_fixture_preserves_ids_and_rejects_invalid_flags_and_overflow() {
    let bytes = include_bytes!("fixtures/strided-ids.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    let expected = [
        (
            0,
            Target::Network {
                address: Ipv4Addr::UNSPECIFIED.into(),
                prefix: 24,
            },
        ),
        (
            2,
            Target::Network {
                address: Ipv4Addr::from(256).into(),
                prefix: 24,
            },
        ),
        (1, Target::Address(Ipv6Addr::from(1u128).into())),
    ];
    let mut actual = Vec::new();
    reader
        .visit_all(|row| {
            actual.push((row.id, row.target));
            assert!(row.payload.is_empty());
            Ok::<_, Error>(())
        })
        .unwrap();

    assert_eq!(actual, expected);

    for (address, id) in [("0.0.0.1", 0), ("0.0.1.1", 2), ("::1", 1)] {
        let rows = reader.lookup_ip(address.parse().unwrap()).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
    }

    let mut invalid = bytes.to_vec();
    invalid[104] &= !0x80;

    assert!(matches!(
        Reader::open(invalid, Limits::default()),
        Err(Error::Invalid("packed block fields"))
    ));

    let mut overflow = bytes.to_vec();
    overflow[108..112].copy_from_slice(&(u32::MAX - 1).to_le_bytes());

    assert!(matches!(
        Reader::open(overflow, Limits::default()),
        Err(Error::Invalid("packed record ID"))
    ));

    let mut duplicate = bytes.to_vec();
    duplicate[104] &= !0x20;

    assert!(matches!(
        Reader::open(duplicate, Limits::default()),
        Err(Error::Invalid("duplicate record ID"))
    ));

    let mut builder = Builder::new(Limits::default());
    for (_, target) in [expected[0], expected[2], expected[1]] {
        builder.push(target, b"").unwrap();
    }

    assert_eq!(builder.into_bytes(b"").unwrap(), bytes.as_slice());
}

#[test]
fn interleaved_families_keep_every_id_across_full_and_partial_blocks() {
    let targets = (0..259u32)
        .flat_map(|value| {
            [
                Target::Network {
                    address: Ipv4Addr::from(value * 256).into(),
                    prefix: 24,
                },
                Target::Network {
                    address: Ipv6Addr::from(u128::from(value) << 64).into(),
                    prefix: 64,
                },
            ]
        })
        .collect::<Vec<_>>();
    let mut builder = Builder::new(Limits::default());
    for &target in &targets {
        builder.push(target, b"shared").unwrap();
    }
    let bytes = builder.into_bytes(b"").unwrap();
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    let mut actual = Vec::new();
    reader
        .visit_all(|row| {
            actual.push((row.id as usize, row.target));
            assert_eq!(row.payload, b"shared");
            Ok::<_, Error>(())
        })
        .unwrap();
    actual.sort_unstable_by_key(|row| row.0);

    assert_eq!(
        actual,
        targets.iter().copied().enumerate().collect::<Vec<_>>()
    );

    #[cfg(feature = "external-sort")]
    {
        use std::io::Cursor;

        use netindex::{ExternalBuilder, ExternalOptions};

        let directory = tempfile::tempdir().unwrap();
        let mut external = ExternalBuilder::new(
            directory.path(),
            Limits::default(),
            ExternalOptions {
                run_records: 31,
                ..Default::default()
            },
        )
        .unwrap();
        for target in targets {
            external.push(target, b"shared").unwrap();
        }
        let mut output = Cursor::new(Vec::new());
        external.write_to(&mut output, b"").unwrap();

        assert_eq!(output.into_inner(), bytes);
    }
}
