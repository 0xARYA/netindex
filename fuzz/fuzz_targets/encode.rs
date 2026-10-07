#![no_main]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use libfuzzer_sys::fuzz_target;
use netindex::{Builder, Error, Limits, Reader, Target};

fuzz_target!(|input: &[u8]| {
    let mut builder = Builder::new(Limits::default());
    let mut targets = Vec::new();
    for row in input.chunks_exact(20).take(128) {
        let number = u128::from_le_bytes(row[..16].try_into().unwrap());
        let prefix = row[17] % 129;
        let target = match row[16] % 7 {
            0 => Target::Address(Ipv4Addr::from(number as u32).into()),
            1 => Target::Address(Ipv6Addr::from(number).into()),
            2 => {
                let prefix = prefix % 33;
                let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);

                Target::Network {
                    address: Ipv4Addr::from(number as u32 & mask).into(),
                    prefix,
                }
            }
            3 => {
                let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);

                Target::Network {
                    address: Ipv6Addr::from(number & mask).into(),
                    prefix,
                }
            }
            4 => Target::Range {
                start: Ipv4Addr::from(number as u32).into(),
                end: Ipv4Addr::from((number as u32).saturating_add(u32::from(row[18]))).into(),
            },
            5 => Target::Range {
                start: Ipv6Addr::from(number).into(),
                end: Ipv6Addr::from(number.saturating_add(u128::from(row[18]))).into(),
            },
            _ => Target::Asn((number as u32).max(1)),
        };

        builder.push(target, &row[18..]).unwrap();
        targets.push((target, row[18..].to_vec()));
    }

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"fuzz").unwrap();
    let reader = Reader::open(bytes, Limits::default()).unwrap();

    assert_eq!(reader.len(), targets.len());

    reader
        .visit_all(|matched| {
            let (target, payload) = targets.get(matched.id as usize).unwrap();

            assert_eq!(matched.target, *target);
            assert_eq!(matched.payload, payload);

            Ok::<_, Error>(())
        })
        .unwrap();

    for &(target, _) in &targets {
        let queries: Vec<_> = match target {
            Target::Address(address) | Target::Network { address, .. } => vec![address],
            Target::Range { start, end } => vec![start, end],
            Target::Asn(number) => {
                let mut actual = Vec::new();
                reader
                    .visit_asn(number, |row| {
                        actual.push((row.id as usize, row.target, row.payload));
                        Ok::<_, Error>(())
                    })
                    .unwrap();

                let expected: Vec<_> = targets
                    .iter()
                    .enumerate()
                    .filter(|(_, (target, _))| *target == Target::Asn(number))
                    .map(|(id, (target, payload))| (id, *target, payload.as_slice()))
                    .collect();

                assert_eq!(actual, expected);

                Vec::new()
            }
        };

        for query in queries {
            let mut actual = Vec::new();
            reader
                .visit_ip(query, |row| {
                    actual.push((row.id as usize, row.target, row.payload));
                    Ok::<_, Error>(())
                })
                .unwrap();

            actual.sort_unstable_by_key(|row| row.0);
            let expected: Vec<_> = targets
                .iter()
                .enumerate()
                .filter(|(_, (target, _))| contains(*target, query))
                .map(|(id, (target, payload))| (id, *target, payload.as_slice()))
                .collect();

            assert_eq!(actual, expected);
        }
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
