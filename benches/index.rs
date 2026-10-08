//! Warm lookup, lookup plus JSON responses, validation, and complete builds.

use std::{
    fs::{self, File},
    hint::black_box,
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use serde::{Deserialize, Serialize};

use netindex::{Builder, Error, Limits, Reader, Target};

#[expect(
    clippy::expect_used,
    reason = "a failed timed operation must fail the benchmark process"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    fs::create_dir_all("target/criterion")?;

    let mut sizes = File::create("target/criterion/index-sizes.csv")?;
    writeln!(sizes, "scenario,records,shared_bytes")?;

    for scenario in ["v4", "v6", "v6-wide", "overlap", "asn"] {
        let targets: Vec<_> = (0..65_536u32)
            .map(|id| match scenario {
                "v4" => Target::Network {
                    address: Ipv4Addr::from(id * 512).into(),
                    prefix: 24,
                },
                "v6" => Target::Network {
                    address: Ipv6Addr::from(u128::from(id) << 64).into(),
                    prefix: 72,
                },
                "v6-wide" => {
                    Target::Address(Ipv6Addr::from((u128::from(id) << 80) | u128::from(id)).into())
                }
                "asn" => Target::Asn(id + 1),
                _ => Target::Range {
                    start: Ipv4Addr::from(id * 32).into(),
                    end: Ipv4Addr::from(id * 32 + 64).into(),
                },
            })
            .collect();

        let bytes = encode(&targets, false)?;
        let reader = Reader::open(bytes.as_slice(), Limits::default())?;

        writeln!(sizes, "{scenario},{},{}", targets.len(), bytes.len())?;

        let queries: Vec<_> = (0..1024u32)
            .map(|id| {
                let key = id.wrapping_mul(2654435761) % 65_536;

                if scenario == "v6-wide" {
                    IpAddr::from(Ipv6Addr::from(
                        (u128::from(key) << 80) | u128::from(key + id % 2),
                    ))
                } else if scenario == "v6" {
                    IpAddr::from(Ipv6Addr::from(
                        (u128::from(key) << 64) + (u128::from(id % 2) << 63),
                    ))
                } else {
                    IpAddr::from(Ipv4Addr::from(
                        key * if scenario == "overlap" { 32 } else { 512 } + id % 512,
                    ))
                }
            })
            .collect();

        verify_queries(&reader, &targets, &queries, scenario, false)?;

        let mut group = criterion.benchmark_group("lookup");
        group.throughput(Throughput::Elements(queries.len() as u64));

        group.bench_function(scenario, |bench| {
            bench.iter(|| {
                let mut sum = 0u64;

                for (id, query) in queries.iter().enumerate() {
                    let mut emit = |matched: netindex::Match<'_>| {
                        sum =
                            sum.wrapping_add(u64::from(matched.id) + matched.payload.len() as u64);

                        black_box(matched.target);

                        Ok::<_, Error>(())
                    };

                    if scenario == "asn" {
                        reader
                            .visit_asn((id as u32 * 7919) % 65_536 + 1, &mut emit)
                            .expect("ASN lookup benchmark failed");
                    } else {
                        reader
                            .visit_ip(*query, &mut emit)
                            .expect("IP lookup benchmark failed");
                    }
                }

                black_box(sum);
            })
        });

        group.finish();

        let mut group = criterion.benchmark_group("lookup-json");
        group.throughput(Throughput::Elements(queries.len() as u64));
        group.bench_function(scenario, |bench| {
            bench.iter(|| {
                for (id, &query) in queries.iter().enumerate() {
                    let request = if scenario == "asn" {
                        Query::Asn((id as u32 * 7919) % 65_536 + 1)
                    } else {
                        Query::Ip(query)
                    };
                    black_box(
                        json_response(&reader, request)
                            .expect("lookup and JSON serialization benchmark failed"),
                    );
                }
            })
        });
        group.finish();

        criterion.bench_function(&format!("open/{scenario}"), |bench| {
            bench.iter(|| {
                Reader::open(black_box(bytes.as_slice()), Limits::default())
                    .expect("validation benchmark failed")
            })
        });

        let mut group = criterion.benchmark_group("build");

        for unique in [false, true] {
            if unique {
                let bytes = encode(&targets, true)?;
                let reader = Reader::open(bytes.as_slice(), Limits::default())?;

                verify_queries(&reader, &targets, &queries, scenario, true)?;
            }

            group.bench_with_input(
                BenchmarkId::new(scenario, if unique { "unique" } else { "shared" }),
                &unique,
                |bench, unique| {
                    bench.iter(|| {
                        encode(black_box(&targets), *unique).expect("build benchmark failed")
                    })
                },
            );
        }

        group.finish();
    }

    directory_experiment(&mut criterion)?;

    criterion.final_summary();

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ResponseRecord {
    id: u32,
    value: u64,
}

