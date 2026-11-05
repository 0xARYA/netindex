//! Independent format-4 fixtures and public writer parity.

use netindex::{Builder, Limits, Reader, Target};

fn fixture() -> Vec<u8> {
    let mut bytes = b"NETINDEX".to_vec();
    bytes.extend(4u32.to_le_bytes());
    bytes.extend(16u32.to_le_bytes());
    for word in [6u64, 0, 0, 6, 4, 4, 0, 0] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(b"meta");
    for id in 0..6u32 {
        bytes.extend((id + 1).to_le_bytes());
        bytes.extend(id.to_le_bytes());
    }
    bytes.extend(3u32.to_le_bytes());
    bytes.extend([1, 0, 0, 0]);
    for (start, length) in [(0u32, 0u32), (0, 1), (1, 3)] {
        bytes.extend(start.to_le_bytes());
        bytes.extend(length.to_le_bytes());
    }
    bytes.extend([0, 1, 1, 0, 2, 1]);
    bytes.extend(b"axyz");
    bytes
}

#[test]
fn independent_fixture_preserves_empty_and_shared_borrowed_values() {
    let bytes = fixture();
    assert_eq!(
        bytes.as_slice(),
        include_bytes!("fixtures/indexed-payloads.nidx")
    );
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    assert_eq!(reader.metadata().unwrap(), b"meta");
    for (id, expected) in [b"".as_slice(), b"a", b"a", b"", b"xyz", b"a"]
        .iter()
        .enumerate()
    {
        let rows = reader.lookup_asn(id as u32 + 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id as u32);
        assert_eq!(rows[0].payload, *expected);
    }
    assert_eq!(
        reader.lookup_asn(2).unwrap()[0].payload.as_ptr(),
        reader.lookup_asn(6).unwrap()[0].payload.as_ptr()
    );
}

#[test]
fn rejects_invalid_version_flags_directory_and_references() {
    let bytes = fixture();
    for (offset, value) in [
        (8, 3),
        (8, 5),
        (12, 0),
        (12, 20),
        (12, 24),
        (12, 32),
        (132, 0),
        (132, 7),
        (136, 0),
        (136, 2),
        (137, 1),
        (140, 1),
        (144, 1),
        (148, 1),
        (156, 0),
        (160, 2),
        (164, 1),
        (164, 3),
        (165, 2),
        (168, 1),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset] = value;
        assert!(
            Reader::open(invalid, Limits::default()).is_err(),
            "offset={offset}, value={value}"
        );
    }
    for end in 0..bytes.len() {
        assert!(
            Reader::open(&bytes[..end], Limits::default()).is_err(),
            "end={end}"
        );
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Reader::open(trailing, Limits::default()).is_err());
}

#[test]
fn zero_width_references_allow_one_shared_value_including_empty() {
    for payload in [b"x".as_slice(), b""] {
        let mut bytes = fixture()[..132].to_vec();
        bytes[56..64].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([0; 4]);
        bytes.extend(0u32.to_le_bytes());
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        let reader = Reader::open(bytes, Limits::default()).unwrap();
        assert_eq!(reader.lookup_asn(6).unwrap()[0].payload, payload);
    }
}

#[test]
fn compact_default_matches_fixture_and_explicit_legacy_remains_readable() {
    let build = || {
        let mut builder = Builder::new(Limits::default());
        for (id, payload) in [b"".as_slice(), b"a", b"a", b"", b"xyz", b"a"]
            .iter()
            .enumerate()
        {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        builder
    };
    assert_eq!(build().into_bytes(b"meta").unwrap(), fixture());
    let legacy = build().into_legacy_bytes(b"meta").unwrap();
    assert_eq!(&legacy[8..12], &3u32.to_le_bytes());
    assert!(fixture().len() < legacy.len());
}

#[test]
fn adaptive_encoding_covers_width_boundaries_and_unshared_fallback() {
    for values in [1usize, 2, 256, 257, 65_536, 65_537] {
        let mut builder = Builder::new(Limits::default());
        for id in 0..values * 4 {
            let value = (id % values) as u32;
            let mut payload = value.to_le_bytes().to_vec();
            payload.resize(4 + (value % 2) as usize, 7);
            builder.push(Target::Asn(id as u32 + 1), &payload).unwrap();
        }
        let bytes = builder.into_bytes(b"").unwrap();
        let reader = Reader::open(bytes, Limits::default()).unwrap();
        for id in [0, values - 1, values, values * 4 - 1] {
            let rows = reader.lookup_asn(id as u32 + 1).unwrap();
            assert_eq!(rows[0].payload[..4], ((id % values) as u32).to_le_bytes());
        }
    }
    for payloads in [
        vec![b"one".as_slice(), b"two"],
        vec![b"one".as_slice(), b"four"],
    ] {
        let mut builder = Builder::new(Limits::default());
        for (id, payload) in payloads.iter().enumerate() {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        let bytes = builder.into_bytes(b"").unwrap();
        assert_eq!(&bytes[8..12], &3u32.to_le_bytes());
    }
}

#[test]
fn directory_overhead_counts_toward_break_even_and_reader_byte_limit() {
    for (records, version) in [(8, 3u32), (9, 4)] {
        let mut builder = Builder::new(Limits::default());
        for id in 0..records {
            let value = id % 6;
            let payload = vec![value as u8; 1 + value % 2];
            builder.push(Target::Asn(id as u32 + 1), &payload).unwrap();
        }
        let bytes = builder.into_bytes(b"").unwrap();
        assert_eq!(&bytes[8..12], &version.to_le_bytes());
        assert!(
            Reader::open(
                bytes.as_slice(),
                Limits {
                    records,
                    bytes: bytes.len() - 1
                }
            )
            .is_err()
        );
        Reader::open(
            bytes.as_slice(),
            Limits {
                records,
                bytes: bytes.len(),
            },
        )
        .unwrap();
    }
}

#[cfg(feature = "external-sort")]
#[test]
fn external_compact_writer_matches_fixture_with_and_without_cache() {
    use netindex::{ExternalBuilder, ExternalOptions};
    use std::io::Cursor;
    for payload_cache_bytes in [0, 65, 4096] {
        let temp = tempfile::tempdir().unwrap();
        let mut builder = ExternalBuilder::new(
            temp.path(),
            Limits::default(),
            ExternalOptions {
                run_records: 2,
                payload_cache_bytes,
                ..Default::default()
            },
        )
        .unwrap();
        for (id, payload) in [b"".as_slice(), b"a", b"a", b"", b"xyz", b"a"]
            .iter()
            .enumerate()
        {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        let mut output = Cursor::new(Vec::new());
        let stats = builder.write_to(&mut output, b"meta").unwrap();
        assert_eq!(stats.artifact_bytes, fixture().len());
        assert_eq!(output.into_inner(), fixture());
        assert_eq!(temp.path().read_dir().unwrap().count(), 0);
    }
}
