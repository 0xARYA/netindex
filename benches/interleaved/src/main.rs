//! Differential and original-input checks precede every interleaving measurement.
use std::{
    fs::File,
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Instant,
};

use netindex::{Builder, Error, Limits, MappedReader, Match, Reader, Target};

type Failure = Box<dyn std::error::Error>;
enum Queries {
    Ips(Vec<IpAddr>),
    Asns(Vec<u32>),
}
impl Queries {
    fn len(&self) -> usize {
        match self {
            Self::Ips(q) => q.len(),
            Self::Asns(q) => q.len(),
        }
    }
    fn prefix(&self, count: usize) -> Self {
        match self {
            Self::Ips(q) => Self::Ips(q[..count.min(q.len())].to_vec()),
            Self::Asns(q) => Self::Asns(q[..count.min(q.len())].to_vec()),
        }
    }
}
const VARIANTS: [usize; 5] = [0, 1, 4, 8, 16];

fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) || (args.len() == 5 && args[4] != "check") {
        return Err("expected FILE SCENARIO RECORDS [check]".into());
    }
    pin()?;
    self_check()?;
    let records: u32 = args[3].parse()?;
    if records == 0 || records > 20_000_000 {
        return Err("invalid record count".into());
    }
    let scenario = &args[2];
    let shape = scenario
        .strip_suffix("-shared")
        .or_else(|| scenario.strip_suffix("-unique"));
    if shape.is_none() && !["v4", "v6", "asn"].contains(&scenario.as_str()) {
        return Err("unknown scenario".into());
    }
    if let Some(shape) = shape
        && (records != 65_536
            || ![
                "dense-asn",
                "sparse-asn",
                "sparse-v6",
                "deep-v6",
                "duplicates-v4",
            ]
            .contains(&shape))
    {
        return Err("unknown shape or record count".into());
    }
    // SAFETY: runner owns an immutable private input copy until this process exits.
    let reader = unsafe {
        MappedReader::map_file(
            &File::open(&args[1])?,
            Limits {
                records: records as usize,
                bytes: 2 * 1024 * 1024 * 1024,
            },
        )
    }?;
    assert_eq!(reader.len(), records as usize);
    reader.visit_all(|row| {
        assert_eq!(row.target, original_target(scenario, row.id));
        assert_eq!(row.payload, payload(scenario, row.id));
        Ok::<_, Error>(())
    })?;
    let patterns: &[&str] = if shape.is_some() {
        &["uniform"]
    } else {
        &["uniform", "hot"]
    };
    for &pattern in patterns {
        let queries = queries(scenario, records, pattern);
        let original = oracle(scenario, records, &queries);
        let baseline = capture(&reader, &queries, 0)?;
        assert_eq!(owned(&baseline), original);
        for variant in VARIANTS {
            let actual = capture(&reader, &queries, variant)?;
            assert_eq!(actual, baseline);
            for ((_, a), (_, b)) in actual.iter().zip(&baseline) {
                assert_eq!(a.payload.as_ptr(), b.payload.as_ptr());
            }
            for count in [0, 1, 3, 4, 5, 7, 8, 15, 16, 17, 33, 257] {
                let short = queries.prefix(count);
                assert_eq!(
                    capture(&reader, &short, variant)?,
                    capture(&reader, &short, 0)?
                );
            }
        }
        let hits = baseline.len() as u64;
        let checksum = baseline.iter().fold(0u64, |sum, (query, row)| {
            sum.wrapping_add(*query as u64 + u64::from(row.id))
        });
        println!(
            "{}",
            serde_json::json!({"phase":"checked","scenario":scenario,"traffic":pattern,
            "records":records,"corpus_queries":queries.len(),"matches":hits,"checksum":checksum,"variants":VARIANTS,
            "full_assertions_checked":records,"borrowed_pointer_parity":true,"partial_and_empty_batches":true})
        );
        if args.len() == 5 {
            continue;
        }
        let passes = if shape.is_some() { 256 } else { 16 };
        for trial in 0..4 {
            let mut variants = VARIANTS;
            if trial % 2 == 1 {
                variants.reverse();
            }
            for variant in variants {
                execute(&reader, &queries, variant, |_, row| {
                    black_box(row);
                    Ok::<_, Error>(())
                })?;
                let mut measured_hits = 0u64;
                let mut measured_checksum = 0u64;
                let started = Instant::now();
                for _ in 0..passes {
                    execute(&reader, &queries, variant, |query, row| {
                        measured_hits += 1;
                        measured_checksum =
                            measured_checksum.wrapping_add(query as u64 + u64::from(row.id));
                        black_box(row.target);
                        black_box(row.payload);
                        Ok::<_, Error>(())
                    })?;
                }
                let elapsed = started.elapsed().as_nanos();
                assert_eq!(measured_hits, hits * passes);
                assert_eq!(measured_checksum, checksum.wrapping_mul(passes));
                let count = queries.len() as u64 * passes;
                println!(
                    "{}",
                    serde_json::json!({"phase":"measure","scenario":scenario,"traffic":pattern,"trial":trial+1,
                    "lanes":variant,"queries":count,"matches":measured_hits,"checksum":measured_checksum,
                    "elapsed_ns":elapsed,"ns_per_query":elapsed as f64/count as f64})
                );
            }
        }
    }
    Ok(())
}

