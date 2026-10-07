//! Warm lookup, validation, and complete build costs for deterministic inputs.

use std::{
    fs::{self, File},
    hint::black_box,
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use netindex::{Builder, Error, Limits, Reader, Target};

#[expect(
    clippy::expect_used,
    reason = "a failed timed operation must fail the benchmark process"
)]
fn main() -> Result<(), Error> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    fs::create_dir_all("target/criterion")?;

    let mut sizes = File::create("target/criterion/index-sizes.csv")?;
    writeln!(sizes, "scenario,records,shared_bytes")?;

    for scenario in ["v4", "v6", "overlap", "asn"] {
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

                if scenario == "v6" {
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

        if matches!(scenario, "v4" | "v6") {
            for &query in &queries {
                let mut actual = Vec::new();

                reader.visit_ip(query, |matched| {
                    actual.push((matched.id as usize, matched.target));
                    Ok::<_, Error>(())
                })?;

                let expected: Vec<_> = targets
                    .iter()
                    .enumerate()
                    .filter_map(|(id, target)| {
                        let Target::Network { address, prefix } = *target else {
                            return None;
                        };

                        let matches = match (address, query) {
                            (IpAddr::V4(network), IpAddr::V4(query)) => {
                                let mask =
                                    u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);

                                u32::from(network) & mask == u32::from(query) & mask
                            }
                            (IpAddr::V6(network), IpAddr::V6(query)) => {
                                let mask =
                                    u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);

                                u128::from(network) & mask == u128::from(query) & mask
                            }
                            _ => false,
                        };

                        matches.then_some((id, *target))
                    })
                    .collect();

                if actual != expected {
                    return Err(Error::Invalid(
                        "packed benchmark disagrees with original-network oracle",
                    ));
                }
            }
        }

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

        criterion.bench_function(&format!("open/{scenario}"), |bench| {
            bench.iter(|| {
                Reader::open(black_box(bytes.as_slice()), Limits::default())
                    .expect("validation benchmark failed")
            })
        });

        let mut group = criterion.benchmark_group("build");

        for unique in [false, true] {
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
