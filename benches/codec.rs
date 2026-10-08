//! Public codec against a benchmark-only fixed layout on sampled cached DB-IP data.

use std::{env, fs::File, hint::black_box, path::Path, time::Duration};

use criterion::Criterion;
use memmap2::MmapOptions;
use netindex::{Limits, codec::Decoder, mmdb};

use support::{schema, schema::decode_native, typed};

mod support;

#[path = "support/fixed_codec.rs"]
mod fixed_codec;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();
    for (kind, variable) in [
        ("asn", "NETINDEX_BENCH_ASN"),
        ("city", "NETINDEX_BENCH_CITY"),
    ] {
        let path = env::var_os(variable)
            .ok_or_else(|| format!("set {variable} to an immutable cached MMDB path"))?;
        compare(&mut criterion, Path::new(&path), kind)?;
    }

    criterion.final_summary();
    Ok(())
}

#[expect(
    clippy::expect_used,
    reason = "timed failures must fail the benchmark process"
)]
#[expect(
    clippy::print_stdout,
    reason = "report record and metadata sizes for the measured sample"
)]
fn compare(criterion: &mut Criterion, path: &Path, kind: &str) -> Result<()> {
    let file = File::open(path)?;
    // SAFETY: Caller-provided cached files must remain immutable throughout the run.
    let native = mmdb::Reader::from_source(unsafe { MmapOptions::new().map(&file)? })?;
    let mut fixed = fixed_codec::Pool::default();
    let mut codec = typed::encoder(kind)?;
    let mut original = Vec::new();
    let mut encoded = Vec::new();
    let mut selected = Vec::new();

    for (position, network) in native.networks(mmdb::WithinOptions::default())?.enumerate() {
        let network = network?;

        if position % 997 != 0 {
            continue;
        }

        let Some(fields) = decode_native(&network, kind)? else {
            continue;
        };

        original.push(fixed.encode(&fields)?);
        encoded.push(typed::encode(&mut codec, &fields)?);
        selected.push(fields);

        if original.len() == 2048 {
            break;
        }
    }

    if original.is_empty() {
        return Err("empty codec benchmark sample".into());
    }

    let fixed_metadata = fixed.metadata(kind)?;
    let fixed_pool = fixed_codec::pool(&fixed_metadata, kind)?;
    let metadata = codec.into_metadata()?;
    let decoder = Decoder::open(&metadata, Limits::default())?;

    for ((fixed, encoded), expected) in original.iter().zip(&encoded).zip(&selected) {
        decoder.record(encoded)?.validate()?;
        let baseline = fixed_codec::decode(fixed, kind, fixed_pool)?;
        let actual = typed::decode(&decoder, encoded, kind)?;

        assert_eq!(&baseline, expected);
        assert_eq!(&actual, expected);
        assert_eq!(serde_json::to_vec(expected)?, serde_json::to_vec(&actual)?);
    }

    println!(
        "{kind}: {} sampled records, fixed {} B / codec {} B; metadata {} B / {} B",
        original.len(),
        original.first().ok_or("empty fixed sample")?.len(),
        decoder.schema().record_length(),
        fixed_metadata.len(),
        metadata.len()
    );

    for json in [false, true] {
        let operation = if json { "json" } else { "fields" };
        criterion.bench_function(&format!("codec/{kind}/{operation}/fixed"), |b| {
            b.iter(|| {
                for bytes in &original {
                    let fields = fixed_codec::decode(black_box(bytes), kind, fixed_pool)
                        .expect("fixed decoding failed");
                    if json {
                        black_box(serde_json::to_vec(&fields).expect("JSON encoding failed"));
                    } else {
                        black_box(fields);
                    }
                }
            })
        });
        criterion.bench_function(&format!("codec/{kind}/{operation}/public"), |b| {
            b.iter(|| {
                for bytes in &encoded {
                    let fields = typed::decode(&decoder, black_box(bytes), kind)
                        .expect("codec decoding failed");
                    if json {
                        black_box(serde_json::to_vec(&fields).expect("JSON encoding failed"));
                    } else {
                        black_box(fields);
                    }
                }
            })
        });
    }

    criterion.bench_function(&format!("codec/{kind}/open/fixed"), |b| {
        b.iter(|| {
            black_box(
                fixed_codec::pool(black_box(&fixed_metadata), kind).expect("fixed opening failed"),
            )
        })
    });
    criterion.bench_function(&format!("codec/{kind}/open/public"), |b| {
        b.iter(|| {
            black_box(
                Decoder::open(black_box(&metadata), Limits::default())
                    .expect("codec opening failed"),
            )
        })
    });

    criterion.bench_function(&format!("codec/{kind}/build/fixed"), |b| {
        b.iter(|| {
            let mut pool = fixed_codec::Pool::default();
            for fields in &selected {
                black_box(pool.encode(fields).expect("fixed encoding failed"));
            }
            black_box(pool.metadata(kind).expect("fixed metadata encoding failed"));
        })
    });
    criterion.bench_function(&format!("codec/{kind}/build/public"), |b| {
        b.iter(|| {
            let mut encoder = typed::encoder(kind).expect("codec schema compilation failed");
            for fields in &selected {
                black_box(typed::encode(&mut encoder, fields).expect("codec encoding failed"));
            }
            black_box(
                encoder
                    .into_metadata()
                    .expect("codec metadata encoding failed"),
            );
        })
    });

    Ok(())
}
