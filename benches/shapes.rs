//! Shape and caller-batch coverage; an original-target scan is the query oracle.

use std::{
    env, fs,
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use netindex::{Builder, Error, Limits, Reader, Target};

const RECORDS: u32 = 65_536;

#[derive(Clone, Copy)]
enum Query {
    Ip(IpAddr),
    Asn(u32),
}

#[expect(clippy::expect_used, reason = "timed failures must fail the benchmark")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let batches = env::var("NETINDEX_BENCH_BATCHES").unwrap_or_else(|_| "1,8,32,256".into());
    let batches: Vec<usize> = batches
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    if batches.is_empty()
        || batches
            .iter()
            .any(|&batch| ![1, 8, 32, 256].contains(&batch))
    {
        return Err("batches must be chosen from 1,8,32,256".into());
    }
    let directory = Path::new("target/shape-fixtures");
    fs::create_dir_all(directory)?;
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    for shape in [
        "dense-asn",
        "sparse-asn",
        "sparse-v6",
        "deep-v6",
        "duplicates-v4",
    ] {
        let targets: Vec<_> = (0..RECORDS).map(|id| target(shape, id)).collect();
        let queries: Vec<_> = (0..256).map(|index| query(shape, index)).collect();
        for unique in [false, true] {
            let kind = if unique { "unique" } else { "shared" };
            let mut builder = Builder::new(Limits::default());
            for (id, &target) in targets.iter().enumerate() {
                builder.push(target, &payload(id as u32, unique))?;
            }
            let bytes = builder.into_bytes(b"shapes-v1")?;
            fs::write(directory.join(format!("{shape}-{kind}.nidx")), &bytes)?;
            let reader = Reader::open(bytes, Limits::default())?;
            for &query in &queries {
                let mut actual = Vec::new();
                visit(&reader, query, |row| {
                    actual.push((row.id, row.target, row.payload.to_vec()));
                    Ok(())
                })?;
                actual.sort_unstable_by_key(|row| row.0);
                let expected: Vec<_> = targets
                    .iter()
                    .enumerate()
                    .filter(|(_, target)| contains(**target, query))
                    .map(|(id, &target)| (id as u32, target, payload(id as u32, unique).to_vec()))
                    .collect();
                assert_eq!(actual, expected, "shape {shape} with {kind} payloads");
            }

            let mut group = criterion.benchmark_group("lookup-shapes");
            for &batch in &batches {
                group.throughput(Throughput::Elements(batch as u64));
                group.bench_function(
                    BenchmarkId::new(format!("{shape}-{kind}"), batch),
                    |bench| {
                        let mut offset = 0;
                        bench.iter(|| {
                            let mut checksum = 0u64;
                            // Rotate batches through the same 256 queries. A one-query
                            // batch must not collapse into one permanently hot key.
                            for query in queries.iter().skip(offset).take(batch) {
                                visit(&reader, black_box(*query), |row| {
                                    checksum = checksum.wrapping_add(u64::from(row.id));
                                    black_box(row.target);
                                    black_box(row.payload);
                                    Ok(())
                                })
                                .expect("shape lookup failed");
                            }
                            offset = (offset + batch) % queries.len();
                            black_box(checksum);
                        });
                    },
                );
            }
            group.finish();
        }
    }
    criterion.final_summary();
    Ok(())
}

fn target(shape: &str, id: u32) -> Target {
    match shape {
        "dense-asn" => Target::Asn(id + 1),
        "sparse-asn" => Target::Asn(id * 65_536 + 1),
        "sparse-v6" => {
            Target::Address(Ipv6Addr::from((u128::from(id) << 112) | u128::from(id)).into())
        }
        "deep-v6" => Target::Network {
            address: Ipv6Addr::from(deep_base(id / 65)).into(),
            prefix: (64 + id % 65) as u8,
        },
        _ => Target::Address(Ipv4Addr::from((id / 64) * 2 + 2).into()),
    }
}

fn query(shape: &str, index: u32) -> Query {
    let id = index.wrapping_mul(2_654_435_761) % RECORDS;
    let miss = index % 2;
    match shape {
        "dense-asn" => Query::Asn(id + 1 + miss * RECORDS),
        "sparse-asn" => Query::Asn(id * 65_536 + 1 + miss),
        "sparse-v6" => Query::Ip(
            Ipv6Addr::from((u128::from(id) << 112) | u128::from(id) | (u128::from(miss) << 32))
                .into(),
        ),
        "deep-v6" => {
            Query::Ip(Ipv6Addr::from(deep_base(id / 65) + (u128::from(miss) << 64)).into())
        }
        _ => Query::Ip(Ipv4Addr::from((id / 64) * 2 + 2 + miss).into()),
    }
}

fn deep_base(group: u32) -> u128 {
    (0x2001_0db8u128 << 96) | (u128::from(group * 2) << 64)
}

fn payload(id: u32, unique: bool) -> [u8; 8] {
    if unique { u64::from(id) } else { 42 }.to_le_bytes()
}

fn visit(
    reader: &Reader<Vec<u8>>,
    query: Query,
    mut emit: impl FnMut(netindex::Match<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    match query {
        Query::Ip(address) => reader.visit_ip(address, &mut emit),
        Query::Asn(number) => reader.visit_asn(number, &mut emit),
    }
}

fn contains(target: Target, query: Query) -> bool {
    match (target, query) {
        (Target::Asn(a), Query::Asn(b)) => a == b,
        (Target::Address(a), Query::Ip(b)) => a == b,
        (
            Target::Network {
                address: IpAddr::V6(a),
                prefix,
            },
            Query::Ip(IpAddr::V6(b)),
        ) => {
            let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);
            u128::from(a) & mask == u128::from(b) & mask
        }
        _ => false,
    }
}
