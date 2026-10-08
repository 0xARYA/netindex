//! Matched MMDB comparisons using the public index and payload codec.

use std::{
    env,
    fs::{self, File},
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use memmap2::{Mmap, MmapOptions};
use netindex::{
    ExternalBuilder, ExternalOptions, Limits, MappedReader, Target, codec::Decoder, mmdb,
};
use serde::Serialize;

use support::{
    schema::{Fields, decode_native},
    typed,
};

mod support;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize, PartialEq)]
struct Response<'a> {
    address: IpAddr,
    prefix: u8,
    fields: Fields<'a>,
}

fn main() -> Result<()> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    for (kind, variable) in [
        ("asn", "NETINDEX_BENCH_ASN"),
        ("city", "NETINDEX_BENCH_CITY"),
    ] {
        let source =
            env::var_os(variable).ok_or_else(|| format!("set {variable} to a cached MMDB path"))?;
        compare(&mut criterion, Path::new(&source), kind)?;
    }

    criterion.final_summary();
    Ok(())
}

#[expect(
    clippy::expect_used,
    reason = "timed failures fail the benchmark process"
)]
#[expect(
    clippy::print_stdout,
    reason = "report artifact sizes and parity counts"
)]
fn compare(criterion: &mut Criterion, source: &Path, kind: &str) -> Result<()> {
    let native = mmdb::Reader::from_source(mapped(source)?)?;
    let output = Path::new("target/mmdb-bench");
    fs::create_dir_all(output)?;
    let run = tempfile::Builder::new().prefix(kind).tempdir_in(output)?;
    let artifact = run.path().join(format!("{kind}.nidx"));

    // A private directory prevents concurrent runs from replacing mapped files.
    let mut builder = ExternalBuilder::new(
        run.path(),
        limits(),
        ExternalOptions {
            payload_cache_bytes: 64 * 1024 * 1024,
            temporary_bytes: 8 * 1024 * 1024 * 1024,
            ..ExternalOptions::default()
        },
    )?;
    let mut encoder = typed::encoder(kind)?;

    mmdb::visit_networks(
        &native,
        mmdb::WithinOptions::default().include_aliased_networks(),
        |target, record| {
            if let Some(fields) = decode_native(&record, kind)? {
                let payload = typed::encode(&mut encoder, &fields)?;
                builder.push(target, &payload)?;

                // Native iteration exposes ::/96 as IPv4. Preserve that query space
                // with an additional IPv6 target, using the public builder.
                if let Target::Network {
                    address: IpAddr::V4(address),
                    prefix,
                } = target
                {
                    builder.push(
                        Target::Network {
                            address: Ipv6Addr::from(u128::from(u32::from(address))).into(),
                            prefix: prefix + 96,
                        },
                        &payload,
                    )?;
                }
            }

            Ok::<_, Box<dyn std::error::Error>>(())
        },
    )?;

    builder.write_to(&mut File::create_new(&artifact)?, &encoder.into_metadata()?)?;

    let index = open(&artifact)?;
    let decoder = Decoder::open(index.metadata()?, limits())?;
    let routed = open_routed(&artifact)?;
    let routed_decoder = Decoder::open(routed.metadata()?, limits())?;
    let workloads = workloads(&native)?;
    let mut checked = 0;

    for (_, queries) in &workloads {
        for &address in queries {
            if native_target(&native, address)? != index_target(&index, address)?
                || index_target(&index, address)? != index_target(&routed, address)?
            {
                return Err(format!("{kind} lookup mismatch at {address}").into());
            }

            let expected = native_response(&native, address, kind)?;
            let actual = index_response(&index, address, kind, &decoder)?;
            if actual != expected
                || index_response(&routed, address, kind, &routed_decoder)? != expected
                || serde_json::to_vec(&actual)? != serde_json::to_vec(&expected)?
                || serde_json::to_vec(&index_response(&routed, address, kind, &routed_decoder)?)?
                    != serde_json::to_vec(&expected)?
            {
                return Err(format!("{kind} response mismatch at {address}").into());
            }

            checked += 1;
        }
    }

    println!(
        "{kind}: MMDB {} bytes; nidx {} bytes; {} assertions; {checked} parity queries",
        fs::metadata(source)?.len(),
        fs::metadata(&artifact)?.len(),
        index.len()
    );

    let mut group = criterion.benchmark_group(format!("mmdb/{kind}"));
    for (name, queries) in workloads {
        if queries.is_empty() {
            return Err(format!("empty {kind} {name} workload").into());
        }

        group.throughput(Throughput::Elements(queries.len() as u64));
        for operation in ["lookup", "fields", "json"] {
            group.bench_function(
                BenchmarkId::new(format!("{name}/{operation}"), "mmdb"),
                |bench| {
                    bench.iter(|| {
                        for &address in &queries {
                            if operation == "lookup" {
                                black_box(
                                    native_target(&native, address).expect("MMDB lookup failed"),
                                );
                            } else {
                                let response = native_response(&native, address, kind)
                                    .expect("MMDB decoding failed");
                                if operation == "json" {
                                    black_box(
                                        serde_json::to_vec(&response)
                                            .expect("MMDB response serialization failed"),
                                    );
                                } else {
                                    black_box(response);
                                }
                            }
                        }
                    });
                },
            );
            for (label, reader, decoder) in [
                ("netindex", &index, &decoder),
                ("netindex-broad", &routed, &routed_decoder),
            ] {
                group.bench_function(
                    BenchmarkId::new(format!("{name}/{operation}"), label),
                    |bench| {
                        bench.iter(|| {
                            for &address in &queries {
                                if operation == "lookup" {
                                    black_box(
                                        index_target(reader, address)
                                            .expect("netindex lookup failed"),
                                    );
                                } else {
                                    let response = index_response(reader, address, kind, decoder)
                                        .expect("netindex decoding failed");
                                    if operation == "json" {
                                        black_box(
                                            serde_json::to_vec(&response)
                                                .expect("netindex response serialization failed"),
                                        );
                                    } else {
                                        black_box(response);
                                    }
                                }
                            }
                        });
                    },
                );
            }
        }
    }
    group.finish();

    let mut group = criterion.benchmark_group(format!("mmdb/{kind}/open"));
    group.bench_function("mmdb-validated", |bench| {
        bench.iter(|| {
            let reader = mmdb::Reader::from_source(mapped(source).expect("MMDB mapping failed"))
                .expect("MMDB opening failed");
            reader.verify().expect("MMDB validation failed");
            black_box(reader);
        })
    });
    for (label, open_reader) in [
        (
            "netindex-validated",
            open as fn(&Path) -> Result<MappedReader>,
        ),
        (
            "netindex-broad-validated",
            open_routed as fn(&Path) -> Result<MappedReader>,
        ),
    ] {
        group.bench_function(label, |bench| {
            bench.iter(|| {
                let reader = open_reader(&artifact).expect("netindex opening failed");
                black_box(
                    Decoder::open(
                        reader.metadata().expect("netindex metadata failed"),
                        limits(),
                    )
                    .expect("codec validation failed"),
                );
                black_box(reader);
            })
        });
    }
    group.finish();

    Ok(())
}

