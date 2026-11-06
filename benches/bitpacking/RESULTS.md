# P1-15: exact-bit compaction - 2026-11-05

Implemented opt-in `Reader::write_bitpacked_to`, plus format-6 validation and
random access. Ordinary builders keep their format-3/4 policy. Apply this to
in-memory, external or partitioned output; it also preserves format-5 aliases.
It emits smaller output or copies the original bytes unchanged. No new production
dependency, decompression cache, worker threads or temporary files are required.

The writer uses a 4,112-byte block buffer and the standard 8 KiB buffered output,
besides small fixed headers and its already-open source reader. This is a scratch
bound from the implementation, not a measured whole-process RSS claim. The reader
retains the existing validation allocation and coverage structures.

## Size and complete-file verification

| Input | Before bytes | After bytes | Saved MiB | Reduction |
| --- | ---: | ---: | ---: | ---: |
| Materialized City | 164,836,510 | 144,169,068 | 19.71 | 12.54% |
| Canonical City base | 62,361,502 | 56,617,042 | 5.48 | 9.21% |
| City with shared aliases | 62,361,614 | 56,617,154 | 5.48 | 9.21% |
| IPtoASN | 27,840,796 | 27,840,796 | 0.00 | 0.00% |

All four final replays verified every assertion's insertion ID, original target,
payload and metadata, plus 100,005 sampled all-match queries and result ordering.
Counts are 20,255,722 materialized City assertions, 5,724,698 canonical assertions,
20,255,722 logical shared-alias assertions and 719,149 IPtoASN assertions.
IPtoASN uses interval sections and stayed byte-identical. Initial recompaction
checks were byte-identical; final-layout output matched those artifacts exactly.
Unit tests also require idempotence, both alias/compaction orders to produce the
same bytes, and identical compaction across all three builder choices.

Final construction-only observations were 272.83 ms materialized City, 76.24 ms
canonical, 123.47 ms aliases and 36.47 ms IPtoASN. These are single observations,
not performance estimates. They exclude opening/preparing input, complete output
verification, native provider conversion and fsync. The rebuild helper materializes
assertions for verification; its memory use does not describe the compaction API.

## Public-reader CPU tradeoff

Ryzen 9 9950X3D2, WSL Ubuntu 24.04, warm ext4 files, CPU 20, Rust 1.95.0 / LLVM
22.1.2, normal portable release settings. Baseline library: `9b2a2b6`.
Three serial process rounds, reversed process order in the middle round, each
with five samples per phase: 15 samples per cell. No own compilation overlapped
these measurements. The machine had substantial unrelated load and timing drift.
ASN bytes and execution paths provide an unchanged-data noise control.

Each lookup sample delivers and consumes 131,072 queries at 4,096 deterministic
existing assertion starts. It is a warm hit-oriented probe, not representative
traffic, misses, batch throughput, cold mmap or an HTTP benchmark. Opening includes
complete validation; visiting emits every assertion and consumes ID/payload length.
Baseline and final helpers use identical public-reader measurement code; the
baseline was captured before the later microbenchmark dispatch refactor.

Median milliseconds [minimum-maximum]:

| Input | Library / file | Open | Visit all | 131,072 lookups |
| --- | --- | ---: | ---: | ---: |
| city | Before / original | 592.70 [447.76-744.13] | 772.39 [618.51-972.06] | 36.74 [30.76-74.17] |
| city | After / original | 474.52 [361.21-607.29] | 738.01 [554.07-854.26] | 40.25 [30.88-78.89] |
| city | After / compact | 646.32 [521.09-712.55] | 943.24 [739.58-1028.17] | 42.20 [32.17-82.96] |
| canonical | Before / original | 185.05 [122.81-247.29] | 221.70 [173.73-268.36] | 27.86 [23.14-38.52] |
| canonical | After / original | 116.93 [102.93-203.74] | 193.86 [146.98-219.63] | 27.66 [21.28-45.16] |
| canonical | After / compact | 193.80 [157.03-279.30] | 280.61 [250.32-366.41] | 34.23 [23.38-51.51] |
| asn | Before / original | 22.32 [15.76-32.30] | 20.98 [17.46-35.94] | 82.16 [55.06-128.44] |
| asn | After / original | 22.56 [16.34-49.67] | 20.16 [17.09-46.38] | 70.63 [48.17-113.02] |
| asn | After / compact | 22.00 [16.13-35.48] | 25.38 [17.18-36.15] | 69.56 [47.06-122.70] |

Within the new reader, compacting City increased median lookup time by 4.8%,
opening by 36.2% and visiting by 27.8%; canonical lookup increased by 23.8%, opening
by 65.7% and visiting by 44.7%. These are observed ratios under variable load,
not universal predictions. ASN's identical files also varied. Use compaction when
its storage/memory benefit is worth the CPU cost; it is deliberately opt-in.

Ordinary byte-column scans dispatch width once per block and then traverse typed
slices. This avoids repeated integer-width dispatch without changing encoded bytes.
Separate `perf stat` process totals corroborate less work on ordinary inputs:
City instructions 115.45 to 107.90 billion (-6.5%), canonical 33.14 to 31.02 billion
(-6.4%). Compact files instead used 131.04 and 37.53 billion, about 21% more than
the new reader's ordinary files. These totals include setup and all five opens,
visits and lookup samples; they do not isolate lookup instructions. Do not claim
an ordinary-file lookup improvement from these data.

