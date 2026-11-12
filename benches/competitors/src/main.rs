//! Matched projections of original assertions: longest prefix or membership.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, OpenOptions},
    hint::black_box,
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Instant,
};

use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;
use netindex::{Builder, Error, Limits, Reader, Target};
use roaring::RoaringBitmap;

type Failure = Box<dyn std::error::Error>;
type Table = IpNetworkTable<u32>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Answer {
    address: IpAddr,
    prefix: u8,
    id: u32,
}

fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("expected FRESH_DIRECTORY FAMILY SHAPE RECORDS".into());
    }
    let directory = Path::new(&args[1]);
    fs::create_dir(directory)?;
    let records: u32 = args[4].parse()?;
    if !(1_000..=1_000_000).contains(&records) {
        return Err("records must be in 1000..=1000000".into());
    }
    let mut clock = Vec::new();
    for _ in 0..10_000 {
        let start = Instant::now();
        clock.push(start.elapsed().as_nanos() as u64);
    }
    clock.sort_unstable();
    println!(
        "{}",
        serde_json::json!({"phase":"clock", "p50_ns":clock[5000], "p95_ns":clock[9500]})
    );
    match args[2].as_str() {
        "v4" | "v6" => prefix(directory, &args[2], &args[3], records)?,
        "asn" => membership(directory, &args[3], records)?,
        _ => return Err("unknown family".into()),
    }
    Ok(())
}

