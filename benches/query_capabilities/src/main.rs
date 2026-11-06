//! Application-level query, counter and build-settings qualification.
use std::{
    env, fs,
    hint::black_box,
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Instant,
};

use netindex::{Builder, Error, Limits, Match, Reader, SharedAlias, Target};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
enum Q {
    Ip(IpAddr),
    Asn(u32),
}
struct Case {
    reader: Reader<Vec<u8>>,
    queries: Vec<Q>,
    ranges: Vec<Target>,
    hash: String,
    bytes: usize,
    new_queries: Vec<NewQuery>,
}
#[cfg(feature = "new-api")]
type NewQuery = netindex::Query;
#[cfg(not(feature = "new-api"))]
type NewQuery = Q;

fn num(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(v) => u32::from(v) as u128,
        IpAddr::V6(v) => u128::from(v),
    }
}
fn bounds(t: Target) -> Option<(bool, u128, u128)> {
    match t {
        Target::Address(ip) => Some((ip.is_ipv6(), num(ip), num(ip))),
        Target::Network { address, prefix } => {
            let width = if address.is_ipv6() { 128 } else { 32 };
            let host = u128::MAX
                .checked_shr(128 - u32::from(width - prefix))
                .unwrap_or(0);
            Some((address.is_ipv6(), num(address), num(address) | host))
        }
        Target::Range { start, end } => Some((start.is_ipv6(), num(start), num(end))),
        Target::Asn(_) => None,
    }
}
fn contains(t: Target, q: Q) -> bool {
    match q {
        Q::Asn(k) => t == Target::Asn(k),
        Q::Ip(ip) => {
            bounds(t).is_some_and(|(v, s, e)| v == ip.is_ipv6() && s <= num(ip) && num(ip) <= e)
        }
    }
}
fn overlaps(t: Target, q: Target) -> bool {
    match (bounds(t), bounds(q)) {
        (Some((v, s, e)), Some((w, a, b))) => v == w && s <= b && e >= a,
        _ => false,
    }
}
fn point(t: Target) -> Q {
    match t {
        Target::Address(ip) | Target::Network { address: ip, .. } => Q::Ip(ip),
        Target::Range { start, .. } => Q::Ip(start),
        Target::Asn(k) => Q::Asn(k),
    }
}
fn range(q: Q) -> Option<Target> {
    let Q::Ip(ip) = q else { return None };
    let end = if ip.is_ipv6() {
        IpAddr::V6(Ipv6Addr::from(num(ip).saturating_add(40)))
    } else {
        IpAddr::V4(Ipv4Addr::from((num(ip) as u32).saturating_add(40)))
    };
    Some(Target::Range { start: ip, end })
}
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn synthetic(name: &str) -> Result<Vec<u8>, Error> {
    let mut b = Builder::new(Limits::default());
    for i in 0..32768u32 {
        let target = match name {
            "packed-v4" | "bitpacked-v4" | "aliases" => Target::Network {
                address: Ipv4Addr::from(i * 17).into(),
                prefix: 32,
            },
            "packed-v6" | "bitpacked-v6" => {
                Target::Address(Ipv6Addr::from(u128::from(i) * ((1u128 << 96) + 17)).into())
            }
            "overlaps" => Target::Range {
                start: Ipv4Addr::from((i / 32) * 128).into(),
                end: Ipv4Addr::from((i / 32) * 128 + 255).into(),
            },
            "asn" => Target::Asn(i / 32 + 1),
            "mixed" => match i % 3 {
                0 => Target::Asn(i / 6 + 1),
                1 => Target::Address(Ipv4Addr::from(i * 17).into()),
                _ => Target::Address(Ipv6Addr::from(u128::from(i) << 96).into()),
            },
            _ => return Err(Error::Invalid("study shape")),
        };
        b.push(target, &i.to_le_bytes())?;
    }
    let mut bytes = b.into_bytes(b"p2-schema-neutral")?;
    if name == "aliases" {
        let r = Reader::open(bytes, Limits::default())?;
        bytes = Vec::new();
        r.write_shared_aliases(
            &[SharedAlias {
                address: Ipv6Addr::from(1u128 << 112),
                prefix: 80,
                first_id: 32768,
                interleaved: false,
            }],
            &mut bytes,
            Limits::default(),
        )?;
    }
    if name.starts_with("bitpacked") {
        let r = Reader::open(bytes, Limits::default())?;
        bytes = Vec::new();
        r.write_bitpacked_to(&mut bytes)?;
    }
    Ok(bytes)
}
fn case(name: &str, seed: u64) -> Result<Case, Box<dyn std::error::Error>> {
    let bytes = if name.ends_with(".nidx") {
        fs::read(name)?
    } else {
        synthetic(name)?
    };
    let byte_length = bytes.len();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let r = Reader::open(
        bytes,
        Limits {
            records: 100_000_000,
            bytes: usize::MAX,
        },
    )?;
    let mut starts = Vec::new();
    let stride = r.len().div_ceil(4096).max(1);
    let mut seen = 0;
    r.visit_all(|row| {
        if seen % stride == 0 {
            starts.push(point(row.target));
        }
        seen += 1;
        Ok::<_, Error>(())
    })?;
    if starts.is_empty() {
        return Err("empty study fixture".into());
    }
    let mut state = seed;
    let mut queries = Vec::new();
    for i in 0..1024 {
        let random = next(&mut state);
        let q = if i % (if seed == 101 { 4 } else { 10 }) == 0 {
            starts[(random as usize) % starts.len().min(16)]
        } else {
            starts[(random as usize) % starts.len()]
        };
        queries.push(if i % (if seed == 101 { 7 } else { 3 }) == 0 {
            match q {
                Q::Ip(IpAddr::V4(v)) => Q::Ip(Ipv4Addr::from(u32::from(v) ^ 0x8000_0001).into()),
                Q::Ip(IpAddr::V6(v)) => {
                    Q::Ip(Ipv6Addr::from(u128::from(v) ^ ((1u128 << 127) + 1)).into())
                }
                Q::Asn(k) => Q::Asn(k.saturating_add(0x8000_0000)),
            }
        } else {
            q
        });
    }
    let ranges = queries.iter().copied().filter_map(range).take(16).collect();
    #[cfg(feature = "new-api")]
    let new_queries = queries
        .iter()
        .map(|q| match *q {
            Q::Ip(v) => netindex::Query::Ip(v),
            Q::Asn(k) => netindex::Query::Asn(k),
        })
        .collect();
    #[cfg(not(feature = "new-api"))]
    let new_queries = queries.clone();
    Ok(Case {
        reader: r,
        queries,
        ranges,
        hash,
        bytes: byte_length,
        new_queries,
    })
}
fn visit<'a>(
    r: &'a Reader<Vec<u8>>,
    q: Q,
    emit: impl FnMut(Match<'a>) -> Result<(), Error>,
) -> Result<(), Error> {
    match q {
        Q::Ip(v) => r.visit_ip(v, emit),
        Q::Asn(k) => r.visit_asn(k, emit),
    }
}
fn consume(row: Match<'_>, sum: &mut u64) {
    *sum = sum.wrapping_add(u64::from(row.id) + row.payload.len() as u64);
    black_box(row.target);
    black_box(row.payload);
}
#[inline(never)]
fn points(c: &Case, repeats: usize) -> Result<u64, Error> {
    let mut sum = 0;
    for _ in 0..repeats {
        for &q in &c.queries {
            visit(&c.reader, black_box(q), |row| {
                consume(row, &mut sum);
                Ok(())
            })?;
        }
    }
    Ok(sum)
}
#[inline(never)]
fn batches(c: &Case, repeats: usize, size: usize) -> Result<u64, Box<dyn std::error::Error>> {
    let mut sum = 0;
    for _ in 0..repeats {
        #[cfg(feature = "new-api")]
        for chunk in c.new_queries.chunks(size) {
            c.reader.visit_batch(black_box(chunk), |_, row| {
                consume(row, &mut sum);
                Ok::<_, Error>(())
            })?;
        }
        #[cfg(not(feature = "new-api"))]
        for chunk in c.new_queries.chunks(size) {
            for &q in chunk {
                visit(&c.reader, black_box(q), |row| {
                    consume(row, &mut sum);
                    Ok(())
                })?;
            }
        }
    }
    Ok(sum)
}
enum Halt {
    Done,
    Core(Error),
}
impl From<Error> for Halt {
    fn from(e: Error) -> Self {
        Self::Core(e)
    }
}
fn caller_member(c: &Case, q: Q) -> Result<bool, Error> {
    let result = match q {
        Q::Ip(ip) => c.reader.visit_ip(ip, |_| Err(Halt::Done)),
        Q::Asn(k) => c.reader.visit_asn(k, |_| Err(Halt::Done)),
    };
    match result {
        Ok(()) => Ok(false),
        Err(Halt::Done) => Ok(true),
        Err(Halt::Core(e)) => Err(e),
    }
}
#[inline(never)]
fn members(c: &Case, repeats: usize, mode: u8) -> Result<u64, Error> {
    let mut sum = 0;
    for _ in 0..repeats {
        for &q in &c.queries {
            let q = black_box(q);
            let found = if mode == 0 {
                let mut found = false;
                visit(&c.reader, q, |_| {
                    found = true;
                    Ok(())
                })?;
                found
            } else if mode == 1 {
                caller_member(c, q)?
            } else {
                #[cfg(feature = "new-api")]
                {
                    match q {
                        Q::Ip(v) => c.reader.contains_ip(v)?,
                        Q::Asn(k) => c.reader.contains_asn(k)?,
                    }
                }
                #[cfg(not(feature = "new-api"))]
                {
                    caller_member(c, q)?
                }
            };
            sum += u64::from(found);
        }
    }
    Ok(sum)
}
#[inline(never)]
fn intersections(c: &Case, repeats: usize, scan: bool, whole: bool) -> Result<u64, Error> {
    let mut sum = 0;
    let all = [
        Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        },
        Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        },
    ];
    let ranges = if whole {
        all.as_slice()
    } else {
        c.ranges.as_slice()
    };
    for _ in 0..repeats {
        for &target in ranges {
            let target = black_box(target);
            if scan {
                c.reader.visit_all(|row| {
                    if overlaps(row.target, target) {
                        consume(row, &mut sum);
                    }
                    Ok::<_, Error>(())
                })?;
            } else {
                #[cfg(feature = "new-api")]
                c.reader.visit_intersecting(target, |row| {
                    consume(row, &mut sum);
                    Ok::<_, Error>(())
                })?;
                #[cfg(not(feature = "new-api"))]
                c.reader.visit_all(|row| {
                    if overlaps(row.target, target) {
                        consume(row, &mut sum);
                    }
                    Ok::<_, Error>(())
                })?;
            }
        }
    }
    Ok(sum)
}
#[inline(never)]
fn inspections(c: &Case, repeats: usize) -> Result<u64, Error> {
    let mut sum = 0;
    for _ in 0..repeats {
        #[cfg(feature = "new-api")]
        {
            let info = black_box(c.reader.inspect()?);
            sum += info.artifact_bytes as u64;
        }
        #[cfg(not(feature = "new-api"))]
        {
            sum += black_box(c.reader.len()) as u64;
        }
    }
    Ok(sum)
}
fn verify(c: &Case) -> Result<(), Box<dyn std::error::Error>> {
    let mut assertions = Vec::new();
    c.reader.visit_all(|row| {
        assertions.push(row);
        Ok::<_, Error>(())
    })?;
    for &q in c.queries.iter().take(16) {
        let mut actual = Vec::new();
        visit(&c.reader, q, |row| {
            actual.push(row);
            Ok(())
        })?;
        let expected: Vec<_> = assertions
            .iter()
            .copied()
            .filter(|row| contains(row.target, q))
            .collect();
        assert_eq!(actual, expected);
    }
    assert_eq!(points(c, 1)?, batches(c, 1, 32)?);
    assert_eq!(members(c, 1, 0)?, members(c, 1, 1)?);
    assert_eq!(members(c, 1, 1)?, members(c, 1, 2)?);
    for &target in &c.ranges {
        #[cfg(feature = "new-api")]
        {
            let mut actual = Vec::new();
            c.reader.visit_intersecting(target, |row| {
                actual.push(row);
                Ok::<_, Error>(())
            })?;
            let expected: Vec<_> = assertions
                .iter()
                .copied()
                .filter(|row| overlaps(row.target, target))
                .collect();
            assert_eq!(actual, expected);
        }
        #[cfg(not(feature = "new-api"))]
        black_box(target);
    }
    Ok(())
}
fn execute(c: &Case, method: &str, repeats: usize) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(match method {
        "point" => points(c, repeats)?,
        "batch-1" => batches(c, repeats, 1)?,
        "batch-8" => batches(c, repeats, 8)?,
        "batch-32" => batches(c, repeats, 32)?,
        "batch-256" => batches(c, repeats, 256)?,
        "member-all" => members(c, repeats, 0)?,
        "member-caller" => members(c, repeats, 1)?,
        "member" => members(c, repeats, 2)?,
        "range-scan" => intersections(c, repeats, true, false)?,
        "range" => intersections(c, repeats, false, false)?,
        "whole-scan" => intersections(c, repeats, true, true)?,
        "whole" => intersections(c, repeats, false, true)?,
        "inspect" => inspections(c, repeats)?,
        _ => return Err("unknown method".into()),
    })
}
struct PerfControl {
    command: fs::File,
    ack: BufReader<fs::File>,
}
impl PerfControl {
    fn open() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Ok(path) = env::var("NETINDEX_PERF_CTL") else {
            return Ok(None);
        };
        let ack = env::var("NETINDEX_PERF_ACK")?;
        Ok(Some(Self {
            command: fs::OpenOptions::new().read(true).write(true).open(path)?,
            ack: BufReader::new(fs::OpenOptions::new().read(true).write(true).open(ack)?),
        }))
    }
    fn set(&mut self, command: &str) -> Result<(), Box<dyn std::error::Error>> {
        writeln!(self.command, "{command}")?;
        self.command.flush()?;
        let mut ack = String::new();
        if self.ack.read_line(&mut ack)? == 0 {
            return Err("perf control closed".into());
        }
        if ack.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace()) != "ack" {
            return Err(format!("unexpected perf acknowledgement: {ack:?}").into());
        }
        Ok(())
    }
}
#[cfg(windows)]
unsafe extern "C" {
    static __ImageBase: u8;
}
fn kernel_addresses() -> serde_json::Value {
    #[cfg(windows)]
    {
        // The MSVC linker defines this symbol at the loaded image base. Taking
        // its address does not read memory; offsets survive Windows ASLR.
        let base = std::ptr::addr_of!(__ImageBase) as usize;
        json!({"point":points as *const () as usize-base,"batch":batches as *const () as usize-base,"member":members as *const () as usize-base,"range":intersections as *const () as usize-base,"inspect":inspections as *const () as usize-base})
    }
    #[cfg(not(windows))]
    {
        serde_json::Value::Null
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let args: Vec<_> = env::args().skip(1).collect();
    let phase = args.first().map(String::as_str).unwrap_or("measure");
    let seed = if phase == "train" { 101 } else { 707 };
    let names: Vec<&str> = if args.len() > 1 {
        args.iter().skip(1).map(String::as_str).collect()
    } else {
        vec![
            "packed-v4",
            "bitpacked-v4",
            "packed-v6",
            "bitpacked-v6",
            "overlaps",
            "asn",
            "mixed",
            "aliases",
        ]
    };
    let methods = [
        "point",
        "batch-1",
        "batch-8",
        "batch-32",
        "batch-256",
        "member-all",
        "member-caller",
        "member",
        "range-scan",
        "range",
        "whole-scan",
        "whole",
        "inspect",
    ];

    match phase {
        "measure" | "membership" | "startup" | "train" | "verify" => {}
        _ => match phase.strip_prefix("counter-") {
            Some(method) if methods.contains(&method) => {}
            Some(method) => return Err(format!("unknown counter method: {method}").into()),
            None => return Err(format!("unknown query study phase: {phase}").into()),
        },
    }

    for name in names {
        let setup = Instant::now();
        let c = case(name, seed)?;
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
        if phase != "startup" && phase != "train" {
            verify(&c)?;
        }
        println!(
            "{}",
            json!({"phase":"input","case":name,"sha256":c.hash,"bytes":c.bytes,"records":c.reader.len(),"seed":seed,"setup_ms":setup_ms,"process_ready_ms":started.elapsed().as_secs_f64()*1000.0,"new_api":cfg!(feature="new-api"),"kernel_rvas":kernel_addresses()})
        );
        #[cfg(feature = "new-api")]
        println!(
            "{}",
            json!({"phase":"inspection","case":name,"inspection":format!("{:?}",c.reader.inspect()?)})
        );
        if phase == "verify" || phase == "startup" {
            continue;
        }
        if let Some(method) = phase.strip_prefix("counter-") {
            let mut control =
                PerfControl::open()?.ok_or("counter mode requires perf control FIFOs")?;
            control.set("enable")?;
            let checksum = execute(&c, method, if method == "inspect" { 100000 } else { 1024 })?;
            black_box(checksum);
            control.set("disable")?;
            println!(
                "{}",
                json!({"phase":"counter","case":name,"method":method,"checksum":checksum})
            );
            continue;
        }
        let rounds = if phase == "train" { 1 } else { 3 };
        for round in 0..rounds {
            let order: Vec<_> = if round == 1 {
                methods.iter().rev().copied().collect()
            } else {
                methods.to_vec()
            };
            for method in order {
                if phase == "membership" && !matches!(method, "member" | "point") {
                    continue;
                }
                if phase == "train"
                    && matches!(
                        method,
                        "range-scan" | "whole-scan" | "member-all" | "member-caller"
                    )
                {
                    continue;
                }
                #[cfg(not(feature = "new-api"))]
                if method == "inspect" {
                    continue;
                }
                if c.ranges.is_empty()
                    && (method.starts_with("range") || method.starts_with("whole"))
                {
                    continue;
                }
                let repeats = if phase == "membership" {
                    1024
                } else if phase == "train" {
                    if method.starts_with("whole") {
                        1
                    } else if method == "inspect" {
                        128
                    } else {
                        512
                    }
                } else if method.ends_with("scan") {
                    1
                } else if method.starts_with("whole") {
                    4
                } else if method == "range" {
                    128
                } else if method == "inspect" {
                    128
                } else {
                    64
                };
                let queries = if method.starts_with("range") {
                    c.ranges.len() * repeats
                } else if method.starts_with("whole") {
                    2 * repeats
                } else if method == "inspect" {
                    repeats
                } else {
                    c.queries.len() * repeats
                };
                let before = Instant::now();
                let checksum = execute(&c, method, repeats)?;
                black_box(checksum);
                let elapsed = before.elapsed().as_secs_f64();
                println!(
                    "{}",
                    json!({"phase":"timing","case":name,"method":method,"round":round,"queries":queries,"checksum":checksum,"ns_per_query":elapsed*1e9/queries as f64})
                );
            }
        }
    }
    Ok(())
}
