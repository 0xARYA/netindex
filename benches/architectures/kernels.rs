use std::{hint::black_box, time::Instant};
use netindex::{study_available, study_kernel, Builder, Limits};
use crate::{original_target, payload, Failure, VARIANTS};

pub fn fixtures(output: &str) -> Result<(), Failure> {
    let output = std::path::Path::new(output);
    std::fs::create_dir(output)?;
    for (scenario, records) in ["v4", "v6", "asn"].into_iter()
        .flat_map(|s| [1_000_000u32, 20_000_000].map(|n| (s.to_string(), n)))
        .chain(["dense-asn", "sparse-asn", "sparse-v6", "deep-v6", "duplicates-v4"].into_iter()
            .flat_map(|s| ["shared", "unique"].map(|p| (format!("{s}-{p}"), 65_536)))) {
        let mut builder = Builder::new(Limits {records: records as usize, bytes: 2 * 1024 * 1024 * 1024});
        for id in 0..records { builder.push(original_target(&scenario, id), &payload(&scenario, id))?; }
        let metadata = if records == 65_536 { b"shapes-v1".as_slice() } else { b"scale-v1".as_slice() };
        std::fs::write(output.join(format!("{scenario}-{records}.nidx")), builder.into_bytes(metadata)?)?;
    }
    Ok(())
}

pub fn run() -> Result<(), Failure> {
    #[cfg(target_arch="x86_64")]
    assert!(study_available(2), "AVX-512 F/BW must execute on this runner");
    #[cfg(target_arch="aarch64")]
    assert!(study_available(3), "NEON must execute on this runner");
    let modes: Vec<u8> = VARIANTS.iter().filter(|&&v| v != 8).map(|&v| if v == 0 {0} else {(v-799) as u8}).collect();
    let mut checks = 0u64;
    for width in [1usize, 2, 4, 8, 16] {
        let max = u128::MAX >> (128-width*8);
        for count in [0usize,1,2,3,4,7,8,15,16,31,32,33,63,64,65,127,128,129,511] {
            let values: Vec<_> = (0..count).map(|i| max / (count.max(1) as u128) * (i/3) as u128).collect();
            for alignment in 0..64 {
                let mut bytes = vec![0xa5; alignment];
                for value in &values { bytes.extend_from_slice(&value.to_le_bytes()[..width]); }
                for q in values.iter().copied().chain([0, max/2, max/2+1, max, u128::MAX])
                    .flat_map(|q| [q.saturating_sub(1), q, q.saturating_add(1)]) {
                    let expected = values.partition_point(|&v| v <= q);
                    for mode in 0..=3 {
                        assert_eq!(study_kernel(&bytes[alignment..], width, q, mode)?, expected,
                            "width={width}, count={count}, alignment={alignment}, mode={mode}");
                        checks += 1;
                    }
                }
            }
        }
    }
    for mode in 0..=3 {
        for width in [0,3,5,6,7,9,17,usize::MAX] { assert!(study_kernel(&[0;32], width,0,mode).is_err()); }
        for width in [2,4,8,16] { assert!(study_kernel(&[0;31],width,0,mode).is_err()); }
    }
    println!("{}", serde_json::json!({"phase":"kernel_checked", "checks":checks,
        "avx512":study_available(2),"neon":study_available(3),"widths":[1,2,4,8,16],
        "alignments":64,"forced_portable_fallback":true}));
    for width in [1usize,2,4,8,16] {
        let max = u128::MAX >> (128-width*8);
        for count in [32usize,256,4096,65536,1048576] {
            let values: Vec<u128> = (0..count).map(|i| max/(count as u128)*i as u128).collect();
            let mut storage = vec![0xa5];
            for value in &values { storage.extend_from_slice(&value.to_le_bytes()[..width]); }
            let bytes = &storage[1..];
            for pattern in ["uniform","hot"] {
                let queries: Vec<_> = (0..1024usize).map(|i| {
                    let at = (i.wrapping_mul(2654435761)) % if pattern == "hot" { count.min(256) } else {count};
                    values[at].saturating_add((i%2) as u128)
                }).collect();
                let expected: usize = queries.iter().map(|q| values.partition_point(|v| v <= q)).sum();
                for trial in 0..4 {
                    let mut order = modes.clone();
                    if trial%2==1 {order.reverse();}
                    for mode in order {
                        for &q in &queries {black_box(study_kernel(black_box(bytes),width,black_box(q),mode)?);}
                        let started = Instant::now();
                        let mut sum = 0usize;
                        for _ in 0..16 {for &q in &queries {sum += black_box(study_kernel(black_box(bytes),width,black_box(q),mode)?);}}
                        let elapsed = started.elapsed().as_nanos();
                        assert_eq!(sum, expected*16);
                        println!("{}",serde_json::json!({"phase":"kernel_measure","width":width,"count":count,
                            "traffic":pattern,"trial":trial+1,"mode":mode,"queries":queries.len()*16,
                            "elapsed_ns":elapsed,"ns_per_query":elapsed as f64/(queries.len()*16) as f64}));
                    }
                }
            }
        }
    }
    Ok(())
}