fn prefix(directory: &Path, family: &str, shape: &str, records: u32) -> Result<(), Failure> {
    if !["disjoint", "overlap"].contains(&shape) {
        return Err("unknown prefix shape".into());
    }
    let v6 = family == "v6";
    let mut builder = Builder::new(limits());
    let mut table = Table::new();
    let mut original = HashMap::new();
    for index in 0..records {
        let address = address(v6, index, false);
        for prefix in if v6 { [64, 56, 48] } else { [24, 20, 16] } {
            let canonical = masked(address, prefix);
            if (prefix != if v6 { 64 } else { 24 } && shape != "overlap") || canonical != address {
                continue;
            }
            let id = original.len() as u32;
            assert!(original.insert((address, prefix), id).is_none());
            assert!(table.insert(IpNetwork::new(address, prefix)?, id).is_none());
            builder.push(Target::Network { address, prefix }, &id.to_le_bytes())?;
        }
    }
    let bytes = builder.into_bytes(b"lpm-projection")?;
    write_new(directory, "netindex.nidx", &bytes)?;
    let artifact_bytes = bytes.len();
    let reader = Reader::open(bytes, limits())?;
    let levels = if v6 { [64, 56, 48] } else { [24, 20, 16] };
    let oracle = |ip: IpAddr| {
        levels.iter().find_map(|&prefix| {
            let address = masked(ip, prefix);
            original.get(&(address, prefix)).map(|&id| Answer {
                address,
                prefix,
                id,
            })
        })
    };
    let index_lookup = |ip| -> Result<Option<Answer>, Error> {
        let mut best: Option<Answer> = None;
        reader.visit_ip(ip, |row| {
            let Target::Network { address, prefix } = row.target else {
                return Err(Error::Invalid("unexpected prefix target"));
            };
            let id = u32::from_le_bytes(
                row.payload
                    .try_into()
                    .map_err(|_| Error::Invalid("payload width"))?,
            );
            if best.is_none_or(|previous| prefix > previous.prefix) {
                best = Some(Answer {
                    address,
                    prefix,
                    id,
                });
            }
            Ok::<_, Error>(())
        })?;
        Ok(best)
    };
    let table_lookup = |ip| {
        table.longest_match(ip).map(|(network, &id)| Answer {
            address: network.network_address(),
            prefix: network.netmask(),
            id,
        })
    };
    let direct = |ip| -> Result<Option<Answer>, Error> {
        reader
            .longest_prefix(ip)?
            .map(|row| {
                let Target::Network { address, prefix } = row.target else {
                    return Err(Error::Invalid("unexpected prefix target"));
                };
                let id = u32::from_le_bytes(
                    row.payload
                        .try_into()
                        .map_err(|_| Error::Invalid("payload width"))?,
                );
                assert_eq!(row.id, id);
                Ok(Answer {
                    address,
                    prefix,
                    id,
                })
            })
            .transpose()
    };
    // Exercise every input prefix's boundaries and its immediate neighbours.
    // The independent oracle performs masked-key equality, never trie traversal.
    let mut boundary_checks = 0u64;
    for &(address, prefix) in original.keys() {
        let width = if v6 { 128 } else { 32 };
        let start = numeric(address);
        let last = start | ((1u128 << (width - prefix)) - 1);
        for point in [
            start.checked_sub(1),
            Some(start),
            Some(last),
            last.checked_add(1),
        ]
        .into_iter()
        .flatten()
        {
            if !v6 && point > u128::from(u32::MAX) {
                continue;
            }
            let ip = from_numeric(v6, point);
            assert_eq!(index_lookup(ip)?, oracle(ip));
            assert_eq!(direct(ip)?, oracle(ip));
            assert_eq!(table_lookup(ip), oracle(ip));
            boundary_checks += 1;
        }
    }
    println!(
        "{}",
        serde_json::json!({"phase":"fixture","family":family,"shape":shape,
        "leaf_records":records,"assertions":original.len(),"boundary_checks":boundary_checks,
        "netindex_bytes":artifact_bytes,"contract":"Option<(network address, prefix, insertion ID)>; unique prefixes; all-match visitor reduced to longest prefix"})
    );
    for traffic in ["uniform", "hot"] {
        let queries: Vec<_> = (0..65_536u32)
            .map(|index| {
                let limit = if traffic == "hot" {
                    records.min(1024)
                } else {
                    records
                };
                let leaf = index.wrapping_mul(2_654_435_761) % limit;
                address(v6, leaf, index % 2 == 1)
            })
            .collect();
        write_new(
            directory,
            &format!("corpus-{traffic}.bin"),
            &queries
                .iter()
                .flat_map(|&ip| numeric(ip).to_le_bytes())
                .collect::<Vec<_>>(),
        )?;
        println!(
            "{}",
            serde_json::json!({"phase":"corpus","traffic":traffic,"queries":queries.len(),"hits":queries.iter().filter(|&&ip|oracle(ip).is_some()).count()})
        );
        for &ip in &queries {
            assert_eq!(index_lookup(ip)?, oracle(ip));
            assert_eq!(direct(ip)?, oracle(ip));
            assert_eq!(table_lookup(ip), oracle(ip));
        }
        let expected: u64 = queries
            .iter()
            .map(|&ip| fingerprint(oracle(ip)))
            .fold(0, u64::wrapping_add);
        for trial in 1..=4 {
            let engines = if trial % 2 == 1 {
                ["netindex", "netindex-direct", "ip_network_table"]
            } else {
                ["ip_network_table", "netindex-direct", "netindex"]
            };
            for engine in engines {
                if engine == "netindex" {
                    measure(traffic, engine, trial, &queries, expected, |&ip| {
                        Ok(fingerprint(index_lookup(ip)?))
                    })?;
                } else if engine == "netindex-direct" {
                    measure(traffic, engine, trial, &queries, expected, |&ip| {
                        Ok(fingerprint(direct(ip)?))
                    })?;
                } else {
                    measure(traffic, engine, trial, &queries, expected, |&ip| {
                        Ok(fingerprint(table_lookup(ip)))
                    })?;
                }
            }
        }
    }
    Ok(())
}

