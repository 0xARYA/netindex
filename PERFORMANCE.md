# Performance audit: 2026-11-04

Positioned temporary-file I/O reduced the complete-deduplication builder's median
time by 25.5% on the measured fixture, without changing output bytes or logical
resource usage. Width-specialized AVX2 kernels improved the optional packed-search
path, but scalar search still generally won. PGO substantially improved synthetic
opening/validation; it remains a consuming-application choice.

[Measurements, confidence intervals, hashes, and provenance](benches/results/2026-11-04-performance-audit.json).
The earlier [deduplication size study](benches/results/2026-11-04-external-dedup.json)
compares against the cache-only implementation and answers a different question.

## Method and limits

Windows x86_64 MSVC, Ryzen 9 9950X3D2, Rust 1.95.0, LLVM 22.1.2. Final timings
ran serially on logical CPU 20 (affinity mask 1048576), without overlapping builds
or profilers. Criterion used 20 samples, 0.3 s warmup and 0.7 s measurement per
scenario. Lookup batches contain 1,024 queries over 65,536 assertions. These are
short, warm synthetic studies on a live desktop, with two passes in changed order;
they do not establish production latency or universal rankings. Preserve both
passes rather than selecting the fastest result.

Lookup/open inputs are already in memory. External builds create fresh temporary
files, with uncontrolled operating-system caching.

An earlier CPU 2 width study showed multi-fold variation in the unchanged
16-byte control. That entire study is excluded from the conclusions. The later
CPU 20 runs are more stable but still variable. Frequency, sibling-core activity,
filesystem cache, and other programs were not fully controlled. Hardware cycle,
branch, and cache counters were not collected.

The JSON records source, executable, and deterministic input hashes. The
complete-deduplication precursor was uncommitted; its source package, binaries,
PDBs, raw ETW captures, disassembly, and Criterion samples remain locally under
`target/perf-study`, `target/dedup-*-profile.json.gz`, and `target/criterion`.
Those ignored artifacts are not included in the repository. The report retains
their hashes, selected observations, and Criterion mean confidence intervals.

No real City conversion, cold-file benchmark, concurrent-reader test, application
HTTP benchmark, or ARM/AVX-512 runtime measurement was performed in this audit.
The minimum converted size remains unmeasured.

## Profile-guided I/O improvement

Eight whole-program ETW captures before and after the change identified avoidable
seek operations in disk-table probes and payload comparisons. The baseline had
771 inclusive samples containing `SetFilePointerEx`; the positioned version had
none. `NtSetInformationFile` appeared in 624 versus 6 inclusive samples. These
overlapping sample counts are diagnostic, not elapsed-time percentages. Captures
include setup and the independent in-memory oracle, were not CPU-pinned, and the
second capture may overlap independent cross-compilation. Use the separate timing
trials for speed claims.

Table cells now use absolute-offset reads and writes. Payload comparisons use an
independently opened reader, compare against already-buffered bytes when possible,
and flush only if a comparison crosses the persisted/buffered boundary. Separate
handles matter on Windows: its positioned file operations change the cursor.
Retries handle short I/O and interruptions; full byte comparison remains the
authority even when hashes collide. Table growth and temporary-disk accounting
are unchanged.

Five baseline/candidate pairs on the same 100,000-record fixture gave medians
628.278 ms and 467.814 ms. Both wrote exactly the in-memory builder's 1,926,943
bytes, with 9,926,855 logical peak temporary bytes and 4,086 retained payload-key
bytes. Every lookup also passed the independent input oracle. The cache-only
implementation is still faster on this fixture, but does not fully deduplicate;
this optimization compares implementations with identical complete-deduplication
semantics. Logical bytes exclude allocator overhead, RSS, and filesystem cache.

