//! Optional broad routes preserve assertion identity, payloads, and fallback behavior.

use netindex::{Builder, Limits, Reader, Target};

#[test]
fn optional_routes_preserve_original_ids_targets_and_borrowed_payloads() {
    let limits = Limits::default();
    let targets = [
        ("12.0.0.0", 24, b"narrow".as_slice()),
        ("10.0.0.0", 8, b"first".as_slice()),
        ("11.0.0.0", 8, b"second".as_slice()),
        ("2200::", 64, b"narrow-six".as_slice()),
        ("2000::", 8, b"first-six".as_slice()),
        ("2100::", 8, b"second-six".as_slice()),
    ];
    let mut builder = Builder::new(limits);

    for (address, prefix, payload) in targets {
        builder
            .push(
                Target::Network {
                    address: address.parse().unwrap(),
                    prefix,
                },
                payload,
            )
            .unwrap();
    }

    let bytes = builder.into_bytes(b"").unwrap();
    let baseline = Reader::open(bytes.as_slice(), limits).unwrap();
    let routed = Reader::open_with_broad_routes(bytes.as_slice(), limits).unwrap();

    for (query, id) in [
        ("10.0.0.0", 1),
        ("10.255.255.255", 1),
        ("11.127.0.1", 2),
        ("12.0.0.42", 0),
        ("2000::", 4),
        ("20ff:ffff:ffff:ffff:ffff:ffff:ffff:ffff", 4),
        ("21ab::42", 5),
        ("2200::42", 3),
    ] {
        let address = query.parse().unwrap();
        let matches = routed.lookup_ip(address).unwrap();
        let (network, prefix, payload) = targets[id];

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, id as u32);
        assert_eq!(
            matches[0].target,
            Target::Network {
                address: network.parse().unwrap(),
                prefix,
            }
        );
        assert_eq!(matches[0].payload, payload);
        assert_eq!(matches, baseline.lookup_ip(address).unwrap());
    }

    for query in ["9.255.255.255", "12.0.1.0", "2200:0:0:1::"] {
        assert!(routed.lookup_ip(query.parse().unwrap()).unwrap().is_empty());
    }

    let mut calls = 0;
    let result = routed.visit_ip("10.1.2.3".parse().unwrap(), |_| {
        calls += 1;

        Err::<(), _>(netindex::Error::Limit("consumer stopped"))
    });

    assert_eq!(calls, 1);
    assert!(matches!(
        result,
        Err(netindex::Error::Limit("consumer stopped"))
    ));
}

#[test]
fn overlapping_sections_keep_every_assertion_when_routes_are_requested() {
    let limits = Limits::default();
    let mut builder = Builder::new(limits);

    for (address, prefix, payload) in [
        ("10.0.0.0", 8, b"broad".as_slice()),
        ("10.0.0.0", 16, b"nested".as_slice()),
        ("10.0.0.0", 8, b"duplicate".as_slice()),
    ] {
        builder
            .push(
                Target::Network {
                    address: address.parse().unwrap(),
                    prefix,
                },
                payload,
            )
            .unwrap();
    }

    let reader = Reader::open_with_broad_routes(builder.into_bytes(b"").unwrap(), limits).unwrap();
    let matches = reader.lookup_ip("10.0.1.2".parse().unwrap()).unwrap();
    let mut actual: Vec<_> = matches.iter().map(|row| (row.id, row.payload)).collect();
    actual.sort_unstable_by_key(|&(id, _)| id);

    assert_eq!(
        actual,
        [
            (0, b"broad".as_slice()),
            (1, b"nested".as_slice()),
            (2, b"duplicate".as_slice()),
        ]
    );
}
