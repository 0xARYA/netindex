//! Independent format-4 fixtures and public writer parity.

use netindex::{Builder, Error, Limits, PayloadReferences, Reader, Target};

const INDEXED_FIXTURE: &[u8] = include_bytes!("fixtures/indexed-payloads.nidx");
const PAYLOADS_BY_ASN: [&[u8]; 6] = [b"", b"a", b"a", b"", b"xyz", b"a"];
const DIRECTORY_START: usize = 132;

#[test]
fn independent_fixture_preserves_empty_and_shared_borrowed_values() {
    let reader = Reader::open(INDEXED_FIXTURE, Limits::default()).unwrap();

    assert_fixture_records(&reader).unwrap();
    assert_eq!(
        reader.lookup_asn(2).unwrap()[0].payload.as_ptr(),
        reader.lookup_asn(6).unwrap()[0].payload.as_ptr()
    );
}

#[test]
fn rejects_invalid_version_flags_directory_and_references() {
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
        let mut invalid = INDEXED_FIXTURE.to_vec();
        invalid[offset] = value;
        assert!(
            Reader::open(invalid, Limits::default()).is_err(),
            "offset={offset}, value={value}"
        );
    }
    for end in 0..INDEXED_FIXTURE.len() {
        assert!(
            Reader::open(&INDEXED_FIXTURE[..end], Limits::default()).is_err(),
            "end={end}"
        );
    }
    let mut trailing = INDEXED_FIXTURE.to_vec();
    trailing.push(0);
    assert!(Reader::open(trailing, Limits::default()).is_err());
}

#[test]
fn zero_width_references_allow_one_shared_value_including_empty() {
    for payload in [b"x".as_slice(), b""] {
        let mut bytes = INDEXED_FIXTURE[..DIRECTORY_START].to_vec();
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
        for (id, payload) in PAYLOADS_BY_ASN.into_iter().enumerate() {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        builder
    };
    let compact = build().into_bytes(b"meta").unwrap();
    let legacy = build().into_legacy_bytes(b"meta").unwrap();
    let legacy = Reader::open(legacy, Limits::default()).unwrap();

    assert_eq!(compact, INDEXED_FIXTURE);
    let inspection = legacy.inspect().unwrap();
    assert_eq!(inspection.format, 3);
    assert!(compact.len() < inspection.artifact_bytes);
    assert_fixture_records(&legacy).unwrap();
}

#[test]
fn adaptive_encoding_selects_reference_width_at_ordinal_boundaries() {
    for (values, references) in [
        (
            1,
            PayloadReferences::Fixed {
                length: 4,
                width: 0,
            },
        ),
        (
            2,
            PayloadReferences::Indexed {
                values: 2,
                width: 1,
            },
        ),
        (
            256,
            PayloadReferences::Indexed {
                values: 256,
                width: 1,
            },
        ),
        (
            257,
            PayloadReferences::Indexed {
                values: 257,
                width: 2,
            },
        ),
        (
            65_536,
            PayloadReferences::Indexed {
                values: 65_536,
                width: 2,
            },
        ),
        (
            65_537,
            PayloadReferences::Indexed {
                values: 65_537,
                width: 3,
            },
        ),
    ] {
        let mut builder = Builder::new(Limits::default());
        for id in 0..values * 4 {
            let value = (id % values) as u32;
            let mut payload = value.to_le_bytes().to_vec();
            payload.resize(4 + (value % 2) as usize, 7);
            builder.push(Target::Asn(id as u32 + 1), &payload).unwrap();
        }
        let bytes = builder.into_bytes(b"").unwrap();
        let reader = Reader::open(bytes, Limits::default()).unwrap();

        assert_eq!(reader.inspect().unwrap().references, references);
        for id in [0, values - 1, values, values * 4 - 1] {
            let rows = reader.lookup_asn(id as u32 + 1).unwrap();
            assert_eq!(rows[0].payload[..4], ((id % values) as u32).to_le_bytes());
        }
    }
}

#[test]
fn unshared_payloads_keep_the_legacy_format() {
    for payloads in [[b"one".as_slice(), b"two"], [b"one".as_slice(), b"four"]] {
        let mut builder = Builder::new(Limits::default());
        for (id, payload) in payloads.iter().enumerate() {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        let bytes = builder.into_bytes(b"").unwrap();
        let reader = Reader::open(bytes, Limits::default()).unwrap();

        assert_eq!(reader.inspect().unwrap().format, 3);
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
        assert!(matches!(
            Reader::open(
                bytes.as_slice(),
                Limits {
                    records,
                    bytes: bytes.len() - 1
                }
            ),
            Err(Error::Limit("artifact bytes"))
        ));
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
    use std::io::Cursor;

    use netindex::{ExternalBuilder, ExternalOptions};

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
        for (id, payload) in PAYLOADS_BY_ASN.into_iter().enumerate() {
            builder.push(Target::Asn(id as u32 + 1), payload).unwrap();
        }
        let mut output = Cursor::new(Vec::new());
        let stats = builder.write_to(&mut output, b"meta").unwrap();
        assert_eq!(stats.artifact_bytes, INDEXED_FIXTURE.len());
        assert_eq!(output.into_inner(), INDEXED_FIXTURE);
        assert_eq!(temp.path().read_dir().unwrap().count(), 0);
    }
}

fn assert_fixture_records(reader: &Reader<impl AsRef<[u8]>>) -> Result<(), Error> {
    assert_eq!(reader.metadata()?, b"meta");
    assert_eq!(reader.len(), PAYLOADS_BY_ASN.len());

    for (id, payload) in PAYLOADS_BY_ASN.into_iter().enumerate() {
        let asn = id as u32 + 1;
        let rows = reader.lookup_asn(asn)?;

        assert_eq!(rows.len(), 1, "ASN {asn}");
        assert_eq!(
            rows.first().map(|row| (row.id, row.target, row.payload)),
            Some((id as u32, Target::Asn(asn), payload))
        );
    }

    Ok(())
}