```powershell
$env:CARGO_PROFILE_RELEASE_DEBUG = '1'
cargo +1.95.0 build --locked --release --features external-sort --example dedup
$taskRunner = [System.Diagnostics.Process]::GetCurrentProcess()
$taskRunner.ProcessorAffinity = [IntPtr]1048576
& target/release/examples/dedup.exe
samply record --save-only --unstable-presymbolicate --iteration-count 8 --output target/dedup-profile.json.gz -- target/release/examples/dedup.exe
Remove-Item Env:CARGO_PROFILE_RELEASE_DEBUG
```

Run baseline and candidate serially with the same input and affinity. This
example times pushes and finalization; setup, validation, and the oracle are
outside the timer. Profiling and timing are separate runs. The fixture deliberately
exceeds its small key cache; it is not a measured provider payload distribution.

## Assembly and SIMD

The old AVX2 function handled runtime widths and performed integer division,
runtime integer decoding, and a 128-bit scalar comparison while narrowing the
search. Four const-width kernels now narrow using unsigned 64-bit comparisons.
Compiler assembly has no division in these kernels and uses the expected
`vpcmpgtb/w/d/q`, `vpmovmskb`, and `vzeroupper`. Disassembly of the linked benchmark,
resolved with its PDB, confirms those four kernels are present. Loads remain
unaligned and bounded by the complete column. Runtime AVX2 detection and the
scalar fallback are retained; the 16-byte path is unchanged.

Mean ns/query, original AVX2 / specialized AVX2 / scalar, in the two passes:

| Family / width | Pass 1 | Pass 2 |
| --- | ---: | ---: |
| IPv4 / 1 | 56.7 / 51.2 / 43.8 | 38.0 / 36.0 / 44.7 |
| IPv4 / 2 | 55.7 / 40.7 / 37.9 | 43.1 / 39.6 / 38.8 |
| IPv4 / 4 | 50.4 / 40.7 / 38.3 | 49.1 / 40.7 / 38.9 |
| IPv6 / 1 | 43.3 / 40.3 / 39.5 | 42.1 / 39.5 / 39.7 |
| IPv6 / 2 | 48.3 / 44.6 / 41.8 | 48.5 / 43.8 / 41.8 |
| IPv6 / 4 | 50.6 / 45.7 / 42.2 | 50.6 / 44.5 / 42.2 |
| IPv6 / 8 | 53.3 / 46.6 / 42.4 | 51.1 / 45.5 / 42.7 |
| IPv6 / 16 | 44.8 / 44.3 / 43.5 | 44.2 / 43.3 / 43.5 |

The 1-byte IPv4 result is particularly variable. The 4-byte IPv4 kernel improved
17–19%, and the IPv6 1–8-byte kernels improved roughly 6–12%. This supports keeping
the optional specialization, not enabling SIMD search by default. The new
`simd-utf8` feature allows `shared-values`/`codec` users to retain accelerated
string validation while selecting scalar search. Existing `simd` enables both.

The new `search` benchmark covers actual encoded widths 1/2/4 for IPv4 and
1/2/4/8/16 for IPv6, with deterministic hits and gaps. Existing randomized parity
tests cover alignments, lengths, and unsigned boundaries. CI checks both search
variants plus UTF-8 SIMD with scalar search.

```powershell
cargo +1.95.0 bench --locked --bench search -- --test
cargo +1.95.0 bench --locked --features simd --bench search -- --test
cargo +1.95.0 rustc --locked --release --features simd,external-sort --lib -- --emit=asm
```

Inspect the latest `target/release/deps/netindex-*.s`. For linked code, use
`llvm-objdump --disassemble --demangle --x86-asm-syntax=intel EXE`; on Windows keep
the matching PDB beside the executable and use `llvm-symbolizer --relative-address`
for addresses relative to its image base. Presence of vector instructions alone
is not evidence of a speedup.

## Should an application use PGO?

It merits a trial, especially if validated opening or JSON generation matters.
The final study compared normal bench optimization, explicit ThinLTO, and PGO
without added ThinLTO. All used the same final search implementation and
`CARGO_PROFILE_BENCH_DEBUG=1`. Normal Cargo optimization is the control; it was
not explicitly built with `lto=off`. The bench profile inherits the repository's
existing release setting `codegen-units=1`.

