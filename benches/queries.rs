//! Warm public query operations on a deterministic, independently checked corpus.

use std::{
    hint::black_box,
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use criterion::{Criterion, Throughput};
use netindex::{Builder, Error, Limits, Match, Query, Reader, Target};

const BLOCKS: u32 = 8_192;
const SAMPLES: u32 = 256;
const BATCH_SIZES: [usize; 2] = [1, 32];

type Row<'a> = (u32, Target, &'a [u8]);

#[expect(clippy::expect_used, reason = "timed failures must fail the benchmark")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Builder::new(Limits::default());
    for block in 0..BLOCKS {
        for target in targets(block) {
            builder.push(target, &block.to_le_bytes())?;
        }
    }
    let reader = Reader::open(builder.into_bytes(b"queries-v1")?, Limits::default())?;
    let blocks = (0..SAMPLES).map(|i| i * 3137 % BLOCKS);
    let mut points = Vec::new();
    let mut ranges = Vec::new();
    let mut mixed = Vec::new();
    let mut expected = Vec::new();
    for block in blocks {
        let hit = address(block, 33);
        let miss = address(block, 257);
        let range = Target::Range {
            start: address(block, 31),
            end: address(block, 33),
        };
        verify(&reader, block, hit, miss, range)?;
        points.extend([hit, miss]);
        ranges.push(range);
        let position = mixed.len();
        mixed.extend([
            Query::Ip(hit),
            Query::Asn(block + 1),
            Query::Intersecting(range),
            Query::Ip(miss),
        ]);
        for (offset, target) in targets(block).into_iter().enumerate() {
            let id = block * 5 + offset as u32;
            for query in if offset == 4 { &[1][..] } else { &[0, 2][..] } {
                expected.push((position + query, id, target, block.to_le_bytes()));
            }
        }
    }
    expected.sort_unstable_by_key(|&(position, id, _, _)| (position, id));
    for size in BATCH_SIZES {
        let mut actual = Vec::new();
        assert_eq!(mixed.len() % size, 0);
        for (index, batch) in mixed.chunks_exact(size).enumerate() {
            let mut last_position = 0;
            reader.visit_batch(batch, |position, entry| {
                assert!(position < batch.len());
                assert!(position >= last_position);
                last_position = position;
                let payload = entry
                    .payload
                    .try_into()
                    .map_err(|_| Error::Invalid("payload length"))?;
                actual.push((index * size + position, entry.id, entry.target, payload));
                Ok::<_, Error>(())
            })?;
        }
        actual.sort_unstable_by_key(|&(position, id, _, _)| (position, id));
        assert_eq!(actual, expected);
    }

    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();
    let mut group = criterion.benchmark_group("query-api");
    group.throughput(Throughput::Elements(points.len() as u64));
    group.bench_function("contains-ip/mixed", |bench| {
        bench.iter(|| {
            let mut hits = 0;
            for &ip in black_box(&points) {
                hits += usize::from(reader.contains_ip(ip).expect("membership failed"));
            }
            black_box(hits)
        });
    });
    group.bench_function("longest-prefix/mixed", |bench| {
        bench.iter(|| {
            for &ip in black_box(&points) {
                black_box(reader.longest_prefix(ip).expect("longest prefix failed"));
            }
        });
    });
    group.throughput(Throughput::Elements(ranges.len() as u64));
    group.bench_function("intersecting-ip/ranges", |bench| {
        bench.iter(|| {
            for &range in black_box(&ranges) {
                reader
                    .visit_intersecting(range, consume)
                    .expect("intersection failed");
            }
        });
    });
    group.throughput(Throughput::Elements(mixed.len() as u64));
    for size in BATCH_SIZES {
        group.bench_function(format!("visit-batch/mixed-{size}"), |bench| {
            bench.iter(|| {
                for batch in black_box(&mixed).chunks_exact(size) {
                    reader
                        .visit_batch(batch, |position, row| {
                            black_box(position);
                            consume(row)
                        })
                        .expect("batch failed");
                }
            });
        });
    }
    group.finish();
    criterion.final_summary();
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "visitor requires a result")]
fn consume(row: Match<'_>) -> Result<(), Error> {
    black_box(row);
    Ok(())
}

fn verify(
    reader: &Reader<Vec<u8>>,
    block: u32,
    hit: IpAddr,
    miss: IpAddr,
    range: Target,
) -> Result<(), Error> {
    let [parent, child, _, overlap, _] = targets(block);
    let id = block * 5;
    let payload = block.to_le_bytes();
    // The interval touches the arbitrary range at its inclusive endpoint.
    // Longest prefix ignores that range and selects the earlier duplicate ID.
    let ip_matches = [
        (id, parent, payload.as_slice()),
        (id + 1, child, payload.as_slice()),
        (id + 2, child, payload.as_slice()),
        (id + 3, overlap, payload.as_slice()),
    ];
    assert!(reader.contains_ip(hit)?);
    assert!(!reader.contains_ip(miss)?);
    assert_eq!(reader.longest_prefix(hit)?.map(row), Some(ip_matches[1]));
    assert!(reader.longest_prefix(miss)?.is_none());

    let mut intersections = Vec::new();
    reader.visit_intersecting(range, |entry| {
        intersections.push(row(entry));
        Ok::<_, Error>(())
    })?;
    intersections.sort_unstable_by_key(|r| r.0);
    assert_eq!(intersections, ip_matches);

    Ok(())
}

fn row(entry: Match<'_>) -> Row<'_> {
    (entry.id, entry.target, entry.payload)
}

fn targets(block: u32) -> [Target; 5] {
    let parent = Target::Network {
        address: address(block, 0),
        prefix: 24,
    };
    let child = Target::Network {
        address: address(block, 32),
        prefix: 27,
    };
    let overlap = Target::Range {
        start: address(block, 33),
        end: address(block, 38),
    };
    [parent, child, child, overlap, Target::Asn(block + 1)]
}

fn address(block: u32, offset: u32) -> IpAddr {
    Ipv4Addr::from(0x0a00_0000 + block * 512 + offset).into()
}