enum Query {
    Ip(IpAddr),
    Asn(u32),
}

fn json_response(
    reader: &Reader<&[u8]>,
    query: Query,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut records = Vec::new();
    let mut emit = |matched: netindex::Match<'_>| {
        let bytes = matched
            .payload
            .try_into()
            .map_err(|_| Error::Invalid("benchmark u64 payload length"))?;
        records.push(ResponseRecord {
            id: matched.id,
            value: u64::from_le_bytes(bytes),
        });

        Ok::<_, Error>(())
    };

    match query {
        Query::Ip(address) => reader.visit_ip(address, &mut emit)?,
        Query::Asn(number) => reader.visit_asn(number, &mut emit)?,
    }

    Ok(serde_json::to_vec(&records)?)
}

fn verify_queries(
    reader: &Reader<&[u8]>,
    targets: &[Target],
    queries: &[IpAddr],
    scenario: &str,
    unique: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    for (position, &query) in queries.iter().enumerate() {
        let mut actual = Vec::new();
        let number = (position as u32 * 7919) % 65_536 + 1;
        let mut emit = |matched: netindex::Match<'_>| {
            actual.push((
                matched.id as usize,
                matched.target,
                matched.payload.to_vec(),
            ));
            Ok::<_, Error>(())
        };

        if scenario == "asn" {
            reader.visit_asn(number, &mut emit)?;
        } else {
            reader.visit_ip(query, &mut emit)?;
        }

        let expected: Vec<_> = targets
            .iter()
            .enumerate()
            .filter(|(_, target)| {
                if scenario == "asn" {
                    **target == Target::Asn(number)
                } else {
                    contains(**target, query)
                }
            })
            .map(|(id, target)| {
                let payload = if unique { id as u64 } else { 42 };

                (id, *target, payload.to_le_bytes())
            })
            .collect();

        actual.sort_unstable_by_key(|row| row.0);

        let request = if scenario == "asn" {
            Query::Asn(number)
        } else {
            Query::Ip(query)
        };
        let mut response: Vec<ResponseRecord> =
            serde_json::from_slice(&json_response(reader, request)?)?;
        response.sort_unstable_by_key(|row| row.id);

        let expected_response: Vec<_> = expected
            .iter()
            .map(|row| ResponseRecord {
                id: row.0 as u32,
                value: if unique { row.0 as u64 } else { 42 },
            })
            .collect();

        if response != expected_response {
            return Err(Error::Invalid(
                "serialized response disagrees with original-target oracle",
            )
            .into());
        }

        if actual.len() != expected.len()
            || actual.iter().zip(&expected).any(|(actual, expected)| {
                actual.0 != expected.0 || actual.1 != expected.1 || actual.2 != expected.2
            })
        {
            return Err(Error::Invalid("benchmark disagrees with original-target oracle").into());
        }
    }

    Ok(())
}

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