Relative time change from the corresponding control, pass 1 / pass 2:

| Workload | ThinLTO | PGO |
| --- | ---: | ---: |
| Open ASN | -3.4% / +19.0% | -25.9% / -26.1% |
| Open IPv4 | +8.2% / -1.7% | -62.3% / -63.3% |
| Open IPv6 | +1.9% / -1.6% | -64.5% / -65.2% |
| Open wide IPv6 | +1.7% / -2.0% | -65.1% / -65.7% |
| Lookup IPv4 | +32.4% / -3.3% | -2.9% / -29.3% |
| Lookup IPv6 | -3.0% / -3.0% | -6.6% / -3.7% |
| Lookup + JSON IPv4 | -3.7% / -8.2% | -19.5% / -23.7% |

ThinLTO is mixed, and lookup-only improvements vary. Opening validates bytes
already in memory; these numbers exclude storage I/O. Training and measurement
are separate runs of the same synthetic distributions. The training filter
times lookup/open only, but setup correctness checks also execute JSON code;
the JSON measurements are not wholly unseen workloads. The compatible
profile-use build emitted no missing-profile warnings. Profile-use release
roundtrip, malformed-payload, and overlap/routing tests also passed.

Benchmark executable sizes were 2,474,496 bytes (control), 2,567,680 (ThinLTO),
and 2,106,368 (PGO). Debug symbols are separate PDBs. These sizes include the
benchmark driver and do not establish an application's footprint.

