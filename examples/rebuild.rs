//! Replay an immutable index through the external builder, retaining every assertion.
//! Usage: rebuild INPUT OUTPUT TEMP_DIRECTORY [external|external-legacy|partitioned|memory|memory-legacy] [CACHE_BYTES_OR_PARTITIONS]. Files must remain immutable while mapped.

#[cfg(all(feature = "external-sort", feature = "mmap"))]
#[expect(
    clippy::print_stdout,
    reason = "emit benchmark phases and measured results"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        env,
        fs::{File, OpenOptions},
        net::IpAddr,
        time::Instant,
    };

    use netindex::{Builder, ExternalBuilder, ExternalOptions, Limits, MappedReader};

    let arguments: Vec<_> = env::args_os().skip(1).collect();
    let [input, output, temporary, ..] = arguments.as_slice() else {
        return Err("usage: rebuild INPUT OUTPUT TEMP_DIRECTORY [external|external-legacy|partitioned|memory|memory-legacy] [CACHE_BYTES_OR_PARTITIONS]".into());
    };
    let mode = arguments
        .get(3)
        .and_then(|arg| arg.to_str())
        .unwrap_or("external");
    let disk_budget = arguments
        .get(4)
        .map(|arg| {
            arg.to_str()
                .ok_or("invalid disk cache budget")?
                .parse::<usize>()
                .map_err(|_| "invalid disk cache budget")
        })
        .transpose()?
        .unwrap_or(if mode == "partitioned" { 16 } else { 0 });
    if arguments.len() > 5
        || (disk_budget != 0 && !mode.starts_with("external") && mode != "partitioned")
        || !matches!(
            mode,
            "external" | "external-legacy" | "partitioned" | "memory" | "memory-legacy"
        )
    {
        return Err("invalid build mode".into());
    }
    let limits = Limits {
        records: 30_000_000,
        bytes: 2 * 1024 * 1024 * 1024,
    };
    let options = ExternalOptions {
        temporary_bytes: 8 * 1024 * 1024 * 1024,
        ..Default::default()
    };

    let started = Instant::now();
    // SAFETY: this benchmark requires immutable private input for the mapping's lifetime.
    let original = unsafe { MappedReader::map_file(&File::open(input)?, limits) }?;
    let input_open_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut assertions = Vec::new();
    assertions.try_reserve_exact(original.len())?;
    original.visit_all(|row| {
        assertions.push(row);
        Ok::<_, netindex::Error>(())
    })?;
    assertions.sort_unstable_by_key(|row| row.id);
    for (id, row) in assertions.iter().enumerate() {
        if row.id != u32::try_from(id)? {
            return Err("input insertion IDs are not a complete permutation".into());
        }
    }
    println!(
        "{}",
        serde_json::json!({"phase":"prepared", "records":assertions.len(),
        "input_open_ms":input_open_ms, "prepare_ms":started.elapsed().as_secs_f64()*1000.0,
        "run_records":options.run_records,"payload_cache_bytes":options.payload_cache_bytes,
        "temporary_limit_bytes":options.temporary_bytes})
    );

    let directory = tempfile::tempdir_in(temporary)?;
    let mut output_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let started = Instant::now();
    let (stats, cache_bytes) = if mode.starts_with("external") || mode == "partitioned" {
        let mut builder = if mode == "external-legacy" {
            ExternalBuilder::new_legacy(directory.path(), limits, options)?
        } else if mode == "partitioned" {
            ExternalBuilder::new_partitioned(directory.path(), limits, options, disk_budget)?
        } else {
            ExternalBuilder::new(directory.path(), limits, options)?
        };
        if mode != "partitioned" {
            builder.set_disk_table_cache(disk_budget)?;
        }
        for row in &assertions {
            builder.push(row.target, row.payload)?;
        }
        let reserved = builder.disk_table_cache_bytes();
        (
            Some(builder.write_to(&mut output_file, original.metadata()?)?),
            reserved,
        )
    } else {
        let mut builder = Builder::new(limits);
        for row in &assertions {
            builder.push(row.target, row.payload)?;
        }
        if mode == "memory-legacy" {
            builder.write_legacy_to(&mut output_file, original.metadata()?)?;
        } else {
            builder.write_to(&mut output_file, original.metadata()?)?;
        }
        (None, 0)
    };
    let artifact_bytes = output_file.metadata()?.len();
    let build_ms = started.elapsed().as_secs_f64() * 1000.0;
    drop(output_file);
    println!(
        "{}",
        serde_json::json!({"phase":"built", "build_ms":build_ms,
        "mode":mode,"records":assertions.len(),"artifact_bytes":artifact_bytes,
        "temporary_peak_bytes":stats.map(|stats| stats.temporary_peak_bytes),
        "disk_cache_budget":if mode == "partitioned" {0} else {disk_budget},
        "partitions":if mode == "partitioned" {Some(disk_budget)} else {None},"disk_cache_bytes":cache_bytes,
        "payload_cache_bytes":stats.map(|stats| stats.payload_cache_bytes)})
    );

    let started = Instant::now();
    // SAFETY: the new private output is closed and remains immutable during verification.
    let rebuilt = unsafe { MappedReader::map_file(&File::open(output)?, limits) }?;
    let output_open_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(original.metadata()?, rebuilt.metadata()?);
    assert_eq!(assertions.len(), rebuilt.len());
    let mut seen = Vec::new();
    seen.try_reserve_exact(assertions.len())?;
    seen.resize(assertions.len(), false);
    let mut verified = 0usize;
    rebuilt.visit_all(|row| -> Result<(), Box<dyn std::error::Error>> {
        let expected = assertions.get(row.id as usize).ok_or("unexpected ID")?;
        let visited = seen.get_mut(row.id as usize).ok_or("unexpected ID")?;
        assert!(!*visited, "duplicate insertion ID");
        *visited = true;
        assert_eq!(row.target, expected.target);
        assert_eq!(row.payload, expected.payload);
        verified += 1;
        Ok(())
    })?;
    assert_eq!(verified, assertions.len());

    let edges: [IpAddr; 5] = [
        "0.0.0.0".parse()?,
        "255.255.255.255".parse()?,
        "::".parse()?,
        "::ffff:192.0.2.1".parse()?,
        "2002:c000:201::".parse()?,
    ];
    let mut random = 0x73c8_6af2_3e1d_0945u64;
    for index in 0..100_005 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let query = if let Some(&edge) = edges.get(index) {
            edge
        } else if index % 2 == 0 {
            IpAddr::from(std::net::Ipv4Addr::from(random as u32))
        } else {
            IpAddr::from(std::net::Ipv6Addr::from(
                (u128::from(random) << 64) | u128::from(random.rotate_left(29)),
            ))
        };
        let expected = original.lookup_ip(query)?;
        let actual = rebuilt.lookup_ip(query)?;
        assert_eq!(expected.len(), actual.len());
        for (left, right) in expected.iter().zip(&actual) {
            assert_eq!(
                (left.id, left.target, left.payload),
                (right.id, right.target, right.payload)
            );
        }
    }
    println!(
        "{}",
        serde_json::json!({"phase":"verified", "assertions":verified,
        "queries":100_005,"output_open_ms":output_open_ms,
        "verification_ms":started.elapsed().as_secs_f64()*1000.0,
        "scope":"opaque assertion replay; full ID/target/payload/metadata identity and sampled all-match query ordering; excludes native conversion and fsync"})
    );
    Ok(())
}

#[cfg(not(all(feature = "external-sort", feature = "mmap")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("enable external-sort and mmap".into())
}
