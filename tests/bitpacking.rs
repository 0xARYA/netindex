//! Exact-bit columns retain independent decoding, errors and ordinary writer bytes.
use std::{
    io,
    net::{IpAddr, Ipv6Addr},
};

use netindex::{Builder, Error, Limits, Reader, SharedAlias, Target};

#[test]
fn independent_partial_bitpacked_fixture_checks_tail_padding_and_versions() {
    let bytes = include_bytes!("fixtures/bitpacked-ipv4.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    for i in 0..17u32 {
        let matches = reader
            .lookup_ip(std::net::Ipv4Addr::from(i * 17).into())
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, i);
        assert!(matches[0].payload.is_empty());
        assert!(
            reader
                .lookup_ip(std::net::Ipv4Addr::from(i * 17 + 1).into())
                .unwrap()
                .is_empty()
        );
    }
    for (at, value) in [
        (140, 1),
        (139, 128),
        (104, 131),
        (106, 0),
        (105, 255),
        (112, 1),
    ] {
        let mut invalid = bytes.to_vec();
        invalid[at] = value;
        assert!(
            Reader::open(invalid, Limits::default()).is_err(),
            "mutation at {at}"
        );
    }
    let mut legacy = bytes.to_vec();
    legacy[8] = 3;
    assert!(Reader::open(legacy, Limits::default()).is_err());
    for length in 0..bytes.len() {
        assert!(Reader::open(&bytes[..length], Limits::default()).is_err());
    }
    let wide = Reader::open(
        include_bytes!("fixtures/bitpacked-ipv6.nidx").as_slice(),
        Limits::default(),
    )
    .unwrap();
    for id in 0..17u32 {
        let value = (u128::MAX >> 1) / 16 * u128::from(id);
        let rows = wide.lookup_ip(Ipv6Addr::from(value).into()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(
            rows[0].target,
            Target::Address(Ipv6Addr::from(value).into())
        );
        assert!(rows[0].payload.is_empty());
    }
}

#[test]
fn compacted_widths_tails_and_explicit_ids_preserve_every_assertion() {
    for bits in [9, 12, 17, 31, 33, 63, 65, 96, 127, 128] {
        for count in [17, 255, 256] {
            let maximum = u128::MAX >> (128 - bits);
            let mut builder = Builder::new(Limits::default());
            let values: Vec<_> = (0..count)
                .map(|i| {
                    if i == 0 {
                        0
                    } else if i == 1 {
                        1
                    } else if i == count - 1 {
                        maximum
                    } else {
                        maximum / (count - 1) * i
                    }
                })
                .collect();
            for &value in values.iter().rev() {
                builder
                    .push(
                        Target::Address(Ipv6Addr::from(value).into()),
                        &value.to_le_bytes(),
                    )
                    .unwrap();
            }
            let original = builder.into_bytes(b"bitpacked").unwrap();
            let reader = Reader::open(original.as_slice(), Limits::default()).unwrap();
            let mut compact = Vec::new();
            let size = reader.write_bitpacked_to(&mut compact).unwrap();
            assert_eq!(size, compact.len());
            assert!(size <= original.len());
            let result = Reader::open(compact.as_slice(), Limits::default()).unwrap();
            assert_eq!(reader.metadata().unwrap(), result.metadata().unwrap());
            let mut a = Vec::new();
            let mut b = Vec::new();
            reader
                .visit_all(|row| {
                    a.push(row);
                    Ok::<_, Error>(())
                })
                .unwrap();
            result
                .visit_all(|row| {
                    b.push(row);
                    Ok::<_, Error>(())
                })
                .unwrap();
            assert_eq!(a, b);
            for &value in &values {
                for query in [value, value.saturating_sub(1), value.saturating_add(1)] {
                    let query: IpAddr = Ipv6Addr::from(query).into();
                    assert_eq!(
                        reader.lookup_ip(query).unwrap(),
                        result.lookup_ip(query).unwrap()
                    );
                }
            }
            let mut again = Vec::new();
            result.write_bitpacked_to(&mut again).unwrap();
            assert_eq!(again, compact);
        }
    }
}

