//! Rejection of malformed artifacts and query consistency after accepted mutations.

use std::net::{IpAddr, Ipv4Addr};

use netindex::{Builder, Error, Limits, Reader, Target};

use super::{contains, encode};

#[test]
fn truncated_corrupt_and_unsupported_files_are_rejected_before_lookup() {
    let targets = [
        Target::Address("203.0.113.42".parse().unwrap()),
        Target::Address("203.0.113.43".parse().unwrap()),
    ];
    let bytes = encode(&targets, b"");

    assert_eq!(&bytes[..8], b"NETINDEX");

    let mut old_signature = bytes.clone();
    old_signature[..8].copy_from_slice(b"IPINDEX\0");

    assert!(matches!(
        Reader::open(old_signature, Limits::default()),
        Err(Error::Invalid("index magic"))
    ));

    for end in 0..bytes.len() {
        assert!(
            Reader::open(&bytes[..end], Limits::default()).is_err(),
            "truncation={end}"
        );
    }

    let mut version = bytes.clone();
    version[8..12].copy_from_slice(&2u32.to_le_bytes());

    assert!(matches!(
        Reader::open(version, Limits::default()),
        Err(Error::Version(2))
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
