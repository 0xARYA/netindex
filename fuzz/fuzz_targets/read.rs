#![no_main]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use libfuzzer_sys::fuzz_target;
use netindex::{CoverageRouter, Error, Limits, Reader, Target};

fuzz_target!(|bytes: &[u8]| {
    let Ok(reader) = Reader::open_with_broad_routes(
        bytes,
        Limits {
            records: 4096,
            bytes: 65_536,
        },
    ) else {
        return;
    };

    let mut assertions = Vec::new();
    reader
        .visit_all(|matched| {
            assertions.push(matched);
            Ok::<_, Error>(())
        })
        .unwrap();

    assert_eq!(assertions.len(), reader.len());

    let coverage = reader.coverage();
    let router = CoverageRouter::new(std::slice::from_ref(&coverage)).unwrap();
    let mut queries = vec![
        IpAddr::from(Ipv4Addr::UNSPECIFIED),
        IpAddr::from(Ipv4Addr::BROADCAST),
        IpAddr::from(Ipv6Addr::UNSPECIFIED),
        IpAddr::from(Ipv6Addr::from(u128::MAX)),
    ];
    let mut asns = Vec::new();
    for assertion in assertions.iter().take(64) {
        match assertion.target {
            Target::Address(address) | Target::Network { address, .. } => queries.push(address),
            Target::Range { start, end } => queries.extend([start, end]),
            Target::Asn(number) => asns.push(number),
        }
    }

    for word in bytes.chunks(16).take(32) {
        let mut value = [0; 16];
        value[..word.len()].copy_from_slice(word);
        let number = u128::from_le_bytes(value);
        queries.extend([
            IpAddr::from(Ipv4Addr::from(number as u32)),
            IpAddr::from(Ipv6Addr::from(number)),
        ]);
        asns.push(number as u32);
    }

    for query in queries {
        let mut actual = Vec::new();
        reader
            .visit_ip(query, |matched| {
                actual.push(matched);
                Ok::<_, Error>(())
            })
            .unwrap();

        let mut expected: Vec<_> = assertions
            .iter()
            .copied()
            .filter(|row| contains(row.target, query))
            .collect();
        actual.sort_unstable_by_key(|row| row.id);
        expected.sort_unstable_by_key(|row| row.id);

        assert_eq!(actual, expected);
        assert!(actual.is_empty() || coverage.may_contain(query));
        assert!(actual.is_empty() || router.candidates(query).any(|candidate| candidate == 0));
    }

    for number in asns {
        let mut actual = Vec::new();
        reader
            .visit_asn(number, |matched| {
                actual.push(matched);
                Ok::<_, Error>(())
            })
            .unwrap();

        let mut expected: Vec<_> = assertions
            .iter()
            .copied()
            .filter(|row| row.target == Target::Asn(number))
            .collect();
        actual.sort_unstable_by_key(|row| row.id);
        expected.sort_unstable_by_key(|row| row.id);

        assert_eq!(actual, expected);
    }
});

fn contains(target: Target, query: IpAddr) -> bool {
    match target {
        Target::Address(address) => address == query,
        Target::Range { start, end } => {
            start.is_ipv4() == query.is_ipv4() && start <= query && query <= end
        }
        Target::Network { address, prefix } => match (address, query) {
            (IpAddr::V4(network), IpAddr::V4(query)) => {
                let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);

                u32::from(network) & mask == u32::from(query) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(query)) => {
                let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);

                u128::from(network) & mask == u128::from(query) & mask
            }
            _ => false,
        },
        Target::Asn(_) => false,
    }
}
