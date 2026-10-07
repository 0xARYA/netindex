//! MMDB conversion preserves projected payloads, aliases, and caller failures.

#![cfg(all(test, feature = "mmdb"))]

#[cfg(feature = "external-sort")]
use std::io::Cursor;
use std::{error::Error as StdError, net::IpAddr};

use netindex::{mmdb, Builder, Limits, Reader, Target};
use serde::Deserialize;

const SOURCE: &[u8] = include_bytes!("fixtures/anonymous.mmdb");

#[derive(Debug, Deserialize)]
struct Proxy {
    #[serde(default)]
    is_public_proxy: bool,
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("native record failed")]
    Native(#[from] mmdb::MaxMindDbError),
    #[error("consumer stopped")]
    Consumer,
}

#[test]
fn converted_records_match_native_presence_flags_endpoints_and_aliases() {
    let source = mmdb::Reader::from_source(SOURCE).unwrap();
    let mut builder = Builder::new(Limits::default());
    let mut queries: Vec<IpAddr> = [
        "6.1.0.3",
        "::ffff:6.1.0.3",
        "2002:0601:0003::1",
        "2001:480:3a::1",
        "abcd:1000::1",
        "203.0.113.42",
        "2001:db8::1",
        "0.0.0.0",
        "255.255.255.255",
        "::",
        "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
    ]
    .into_iter()
    .map(|address| address.parse().unwrap())
    .collect();

    mmdb::visit_networks(&source, alias_options(), |target, record| {
        let network = record.network()?;
        queries.extend([network.ip(), network.broadcast()]);

        if let Some(proxy) = record.decode::<Proxy>()? {
            builder.push(target, &[u8::from(proxy.is_public_proxy)])?;
        }

        Ok::<_, Box<dyn StdError>>(())
    })
    .unwrap();

    let converted = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();

    for address in queries {
        assert_native(&source, &converted, address).unwrap();
    }
}

#[test]
fn callbacks_can_skip_records_and_native_options_control_aliases() {
    let source = mmdb::Reader::from_source(SOURCE).unwrap();
    let mut builder = Builder::new(Limits::default());

    mmdb::visit_networks(&source, mmdb::WithinOptions::default(), |target, record| {
        if record
            .decode::<Proxy>()?
            .is_some_and(|row| row.is_public_proxy)
        {
            builder.push(target, b"proxy")?;
        }

        Ok::<_, Box<dyn StdError>>(())
    })
    .unwrap();

    let converted = Reader::open(builder.into_bytes(b"").unwrap(), Limits::default()).unwrap();

    assert_eq!(
        converted
            .lookup_ip("6.1.0.3".parse().unwrap())
            .unwrap()
            .len(),
        1
    );
    assert!(converted
        .lookup_ip("6.1.0.2".parse().unwrap())
        .unwrap()
        .is_empty());
    assert!(converted
        .lookup_ip("::ffff:6.1.0.3".parse().unwrap())
        .unwrap()
        .is_empty());
}

#[test]
fn invalid_native_structure_fails_before_callbacks_and_consumer_errors_stop_delivery() {
    let mut corrupt = SOURCE.to_vec();
    corrupt[..6].fill(0xff);
    let source = mmdb::Reader::from_source(corrupt).unwrap();
    let mut delivered = 0;

    let result = mmdb::visit_networks(&source, alias_options(), |_, _| {
        delivered += 1;
        Ok::<_, Failure>(())
    });

    assert!(matches!(result, Err(Failure::Native(_))));
    assert_eq!(delivered, 0);

    let source = mmdb::Reader::from_source(SOURCE).unwrap();

    let result = mmdb::visit_networks(&source, alias_options(), |_, _| {
        delivered += 1;
        Err(Failure::Consumer)
    });

    assert!(matches!(result, Err(Failure::Consumer)));
    assert_eq!(delivered, 1);
}

#[cfg(feature = "external-sort")]
#[test]
fn native_iteration_feeds_spilled_external_builds_without_collecting_records() {
    use netindex::{ExternalBuilder, ExternalOptions};

    let directory = tempfile::tempdir().unwrap();
    let source = mmdb::Reader::from_source(SOURCE).unwrap();
    let mut builder = ExternalBuilder::new(
        directory.path(),
        Limits::default(),
        ExternalOptions {
            run_records: 2,
            ..ExternalOptions::default()
        },
    )
    .unwrap();

    mmdb::visit_networks(&source, alias_options(), |target, record| {
        if let Some(proxy) = record.decode::<Proxy>()? {
            builder.push(target, &[u8::from(proxy.is_public_proxy)])?;
        }

        Ok::<_, Box<dyn StdError>>(())
    })
    .unwrap();

    let mut output = Cursor::new(Vec::new());
    builder.write_to(&mut output, b"").unwrap();
    let converted = Reader::open(output.into_inner(), Limits::default()).unwrap();

    for query in [
        "6.1.0.3",
        "::ffff:6.1.0.3",
        "2002:0601:0003::1",
        "203.0.113.42",
    ] {
        assert_native(&source, &converted, query.parse().unwrap()).unwrap();
    }
}

fn alias_options() -> mmdb::WithinOptions {
    mmdb::WithinOptions::default().include_aliased_networks()
}

fn assert_native<B: AsRef<[u8]>>(
    source: &mmdb::Reader<&[u8]>,
    converted: &Reader<B>,
    address: IpAddr,
) -> Result<(), Box<dyn StdError>> {
    let native = source.lookup(address)?;
    let expected = native.decode::<Proxy>()?;
    let matches = converted.lookup_ip(address)?;

    match expected {
        Some(proxy) => {
            assert_eq!(matches.len(), 1, "{address}");

            let matched = matches.first().ok_or("missing converted record")?;

            assert_eq!(
                matched.payload,
                &[u8::from(proxy.is_public_proxy)],
                "{address}"
            );

            let network = native.network()?;

            assert_eq!(
                matched.target,
                Target::Network {
                    address: network.ip(),
                    prefix: network.prefix(),
                },
                "{address}"
            );
        }
        None => assert!(matches.is_empty(), "{address}"),
    }

    Ok(())
}
