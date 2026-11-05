//! Shared windows preserve logical IDs, sorted visits, and borrowed values.

#![cfg(test)]

use netindex::{Builder, Error, Limits, Match, Reader, SharedAlias, Target};

fn network(address: &str, prefix: u8) -> Target {
    Target::Network {
        address: address.parse().unwrap(),
        prefix,
    }
}

fn windows() -> [SharedAlias; 2] {
    [
        SharedAlias {
            address: "::".parse().unwrap(),
            prefix: 96,
            first_id: 1,
            interleaved: true,
        },
        SharedAlias {
            address: "2002::".parse().unwrap(),
            prefix: 16,
            first_id: 7,
            interleaved: false,
        },
    ]
}

fn sources() -> [(Target, &'static [u8]); 3] {
    [
        (network("0.0.0.0", 0), b"broad"),
        (network("192.0.2.0", 24), b""),
        (network("192.0.2.0", 24), b"duplicate"),
    ]
}

fn base() -> Vec<u8> {
    let mut builder = Builder::new(Limits::default());
    for (target, payload) in sources() {
        builder.push(target, payload).unwrap();
    }
    builder.push(network("2001:db8::", 32), b"native").unwrap();
    builder.push(Target::Asn(42), b"asn").unwrap();
    builder.into_bytes(b"meta").unwrap()
}

fn envelope(base: &[u8]) -> Vec<u8> {
    // Independent framing with two format-5 descriptors.
    let mut bytes = b"NETINDEX".to_vec();
    bytes.extend(5u32.to_le_bytes());
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(0u128.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend([96, 2, 0, 0]);
    bytes.extend((0x2002u128 << 112).to_le_bytes());
    bytes.extend(7u32.to_le_bytes());
    bytes.extend([16, 1, 0, 0]);
    bytes.extend(base);
    bytes
}

fn all<B: AsRef<[u8]>>(reader: &Reader<B>) -> Vec<Match<'_>> {
    let mut out = Vec::new();
    reader
        .visit_all(|row| {
            out.push(row);
            Ok::<_, Error>(())
        })
        .unwrap();
    out
}

#[test]
fn shared_and_materialized_outputs_match_including_duplicates_overlaps_and_order() {
    let limits = Limits::default();
    let bytes = base();
    let expected = envelope(&bytes);
    let mut output = Vec::new();
    Reader::open(bytes, limits)
        .unwrap()
        .write_shared_aliases(&windows(), &mut output, limits)
        .unwrap();
    assert_eq!(output, expected);
    let reader = Reader::open_with_broad_routes(output, limits).unwrap();

    let mut flat = Builder::new(limits);
    for (source, payload) in sources() {
        flat.push(source, payload).unwrap();
        let Target::Network { address, prefix } = source else {
            panic!("expected a network")
        };
        let address = u32::from(match address {
            std::net::IpAddr::V4(ip) => ip,
            _ => panic!("expected IPv4"),
        });
        flat.push(
            Target::Network {
                address: std::net::Ipv6Addr::from(u128::from(address)).into(),
                prefix: prefix + 96,
            },
            payload,
        )
        .unwrap();
    }
    flat.push(network("2001:db8::", 32), b"native").unwrap();
    for (target, payload) in [
        (network("2002::", 16), b"broad".as_slice()),
        (network("2002:c000:200::", 40), b""),
        (network("2002:c000:200::", 40), b"duplicate"),
    ] {
        flat.push(target, payload).unwrap();
    }
    flat.push(Target::Asn(42), b"asn").unwrap();
    let flat = Reader::open(flat.into_bytes(b"meta").unwrap(), limits).unwrap();
    assert_eq!(reader.len(), 11);
    assert_eq!(reader.metadata().unwrap(), flat.metadata().unwrap());
    assert_eq!(all(&reader), all(&flat));
    assert_eq!(reader.lookup_asn(42).unwrap(), flat.lookup_asn(42).unwrap());

    for ip in [
        "192.0.2.1",
        "::c000:201",
        "2002:c000:201:ffff:ffff:ffff:ffff:ffff",
        "2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "2001:db8::1",
        "2003::",
        "::1:0:0",
    ] {
        let ip = ip.parse().unwrap();
        let found = reader.lookup_ip(ip).unwrap();
        assert_eq!(found, flat.lookup_ip(ip).unwrap());
        assert!(found.is_empty() || reader.coverage().may_contain(ip));
    }
    let v4 = reader.lookup_ip("192.0.2.1".parse().unwrap()).unwrap();
    let v6 = reader
        .lookup_ip("2002:c000:201::1".parse().unwrap())
        .unwrap();
    assert_eq!(v4[2].payload.as_ptr(), v6[2].payload.as_ptr());
    let mut ids = Vec::new();
    let error = reader.visit_ip("2002:c000:201::1".parse().unwrap(), |row| {
        ids.push(row.id);
        Err::<(), _>(Error::Invalid("stop"))
    });
    assert!(matches!(error, Err(Error::Invalid("stop"))));
    assert_eq!(ids, [7]);
}

#[test]
fn malformed_envelopes_and_logical_limits_are_rejected() {
    let bytes = envelope(&base());
    for (offset, value) in [
        (12, 0),
        (12, 65),
        (32, 0),
        (36, 97),
        (37, 0),
        (38, 1),
        (56, 5),
        (56, 9),
        (61, 2),
        (63, 1),
        (72, 5),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(
            Reader::open(bad, Limits::default()).is_err(),
            "offset={offset} value={value}"
        );
    }
    for end in 0..bytes.len() {
        assert!(
            Reader::open(&bytes[..end], Limits::default()).is_err(),
            "end={end}"
        );
    }
    assert!(matches!(
        Reader::open(
            &bytes,
            Limits {
                records: 10,
                bytes: bytes.len()
            }
        ),
        Err(Error::Limit("records"))
    ));
    assert!(matches!(
        Reader::open(
            &bytes,
            Limits {
                records: 11,
                bytes: bytes.len() - 1
            }
        ),
        Err(Error::Limit("artifact bytes"))
    ));
}

#[test]
fn native_intersections_and_incompatible_ipv4_sources_fail_before_writing() {
    let limits = Limits::default();
    let mut builder = Builder::new(limits);
    builder.push(network("0.0.0.0", 0), b"x").unwrap();
    builder.push(network("::", 0), b"broad native").unwrap();
    let reader = Reader::open(builder.into_bytes(b"").unwrap(), limits).unwrap();
    let mut output = Vec::new();
    assert!(matches!(
        reader.write_shared_aliases(&windows()[..1], &mut output, limits),
        Err(Error::Invalid("shared alias overlaps native IPv6"))
    ));
    assert!(output.is_empty());

    let mut builder = Builder::new(limits);
    builder
        .push(Target::Address("192.0.2.1".parse().unwrap()), b"x")
        .unwrap();
    let reader = Reader::open(builder.into_bytes(b"").unwrap(), limits).unwrap();
    assert!(matches!(
        reader.write_shared_aliases(&windows()[..1], &mut output, limits),
        Err(Error::Invalid("shared alias IPv4 order or kind"))
    ));
    assert!(output.is_empty());
}

#[test]
fn independent_format5_fixture_maps_only_its_declared_window() {
    let bytes = include_bytes!("fixtures/shared-alias.nidx");
    let reader = Reader::open(bytes.as_slice(), Limits::default()).unwrap();
    assert_eq!(
        all(&reader),
        [
            Match {
                id: 0,
                target: network("0.0.0.0", 0),
                payload: b"x"
            },
            Match {
                id: 1,
                target: network("2002::", 16),
                payload: b"x"
            },
        ]
    );
    assert_eq!(
        reader
            .lookup_ip("2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap())
            .unwrap()[0]
            .id,
        1
    );
    assert!(
        reader
            .lookup_ip("2003::".parse().unwrap())
            .unwrap()
            .is_empty()
    );
}