fn membership(directory: &Path, shape: &str, records: u32) -> Result<(), Failure> {
    if !["dense", "sparse", "clustered"].contains(&shape) {
        return Err("unknown membership shape".into());
    }
    let key = |id: u32| match shape {
        "dense" => id + 1,
        "sparse" => ((u64::from(id) * 0x9e37_79b1) % u64::from(u32::MAX) + 1) as u32,
        _ => (id / 1024) * 4096 + id % 1024 + 1,
    };
    let mut builder = Builder::new(limits());
    let mut bitmap = RoaringBitmap::new();
    let original: HashSet<_> = (0..records).map(key).collect();
    assert_eq!(original.len(), records as usize);
    for id in 0..records {
        let value = key(id);
        builder.push(Target::Asn(value), b"")?;
        assert!(bitmap.insert(value));
    }
    let mut optimized = bitmap.clone();
    optimized.optimize();
    let mut serialized = Vec::new();
    bitmap.serialize_into(&mut serialized)?;
    write_new(directory, "roaring-default.rbm", &serialized)?;
    assert_eq!(
        RoaringBitmap::deserialize_from(serialized.as_slice())?,
        bitmap
    );
    serialized.clear();
    optimized.serialize_into(&mut serialized)?;
    write_new(directory, "roaring-optimized.rbm", &serialized)?;
    assert_eq!(
        RoaringBitmap::deserialize_from(serialized.as_slice())?,
        bitmap
    );
    let bytes = builder.into_bytes(b"membership-projection")?;
    write_new(directory, "netindex.nidx", &bytes)?;
    let artifact_bytes = bytes.len();
    let reader = Reader::open(bytes, limits())?;
    let index_lookup = |key| -> Result<bool, Error> {
        let mut found = false;
        reader.visit_asn(key, |_| {
            found = true;
            Ok::<_, Error>(())
        })?;
        Ok(found)
    };
    let mut boundary_checks = 0u64;
    for &value in &original {
        for query in [value.checked_sub(1), Some(value), value.checked_add(1)]
            .into_iter()
            .flatten()
        {
            let expected = original.contains(&query);
            assert_eq!(index_lookup(query)?, expected);
            assert_eq!(bitmap.contains(query), expected);
            assert_eq!(optimized.contains(query), expected);
            boundary_checks += 1;
        }
    }
    println!(
        "{}",
        serde_json::json!({"phase":"fixture","family":"asn","shape":shape,
        "assertions":records,"boundary_checks":boundary_checks,"netindex_bytes":artifact_bytes,
        "roaring_default_bytes":bitmap.serialized_size(),"roaring_optimized_bytes":optimized.serialized_size(),
        "roaring_default_containers":{"array":bitmap.statistics().n_array_containers,"bitset":bitmap.statistics().n_bitset_containers,"run":bitmap.statistics().n_run_containers},
        "roaring_optimized_containers":{"array":optimized.statistics().n_array_containers,"bitset":optimized.statistics().n_bitset_containers,"run":optimized.statistics().n_run_containers},
        "contract":"Boolean membership over unique u32 ASN keys, empty netindex payloads"})
    );
    for traffic in ["uniform", "hot"] {
        let queries: Vec<_> = (0..65_536u32)
            .map(|index| {
                let limit = if traffic == "hot" {
                    records.min(1024)
                } else {
                    records
                };
                let id = index.wrapping_mul(2_654_435_761) % limit;
                if index % 2 == 0 {
                    key(id)
                } else {
                    key(records + id)
                }
            })
            .collect();
        write_new(
            directory,
            &format!("corpus-{traffic}.bin"),
            &queries
                .iter()
                .flat_map(|key| key.to_le_bytes())
                .collect::<Vec<_>>(),
        )?;
        println!(
            "{}",
            serde_json::json!({"phase":"corpus","traffic":traffic,"queries":queries.len(),"hits":queries.iter().filter(|key| original.contains(key)).count()})
        );
        for &value in &queries {
            let expected = original.contains(&value);
            assert_eq!(index_lookup(value)?, expected);
            assert_eq!(bitmap.contains(value), expected);
            assert_eq!(optimized.contains(value), expected);
        }
        let expected = queries.iter().filter(|key| original.contains(key)).count() as u64;
        for trial in 1..=6 {
            let engines = match trial {
                1 => ["netindex", "roaring-default", "roaring-optimized"],
                2 => ["roaring-default", "roaring-optimized", "netindex"],
                3 => ["roaring-optimized", "netindex", "roaring-default"],
                4 => ["roaring-optimized", "roaring-default", "netindex"],
                5 => ["roaring-default", "netindex", "roaring-optimized"],
                _ => ["netindex", "roaring-optimized", "roaring-default"],
            };
            for engine in engines {
                match engine {
                    "netindex" => measure(traffic, engine, trial, &queries, expected, |&key| {
                        Ok(u64::from(index_lookup(key)?))
                    })?,
                    "roaring-default" => {
                        measure(traffic, engine, trial, &queries, expected, |&key| {
                            Ok(u64::from(bitmap.contains(key)))
                        })?
                    }
                    _ => measure(traffic, engine, trial, &queries, expected, |&key| {
                        Ok(u64::from(optimized.contains(key)))
                    })?,
                }
            }
        }
    }
    Ok(())
}

