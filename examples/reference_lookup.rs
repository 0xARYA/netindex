//! Measure validated opening and warm opaque-payload lookup on an immutable file.
//! Usage: reference_lookup FILE. Linux reports process RSS immediately after open.

use std::{fs::File, hint::black_box, net::IpAddr, time::Instant};

use netindex::{Error, Limits, MappedReader, Target};
use serde_json::json;

type Failure = Box<dyn std::error::Error>;

const QUERY_COUNT: usize = 16_384;
const QUERY_REPETITIONS: usize = 8;
const SAMPLES: usize = 9;

#[derive(Clone, Copy)]
enum Query {
    Ip(IpAddr),
    Asn(u32),
}

#[expect(clippy::print_stdout, reason = "emit measured benchmark results")]
fn main() -> Result<(), Failure> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: reference_lookup FILE")?;
    let input = File::open(&path)?;
    let file_bytes = input.metadata()?.len();
    let started = Instant::now();
    // SAFETY: the caller holds this privately staged input immutable.
    let reader = unsafe {
        MappedReader::map_file(
            &input,
            Limits {
                records: 30_000_000,
                bytes: 2 * 1024 * 1024 * 1024,
            },
        )
    }?;
    let open_ns = started.elapsed().as_nanos();
    let open_rss_kib = rss();
    println!(
        "{}",
        json!({"phase":"open", "records":reader.len(), "file_bytes":file_bytes, "open_ns":open_ns, "open_rss_kib":open_rss_kib})
    );

    let hits = sampled_target_queries(&reader)?;
    let mixed = uniform_mixed_queries();
    for (pattern, queries) in [("sampled-target-hits", hits), ("uniform-mixed-ip", mixed)] {
        if queries.is_empty() {
            continue;
        }
        let expected = lookup_digest(&reader, &queries)?;
        let mut ns_per_query = Vec::new();
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let actual = lookup_digest(&reader, &queries)?;
            let elapsed = started.elapsed().as_nanos();
            assert_eq!(actual, expected);
            ns_per_query.push(elapsed as f64 / (queries.len() * QUERY_REPETITIONS) as f64);
        }
        println!(
            "{}",
            json!({"phase":"lookup", "pattern":pattern, "queries_per_sample":queries.len()*QUERY_REPETITIONS, "ns_per_query":ns_per_query, "matches_per_sample":expected.0, "digest":expected.1, "rss_kib":rss()})
        );
    }
    Ok(())
}

fn sampled_target_queries(reader: &MappedReader) -> Result<Vec<Query>, Error> {
    let mut hits = Vec::new();
    let stride = (reader.len() / QUERY_COUNT).max(1);
    let mut at = 0usize;
    reader.visit_all(|row| {
        if at.is_multiple_of(stride) && hits.len() < QUERY_COUNT {
            hits.push(match row.target {
                Target::Address(ip)
                | Target::Network { address: ip, .. }
                | Target::Range { start: ip, .. } => Query::Ip(ip),
                Target::Asn(asn) => Query::Asn(asn),
            });
        }
        at += 1;
        Ok::<_, Error>(())
    })?;
    Ok(hits)
}

fn uniform_mixed_queries() -> Vec<Query> {
    let mut random = 0xd1b5_4a32_d192_ed03u64;
    let mut mixed = Vec::new();
    for index in 0..QUERY_COUNT {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        mixed.push(Query::Ip(if index % 2 == 0 {
            std::net::Ipv4Addr::from(random as u32).into()
        } else {
            std::net::Ipv6Addr::from(
                (u128::from(random) << 64) | u128::from(random.rotate_left(29)),
            )
            .into()
        }));
    }
    mixed
}

fn lookup_digest(reader: &MappedReader, queries: &[Query]) -> Result<(u64, u64), Error> {
    let mut digest = 0u64;
    let mut count = 0;
    for _ in 0..QUERY_REPETITIONS {
        for query in queries {
            let mut visit = |row: netindex::Match<'_>| {
                digest = digest.rotate_left(3)
                    ^ u64::from(row.id)
                    ^ row.payload.len() as u64
                    ^ u64::from(row.payload.first().copied().unwrap_or(0))
                    ^ (u64::from(row.payload.last().copied().unwrap_or(0)) << 8);
                count += 1;
                Ok::<_, Error>(())
            };
            match query {
                Query::Ip(ip) => reader.visit_ip(black_box(*ip), &mut visit)?,
                Query::Asn(asn) => reader.visit_asn(black_box(*asn), &mut visit)?,
            }
        }
    }
    Ok(black_box((count, digest)))
}

fn rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}
