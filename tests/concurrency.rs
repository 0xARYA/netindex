//! Shared readers retain borrowed payloads and assertion identity across workers.

#![cfg(test)]

use std::{net::IpAddr, sync::Arc, thread};

use netindex::{Error, Match, Reader, Target};

#[test]
fn owned_and_borrowed_readers_support_concurrent_ip_and_asn_visitors() {
    assert_send_sync::<Reader<Vec<u8>>>();
    assert_send_sync::<Reader<&[u8]>>();

    for bytes in [
        include_bytes!("fixtures/mixed.nidx").as_slice(),
        include_bytes!("fixtures/packed-ipv4.nidx").as_slice(),
        include_bytes!("fixtures/packed-ipv6.nidx").as_slice(),
        include_bytes!("fixtures/interval.nidx").as_slice(),
    ] {
        let owned = Arc::new(Reader::open(bytes.to_vec(), Default::default()).unwrap());
        let borrowed = Reader::open(bytes, Default::default()).unwrap();
        let addresses = queries(&borrowed);
        let expected = collect(&borrowed, &addresses).unwrap();

        thread::scope(|scope| {
            let mut workers = Vec::new();
            for _ in 0..4 {
                let owned = Arc::clone(&owned);
                let addresses = &addresses;
                let borrowed = &borrowed;
                let expected = &expected;

                workers.push(scope.spawn(move || {
                    let owned_matches = collect(&owned, addresses)?;
                    let borrowed_matches = collect(borrowed, addresses)?;

                    assert_eq!(&owned_matches, expected);
                    assert_eq!(&borrowed_matches, expected);

                    Ok::<_, Error>(())
                }));
            }

            for worker in workers {
                worker.join().unwrap().unwrap();
            }
        });
    }
}

fn assert_send_sync<T: Send + Sync>() {}

fn queries(reader: &Reader<&[u8]>) -> Vec<IpAddr> {
    let mut addresses = vec!["0.0.0.0".parse().unwrap(), "::".parse().unwrap()];

    reader
        .visit_all(|matched| {
            match matched.target {
                Target::Address(address) | Target::Network { address, .. } => {
                    addresses.push(address)
                }
                Target::Range { start, end } => addresses.extend([start, end]),
                Target::Asn(_) => {}
            }

            Ok::<_, Error>(())
        })
        .unwrap();

    addresses
}

fn collect<'a, B: AsRef<[u8]>>(
    reader: &'a Reader<B>,
    addresses: &[IpAddr],
) -> Result<Vec<Match<'a>>, Error> {
    let mut matches = Vec::new();
    for &address in addresses {
        reader.visit_ip(address, |matched| {
            matches.push(matched);
            Ok::<_, Error>(())
        })?;
    }

    for number in [0, 64500, u32::MAX] {
        reader.visit_asn(number, |matched| {
            matches.push(matched);
            Ok::<_, Error>(())
        })?;
    }

    Ok(matches)
}
