//! Warm lookup, validation, and in-memory construction on deterministic inputs.

use std::{
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};
use netindex::{Builder, Error, Limits, Reader, Target};

const RECORDS: u32 = 65_536;
const QUERIES: u32 = 1_024;

#[derive(Clone, Copy)]
enum Shape {
    Ipv4,
    WideIpv6,
    Asn,
}

impl Shape {
    fn name(self) -> &'static str {
        match self {
            Self::Ipv4 => "ipv4",
            Self::WideIpv6 => "wide-ipv6",
            Self::Asn => "asn",
        }
    }

    fn target(self, key: u32) -> Target {
        match self {
            Self::Ipv4 => Target::Address(Ipv4Addr::from(key).into()),
            Self::WideIpv6 => {
                let address = (u128::from(key) << 80) | u128::from(key);
                Target::Address(Ipv6Addr::from(address).into())
            }
            Self::Asn => Target::Asn(key),
        }
    }

    fn visit(self, reader: &Reader<&[u8]>, key: u32) -> Result<(), Error> {
        let emit = |row: netindex::Match<'_>| {
            black_box(row);
            Ok::<_, Error>(())
        };
        match self.target(key) {
            Target::Address(address) => reader.visit_ip(address, emit),
            Target::Asn(number) => reader.visit_asn(number, emit),
            _ => Err(Error::Invalid("benchmark requires exact targets")),
        }
    }

    fn ip(self, key: u32) -> Result<IpAddr, Error> {
        match self.target(key) {
            Target::Address(address) => Ok(address),
            _ => Err(Error::Invalid("benchmark requires an IP shape")),
        }
    }
}

#[expect(clippy::expect_used, reason = "timed failures fail the benchmark")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();
    let queries: Vec<_> = (0..QUERIES)
        .map(|position| {
            let id = position.wrapping_mul(2654435761) % RECORDS;
            (id * 2 + 2 + position % 2, id, position % 2 == 0)
        })
        .collect();

    let mut group = criterion.benchmark_group("index");
    for shape in [Shape::Ipv4, Shape::WideIpv6, Shape::Asn] {
        let input: Vec<_> = (0..RECORDS)
            .map(|id| (shape.target(id * 2 + 2), (id % 4096).to_le_bytes()))
            .collect();
        let bytes = encode(&input)?;
        let reader = Reader::open(bytes.as_slice(), Limits::default())?;
        for &(key, id, hit) in &queries {
            let rows = if matches!(shape, Shape::Asn) {
                reader.lookup_asn(key)?
            } else {
                reader.lookup_ip(shape.ip(key)?)?
            };
            assert_eq!(rows.len(), usize::from(hit));
            if hit {
                let row = rows.first().ok_or("missing exact match")?;
                assert_eq!(row.id, id);
                assert_eq!(row.target, shape.target(key));
                assert_eq!(row.payload, (id % 4096).to_le_bytes());
            }
        }

        group.throughput(Throughput::Elements(queries.len() as u64));
        group.bench_function(BenchmarkId::new("lookup", shape.name()), |bench| {
            bench.iter(|| {
                for &(key, _, _) in black_box(&queries) {
                    shape.visit(&reader, key).expect("exact lookup failed");
                }
            });
        });

        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(BenchmarkId::new("validate", shape.name()), |bench| {
            bench.iter(|| {
                Reader::open(black_box(bytes.as_slice()), Limits::default())
                    .expect("validation failed")
            });
        });
    }

    let input: Vec<_> = (0..RECORDS)
        .map(|id| (Shape::Ipv4.target(id * 2 + 2), (id % 4096).to_le_bytes()))
        .collect();
    group.throughput(Throughput::Elements(input.len() as u64));
    group.bench_function("build/ipv4", |bench| {
        bench.iter(|| encode(black_box(&input)).expect("build failed"));
    });
    group.finish();
    criterion.final_summary();
    Ok(())
}

fn encode(input: &[(Target, [u8; 4])]) -> Result<Vec<u8>, Error> {
    let mut builder = Builder::new(Limits::default());
    for (target, payload) in input {
        builder.push(*target, payload)?;
    }
    builder.into_bytes(b"index-v2")
}
