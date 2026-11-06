//! Reproducible synthetic external-build size and resource comparison.

use std::{
    io::{Cursor, Write},
    time::Instant,
};

use netindex::{Builder, ExternalBuilder, ExternalOptions, Limits, Reader, Target};

const RECORDS: u32 = 100_000;
const DISTINCT_PAYLOADS: u32 = 4096;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let limits = Limits {
        records: RECORDS as usize,
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

    for id in 0..RECORDS {
        actual.push(Target::Asn(id + 1), &payload(id)?)?;
    }

    let mut output = Cursor::new(Vec::new());
    let stats = actual.write_to(&mut output, b"dedup-v1")?;
    let elapsed = started.elapsed();

    for id in 0..RECORDS {
        expected.push(Target::Asn(id + 1), &payload(id)?)?;
    }
    let expected = expected.into_bytes(b"dedup-v1")?;
    if expected != *output.get_ref() {
        return Err("external artifact differs from in-memory artifact".into());
    }

    let reader = Reader::open(output.into_inner(), limits)?;
    for id in 0..RECORDS {
        let key = id % DISTINCT_PAYLOADS;
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
        "distinct_payloads": DISTINCT_PAYLOADS, "artifact_bytes": stats.artifact_bytes,
        "in_memory_bytes": expected.len(), "identical_to_in_memory": true,
        "temporary_peak_bytes": stats.temporary_peak_bytes,
        "payload_cache_bytes": stats.payload_cache_bytes,
        "build_elapsed_ms": elapsed.as_secs_f64() * 1000.0,
    });
    writeln!(std::io::stdout().lock(), "{report}")?;
    Ok(())
}

fn payload(id: u32) -> Result<Vec<u8>, &'static str> {
    let key = id % DISTINCT_PAYLOADS;
    let mut bytes = vec![0x5a; 32 + (key % 97) as usize];
    bytes
        .get_mut(..4)
        .ok_or("payload buffer")?
        .copy_from_slice(&key.to_le_bytes());
    Ok(bytes)
}
