# P1-08: native AVX-512 and NEON qualification

Qualified on 2026-11-05 UTC. Both implementations executed on their native ISA;
these are runtime measurements, not cross-compilation results. Keep the tested
implementations as research. AVX-512 has workload-dependent gains and regressions;
packed-mask NEON is promising for hot IP lookups but needs broader workload and
microarchitecture qualification before a library change.

## Hardware and method

| Host | Execution environment | Affinity | Runtime feature assertion |
| --- | --- | --- | --- |
| AMD Ryzen 9 9950X3D2, 16 cores/32 threads | Ubuntu 24.04, WSL2 / Microsoft hypervisor, x86-64 | CPU 20 of 0–31 | AVX-512F and BW |
| ARM Neoverse-N2, 4 virtual CPUs, 128 MiB reported shared L3 | GitHub `ubuntu-24.04-arm`, native AArch64, little endian | CPU 3 of 0–3 | NEON / ASIMD |

The [ARM run succeeded](https://github.com/0xARYA/netindex/actions/runs/41988692543)
at `707ddb163b4c046a3859ed93b5fb33cc5878c1b9`. Both receipts identify that
revision. Independently reconstructed generated code matches both snapshots
after normalizing Windows/Linux source line endings; production sources match
their original receipts. No production code, format, public API or version changed.

Rust 1.95.0 / LLVM 22.1.2, portable release, one codegen unit, `simd` enabled,
no custom target flags, PGO or LTO. Warm operation only. Each case has two fresh
processes, four alternating-order trials per process and a per-method warmup.
Affinity pins the measuring thread; it does not isolate the host or control
frequency, temperature, scheduling noise or shared-cache contention.

Controls are the ordinary complete visitor (AVX2 on x86, scalar packed search
on ARM), eight-query interleaving, and forced scalar packed search. Candidates
replace only the packed predecessor tail: AVX-512 for widths 1/2/4/8/16, and
NEON byte-scan or packed-mask reduction for widths 1/2/4/8. NEON width 16 and
unavailable ISAs take scalar fallbacks. ASN and overlapping-target paths remain
controls; differences there are noise, not SIMD gains. Runtime guard/dispatch
cost is included. The ordinary reader caches its existing detection; the study
visitor constructs its search selection per query.

## Complete visitor results

Nanoseconds per query, median of eight trials. Speedup is ordinary/candidate;
values below 1 mean a regression. Compare controls within a host.

| Host and case | Ordinary | Candidate | Speedup |
| --- | ---: | ---: | ---: |
| AVX-512: IPv4 1M, uniform | 62.03 | 60.22 | 1.03× |
| AVX-512: IPv4 20M, uniform | 176.67 | 142.89 | 1.24× |
| AVX-512: IPv4 20M, hot | 38.58 | 40.36 | 0.96× |
| AVX-512: IPv6 1M, uniform | 107.24 | 104.57 | 1.03× |
| AVX-512: IPv6 20M, uniform | 291.60 | 318.15 | 0.92× |
| AVX-512: sparse IPv6, shared payload | 42.07 | 46.35 | 0.91× |
| Packed NEON: IPv4 1M, hot | 81.34 | 70.32 | 1.16× |
| Packed NEON: IPv4 20M, hot | 81.17 | 70.34 | 1.15× |
| Packed NEON: IPv6 1M, hot | 84.45 | 72.78 | 1.16× |
| Packed NEON: IPv6 20M, hot | 89.04 | 77.80 | 1.14× |
| Packed NEON: IPv4 20M, uniform | 276.10 | 254.31 | 1.09× |
| Packed NEON: IPv6 20M, uniform | 472.76 | 453.39 | 1.04× |

The hot corpus selects among the first 1,024 keys. Packed NEON hot-IP speedups
remain 1.14–1.16× in the separate process medians. It beats the byte-scan
version by 1.16–1.19× on those cases. However, on 20M uniform IPv6 the byte-scan
version is faster (430.27 ns versus 453.39 ns), and eight-query interleaving
also beats packed NEON (443.12 ns). There is no universal winning reduction.

The AVX-512 20M IPv4 uniform improvement varies from 1.15× to 1.42× by process.
The 20M IPv6 uniform result varies from 1.01× to 0.87×; do not treat its pooled
regression as a stable exact percentage. Even the untouched 20M ASN path varies
by about 9% between ordinary and candidate labels on x86. These observations
limit claims from the virtualized host, and reinforce retaining research rather
than installing a blanket AVX-512 policy.

All 22 case/traffic medians, trial extrema and separate process medians are in
the [AVX-512 measurements](../results/2026-11-05-architectures-avx512.json) and
[NEON measurements](../results/2026-11-05-architectures-neon.json).

## Kernels and generated instructions

Synthetic kernels span five widths, five lengths (32 to 1,048,576 elements)
and uniform/hot queries. They span the full unsigned range without overflow;
small-width large columns consequently contain duplicates. AVX-512 versus
the ordinary control (AVX2, scalar at width 16) ranges from 0.77× to 3.29× across
the 50 cells. Applicable
packed NEON versus the scalar control ranges from 0.84× to 2.58×. Width-16
NEON is scalar, so its small timing differences establish no vector benefit.
These short kernel measurements do not substitute for complete visitor results.

Disassembly from the actual retained executables confirms:

- Five AVX-512 specializations, 157–270 bytes: unsigned vector comparison,
  mask extraction and trailing-zero count. The 128-bit specialization compares
  high words first, then low words on equality; it uses two 64-bit comparison
  masks. No truncation of 128-bit keys.
- Eight NEON specializations: unsigned `cmhi` and either byte scanning or
  packed reduction. The packed functions are 172–196 bytes versus 292–648
  bytes for scanning. LLVM lowers packing to `shrn` or `xtn`, followed by
  `fmov`, `rbit` and `clz`, avoiding the scan's many lane extractions/branches.

The [x86 instruction summary](../results/2026-11-05-architectures-avx512-asm.json)
and [ARM instruction summary](../results/2026-11-05-architectures-neon-asm.json)
are regenerated by `inspect_asm.py`; they identify addresses, symbol sizes and
instruction counts rather than estimating from intrinsics alone.

## Verification and evidence

The two final runs contain **68 processes, 4,384 timing rows, 88 full-contract
prechecks and 25,612,800 kernel comparisons**. Each host passed its ten generated
core unit tests and study Clippy check. All 16 input-file SHA-256 values match
between hosts and the prior P1 exact-key and shape fixtures, including the six
1M/20M files. Every original assertion, query result ID, target, payload, callback
order and borrowed payload address is checked before timing. Empty/partial and
mixed-family cases, duplicates, broad routes and every tested callback-error
prefix pass. Both hosts exercise the unavailable ISA's portable fallback.

Kernel checks cover 64 byte alignments, empty/short columns, vector boundaries,
duplicates, values on both sides of signed boundaries, unsigned maxima, invalid
widths and incomplete columns. Retained file hashes and all timing coordinates,
scaled hit counts and checksums pass `report.py`. `verify_source.py` independently
reconstructs 25 prepared files and checks 15 original source files per host;
`verify_inputs.py` compares both native receipts with the frozen P1 files.

Repository checks passed: fmt, Clippy default/all features, tests default/all
features including ignored tests, warning-denied documentation, and all-feature
packaging with local `--allow-dirty`.

Raw local evidence: `/root/.cache/netindex-perf-20261105/architectures-avx512-final`.
Downloaded ARM evidence, including executable, inputs and disassembly:
`target/architectures-neon-final`. Compact receipts and prepared snapshots are
committed beside the measurement JSON. The Actions artifact expires after 30 days;
the local downloaded copy remains independent of that retention period.
The original receipts' `started_at` field was captured at finalization; the
harness now records separate start/completion fields. Timing rows are unaffected.

No PMU counters, bare-metal isolation, cold-page runs, Intel AVX-512, Apple NEON,
other ARM cores, SVE/SVE2, or AVX-512/NEON cross-query gather implementation was
qualified. Wider vectors are not a promise of gains on other machines. A future
production proposal should qualify the packed NEON candidate and selective
AVX-512 policies against real distributions and a second microarchitecture,
while retaining these controls and complete lookup semantics.

Next priority: **P1-09 payload distribution**.