For a real application, follow the [Rust PGO workflow](https://doc.rust-lang.org/nightly/rustc/profile-guided-optimization.html):
instrument, train on representative traffic, merge with the matching toolchain's
`llvm-profdata`, then rebuild with profile-use. Validate on an independent traffic
mix, including misses, overlaps, wide IPv6, actual payload decoding, startup, and
error paths. Keep target and other codegen flags consistent; monitor missing-profile
warnings and retrain after code/compiler/workload changes. Compare binary size,
startup, throughput, and tail latency before adopting it. PGO plus ThinLTO was
not measured here.

Minimal Windows reproduction of this benchmark experiment:

```powershell
rustup component add llvm-tools-preview --toolchain 1.95.0
$taskTarget = 'x86_64-pc-windows-msvc'
$taskStudy = Join-Path (Get-Location) 'target/pgo-study'
$env:CARGO_PROFILE_BENCH_DEBUG = '1'
$env:RUSTFLAGS = "-Cprofile-generate=$taskStudy/raw"
cargo +1.95.0 bench --locked --target $taskTarget --target-dir $taskStudy --features simd --bench index --no-run
$taskExecutable = Get-ChildItem "$taskStudy/$taskTarget/release/deps/index-*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $taskExecutable.FullName --bench --profile-time 1 '^(lookup|open)/'
$taskProfdata = Join-Path (rustc +1.95.0 --print sysroot) "lib/rustlib/$taskTarget/bin/llvm-profdata.exe"
$taskRaw = Get-ChildItem "$taskStudy/raw/*.profraw"
& $taskProfdata merge "-o=$taskStudy/merged.profdata" $taskRaw.FullName
$env:RUSTFLAGS = "-Cprofile-use=$taskStudy/merged.profdata -Cllvm-args=-pgo-warn-missing-function"
cargo +1.95.0 bench --locked --target $taskTarget --target-dir $taskStudy --features simd --bench index --no-run
Remove-Item Env:RUSTFLAGS, Env:CARGO_PROFILE_BENCH_DEBUG
```

Copy the resulting executable before another build and run paired timed trials
separately from instrumentation. Use explicit `--target` so host build scripts
and procedural macros are not instrumented. Preserve/restore existing environment
flags if adapting this recipe. Real applications should train their application
binary with their intended settings, rather than use this benchmark's profile.
[Cargo takes profile settings from the consuming workspace](https://doc.rust-lang.org/cargo/reference/profiles.html);
the library does not ship a trained profile or dictate application release policy.

## Related projects and adoption decisions

These are design comparisons and proposed experiments, not cross-project speed
claims. No third-party implementation code was copied.

| Project | Useful approach | Decision for netindex |
| --- | --- | --- |
| [IP2Location Go](https://github.com/ip2location/ip2location-go/blob/master/ip2location.go) | `ReaderAt` access and a prefix directory around binary search. | Positioned external-build I/O is now measured and adopted. Existing optional address directories remain workload-dependent. |
| [Abseil Swiss tables](https://abseil.io/about/design/swisstables) | Compact control bytes filter candidate hash slots with SIMD before full equality. | Consider grouped metadata for a budgeted disk-table page cache; retain exact payload equality. Reorganizing memory metadata alone will not eliminate disk probes. |
| [DuckDB external aggregation](https://duckdb.org/2024/03/29/external-aggregation) | Partitioning and spillable pages bound larger-than-memory aggregation. | Prototype partitioned/sequential dedup if real builds remain I/O-bound; preserve first-seen offsets, insertion IDs, cleanup and the total temporary-disk limit. |
| [Velox hash table](https://facebookincubator.github.io/velox/develop/hash-table.html) | Vectorized lookup across batches and compact bucket metadata. | A benchmark-only batch-query prototype could interleave independent searches and expose latency hiding before adding an API. Do not replace all-match semantics with single-result lookup. |
| [simdjson dispatch](https://github.com/simdjson/simdjson/blob/master/doc/implementation-selection.md), [memchr](https://github.com/BurntSushi/memchr) | Portable baseline with runtime-selected architecture-specific kernels and broad correctness testing. | Retain runtime dispatch and scalar fallback; expand tests before adding an architecture. The existing UTF-8 dependency already supplies additional architecture coverage. |
| [Quickwit bitpacking](https://github.com/quickwit-oss/bitpacking) | SIMD decoding of fixed-size compressed integer blocks. | Measure block-decode costs against direct borrowed access first. Its different SIMD layouts are not interchangeable; adoption would require a deliberate format/compatibility decision. |
| [RocksDB partitioned indexes](https://github.com/facebook/rocksdb/wiki/Partitioned-Index-Filters) | Partition metadata and cache it in bounded pieces. | Consider a budgeted page cache after measuring the real dedup probe working set; account cached bytes explicitly. |
| [MaxMind DB](https://maxmind.github.io/MaxMind-DB/) | Pointer sharing reuses subtrees and mapped IPv4 data. | Alias sharing is a format-level opportunity, separate from payload deduplication. Any design must preserve original targets, insertion IDs, duplicates and all matches. |

The next useful evidence is real input payload cardinality, size distribution,
repeat distance, and time split between decoding, payload comparisons, sorting,
and writing. Then test page caching or partitioned dedup on unchanged input.
Compact payload references and shared aliases need separate format proposals.
Additional 128-bit, AVX-512, or NEON search should wait for repeatable gains on
hardware running the full lookup workload. An ARM compile check establishes
portability, not performance.

The measurement hierarchy follows [Abseil's microbenchmark guidance](https://abseil.io/fast/39):
inspect hot code and use narrow experiments, then validate the result in larger
workloads. Neither vector instructions nor a winning synthetic case establishes
that the library is maximally optimized.

## Verification

Observed passes: formatting; Clippy for default and all features with warnings
denied; default, all-feature (including ignored), and `shared-values,simd-utf8`
tests; scalar and SIMD all-width benchmark correctness; documentation with
warnings denied; and local all-feature packaging with `--allow-dirty`.
All-feature library cross-checks passed for Linux x86_64 and Windows ARM64.
Profile-use release roundtrip, payload, and routing tests passed separately.
Remote CI and non-Windows runtime tests were not run here.
