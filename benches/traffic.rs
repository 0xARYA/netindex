//! Exact-key traffic distributions with an independent hit, ID and payload oracle.

use std::{
    env, fs,
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use netindex::{Builder, Error, Limits, Reader, Target};

#[expect(clippy::expect_used, reason = "timed failures must fail the benchmark")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sizes = env::var("NETINDEX_BENCH_SIZES").unwrap_or_else(|_| "100000".into());
    let sizes: Vec<u32> = sizes.split(',').map(str::parse).collect::<Result<_, _>>()?;
    if sizes.is_empty() || sizes.iter().any(|&size| size == 0 || size > 20_000_000) {
        return Err("sizes must be in 1..=20000000".into());
    }
    let directory = Path::new("target/traffic-fixtures");
    fs::create_dir_all(directory)?;
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    for records in sizes {
        for family in ["v4", "v6", "asn"] {
            let limits = Limits {
                records: records as usize,
                bytes: 2 * 1024 * 1024 * 1024,
            };
            let mut builder = Builder::new(limits);
            for id in 0..records {
                let key = id * 2 + 2;
                let target = if family == "asn" {
                    Target::Asn(key)
                } else {
                    Target::Address(address(key, family))
                };
                builder.push(target, &(id % 4096).to_le_bytes())?;
            }
            let bytes = builder.into_bytes(b"scale-v1")?;
            // Save immutable fixture bytes for input hashing after timing runs.
            fs::write(directory.join(format!("{family}-{records}.nidx")), &bytes)?;
            let reader = Reader::open(bytes, limits)?;
            for pattern in [
                "uniform-mixed",
                "mostly-hits",
                "mostly-misses",
                "hot-skew",
                "locality",
            ] {
                let queries = queries(records, pattern);
                let expected_hits = match pattern {
                    "mostly-hits" => 16_221,
                    "mostly-misses" => 163,
                    _ => 8_192,
                };
                assert_eq!(
                    queries.iter().filter(|&&key| key % 2 == 0).count(),
                    expected_hits
                );
                for &key in &queries {
                    let rows = if family == "asn" {
                        reader.lookup_asn(key)?
                    } else {
                        reader.lookup_ip(address(key, family))?
                    };
                    let hit = key % 2 == 0;
                    assert_eq!(rows.len(), usize::from(hit));
                    if hit {
                        let id = key / 2 - 1;
                        let row = rows.first().ok_or("missing exact-key hit")?;
                        assert_eq!(row.id, id);
                        assert_eq!(row.payload, (id % 4096).to_le_bytes());
                        assert_eq!(
                            row.target,
                            if family == "asn" {
                                Target::Asn(key)
                            } else {
                                Target::Address(address(key, family))
                            }
                        );
                    }
                }

                let mut group = criterion.benchmark_group("lookup-traffic");
                group.throughput(Throughput::Elements(queries.len() as u64));
                group.bench_function(
                    BenchmarkId::new(format!("{family}-{pattern}"), records),
                    |bench| {
                        bench.iter(|| {
                            let mut checksum = 0u64;
                            for &key in &queries {
                                let mut emit = |row: netindex::Match<'_>| {
                                    checksum = checksum.wrapping_add(u64::from(row.id));
                                    black_box(row.target);
                                    black_box(row.payload);
                                    Ok::<_, Error>(())
                                };
                                if family == "asn" {
                                    reader
                                        .visit_asn(black_box(key), &mut emit)
                                        .expect("ASN lookup failed");
                                } else {
                                    reader
                                        .visit_ip(black_box(address(key, family)), &mut emit)
                                        .expect("IP lookup failed");
                                }
                            }
                            black_box(checksum);
                        });
                    },
                );
                group.finish();
            }
        }
    }
    criterion.final_summary();
    Ok(())
}

fn address(key: u32, family: &str) -> IpAddr {
    if family == "v6" {
        Ipv6Addr::from((0x2001_0db8u128 << 96) | u128::from(key)).into()
    } else {
        Ipv4Addr::from(key).into()
    }
}

fn queries(records: u32, pattern: &str) -> Vec<u32> {
    (0..16384u32)
        .map(|index| {
            let uniform = index.wrapping_mul(2654435761) % records;
            let id = match pattern {
                "hot-skew" if (index / 2) % 10 != 0 => (index / 20) % 16 % records,
                "locality" => ((index / 32) * (records / 512) + index % 32) % records,
                _ => uniform,
            };
            let hit = match pattern {
                "mostly-hits" => index % 100 != 99,
                "mostly-misses" => index % 100 == 99,
                _ => index % 2 == 0,
            };
            id * 2 + 2 + u32::from(!hit)
        })
        .collect()
}
