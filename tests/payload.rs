//! Fixed-length payload references preserve borrowed bytes and insertion-order identity.

#![cfg(test)]

use netindex::{Builder, Error, Limits, Reader, Target};

#[test]
fn independent_fixed_reference_fixture_rejects_bad_ids_and_descriptors() {
    let bytes = include_bytes!("fixtures/fixed-payloads.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    let expected = [b"aa", b"bb", b"bb", b"aa", b"cc", b"bb"];

    for (id, payload) in expected.iter().enumerate() {
        let rows = reader.lookup_asn(id as u32 + 1).unwrap();
        assert_eq!(rows[0].id, id as u32);
        assert_eq!(rows[0].payload, *payload);
    }

    assert_eq!(
        reader.lookup_asn(2).unwrap()[0].payload.as_ptr(),
        reader.lookup_asn(6).unwrap()[0].payload.as_ptr()
    );

    for (offset, value) in [
        (12, 12),
        (128, 0),
        (132, 2),
        (133, 1),
        (136, 1),
        (137, 3),
        (140, 1),
    ] {
        let mut invalid = bytes.to_vec();
        invalid[offset] = value;
        assert!(
            Reader::open(invalid, Limits::default()).is_err(),
            "offset={offset} value={value}"
        );
    }

    for end in 0..bytes.len() {
        assert!(Reader::open(&bytes[..end], Limits::default()).is_err());
    }
}

#[test]
fn fixed_lengths_cross_reference_width_and_pool_boundaries_without_losing_payloads() {
    for count in [1usize, 2, 3, 8, 9, 16, 17, 256, 257, 1025] {
        let mut builder = Builder::new(Limits::default());
        let mut expected = Vec::new();
        for id in 0..count * 2 {
            let value = (id % count) as u32;
            let payload = value.to_le_bytes();
            builder.push(Target::Asn(id as u32 + 1), &payload).unwrap();
            expected.push(payload);
        }

        let bytes = builder.into_bytes(b"").unwrap();
        let reader = Reader::open(bytes, Limits::default()).unwrap();

        reader
            .visit_all(|row| {
                assert_eq!(row.payload, expected[row.id as usize]);
                Ok::<_, Error>(())
            })
            .unwrap();
    }

    let mut builder = Builder::new(Limits::default());
    for asn in 1..=4 {
        builder.push(Target::Asn(asn), b"").unwrap();
    }

    let reader = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();

    assert_eq!(reader.lookup_asn(4).unwrap()[0].payload, b"");
}

#[test]
fn independent_uniform_kind_fixture_preserves_networks_and_rejects_ambiguous_flags() {
    let bytes = include_bytes!("fixtures/uniform-kind.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();

    for (address, network, id) in [("10.0.0.42", "10.0.0.0", 0), ("10.0.1.99", "10.0.1.0", 1)] {
        let rows = reader.lookup_ip(address.parse().unwrap()).unwrap();
        assert_eq!(
            rows[0].target,
            Target::Network {
                address: network.parse().unwrap(),
                prefix: 24
            }
        );
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].payload, b"x");
    }

    for (offset, value) in [(104, 0x81), (105, 8), (104, 0xc0), (120, 1)] {
        let mut invalid = bytes.to_vec();
        invalid[offset] = value;
        assert!(
            Reader::open(invalid, Limits::default()).is_err(),
            "offset={offset} value={value}"
        );
    }
}

#[test]
fn mixed_kind_blocks_and_unordered_ids_keep_their_original_targets() {
    let targets: Vec<_> = (0..800u32)
        .rev()
        .map(|id| {
            let address = std::net::Ipv4Addr::from(id * 256).into();
            if id < 300 || id % 2 == 0 {
                Target::Network {
                    address,
                    prefix: 24,
                }
            } else {
                Target::Address(address)
            }
        })
        .collect();

    let mut builder = Builder::new(Limits::default());
    for &target in &targets {
        builder.push(target, b"same").unwrap();
    }

    let reader = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();

    reader
        .visit_all(|row| {
            assert_eq!(row.target, targets[row.id as usize]);
            assert_eq!(row.payload, b"same");
            Ok::<_, Error>(())
        })
        .unwrap();
}
