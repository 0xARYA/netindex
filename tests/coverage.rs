//! Conservative routing must never discard an exact match.

#![cfg(test)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use netindex::{Builder, CoverageRouter, Error, Limits, Reader, Target};

#[test]
fn coverage_matches_bucket_boundaries_and_preserves_ranges_and_full_networks() {
    let cases = [
        vec![Target::Range {
            start: Ipv4Addr::from((63 << 20) + 7).into(),
            end: Ipv4Addr::from((129 << 20) + 3).into(),
        }],
        vec![Target::Range {
            start: Ipv6Addr::from((63u128 << 116) + 7).into(),
            end: Ipv6Addr::from((129u128 << 116) + 3).into(),
        }],
        vec![Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        }],
        vec![Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        }],
        vec![
            Target::Address(Ipv4Addr::BROADCAST.into()),
            Target::Address(Ipv6Addr::from(u128::MAX).into()),
        ],
        vec![Target::Asn(64512)],
        vec![],
    ];
    let readers: Vec<_> = cases.iter().map(|targets| reader(targets)).collect();
    let coverage: Vec<_> = readers
        .iter()
        .map(|reader| reader.coverage().unwrap())
        .collect();
    let router = CoverageRouter::new(&coverage).unwrap();

    for bucket in 0..4096u128 {
        for offset in [0, 3, 7, (1u128 << 20) - 1] {
            let address = IpAddr::from(Ipv4Addr::from(((bucket << 20) + offset) as u32));
            let candidates: Vec<_> = router.candidates(address).collect();

            assert_eq!(candidates.contains(&0), (63..=129).contains(&bucket));
            assert!(candidates.contains(&2));
            assert!(!candidates.contains(&1));
            assert!(!candidates.contains(&3));

            check_matches(&readers, &coverage, &candidates, address);
        }

        for offset in [0, 3, 7, (1u128 << 116) - 1] {
            let address = IpAddr::from(Ipv6Addr::from((bucket << 116) + offset));
            let candidates: Vec<_> = router.candidates(address).collect();

            assert_eq!(candidates.contains(&1), (63..=129).contains(&bucket));
            assert!(candidates.contains(&3));
            assert!(!candidates.contains(&0));
            assert!(!candidates.contains(&2));

            check_matches(&readers, &coverage, &candidates, address);
        }
    }
}

#[test]
fn routing_supports_empty_sets_and_the_highest_source_bit() {
    let empty = CoverageRouter::new(&[]).unwrap();
    let query = "203.0.113.42".parse().unwrap();

    assert_eq!(empty.candidates(query).count(), 0);

    let coverage = reader(&[Target::Address(query), Target::Address(query)])
        .coverage()
        .unwrap();
    let router = CoverageRouter::new(&vec![coverage.clone(); 64]).unwrap();

    assert_eq!(
        router.candidates(query).collect::<Vec<_>>(),
        (0..64).collect::<Vec<_>>()
    );

    assert!(matches!(
        CoverageRouter::new(&vec![coverage; 65]),
        Err(Error::Limit("routed readers"))
    ));
}

#[test]
fn fixed_components_produce_conservative_coverage() {
    for bytes in [
        include_bytes!("fixtures/interval.ipidx").as_slice(),
        include_bytes!("fixtures/packed-ipv4.ipidx").as_slice(),
        include_bytes!("fixtures/packed-ipv6.ipidx").as_slice(),
        include_bytes!("fixtures/mixed.ipidx").as_slice(),
    ] {
        let reader = Reader::open(bytes, Limits::default()).unwrap();
        let coverage = reader.coverage().unwrap();

        reader
            .visit_all(|matched| {
                let address = match matched.target {
                    Target::Address(address) | Target::Network { address, .. } => address,
                    Target::Range { start, .. } => start,
                    Target::Asn(_) => return Ok::<_, Error>(()),
                };

                assert!(coverage.may_contain(address));

                Ok(())
            })
            .unwrap();
    }
}

fn reader(targets: &[Target]) -> Reader<Vec<u8>> {
    let mut builder = Builder::new(Limits::default());
    for &target in targets {
        builder.push(target, b"opaque").unwrap();
    }

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"").unwrap();

    Reader::open(bytes, Limits::default()).unwrap()
}

fn check_matches(
    readers: &[Reader<Vec<u8>>],
    coverage: &[netindex::Coverage],
    candidates: &[usize],
    address: IpAddr,
) {
    for (position, reader) in readers.iter().enumerate() {
        reader
            .visit_ip(address, |_| {
                assert!(coverage[position].may_contain(address));
                assert!(candidates.contains(&position));
                Ok::<_, Error>(())
            })
            .unwrap();
    }
}
