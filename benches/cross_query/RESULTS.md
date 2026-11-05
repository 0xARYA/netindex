# Cross-query SIMD results - 2026-11-04

Decision: retain these kernels as research; adopt no library default or public
batch API. Large uniform cases can benefit substantially, while hot/small
cases regress. No single kernel establishes dominance across the matrix.

[Reproduction](README.md), [primary receipt](../results/2026-11-04-cross-query.json),
[fresh repeats](../results/2026-11-04-cross-query-repeat.json),
[assembly/counter probe](../results/2026-11-04-cross-query-asm.json),
[integrity verification](../results/2026-11-04-cross-query-verification.json).

## Protocol and correctness

32 primary processes plus six fresh repeats produced 1,792 timing measurements.
All original assertions and complete oracle/scalar/candidate output sequences
agree, including IDs, targets, duplicates, payload bytes and borrowed pointers.
Mixed-family interval/packed/broad-route fixtures, empty/partial batches and
typed callback-error prefixes pass. Forced scalar versions of every new kernel
are checked on every corpus and error fixture. Twelve generated-core unit tests
pass, including linear search oracles at 32 byte alignments and unsigned
boundaries, inactive groups, row-count boundaries and invalid bounds.

All methods share each executable, validated immutable input and query corpus.
Builds differ only in optional existing within-query tail SIMD; cross-query
AVX2 is active in both. Four forward/reverse trials include setup, ISA/bounds
checks, query-major visitors and payload consumption. No timed output allocation,
application decoding or JSON. Portable release/codegen-units=1, no PGO/LTO.

Numbers below are four-trial medians of aggregate ns/query, not latency
percentiles. Best interleaved is the minimum median across P1-01 groups 4/8/16
for that row: a post-hoc comparison reference, not an adaptive routing policy.
One-lane controls and every individual trial remain in the receipt. IP variant
112 calls the same kernel as 108 and is a duplicate timing control; only ASN
112 uses native eight-lane u32 gathers.

## Primary 20M cases

| Family / shape | Build | Traffic | Scalar | Best interleaved (lanes) | AVX2 4 | AVX2 8 | ASN u32 8 / IP duplicate |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 36.8 | 46.7 (16) | 57.5 | 48.8 | 49.1 |
| v4 | simd | uniform | 96.1 | 107.3 (16) | 109.3 | 100.1 | 98.4 |
| v4 | default | hot | 39.3 | 47.2 (16) | 57.9 | 49.3 | 49.1 |
| v4 | default | uniform | 141.0 | 95.9 (16) | 98.9 | 89.5 | 90.6 |
| v6 | default | hot | 40.8 | 67.0 (16) | 85.1 | 64.6 | 64.6 |
| v6 | default | uniform | 195.1 | 168.0 (16) | 140.7 | 110.1 | 111.4 |
| v6 | simd | hot | 41.9 | 67.5 (16) | 85.3 | 64.7 | 65.6 |
| v6 | simd | uniform | 294.5 | 250.0 (16) | 208.4 | 135.5 | 139.7 |
| asn | simd | hot | 21.9 | 45.5 (8) | 61.0 | 39.2 | 34.0 |
| asn | simd | uniform | 360.0 | 201.3 (8) | 305.8 | 127.9 | 104.0 |
| asn | default | hot | 22.6 | 47.7 (8) | 60.6 | 39.2 | 33.8 |
| asn | default | uniform | 159.6 | 134.7 (8) | 252.4 | 127.9 | 139.8 |

## Fresh-process 20M repeats

All six family/build cases repeat without changing source, executable,
input, queries or protocol. Absolute timings and relative rankings vary;
do not treat a single best number as a stable statistical speedup.

| Family / shape | Build | Traffic | Scalar | Best interleaved (lanes) | AVX2 4 | AVX2 8 | ASN u32 8 / IP duplicate |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 40.6 | 50.6 (16) | 63.5 | 51.4 | 53.0 |
| v4 | simd | uniform | 113.5 | 123.0 (8) | 131.4 | 144.1 | 131.5 |
| v4 | default | hot | 39.7 | 50.9 (8) | 56.7 | 49.9 | 50.0 |
| v4 | default | uniform | 266.5 | 136.9 (8) | 135.6 | 159.3 | 126.3 |
| v6 | default | hot | 42.6 | 72.4 (16) | 86.9 | 68.8 | 67.3 |
| v6 | default | uniform | 270.8 | 216.0 (8) | 223.6 | 162.7 | 166.2 |
| v6 | simd | hot | 44.1 | 71.1 (16) | 88.2 | 65.1 | 66.7 |
| v6 | simd | uniform | 311.1 | 292.5 (16) | 304.6 | 289.9 | 279.4 |
| asn | simd | hot | 22.1 | 45.8 (8) | 61.3 | 39.7 | 34.0 |
| asn | simd | uniform | 357.6 | 152.6 (8) | 251.4 | 126.8 | 145.8 |
| asn | default | hot | 22.0 | 48.0 (8) | 60.9 | 39.3 | 34.2 |
| asn | default | uniform | 155.6 | 159.6 (8) | 161.9 | 91.0 | 89.8 |

For the fresh default-build uniform cases:

- v4: variant 108, 159.3 versus scalar 266.5 ns/query (40.2% lower); best interleaved 136.9. Hot cost is 1.26x scalar.
- v6: variant 108, 162.7 versus scalar 270.8 ns/query (39.9% lower); best interleaved 216.0. Hot cost is 1.61x scalar.
- asn: variant 112, 89.8 versus scalar 155.6 ns/query (42.3% lower); best interleaved 159.6. Hot cost is 1.56x scalar.