## Column kernels, SIMD and assembly

The study checks independent expected values and predecessor results at each
value, adjacent values and extrema. Synthetic widths cover 0-128 bits and partial
blocks; real columns are sampled deterministically from the retained files.
Timing dispatch is outside the query loop. Each method has three rounds of
262,144 queries, seeded query order, consumed checksums and a reversed middle round.
`hybrid-direct` retains native byte access when padded bitpacking would be larger.
Microkernel timings exclude public reader/fence/payload/validation work.

For portable synthetic 12-bit columns: detected byte search 8.04 ns/query,
bit-direct 11.38, full scalar decode 205.94 and BP8x absolute decode 19.04.
Portable canonical IPv4: 16.56, 19.38, 220.62 and 31.71 respectively. Complete
256-value decoding per single query does not justify replacing direct probes.

The optional study dependency is pinned
[Quickwit bitpacking 0.9.3](https://github.com/quickwit-oss/bitpacking).
Its BP8x layout is incompatible with the horizontal format proposed here; its
256-value `u32` SIMD path qualifies only that subset, not partial blocks or full
IPv6 deltas. Sorted coding also requires prefix reconstruction.

Linked assembly was inspected for the production bitpacked upper bound, direct
column probes, decode wrapper and differential integration. Production uses
bounded scalar loads/shifts/masks, with no decoded-block allocation. The tested
portable differential implementation retained out-of-line vector-intrinsic calls.
A separate `-Ctarget-cpu=native` build removed the standalone integration symbol
and was measured too. In its canonical IPv4 comparison, detected byte/direct-bit/
absolute SIMD/differential SIMD medians were 16.47/21.53/29.46/111.11 ns/query.
The final native/portable build controls ran serially after other own work. The
portable control gave 21.78/22.69/35.55/1,231.01 ns/query. This corroborates the
differential decoder's build sensitivity; unrelated load still varied, so these
wall times do not establish exact cross-build speedups. Native whole-block
decoding still lost to direct access in this workload.

No SIMD decoder or dependency was added to the library. Existing runtime SIMD
continues on ordinary columns. Locality-amortized batch decoding, decode caches,
PGO, cold storage and native Windows/ARM/AVX-512 qualification remain unmeasured
for this format. No claim of superiority to MMDB or other formats follows.

## Compatibility and checks

[FORMAT.md](../../FORMAT.md) defines format 6: exact-bit marker, count/width fields,
least-significant-bit packing, 16 zero padding bytes and zero unused tail bits.
Native columns coexist with bit columns; metadata, payload references and IDs do
not change. Older readers reject format 6, including when embedded in format 5.
The captured old reader explicitly rejected the independent format-6 fixture with
`Version(6)`. Upgrade readers before deploying opt-in output. Ordinary and legacy
writers retain their prior selection; requesting legacy output does not authorize
subsequent compaction for old consumers. Format 6 remains in unreleased 3.0.0.
This records P1-16's compatibility decision for P1-15.

All seven CONTRIBUTING checks passed on Windows, including default/all-feature
Clippy and tests, ignored tests, warnings-as-errors docs and package verification.
Independent IPv4/IPv6 format-6 fixtures cover partial columns, 127-bit crossings,
malformed padding/tails/counts/shifts, old-version markers and truncation. Unit
checks exhaust all bit widths 0-127; integration checks preserve explicit IDs,
payload encodings, alias semantics, failed output causes and all builder paths.
Linux AddressSanitizer passed 29 unit and four integration tests. Seeded nightly
fuzz campaigns completed 11,461,224 read runs and 12,808 encode runs, 61 seconds
each, with no findings. Encoding now checks ordinary and compacted roundtrips
against the original-input oracle; CI seeds both fuzz targets. Short campaigns
are smoke coverage, not proof of exhaustive correctness.

## Reproduction and receipts

```sh
cargo +1.95.0 build --locked --release --all-features --example rebuild
target/release/examples/rebuild INPUT.nidx OUTPUT.nidx SCRATCH_DIR bitpacked
cargo +1.95.0 build --locked --release --manifest-path benches/bitpacking/Cargo.toml
taskset -c 20 benches/bitpacking/target/release/netindex-bitpacking-study 262144 INPUT.nidx
taskset -c 20 benches/bitpacking/target/release/netindex-bitpacking-study reader INPUT.nidx OUTPUT.nidx
# Separate build directory for native-CPU control:
CARGO_TARGET_DIR=target/bitpacking-native RUSTFLAGS='-Ctarget-cpu=native' \
  cargo +1.95.0 build --locked --release --manifest-path benches/bitpacking/Cargo.toml
```

The microkernel input parser accepts original formats 3/4; use `reader` for
compacted/enveloped files. For a before/after reader comparison, build this helper
against the library at `9b2a2b6` and the current tree. Native CPU flags belong to
the benchmark application and are not a new library requirement.

[Raw receipts](raw/) include all final reader/microkernel rows, process counters,
full replay receipts, input/output/executable hashes, exact compiled core-source
hashes, portable/native assembly excerpts, sanitizer and fuzz logs. Summary and
ranges are in [summary.json](raw/summary.json). Inputs remain private retained
fixtures; they are not shipped or downloadable. Source hashes describe the WSL
compiled snapshot; subsequent changes only added an alias-composition assertion
and updated API documentation. New test/check receipts accompany the final tree.

Log/assembly trailing whitespace is normalized for repository checks.
