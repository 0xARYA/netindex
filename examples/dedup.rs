//! Reproducible synthetic external-build size and resource comparison.

#[cfg(feature = "external-sort")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::Cursor, time::Instant};

    use netindex::{Builder, ExternalBuilder, ExternalOptions, Limits, Reader, Target};

    let directory = tempfile::tempdir()?;
    let limits = Limits {
        records: 100_000,
        bytes: 64 * 1024 * 1024,
    };
    let options = ExternalOptions {
        run_records: 1024,
        payload_cache_bytes: 4096,
        ..Default::default()
    };
    let mut expected = Builder::new(limits);
    let mut actual = ExternalBuilder::new(directory.path(), limits, options)?;
    let started = Instant::now();

    for id in 0..100_000u32 {
        let key = id % 4096;
        let mut payload = vec![0x5a; 32 + (key % 97) as usize];
        payload
            .get_mut(..4)
            .ok_or("payload buffer")?
            .copy_from_slice(&key.to_le_bytes());
        actual.push(Target::Asn(id + 1), &payload)?;
    }

    let mut output = Cursor::new(Vec::new());
    let stats = actual.write_to(&mut output, b"dedup-v1")?;
    let elapsed = started.elapsed();

    for id in 0..100_000u32 {
        let key = id % 4096;
        let mut payload = vec![0x5a; 32 + (key % 97) as usize];
        payload
            .get_mut(..4)
            .ok_or("payload buffer")?
            .copy_from_slice(&key.to_le_bytes());
        expected.push(Target::Asn(id + 1), &payload)?;
    }
    let expected = expected.into_bytes(b"dedup-v1")?;
    let identical = expected == *output.get_ref();
    let reader = Reader::open(output.into_inner(), limits)?;
    for id in 0..100_000u32 {
        let key = id % 4096;
        let matches = reader.lookup_asn(id + 1)?;
        let row = matches.first().ok_or("missing assertion")?;
        assert_eq!(matches.len(), 1);
        assert_eq!(row.id, id);
        assert_eq!(row.target, Target::Asn(id + 1));
        assert_eq!(row.payload.len(), 32 + (key % 97) as usize);
        assert_eq!(row.payload.get(..4), Some(key.to_le_bytes().as_slice()));
        assert!(
            row.payload
                .get(4..)
                .ok_or("payload tail")?
                .iter()
                .all(|&byte| byte == 0x5a)
        );
    }

    let report = serde_json::json!({
        "fixture": "dedup-v1", "records": stats.records,
        "distinct_payloads": 4096, "artifact_bytes": stats.artifact_bytes,
        "in_memory_bytes": expected.len(), "identical_to_in_memory": identical,
        "temporary_peak_bytes": stats.temporary_peak_bytes,
        "payload_cache_bytes": stats.payload_cache_bytes,
        "build_elapsed_ms": elapsed.as_secs_f64() * 1000.0,
    });
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{report}")?;
    Ok(())
}

#[cfg(not(feature = "external-sort"))]
fn main() {}