fn native_response<'a>(
    native: &'a mmdb::Reader<Mmap>,
    address: IpAddr,
    kind: &str,
) -> Result<Option<Response<'a>>> {
    let result = native.lookup(address)?;

    let Some(fields) = decode_native(&result, kind)? else {
        return Ok(None);
    };

    let network = result.network()?;

    Ok(Some(Response {
        address: network.ip(),
        prefix: network.prefix(),
        fields,
    }))
}

fn native_target(native: &mmdb::Reader<Mmap>, address: IpAddr) -> Result<Option<Target>> {
    let result = native.lookup(address)?;

    if !result.has_data() {
        return Ok(None);
    }

    let network = result.network()?;

    Ok(Some(Target::Network {
        address: network.ip(),
        prefix: network.prefix(),
    }))
}

fn index_target(index: &MappedReader, address: IpAddr) -> Result<Option<Target>> {
    let mut target = None;
    index.visit_ip(address, |row| {
        if target.replace(row.target).is_some() {
            return Err("multiple MMDB conversion matches".into());
        }

        Ok::<_, Box<dyn std::error::Error>>(())
    })?;

    Ok(target)
}

fn index_response<'a>(
    index: &'a MappedReader,
    address: IpAddr,
    kind: &str,
    decoder: &Decoder<'a>,
) -> Result<Option<Response<'a>>> {
    let mut response = None;
    index.visit_ip(address, |row| {
        if response.is_some() {
            return Err("multiple MMDB conversion matches".into());
        }

        let Target::Network { address, prefix } = row.target else {
            return Err("unexpected converted target".into());
        };

        response = Some(Response {
            address,
            prefix,
            fields: typed::decode(decoder, row.payload, kind)?,
        });

        Ok::<_, Box<dyn std::error::Error>>(())
    })?;

    Ok(response)
}

fn open(path: &Path) -> Result<MappedReader> {
    // SAFETY: Benchmark-owned derived files remain immutable while readers exist.
    Ok(unsafe { MappedReader::map_file(&File::open(path)?, limits()) }?)
}

fn open_routed(path: &Path) -> Result<MappedReader> {
    // SAFETY: The private artifact remains unchanged while any benchmark reader exists.
    Ok(unsafe { MappedReader::map_file_with_broad_routes(&File::open(path)?, limits()) }?)
}

fn mapped(path: &Path) -> Result<Mmap> {
    // SAFETY: Supplied cached inputs must remain unchanged for the benchmark run.
    Ok(unsafe { MmapOptions::new().map(&File::open(path)?) }?)
}

fn limits() -> Limits {
    Limits {
        records: 50_000_000,
        bytes: 3 * 1024 * 1024 * 1024,
    }
}

fn workloads(native: &mmdb::Reader<Mmap>) -> Result<Vec<(&'static str, Vec<IpAddr>)>> {
    let mut hits = [Vec::new(), Vec::new()];
    for (position, record) in native.networks(mmdb::WithinOptions::default())?.enumerate() {
        let record = record?;
        if position % 997 != 0 {
            continue;
        }

        let address = record.network()?.ip();
        let values = hits
            .get_mut(usize::from(address.is_ipv6()))
            .ok_or("invalid address family")?;
        if values.len() < 1024 {
            values.push(address);
        }
    }

    let [hits4, hits6] = hits;
    Ok(vec![
        (
            "aliases",
            ["::8.8.8.8", "::ffff:8.8.8.8", "2002:0808:0808::", "::1"]
                .into_iter()
                .map(str::parse)
                .collect::<std::result::Result<Vec<_>, _>>()?,
        ),
        ("ipv4-hit", hits4),
        ("ipv6-hit", hits6),
        (
            "ipv4-uniform",
            (0..1024)
                .map(|id| Ipv4Addr::from(mix(id) as u32).into())
                .collect(),
        ),
        (
            "ipv6-uniform",
            (0..1024)
                .map(|id| {
                    Ipv6Addr::from((u128::from(mix(id)) << 64) | u128::from(mix(id + 42))).into()
                })
                .collect(),
        ),
    ])
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