The default IPv4 duplicate controls 108/112 have fresh medians 159.3
and 126.3 ns/query despite invoking the same IP algorithm.
This exposes timing-position/code-layout/host uncertainty; the data cannot
attribute the difference to SIMD work. Native ASN versus 64-bit eight-query
kernel rankings also change between primary and fresh cases. Its lower hot
cost does not establish universal superiority. No significance claim is made.

## Shape controls (65,536 assertions)

| Family / shape | Build | Traffic | Scalar | Best interleaved (lanes) | AVX2 4 | AVX2 8 | ASN u32 8 / IP duplicate |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| dense-asn-shared | default | uniform | 19.1 | 36.4 (8) | 49.3 | 31.5 | 27.9 |
| dense-asn-shared | simd | uniform | 18.8 | 35.6 (8) | 49.1 | 31.1 | 27.6 |
| dense-asn-unique | simd | uniform | 19.5 | 36.0 (8) | 48.6 | 31.1 | 27.7 |
| dense-asn-unique | default | uniform | 22.0 | 39.5 (8) | 52.0 | 32.5 | 29.0 |
| sparse-asn-shared | default | uniform | 21.3 | 32.8 (8) | 51.2 | 33.1 | 31.0 |
| sparse-asn-shared | simd | uniform | 21.4 | 32.1 (8) | 50.9 | 32.8 | 30.0 |
| sparse-asn-unique | simd | uniform | 23.8 | 33.2 (8) | 51.6 | 33.4 | 30.7 |
| sparse-asn-unique | default | uniform | 23.6 | 33.4 (8) | 51.8 | 33.2 | 30.6 |
| sparse-v6-shared | default | uniform | 39.8 | 53.5 (16) | 63.5 | 54.8 | 54.6 |
| sparse-v6-shared | simd | uniform | 40.0 | 54.2 (16) | 64.0 | 55.4 | 55.1 |
| sparse-v6-unique | simd | uniform | 41.3 | 55.4 (16) | 63.9 | 57.2 | 56.8 |
| sparse-v6-unique | default | uniform | 40.8 | 53.8 (16) | 63.6 | 55.6 | 55.3 |
| deep-v6-shared | default | uniform | 1060.6 | 1069.9 (4) | 1073.1 | 1062.6 | 1071.5 |
| deep-v6-shared | simd | uniform | 1106.5 | 1105.4 (4) | 1115.7 | 1124.3 | 1109.6 |
| deep-v6-unique | simd | uniform | 1089.4 | 1082.5 (4) | 1090.4 | 1087.0 | 1093.3 |
| deep-v6-unique | default | uniform | 1100.8 | 1094.8 (8) | 1111.0 | 1089.7 | 1153.8 |
| duplicates-v4-shared | default | uniform | 885.2 | 881.5 (8) | 885.3 | 886.5 | 887.2 |
| duplicates-v4-shared | simd | uniform | 878.4 | 874.9 (4) | 871.3 | 878.8 | 868.6 |
| duplicates-v4-unique | simd | uniform | 879.8 | 876.5 (4) | 877.5 | 875.8 | 875.1 |
| duplicates-v4-unique | default | uniform | 885.2 | 890.8 (16) | 891.1 | 895.4 | 887.5 |

Dense/sparse ASN and sparse IPv6 expose small warm-index costs with shared
and unique payloads. Deep-prefix/duplicate-address cases retain scalar interval
traversal: they check fallback semantics/overhead, not vector interval traversal.

## Assembly and resource boundary

Each executable contains five AVX2 kernel specializations. The 64-bit kernels
use VPGATHERQQ and VPCMPGTQ; native ASN eight-lane uses VPGATHERDD and VPCMPGTD.
Unsigned keys use sign-bit transforms; IP fences compare both 64-bit halves.
The four-query 64-bit kernels have no explicit RSP-addressed memory operands
(register saves are separate). The eight-query 64-bit kernels retain a loop and
stack-backed state; native u32 ASN provides a separate compact-state comparison.
Static sizes/counts and excerpts are in the assembly receipt. They describe
whole functions, not runtime instructions, spill costs or cache misses.

The perf probe failed because this WSL kernel lacks its matching perf tool.
No hardware-counter or achieved memory-parallelism claim is made. Guest CPU
pinning cannot control host scheduling/cache/frequency conditions. Comparing
different builds or the old P1-01 executable does not isolate a causal effect.

The first exploratory pass used 4/8-query 64-bit kernels. Inspection motivated
adding the native u32 ASN candidate. All final methods and controls were then
remeasured in full; final receipts refer to the frozen final sources only.

## Decision and next work

- P1-02 is complete as a qualified prototype comparison. Keep the useful large-
  uniform candidates and controls; hot/small regressions prevent blanket adoption.
- Preserve existing tail SIMD, fallbacks, on-disk bytes and public APIs. There is
  no package version bump or registry release for this research checkpoint.
- Next priority is P1-03 Eytzinger fence layouts. Reuse identical semantic
  contracts and scalar controls; keep SIMD/interleaving results as comparators.
- P1-06 can later evaluate lower-overhead family-specific entry points, dispatch
  hoisting, explicit register-resident 64-bit state and workload-aware selection.
  None is assumed to win without measurement.
- ARM/AVX-512, cold/>RAM operation, application traffic, hardware counters and
  held-out PGO/build qualification remain separate tasks. Four-trial warm
  synthetic evidence establishes no universal advantage or production capacity.
