//! Standalone application-adapter benchmark; prepared with external source snapshots.
#![allow(dead_code)]
use ip_intel_format::{Limits, Source};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Instant,
};

mod mmdb {
    pub use ip_intel_format::native::Metadata;
    mod database;
    pub(crate) use database::Database;
    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("MMDB: {0}")]
        Mmdb(#[from] maxminddb::MaxMindDbError),
        #[error("invalid {0}")]
        Invalid(&'static str),
        #[error("unsupported {0}")]
        Unsupported(&'static str),
        #[error("limit {0}")]
        Limit(&'static str),
        #[error("I/O {operation}: {error}")]
        Io {
            operation: &'static str,
            error: std::io::Error,
        },
        #[error("record {dataset:?} {network}: {error}")]
        Record {
            dataset: crate::Source,
            network: ipnet::IpNet,
            error: Box<Error>,
        },
    }
    pub(crate) fn io_error(operation: &'static str) -> impl FnOnce(std::io::Error) -> Error {
        move |error| Error::Io { operation, error }
    }
    pub(crate) fn validate_record_fields(fields: &[Option<&str>]) -> Result<(), Error> {
        if fields.iter().flatten().any(|field| field.len() > 64 * 1024) {
            return Err(Error::Limit("record field bytes"));
        }
        Ok(())
    }
}
mod sources {
    pub use crate::mmdb::Error as MmdbError;
    pub(crate) use crate::{Limits, Source};
    pub mod geo {
        pub use ip_intel_format::geo::Coordinates;
        pub mod model;
        pub mod schema;
    }
    mod native_component {
        #[derive(Debug, thiserror::Error)]
        pub enum Error {
            #[error("index: {0}")]
            Index(#[from] netindex::Error),
            #[error("invalid {0}")]
            Invalid(&'static str),
        }
        pub struct Asn<'a> {
            pub number: u32,
            pub organization: Option<&'a str>,
            pub domain: Option<&'a str>,
        }
        pub struct PayloadRef<'a> {
            pub asn: Option<Asn<'a>>,
            pub geo: Option<super::geo::model::Fields<'a>>,
            pub public_proxy: bool,
            pub network_override: Option<ipnet::IpNet>,
        }
        mod payload;
        pub(super) fn encode(
            output: &mut Vec<u8>,
            value: &PayloadRef,
            pool: &mut netindex::values::ValuePoolBuilder,
        ) -> Result<(), Error> {
            payload::encode_into(output, value, Some(pool))
        }
    }
    pub fn native(path: &std::path::Path) -> Result<crate::mmdb::Database<Vec<u8>>, MmdbError> {
        crate::mmdb::Database::open(
            Source::MaxmindCity,
            path,
            "GeoLite2-City",
            Limits::collector(),
            |entry| {
                geo::schema::decode(entry, Source::MaxmindCity)?
                    .ok_or(MmdbError::Invalid("record"))?;
                Ok(())
            },
        )
    }
    pub fn lookup(
        db: &crate::mmdb::Database<Vec<u8>>,
        ip: std::net::IpAddr,
    ) -> Result<Option<ip_intel_format::geo::Record>, Box<dyn std::error::Error>> {
        let result = db.lookup(ip)?;
        let Some(fields) = geo::schema::decode(&result, Source::MaxmindCity)? else {
            return Ok(None);
        };
        let n = result.network()?;
        Ok(fields.into_record(Source::MaxmindCity, ipnet::IpNet::new(n.ip(), n.prefix())?))
    }
    pub fn convert(
        input: &std::path::Path,
        output: &std::path::Path,
        temp: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use native_component::PayloadRef;
        use sha2::{Digest, Sha256};
        let begin = std::time::Instant::now();
        let bytes = std::fs::read(input)?;
        let native = crate::mmdb::Database::from_bytes(
            Source::MaxmindCity,
            bytes.as_slice(),
            "GeoLite2-City",
            Limits::collector(),
            |entry| {
                geo::schema::decode(entry, Source::MaxmindCity)?
                    .ok_or(MmdbError::Invalid("record"))?;
                Ok(())
            },
        )?;
        let receipt = native.metadata().clone();
        let native_validation_ms = begin.elapsed().as_secs_f64() * 1000.;
        drop(native);
        let raw = netindex::mmdb::Reader::from_source(bytes.as_slice())?;
        let dir = tempfile::tempdir_in(temp)?;
        let limits = netindex::Limits {
            bytes: 2 * 1024 * 1024 * 1024,
            records: 30_000_000,
        };
        let options = netindex::ExternalOptions {
            temporary_bytes: 8 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        let mut builder = netindex::ExternalBuilder::new(dir.path(), limits, options)?;
        let mut pool = netindex::values::ValuePoolBuilder::new(netindex::Limits {
            bytes: options.payload_cache_bytes,
            records: limits.records,
        });
        let mut encoded = Vec::new();
        let mut records = 0usize;
        let build_start = std::time::Instant::now();
        netindex::mmdb::visit_networks(
            &raw,
            netindex::mmdb::WithinOptions::default().include_aliased_networks(),
            |target, entry| -> Result<(), Box<dyn std::error::Error>> {
                let mut payload = PayloadRef {
                    asn: None,
                    geo: geo::schema::decode(&entry, Source::MaxmindCity)?,
                    public_proxy: false,
                    network_override: None,
                };
                native_component::encode(&mut encoded, &payload, &mut pool)?;
                builder.push(target, &encoded)?;
                records += 1;
                if raw.metadata().ip_version == 6 {
                    match target {
                        netindex::Target::Network {
                            address: std::net::IpAddr::V4(address),
                            prefix,
                        } => {
                            builder.push(
                                netindex::Target::Network {
                                    address: std::net::Ipv6Addr::from(u32::from(address) as u128)
                                        .into(),
                                    prefix: prefix + 96,
                                },
                                &encoded,
                            )?;
                            records += 1;
                        }
                        netindex::Target::Network {
                            address: std::net::IpAddr::V6(address),
                            prefix,
                        } if prefix < 96 && address.is_unspecified() => {
                            payload.network_override =
                                Some(ipnet::IpNet::new(address.into(), prefix)?);
                            native_component::encode(&mut encoded, &payload, &mut pool)?;
                            builder.push(
                                netindex::Target::Network {
                                    address: std::net::Ipv4Addr::UNSPECIFIED.into(),
                                    prefix: 0,
                                },
                                &encoded,
                            )?;
                            records += 1;
                        }
                        _ => {}
                    }
                }
                Ok(())
            },
        )?;
        let receipt = ip_intel_format::native_component::Metadata {
            version: 3,
            native: receipt,
            native_sha256: Some(format!("{:x}", Sha256::digest(&bytes))),
            records,
        };
        let json = serde_json::to_vec(&receipt)?;
        let mut metadata = b"BWNVPOOL".to_vec();
        metadata.extend_from_slice(&(u32::try_from(json.len())?).to_le_bytes());
        metadata.extend_from_slice(&json);
        metadata.extend_from_slice(&pool.into_bytes()?);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let stats = builder.write_to(&mut file, &metadata)?;
        drop(file);
        let encode_build_ms = build_start.elapsed().as_secs_f64() * 1000.;
        let validate = std::time::Instant::now();
        // SAFETY: benchmark-owned output is closed and immutable.
        let db = unsafe {
            ip_intel_format::native_component::Database::map_file(
                &std::fs::File::open(output)?,
                Limits::collector(),
            )
        }?;
        assert_eq!(db.metadata().records, records);
        println!(
            "{}",
            serde_json::json!({"phase":"converted","records":records,"artifact_bytes":stats.artifact_bytes,
            "native_validation_ms":native_validation_ms,"encode_build_ms":encode_build_ms,
            "normalized_output_validation_ms":validate.elapsed().as_secs_f64()*1000.,
            "total_ms":begin.elapsed().as_secs_f64()*1000.,"temporary_peak_bytes":stats.temporary_peak_bytes,
            "payload_cache_bytes":stats.payload_cache_bytes})
        );
        Ok(())
    }
}
fn open_component(
    path: &Path,
) -> Result<ip_intel_format::native_component::Database<memmap2::Mmap>, Box<dyn std::error::Error>>
{
    // SAFETY: benchmark files remain immutable for the entire mapping lifetime.
    Ok(unsafe {
        ip_intel_format::native_component::Database::map_file(
            &std::fs::File::open(path)?,
            Limits::collector(),
        )
    }?)
}

fn native_layout(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let reader = maxminddb::Reader::from_source(bytes.as_slice())?;
    let meta = reader.metadata();

    let marker = b"\xab\xcd\xefMaxMind.com";
    let footer = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .ok_or("missing MMDB metadata marker")?;
    let tree_bytes = usize::try_from(meta.node_count)?
        .checked_mul(usize::from(meta.record_size / 4))
        .ok_or("MMDB search tree size overflow")?;
    let data_start = tree_bytes
        .checked_add(16)
        .ok_or("MMDB data offset overflow")?;
    if bytes.get(tree_bytes..data_start) != Some(&[0; 16]) || footer < data_start {
        return Err("invalid MMDB section boundaries".into());
    }

    Ok(serde_json::json!({
        "bytes": bytes.len(), "tree_bytes": tree_bytes, "separator_bytes": 16,
        "data_bytes": footer - data_start, "metadata_bytes": bytes.len() - footer,
        "nodes": meta.node_count, "record_bits": meta.record_size
    }))
}

#[cfg(feature = "component-layout")]
fn component_layout(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let limits = netindex::Limits {
        records: 100_000_000,
        bytes: usize::MAX,
    };
    let info = netindex::Reader::open(bytes, limits)?.inspect()?;
    let netindex::PayloadReferences::Indexed { values, width } = info.references else {
        return Err("expected indexed references in City component".into());
    };

    Ok(serde_json::json!({
        "bytes": info.artifact_bytes, "header_bytes": info.header_bytes,
        "metadata_bytes": info.metadata_bytes, "payload_bytes": info.payload_bytes,
        "reference_bytes": info.reference_bytes,
        "reference_layout": {"kind":"indexed", "values":values, "ordinal_bytes":width},
        "ipv4_bytes": info.ipv4.bytes, "ipv6_bytes": info.ipv6.bytes,
        "ipv4_records": info.ipv4.records, "ipv6_records": info.ipv6.records,
        "ipv4_bitpacked_blocks": info.ipv4.bitpacked_blocks,
        "ipv6_bitpacked_blocks": info.ipv6.bitpacked_blocks,
        "asn_bytes": info.asn.bytes, "records": info.records,
        "physical_records": info.physical_records, "alias_windows": info.alias_windows,
        "format": info.format, "base_format": info.base_format,
    }))
}

#[cfg(not(feature = "component-layout"))]
fn component_layout(_path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Err("layout-converted requires the local netindex adapter".into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).ok_or("mode required")?;
    let input = Path::new(args.get(2).ok_or("input required")?);
    let layout = match mode.as_str() {
        "layout-native" => Some(native_layout(input)?),
        "layout-converted" => Some(component_layout(input)?),
        _ => None,
    };
    if let Some(layout) = layout {
        println!("{layout}");
        return Ok(());
    }

    if mode == "convert" {
        return sources::convert(
            input,
            Path::new(args.get(3).ok_or("output required")?),
            Path::new(args.get(4).ok_or("temporary directory required")?),
        );
    }
    if !matches!(
        mode.as_str(),
        "native" | "converted" | "parity" | "reload-native" | "reload-converted"
    ) {
        return Err("unknown benchmark mode".into());
    }
    let start = Instant::now();
    let native = if matches!(mode.as_str(), "native" | "parity" | "reload-native") {
        Some(sources::native(input)?)
    } else {
        None
    };
    let converted = if matches!(mode.as_str(), "converted" | "parity" | "reload-converted") {
        let path = Path::new(args.get(3).ok_or("component required")?);
        Some(open_component(path)?)
    } else {
        None
    };
    let open_ms = start.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::json!({"phase":"opened","mode":mode,"open_ms":open_ms})
    );
    if mode.starts_with("reload-") {
        let replacement = Path::new(args.get(4).ok_or("replacement required")?);
        let start = Instant::now();
        if mode == "reload-native" {
            let next = sources::native(replacement)?;
            let reload_ms = start.elapsed().as_secs_f64() * 1000.;
            std::hint::black_box((&native, &next));
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::hint::black_box((&native, &next));
            println!(
                "{}",
                serde_json::json!({"phase":"reload","reload_ms":reload_ms})
            );
        } else {
            let next = open_component(replacement)?;
            let reload_ms = start.elapsed().as_secs_f64() * 1000.;
            std::hint::black_box((&converted, &next));
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::hint::black_box((&converted, &next));
            println!(
                "{}",
                serde_json::json!({"phase":"reload","reload_ms":reload_ms})
            );
        }
        return Ok(());
    }
    let mut queries = addresses();
    if let Some(path) = args.get(4) {
        let trace = std::fs::read_to_string(path)?;
        let supplied = trace
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::parse::<IpAddr>)
            .collect::<Result<Vec<_>, _>>()?;
        if supplied.is_empty() {
            return Err("empty query trace".into());
        }
        queries = supplied;
    }
    if mode == "parity" {
        add_boundaries(input, &mut queries)?;
    }
    if mode == "parity" {
        for &ip in &queries {
            let a = sources::lookup(native.as_ref().ok_or("missing native")?, ip)?;
            let b = converted
                .as_ref()
                .ok_or("missing component")?
                .lookup(ip)?
                .and_then(|r| r.geo);
            assert_eq!(
                serde_json::to_value(a)?,
                serde_json::to_value(b)?,
                "query {ip}"
            );
        }
        println!(
            "{}",
            serde_json::json!({"phase":"parity","queries":queries.len(),"mismatches":0})
        );
    } else {
        let mut hits = 0;
        let mut timings = Vec::new();
        for &ip in queries.iter().take(256) {
            if let Some(db) = &native {
                std::hint::black_box(sources::lookup(db, ip)?);
            } else {
                std::hint::black_box(converted.as_ref().ok_or("missing component")?.lookup(ip)?);
            }
        }
        let gate = std::env::var_os("NETINDEX_PROFILE_LOOKUPS").is_some();
        if gate {
            profile_gate("lookup-ready")?;
        }
        for ip in queries {
            let start = Instant::now();
            let found = if let Some(db) = &native {
                std::hint::black_box(sources::lookup(db, ip)?).is_some()
            } else {
                std::hint::black_box(converted.as_ref().ok_or("missing component")?.lookup(ip)?)
                    .is_some()
            };
            timings.push(start.elapsed().as_nanos());
            hits += usize::from(found);
        }
        if gate {
            profile_gate("lookup-finished")?;
        }
        timings.sort_unstable();
        let n = timings.len();
        println!(
            "{}",
            serde_json::json!({"phase":"lookups","queries":n,"hits":hits,"p50_ns":timings[n/2],"p95_ns":timings[n*95/100],"p99_ns":timings[n*99/100]})
        );
        // Allow the external memory sampler to observe the serving working set.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Ok(())
}

fn profile_gate(phase: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    println!("{}", serde_json::json!({"phase": phase}));
    std::io::stdout().flush()?;
    if std::io::stdin().read_line(&mut String::new())? == 0 {
        return Err("missing profiler acknowledgement".into());
    }
    Ok(())
}

fn addresses() -> Vec<IpAddr> {
    let mut result: Vec<IpAddr> = [
        "0.0.0.0",
        "255.255.255.255",
        "::",
        "::ffff:192.0.2.1",
        "2002:c000:201::",
    ]
    .iter()
    .map(|s| s.parse().unwrap())
    .collect();
    let mut random = 0x73c8_6af2_3e1d_0945u64;
    for index in 0..100_000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        result.push(if index % 2 == 0 {
            Ipv4Addr::from(random as u32).into()
        } else {
            Ipv6Addr::from((u128::from(random) << 64) | u128::from(random.rotate_left(29))).into()
        });
    }
    result
}

fn add_boundaries(
    input: &Path,
    queries: &mut Vec<IpAddr>,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(input)?;
    let raw = maxminddb::Reader::from_source(bytes)?;
    for (index, entry) in raw.networks(Default::default())?.enumerate() {
        let entry = entry?;
        if index % 4096 != 0 {
            continue;
        }
        let network = entry.network()?;
        match network.ip() {
            IpAddr::V4(address) => {
                let first = u32::from(address);
                let last = first
                    | u32::MAX
                        .checked_shr(u32::from(network.prefix()))
                        .unwrap_or(0);
                queries.push(Ipv4Addr::from(first).into());
                queries.push(Ipv4Addr::from(last).into());
                for ip in [first, last] {
                    queries.push(Ipv6Addr::from(u128::from(ip)).into());
                    queries.push(Ipv6Addr::from((0xffffu128 << 32) | u128::from(ip)).into());
                    queries
                        .push(Ipv6Addr::from((0x2002u128 << 112) | (u128::from(ip) << 80)).into());
                }
            }
            IpAddr::V6(address) => {
                let first = u128::from(address);
                let last = first
                    | u128::MAX
                        .checked_shr(u32::from(network.prefix()))
                        .unwrap_or(0);
                queries.push(Ipv6Addr::from(first).into());
                queries.push(Ipv6Addr::from(last).into());
            }
        }
    }
    Ok(())
}