fn measure<T>(
    traffic: &str,
    engine: &str,
    trial: u32,
    queries: &[T],
    expected: u64,
    lookup: impl Fn(&T) -> Result<u64, Error>,
) -> Result<(), Failure> {
    for query in queries {
        black_box(lookup(black_box(query))?);
    }
    let mut samples = Vec::with_capacity((queries.len() * 16).div_ceil(1021));
    let mut checksum = 0u64;
    let started = Instant::now();
    for index in 0..queries.len() * 16 {
        let now = index.is_multiple_of(1021).then(Instant::now);
        checksum = checksum.wrapping_add(black_box(lookup(black_box(
            &queries[index % queries.len()],
        ))?));
        if let Some(now) = now {
            samples.push(now.elapsed().as_nanos() as u64);
        }
    }
    let elapsed = started.elapsed().as_nanos();
    assert_eq!(checksum, expected.wrapping_mul(16));
    let mut histogram = BTreeMap::new();
    for &sample in &samples {
        *histogram.entry(sample).or_insert(0u64) += 1;
    }
    samples.sort_unstable();
    println!(
        "{}",
        serde_json::json!({"phase":"measure","traffic":traffic,"engine":engine,"trial":trial,
        "queries":queries.len()*16,"elapsed_ns":elapsed,"ns_per_query":elapsed as f64/(queries.len()*16) as f64,
        "checksum":checksum,"sample_count":samples.len(),"p50_ns":samples[samples.len()/2],
        "p95_ns":samples[samples.len()*95/100],"p99_ns":samples[samples.len()*99/100],"histogram_ns":histogram})
    );
    Ok(())
}

fn address(v6: bool, index: u32, gap: bool) -> IpAddr {
    if v6 {
        Ipv6Addr::from((0x2001_0db8u128 << 96) | ((u128::from(index) * 2 + u128::from(gap)) << 64))
            .into()
    } else {
        Ipv4Addr::from(index * 512 + if gap { 256 } else { 0 }).into()
    }
}
fn numeric(address: IpAddr) -> u128 {
    match address {
        IpAddr::V4(ip) => u128::from(u32::from(ip)),
        IpAddr::V6(ip) => u128::from(ip),
    }
}
fn from_numeric(v6: bool, address: u128) -> IpAddr {
    if v6 {
        Ipv6Addr::from(address).into()
    } else {
        Ipv4Addr::from(address as u32).into()
    }
}
fn masked(address: IpAddr, prefix: u8) -> IpAddr {
    let width = if address.is_ipv6() { 128 } else { 32 };
    let mask = u128::MAX
        .checked_shl(u32::from(width - prefix))
        .unwrap_or(0);
    from_numeric(address.is_ipv6(), numeric(address) & mask)
}
fn fingerprint(answer: Option<Answer>) -> u64 {
    answer.map_or(0, |row| {
        let ip = numeric(row.address);
        (ip as u64)
            .wrapping_add((ip >> 64) as u64)
            .wrapping_add(u64::from(row.id) + u64::from(row.prefix) + 1)
    })
}
fn write_new(directory: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?
        .write_all(bytes)
}
fn limits() -> Limits {
    Limits {
        records: 2_000_000,
        bytes: 128 * 1024 * 1024,
    }
}