fn encode(targets: &[Target], unique: bool) -> Result<Vec<u8>, Error> {
    let mut builder = Builder::new(Limits::default());

    for (id, target) in targets.iter().enumerate() {
        let payload = if unique { id as u64 } else { 42 };
        builder.push(*target, &payload.to_le_bytes())?;
    }

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"benchmark")?;

    Ok(bytes)
}

#[expect(
    clippy::expect_used,
    reason = "a failed timed operation must fail the benchmark process"
)]
fn directory_experiment(criterion: &mut Criterion) -> Result<(), Error> {
    let mut report = File::create("target/criterion/directory-sizes.csv")?;
    writeln!(report, "family,directory_bits,additional_bytes")?;

    for family_bits in [32, 128] {
        let starts: Vec<_> = (0..65_536u128)
            .map(|number| number << (family_bits - 16))
            .collect();

        let queries: Vec<_> = (0..1024u128)
            .map(|number| ((number * 7919 % 65_536) << (family_bits - 16)) + number % 251)
            .chain([0, u128::MAX.checked_shr(128 - family_bits).unwrap_or(0)])
            .collect();

        let mut group = criterion.benchmark_group(format!("predecessor-model/v{family_bits}"));
        group.throughput(Throughput::Elements(queries.len() as u64));

        group.bench_function("binary", |bench| {
            bench.iter(|| {
                let mut checksum = 0;

                for &query in &queries {
                    checksum ^= black_box(starts.as_slice())
                        .partition_point(|&start| start <= black_box(query));
                }

                black_box(checksum)
            })
        });

        for directory_bits in [4, 8, 12, 16] {
            let directory = Directory::new(&starts, family_bits, directory_bits);

            for &query in &queries {
                if directory.predecessor(&starts, query)?
                    != starts
                        .partition_point(|&start| start <= query)
                        .checked_sub(1)
                {
                    return Err(Error::Invalid(
                        "experimental address directory disagrees with binary predecessor",
                    ));
                }
            }

            writeln!(
                report,
                "{family_bits},{directory_bits},{}",
                directory.offsets.len() * size_of::<u32>()
            )?;

            group.bench_with_input(
                BenchmarkId::new("directory", directory_bits),
                &directory,
                |bench, directory| {
                    bench.iter(|| {
                        let mut checksum = 0;

                        for &query in &queries {
                            let position = directory
                                .predecessor(black_box(&starts), black_box(query))
                                .expect("experimental directory lookup failed");

                            checksum ^= position.map_or(0, |position| position + 1);
                        }

                        black_box(checksum)
                    })
                },
            );
        }

        group.finish();
    }

    Ok(())
}

struct Directory {
    shift: u32,
    offsets: Vec<u32>,
}

impl Directory {
    fn new(starts: &[u128], family_bits: u32, directory_bits: u32) -> Self {
        let buckets = 1usize << directory_bits;
        let shift = family_bits - directory_bits;
        let mut offsets = Vec::with_capacity(buckets + 1);

        for bucket in 0..buckets {
            offsets.push(starts.partition_point(|&start| start >> shift < bucket as u128) as u32);
        }

        offsets.push(starts.len() as u32);

        Self { shift, offsets }
    }

    fn predecessor(&self, starts: &[u128], query: u128) -> Result<Option<usize>, Error> {
        let bucket =
            usize::try_from(query >> self.shift).map_err(|_| Error::Invalid("directory query"))?;

        let low = self
            .offsets
            .get(bucket)
            .copied()
            .ok_or(Error::Invalid("directory bucket"))? as usize;
        let high = self
            .offsets
            .get(bucket + 1)
            .copied()
            .ok_or(Error::Invalid("directory next bucket"))? as usize;
        let low = low.saturating_sub(1);

        let rows = starts
            .get(low..high)
            .ok_or(Error::Invalid("directory bounds"))?;

        Ok(rows
            .partition_point(|&start| start <= query)
            .checked_sub(1)
            .map(|local| low + local))
    }
}
