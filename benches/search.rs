//! Packed predecessor lookup over every encoded integer width, with hits and gaps.

use std::{
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput};

use netindex::{Builder, Error, Limits, Reader, Target};

const RECORDS: u32 = 65_536;

#[expect(
    clippy::expect_used,
    reason = "failed timed operations must fail the benchmark"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();

    for ipv6 in [false, true] {
        for width in [1, 2, 4, 8, 16] {
            if !ipv6 && width > 4 {
                continue;
            }
            let mut builder = Builder::new(Limits::default());
            for id in 0..RECORDS {
                let address = address(value(id, width), ipv6)?;
                builder.push(Target::Address(address), &id.to_le_bytes())?;
            }
            let bytes = builder.into_bytes(b"")?;
            // The first descriptor follows the empty metadata and 80-byte header.
            assert_eq!(
                bytes.get(104).copied().ok_or("missing descriptor")? & 0x1f,
                width
            );
            let reader = Reader::open(bytes.as_slice(), Limits::default())?;
            let mut queries = Vec::new();
            for index in 0..1024u32 {
                let key = index.wrapping_mul(2654435761) % RECORDS;
                let hit = index % 2 == 0;
                let query = if hit {
                    value(key, width)
                } else {
                    value((key / 256) * 256 + 255, width) + 1
                };
                let query = address(query, ipv6)?;
                let rows = reader.lookup_ip(query)?;
                assert_eq!(rows.len(), usize::from(hit));
                if hit {
                    let row = rows.first().ok_or("missing hit")?;
                    assert_eq!(row.id, key);
                    assert_eq!(row.target, Target::Address(query));
                    assert_eq!(row.payload, key.to_le_bytes());
                }
                queries.push(query);
            }

            let mut group = criterion.benchmark_group("lookup-width");
            group.throughput(Throughput::Elements(queries.len() as u64));
            group.bench_function(
                BenchmarkId::new(if ipv6 { "v6" } else { "v4" }, width),
                |bench| {
                    bench.iter(|| {
                        let mut checksum = 0u64;
                        for &query in &queries {
                            reader
                                .visit_ip(black_box(query), |row| {
                                    checksum = checksum.wrapping_add(u64::from(row.id));
                                    black_box(row.target);
                                    black_box(row.payload);
                                    Ok::<_, Error>(())
                                })
                                .expect("packed lookup failed");
                        }
                        black_box(checksum);
                    });
                },
            );
            group.finish();
        }
    }
    criterion.final_summary();
    Ok(())
}

fn value(id: u32, width: u8) -> u128 {
    let (shift, step) = match width {
        1 => (16, 1),
        2 => (24, 251),
        4 => (24, 65537),
        8 => (64, (1u128 << 32) + 1),
        _ => (112, (1u128 << 80) + 1),
    };
    (u128::from(id / 256) << shift) + u128::from(id % 256) * step
}

fn address(value: u128, ipv6: bool) -> Result<IpAddr, Box<dyn std::error::Error>> {
    Ok(if ipv6 {
        Ipv6Addr::from(value).into()
    } else {
        Ipv4Addr::from(u32::try_from(value)?).into()
    })
}