#[test]
fn aliases_and_output_errors_survive_compaction() {
    let mut builder = Builder::new(Limits::default());
    for i in 0..1041u32 {
        builder
            .push(
                Target::Network {
                    address: std::net::Ipv4Addr::from(i * 17).into(),
                    prefix: 32,
                },
                &i.to_le_bytes(),
            )
            .unwrap();
    }
    let original = builder.into_bytes(b"aliases").unwrap();
    let plain = Reader::open(original, Limits::default()).unwrap();
    let alias = SharedAlias {
        address: Ipv6Addr::from(1u128 << 112),
        prefix: 96,
        first_id: 1041,
        interleaved: false,
    };
    let mut envelope = Vec::new();
    plain
        .write_shared_aliases(&[alias], &mut envelope, Limits::default())
        .unwrap();
    let reader = Reader::open(envelope, Limits::default()).unwrap();
    let mut compact = Vec::new();
    reader.write_bitpacked_to(&mut compact).unwrap();
    let mut compact_plain = Vec::new();
    plain.write_bitpacked_to(&mut compact_plain).unwrap();
    let mut reverse = Vec::new();
    Reader::open(compact_plain, Limits::default())
        .unwrap()
        .write_shared_aliases(&[alias], &mut reverse, Limits::default())
        .unwrap();
    assert_eq!(compact, reverse);
    let result = Reader::open(compact, Limits::default()).unwrap();
    assert_eq!(result.len(), 2082);
    for i in 0..1041u32 {
        for address in [
            IpAddr::from(std::net::Ipv4Addr::from(i * 17)),
            IpAddr::from(Ipv6Addr::from((1u128 << 112) + u128::from(i * 17))),
        ] {
            assert_eq!(
                reader.lookup_ip(address).unwrap(),
                result.lookup_ip(address).unwrap()
            );
        }
    }
    struct Failed;
    impl io::Write for Failed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "bitpack sink"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let error = result.write_bitpacked_to(Failed).unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
#[cfg(feature = "external-sort")]
fn both_builders_and_partitioning_produce_identical_compacted_bytes() {
    use netindex::{ExternalBuilder, ExternalOptions};
    use std::io::Cursor;
    let directory = tempfile::tempdir().unwrap();
    let mut memory = Builder::new(Limits::default());
    let mut disk = ExternalBuilder::new(
        directory.path(),
        Limits::default(),
        ExternalOptions::default(),
    )
    .unwrap();
    let mut partitions = ExternalBuilder::new_partitioned(
        directory.path(),
        Limits::default(),
        ExternalOptions::default(),
        4,
    )
    .unwrap();
    for id in (0..1041u32).rev() {
        let target = Target::Address(std::net::Ipv4Addr::from(id * 17).into());
        let payload = vec![(id % 17) as u8; 4 + (id % 3) as usize];
        memory.push(target, &payload).unwrap();
        disk.push(target, &payload).unwrap();
        partitions.push(target, &payload).unwrap();
    }
    let mut disk_bytes = Cursor::new(Vec::new());
    let mut partition_bytes = Cursor::new(Vec::new());
    disk.write_to(&mut disk_bytes, b"builders").unwrap();
    partitions
        .write_to(&mut partition_bytes, b"builders")
        .unwrap();
    let memory_bytes = memory.into_bytes(b"builders").unwrap();
    let mut expected = Vec::new();
    Reader::open(memory_bytes, Limits::default())
        .unwrap()
        .write_bitpacked_to(&mut expected)
        .unwrap();
    assert_eq!(&expected[8..12], &6u32.to_le_bytes());
    for original in [disk_bytes.into_inner(), partition_bytes.into_inner()] {
        let mut actual = Vec::new();
        Reader::open(original, Limits::default())
            .unwrap()
            .write_bitpacked_to(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