#[inline(never)]
fn execute<'a, B: AsRef<[u8]>, E: From<Error>>(
    reader: &'a Reader<B>,
    queries: &Queries,
    lanes: usize,
    mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
) -> Result<(), E> {
    if lanes == 0 {
        match queries {
            Queries::Ips(q) => {
                for (index, &q) in q.iter().enumerate() {
                    reader.visit_ip(q, |row| emit(index, row))?;
                }
            }
            Queries::Asns(q) => {
                for (index, &q) in q.iter().enumerate() {
                    reader.visit_asn(q, |row| emit(index, row))?;
                }
            }
        }
        return Ok(());
    }
    match lanes {
        1 => batch::<1, _, _>(reader, queries, emit),
        4 => batch::<4, _, _>(reader, queries, emit),
        8 => batch::<8, _, _>(reader, queries, emit),
        16 => batch::<16, _, _>(reader, queries, emit),
        _ => Err(Error::Invalid("interleave lanes").into()),
    }
}
fn batch<'a, const N: usize, B: AsRef<[u8]>, E: From<Error>>(
    reader: &'a Reader<B>,
    queries: &Queries,
    emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
) -> Result<(), E> {
    match queries {
        Queries::Ips(q) => reader.study_ips::<N, E>(q, emit),
        Queries::Asns(q) => reader.study_asns::<N, E>(q, emit),
    }
}
fn capture<'a, B: AsRef<[u8]>>(
    reader: &'a Reader<B>,
    queries: &Queries,
    variant: usize,
) -> Result<Vec<(usize, Match<'a>)>, Error> {
    let mut rows = Vec::new();
    execute(reader, queries, variant, |query, row| {
        rows.push((query, row));
        Ok::<_, Error>(())
    })?;
    Ok(rows)
}
type Owned = (usize, u32, Target, Vec<u8>);
fn owned(rows: &[(usize, Match<'_>)]) -> Vec<Owned> {
    rows.iter()
        .map(|(q, row)| (*q, row.id, row.target, row.payload.to_vec()))
        .collect()
}
fn original_target(scenario: &str, id: u32) -> Target {
    match scenario {
        "asn" => Target::Asn(id * 2 + 2),
        "v4" => Target::Address(Ipv4Addr::from(id * 2 + 2).into()),
        "v6" => {
            Target::Address(Ipv6Addr::from((0x2001_0db8u128 << 96) + u128::from(id * 2 + 2)).into())
        }
        s if s.starts_with("dense-asn-") => Target::Asn(id + 1),
        s if s.starts_with("sparse-asn-") => Target::Asn(id * 65_536 + 1),
        s if s.starts_with("sparse-v6-") => {
            Target::Address(Ipv6Addr::from((u128::from(id) << 112) | u128::from(id)).into())
        }
        s if s.starts_with("deep-v6-") => Target::Network {
            address: Ipv6Addr::from(deep_base(id / 65)).into(),
            prefix: (64 + id % 65) as u8,
        },
        _ => Target::Address(Ipv4Addr::from((id / 64) * 2 + 2).into()),
    }
}
fn payload(scenario: &str, id: u32) -> Vec<u8> {
    if scenario.ends_with("-unique") {
        u64::from(id).to_le_bytes().to_vec()
    } else if scenario.ends_with("-shared") {
        42u64.to_le_bytes().to_vec()
    } else {
        (id % 4096).to_le_bytes().to_vec()
    }
}
fn queries(scenario: &str, records: u32, pattern: &str) -> Queries {
    let shape = scenario.ends_with("-shared") || scenario.ends_with("-unique");
    let size: u32 = if shape { 256 } else { 65_536 };
    let mut ips = Vec::new();
    let mut asns = Vec::new();
    for index in 0..size {
        let limit = if pattern == "hot" {
            records.min(1024)
        } else {
            records
        };
        let id = index.wrapping_mul(2_654_435_761u32) % limit;
        let miss = index % 2;
        match scenario {
            "asn" => asns.push(id * 2 + 2 + miss),
            "v4" => ips.push(Ipv4Addr::from(id * 2 + 2 + miss).into()),
            "v6" => ips.push(
                Ipv6Addr::from((0x2001_0db8u128 << 96) + u128::from(id * 2 + 2 + miss)).into(),
            ),
            s if s.starts_with("dense-asn-") => asns.push(id + 1 + miss * records),
            s if s.starts_with("sparse-asn-") => asns.push(id * 65_536 + 1 + miss),
            s if s.starts_with("sparse-v6-") => ips.push(
                Ipv6Addr::from((u128::from(id) << 112) | u128::from(id) | (u128::from(miss) << 32))
                    .into(),
            ),
            s if s.starts_with("deep-v6-") => {
                ips.push(Ipv6Addr::from(deep_base(id / 65) + (u128::from(miss) << 64)).into())
            }
            _ => ips.push(Ipv4Addr::from((id / 64) * 2 + 2 + miss).into()),
        }
    }
    if !asns.is_empty() {
        asns.extend([0, 1, u32::MAX]);
        Queries::Asns(asns)
    } else {
        ips.extend::<[IpAddr; 4]>([
            Ipv4Addr::UNSPECIFIED.into(),
            Ipv4Addr::BROADCAST.into(),
            Ipv6Addr::UNSPECIFIED.into(),
            Ipv6Addr::from(u128::MAX).into(),
        ]);
        Queries::Ips(ips)
    }
}
fn oracle(scenario: &str, records: u32, queries: &Queries) -> Vec<Owned> {
    let shape = scenario.ends_with("-shared") || scenario.ends_with("-unique");
    let mut output = Vec::new();
    for index in 0..queries.len() {
        if shape {
            let mut found: Vec<_> = (0..records)
                .filter_map(|id| {
                    let target = original_target(scenario, id);
                    contains(target, queries, index).then_some((
                        index,
                        id,
                        target,
                        payload(scenario, id),
                    ))
                })
                .collect();
            found.sort_unstable_by_key(|row| {
                let (a, b, _) = bounds(row.2);
                (a, b, row.1)
            });
            output.extend(found);
        } else {
            let key = match queries {
                Queries::Asns(q) => u128::from(q[index]),
                Queries::Ips(q) => {
                    if q[index].is_ipv6() != (scenario == "v6") {
                        continue;
                    }
                    let base = if scenario == "v6" {
                        0x2001_0db8u128 << 96
                    } else {
                        0
                    };
                    let Some(key) = numeric(q[index]).checked_sub(base) else {
                        continue;
                    };
                    key
                }
            };
            if key >= 2 && key <= u128::from(records * 2) && key % 2 == 0 {
                let id = (key / 2 - 1) as u32;
                output.push((
                    index,
                    id,
                    original_target(scenario, id),
                    payload(scenario, id),
                ));
            }
        }
    }
    output
}
fn numeric(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(ip) => u128::from(u32::from(ip)),
        IpAddr::V6(ip) => u128::from(ip),
    }
}
fn deep_base(group: u32) -> u128 {
    (0x2001_0db8u128 << 96) | (u128::from(group * 2) << 64)
}
fn bounds(target: Target) -> (u128, u128, u8) {
    match target {
        Target::Asn(n) => (u128::from(n), u128::from(n), 2),
        Target::Address(ip) => (numeric(ip), numeric(ip), u8::from(ip.is_ipv6())),
        Target::Range { start, end } => (numeric(start), numeric(end), u8::from(start.is_ipv6())),
        Target::Network { address, prefix } => {
            let width = if address.is_ipv6() { 128 } else { 32 };
            let host = u128::MAX
                .checked_shr(128 - u32::from(width - prefix))
                .unwrap_or(0);
            (
                numeric(address),
                numeric(address) | host,
                u8::from(address.is_ipv6()),
            )
        }
    }
}
fn contains(target: Target, queries: &Queries, index: usize) -> bool {
    let (start, end, family) = bounds(target);
    let (value, query_family) = match queries {
        Queries::Ips(q) => (numeric(q[index]), u8::from(q[index].is_ipv6())),
        Queries::Asns(q) => (u128::from(q[index]), 2),
    };
    family == query_family && start <= value && value <= end
}

fn self_check() -> Result<(), Failure> {
    let v4: IpAddr = "192.0.2.1".parse()?;
    let v6: IpAddr = "2001:db8::1".parse()?;
    let targets = [
        Target::Address(v4),
        Target::Address(v4),
        Target::Network {
            address: "0.0.0.0".parse()?,
            prefix: 0,
        },
        Target::Range {
            start: v4,
            end: "192.0.2.9".parse()?,
        },
        Target::Address(v6),
        Target::Range {
            start: v6,
            end: "2001:db8::9".parse()?,
        },
        Target::Network {
            address: "::".parse()?,
            prefix: 0,
        },
        Target::Asn(64512),
        Target::Asn(64512),
        Target::Asn(u32::MAX),
    ];
    let mut sets = vec![targets.to_vec()];
    for broad in [false, true] {
        let mut targets = Vec::new();
        for id in 0..64u32 {
            targets.push(if broad {
                Target::Network {
                    address: Ipv4Addr::from((192 + id) << 24).into(),
                    prefix: 8,
                }
            } else {
                Target::Address(Ipv4Addr::from(0xc000_0201 + id * 2).into())
            });
            targets.push(if broad {
                Target::Network {
                    address: Ipv6Addr::from(u128::from(0x2001_0db8u32 + id) << 96).into(),
                    prefix: 32,
                }
            } else {
                Target::Address(
                    Ipv6Addr::from((0x2001_0db8u128 << 96) + u128::from(id * 2 + 1)).into(),
                )
            });
        }
        targets.extend([
            Target::Asn(64512),
            Target::Asn(64512),
            Target::Asn(u32::MAX),
        ]);
        sets.push(targets);
    }
    sets.push(Vec::new());
    for (configuration, targets) in sets.iter().enumerate() {
        let mut builder = Builder::new(Limits::default());
        for (id, &target) in targets.iter().enumerate() {
            builder.push(target, &(id as u64).to_le_bytes())?;
        }
        let bytes = builder.into_bytes(b"selfcheck")?;
        let reader = if configuration == 2 {
            Reader::open_with_broad_routes(bytes, Limits::default())?
        } else {
            Reader::open(bytes, Limits::default())?
        };
        enum Stop {
            Library(Error),
            Marker,
        }
        impl From<Error> for Stop {
            fn from(error: Error) -> Self {
                Self::Library(error)
            }
        }
        for queries in [
            Queries::Ips(vec![v4, v6, "255.255.255.255".parse()?, "::".parse()?, v4]),
            Queries::Asns(vec![0, 64512, u32::MAX, 64512, 1]),
        ] {
            let expected = capture(&reader, &queries, 0)?;
            let mut original = Vec::new();
            for q in 0..queries.len() {
                let mut rows: Vec<_> = targets
                    .iter()
                    .enumerate()
                    .filter(|(_, target)| contains(**target, &queries, q))
                    .map(|(id, &target)| (q, id as u32, target, (id as u64).to_le_bytes().to_vec()))
                    .collect();
                rows.sort_unstable_by_key(|row| {
                    let (a, b, _) = bounds(row.2);
                    (a, b, row.1)
                });
                original.extend(rows);
            }
            assert_eq!(owned(&expected), original);
            for variant in VARIANTS {
                assert_eq!(capture(&reader, &queries, variant)?, expected);
                for failed_at in 0..expected.len() {
                    let mut observed = Vec::new();
                    let result = execute(&reader, &queries, variant, |query, row| {
                        observed.push((query, row));
                        if observed.len() == failed_at + 1 {
                            Err(Stop::Marker)
                        } else {
                            Ok(())
                        }
                    });
                    match result {
                        Err(Stop::Marker) => {}
                        Err(Stop::Library(error)) => return Err(error.into()),
                        Ok(()) => return Err("lost visitor error".into()),
                    }
                    assert_eq!(observed, expected[..=failed_at]);
                }
            }
        }
        assert!(matches!(
            reader.study_asns::<0, Error>(&[], |_, _| Ok(())),
            Err(Error::Invalid("interleave lanes"))
        ));
        assert!(matches!(
            reader.study_ips::<17, Error>(&[], |_, _| Ok(())),
            Err(Error::Invalid("interleave lanes"))
        ));
    }
    assert!(Reader::open(vec![0; 80], Limits::default()).is_err());
    Ok(())
}
fn pin() -> std::io::Result<()> {
    // SAFETY: CPU 20 fits the initialized mask; PID zero selects this thread.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(20, &mut set);
        if libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
