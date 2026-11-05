# Learned-fence results - 2026-11-04

Decision: retain guarded learned prediction as a workload-specific research
candidate. No production default, public API, encoding or release change.
Large evenly spaced inputs are favorable; small nonlinear cases and hot
traffic prevent a universal adoption claim. This is an independent Rust
corridor prototype inspired by RadixSpline, not an upstream port/benchmark.

[Protocol/reproduction/reference](README.md), [primary matrix](../results/2026-11-04-learned.json),
[fresh repeats](../results/2026-11-04-learned-repeat.json),
[assembly/counters](../results/2026-11-04-learned-asm.json),
[integrity/fixture reproduction](../results/2026-11-04-learned-verification.json).

## Scope and correctness

144 primary processes plus 66 fresh repeats yield 5,280 timings. Error
settings 4/32/128 run in separate processes, default/SIMD builds on WSL CPU 20.
Portable release/codegen-units=1, no PGO/LTO/custom target flags. Both builds
detect AVX2 at runtime. Four forward/reverse trials and a warmup precede each
measurement. Every table entry is median warm aggregate ns/query, not latency
percentiles or statistical significance. Process/host drift remains relevant.

0 ordinary; 108 prior cross-query AVX2; 700 native sorted-key cache; 701 learned
binary segment routing; 702 learned radix routing. Learned routes search a
window of at most 2*error+5 keys, then validate global neighbors and fall back
to full native search if necessary. Requested error is not a universal bound
for unseen gaps/duplicate jumps/floating rounding; global checks prove exact
rank. Single-segment models bypass segment routing and allocate no radix.
Correction remains scalar/native; no new explicit SIMD kernel is added.
Estimate/search retain non-inlined boundaries for linked-body inspection;
call/result-record costs are timed. Inlining/specialization is not an isolated
factor here. Both native-control/learned search return rank/window/recovery
state; the timed reader uses rank, with no mutable per-query counters.

Every original assertion and full independent original-input query oracle
passes, including targets/IDs, duplicate multiplicity, borrowed payload bytes/
pointers, partial/empty/mixed batches, broad routes and typed callback-error
prefixes. Forced rank-zero predictions/scalar paths and 15 generated-core
tests cover recovery, NaN slopes, unsigned limits, gaps, duplicates, sortedness,
allocation budgets and preparation identity. Fresh preparation reproduces
the full generated tree. Eight new curved/clustered files reproduce byte-for-
byte. Header-derived eligibility, capacities/radix dimensions, probes, all
timing coordinates/result checksums and receipt/source hashes agree.

## Fixed error 4 qualification on 20M inputs

For each row, ranges span primary/repeat and default/SIMD processes. Keep
error 4 and route 702 fixed across this comparison; these are not post-hoc
fastest learned variants. The best-control comparator is the minimum median
of 0/108/700 in the same process and is optimistic rather than deployable.
Ratios above 1 favor route 702; ratios below 1 indicate regressions.

| Family | Traffic | Ordinary /702 range | Best fixed control /702 range |
| --- | --- | ---: | ---: |
| v4 | uniform | 1.00-2.35x | 0.98-1.43x |
| v4 | hot | 0.88-0.91x | 0.88-0.91x |
| v6 | uniform | 1.37-2.69x | 1.01-1.54x |
| v6 | hot | 0.89-1.01x | 0.89-1.01x |
| asn | uniform | 6.41-10.70x | 2.97-4.21x |
| asn | hot | 1.85-1.91x | 1.85-1.91x |

The large original inputs have uniformly spaced keys and each uses one
segment. The nonlinear controls contain only 256 packed fences per file, so
they qualify small-array overhead and shape behavior, not large nonlinear
indexes or provider traffic. Their midpoint gaps are included in timings.
Wider-than-u64 sparse IPv6 and overlapping/interval shapes keep ordinary
fallback. Those rows do not measure learned acceleration.

## Additional cache and startup

One native key array/model per admitted section, shared by all comparators.
The 128 MiB bound covers vector element capacities (keys, 40-byte segments
including spare capacity, u32 radix entries), not mapping/validation, existing
reader directory, objects, allocator overhead or RSS. Ordinary controls also
retain these arrays. Build time follows ordinary validation and precedes
queries; it does not isolate model fitting from native-key extraction.
Zero-copy/strided correction is not implemented or measured.

| 20M family | Error | Additional bytes | Segments | Radix entries | Build ms range, four processes |
| --- | ---: | ---: | ---: | ---: | ---: |
| v4 | 4 | 312,580 | 1 | 0 | 0.366-0.571 |
| v4 | 32 | 312,580 | 1 | 0 | 0.374-0.378 |
| v4 | 128 | 312,580 | 1 | 0 | 0.368-0.427 |
| v6 | 4 | 625,080 | 1 | 0 | 0.416-0.627 |
| v6 | 32 | 625,080 | 1 | 0 | 0.421-0.568 |
| v6 | 128 | 625,080 | 1 | 0 | 0.412-0.494 |
| asn | 4 | 80,000,080 | 1 | 0 | 74.386-83.468 |
| asn | 32 | 80,000,080 | 1 | 0 | 74.166-76.800 |
| asn | 128 | 80,000,080 | 1 | 0 | 74.630-102.737 |

## Untimed guard diagnostics

Primary: 0 full-search recoveries / 4,738,704 eligible model queries; binary/radix probe statistics agree.
Repeat: 0 full-search recoveries / 2,373,156 eligible model queries; binary/radix probe statistics agree.

These include model domain fast paths. Probe counts are outside timing and
do not measure branch/cache hardware events. Forced bad-prediction and NaN
tests qualify recovery even if ordinary corpora need none. A fallback-heavy
throughput/latency profile is not measured by these diagnostics.

## Linked assembly and counters

Retained complete estimate/search symbol bodies cover both native key widths.
Generic execute summaries include inlined dispatch, emission and fallbacks.
Static code observations identify floating interpolation, conversions, bounds
and branch/search work; they do not establish runtime causality or cost.
Counter probe: unavailable.

| Build | Kernel | Bytes | Floating multiply | Floating divide | Conditional jumps |
| --- | --- | ---: | ---: | ---: | ---: |
| default | search | 520 | 0 | 0 | 19 |
| default | search | 498 | 0 | 0 | 19 |
| default | estimate | 605 | 1 | 0 | 15 |
| default | estimate | 605 | 1 | 0 | 15 |
| simd | search | 498 | 0 | 0 | 19 |
| simd | search | 520 | 0 | 0 | 19 |
| simd | estimate | 605 | 1 | 0 | 15 |
| simd | estimate | 605 | 1 | 0 | 15 |

## 20M primary medians

| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 128 | uniform | 253.5 | 243.0 | 260.9 | 235.8 | 250.4 |
| v4 | simd | 128 | hot | 37.6 | 49.7 | 58.7 | 51.8 | 51.8 |
| v4 | default | 128 | uniform | 222.7 | 144.5 | 153.5 | 140.0 | 171.3 |
| v4 | default | 128 | hot | 39.7 | 50.1 | 54.9 | 48.9 | 49.2 |
| v4 | simd | 32 | uniform | 213.1 | 207.4 | 212.5 | 226.8 | 212.7 |
| v4 | simd | 32 | hot | 37.8 | 50.0 | 57.8 | 49.3 | 49.2 |
| v4 | default | 32 | uniform | 287.8 | 204.2 | 216.1 | 192.1 | 184.4 |
| v4 | default | 32 | hot | 40.8 | 50.1 | 54.5 | 45.7 | 46.0 |
| v4 | simd | 4 | uniform | 133.1 | 129.9 | 158.6 | 117.7 | 132.9 |
| v4 | simd | 4 | hot | 38.3 | 49.5 | 56.5 | 43.4 | 43.4 |
| v4 | default | 4 | uniform | 249.6 | 162.2 | 208.4 | 165.1 | 144.6 |
| v4 | default | 4 | hot | 40.8 | 50.5 | 54.4 | 46.6 | 46.0 |
| v6 | default | 4 | uniform | 284.2 | 143.9 | 165.5 | 102.7 | 105.8 |
| v6 | default | 4 | hot | 41.8 | 65.2 | 56.4 | 45.4 | 45.3 |
| v6 | simd | 4 | uniform | 354.4 | 263.2 | 313.3 | 235.0 | 259.5 |
| v6 | simd | 4 | hot | 43.2 | 65.1 | 60.9 | 45.1 | 45.2 |
| v6 | default | 32 | uniform | 272.6 | 145.7 | 157.1 | 115.2 | 113.3 |
| v6 | default | 32 | hot | 41.4 | 66.0 | 57.1 | 47.1 | 47.1 |
| v6 | simd | 32 | uniform | 305.4 | 231.1 | 256.4 | 225.4 | 200.2 |
| v6 | simd | 32 | hot | 43.6 | 65.5 | 61.0 | 48.6 | 49.0 |
| v6 | default | 128 | uniform | 330.9 | 208.6 | 232.0 | 222.4 | 190.3 |
| v6 | default | 128 | hot | 42.5 | 71.0 | 59.0 | 51.1 | 51.8 |
| v6 | simd | 128 | uniform | 290.7 | 149.9 | 181.5 | 153.5 | 180.0 |
| v6 | simd | 128 | hot | 43.2 | 64.5 | 60.2 | 55.0 | 54.3 |
| asn | simd | 128 | uniform | 314.7 | 133.3 | 380.4 | 92.2 | 87.5 |
| asn | simd | 128 | hot | 22.5 | 39.7 | 22.8 | 16.8 | 16.8 |
| asn | default | 128 | uniform | 316.8 | 98.8 | 257.3 | 77.1 | 57.7 |
| asn | default | 128 | hot | 22.0 | 39.8 | 22.7 | 16.7 | 16.8 |
| asn | simd | 32 | uniform | 150.3 | 91.5 | 166.8 | 48.9 | 47.6 |
| asn | simd | 32 | hot | 22.1 | 39.6 | 22.6 | 14.5 | 14.6 |
| asn | default | 32 | uniform | 438.4 | 181.6 | 436.2 | 81.0 | 58.0 |
| asn | default | 32 | hot | 22.6 | 40.7 | 25.2 | 16.2 | 16.5 |
| asn | simd | 4 | uniform | 418.7 | 157.6 | 355.8 | 47.8 | 42.7 |
| asn | simd | 4 | hot | 23.1 | 40.3 | 23.3 | 12.1 | 12.5 |
| asn | default | 4 | uniform | 455.1 | 179.3 | 452.2 | 54.5 | 42.5 |
| asn | default | 4 | hot | 21.9 | 39.6 | 23.5 | 11.7 | 11.8 |

## Fresh-process repeat medians

| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 128 | uniform | 156.3 | 146.4 | 184.6 | 131.4 | 147.8 |
| v4 | simd | 128 | hot | 37.8 | 49.7 | 57.5 | 51.6 | 50.7 |
| v4 | default | 128 | uniform | 255.7 | 159.9 | 177.1 | 168.7 | 156.3 |
| v4 | default | 128 | hot | 40.4 | 49.8 | 53.2 | 48.5 | 48.8 |
| v4 | simd | 32 | uniform | 140.9 | 125.6 | 163.4 | 115.2 | 109.0 |
| v4 | simd | 32 | hot | 37.8 | 49.4 | 55.8 | 45.0 | 45.1 |
| v4 | default | 32 | uniform | 217.4 | 148.5 | 155.5 | 142.0 | 144.3 |
| v4 | default | 32 | hot | 40.0 | 50.2 | 53.8 | 46.6 | 46.6 |
| v4 | simd | 4 | uniform | 104.1 | 98.7 | 124.7 | 93.7 | 96.2 |
| v4 | simd | 4 | hot | 38.6 | 49.6 | 57.5 | 42.6 | 42.9 |
| v4 | default | 4 | uniform | 244.9 | 149.0 | 154.3 | 124.7 | 104.3 |
| v4 | default | 4 | hot | 39.7 | 49.5 | 54.9 | 43.7 | 43.7 |
| v6 | default | 4 | uniform | 260.0 | 128.3 | 149.6 | 107.8 | 99.5 |
| v6 | default | 4 | hot | 41.7 | 66.1 | 56.7 | 45.7 | 47.0 |
| v6 | simd | 4 | uniform | 349.7 | 232.1 | 271.8 | 176.9 | 150.7 |
| v6 | simd | 4 | hot | 45.2 | 65.2 | 60.8 | 45.5 | 45.0 |
| v6 | default | 32 | uniform | 298.2 | 198.3 | 190.7 | 141.7 | 187.6 |
| v6 | default | 32 | hot | 43.5 | 66.9 | 58.7 | 47.6 | 47.6 |
| v6 | simd | 32 | uniform | 356.4 | 223.6 | 214.7 | 173.5 | 197.6 |
| v6 | simd | 32 | hot | 44.0 | 66.5 | 61.0 | 48.6 | 48.5 |
| v6 | default | 128 | uniform | 234.0 | 142.4 | 159.2 | 129.7 | 133.4 |
| v6 | default | 128 | hot | 47.2 | 70.0 | 60.6 | 55.6 | 54.9 |
| v6 | simd | 128 | uniform | 330.5 | 239.5 | 251.6 | 186.1 | 163.0 |
| v6 | simd | 128 | hot | 43.1 | 65.9 | 60.9 | 54.3 | 53.7 |
| asn | simd | 128 | uniform | 238.4 | 106.4 | 279.1 | 61.2 | 59.2 |
| asn | simd | 128 | hot | 22.0 | 39.5 | 22.6 | 16.6 | 16.6 |
| asn | default | 128 | uniform | 357.7 | 149.2 | 350.2 | 94.5 | 70.9 |
| asn | default | 128 | hot | 22.2 | 39.5 | 22.9 | 16.7 | 16.7 |
| asn | simd | 32 | uniform | 318.1 | 133.6 | 344.5 | 49.9 | 50.8 |
| asn | simd | 32 | hot | 21.9 | 39.4 | 22.5 | 14.5 | 14.4 |
| asn | default | 32 | uniform | 176.0 | 92.1 | 206.5 | 50.6 | 47.8 |
| asn | default | 32 | hot | 22.7 | 39.6 | 22.6 | 14.6 | 14.7 |
| asn | simd | 4 | uniform | 423.5 | 171.6 | 433.3 | 56.7 | 42.0 |
| asn | simd | 4 | hot | 22.6 | 39.5 | 23.2 | 11.8 | 11.9 |
| asn | default | 4 | uniform | 263.0 | 121.8 | 337.7 | 43.1 | 41.1 |
| asn | default | 4 | hot | 21.9 | 39.2 | 22.4 | 11.6 | 11.6 |
| curved-v4-shared | default | 4 | uniform | 38.1 | 46.7 | 41.3 | 51.1 | 50.2 |
| curved-v4-shared | simd | 4 | uniform | 41.4 | 49.0 | 45.1 | 51.9 | 50.8 |
| curved-v4-shared | default | 32 | uniform | 38.1 | 47.1 | 42.3 | 53.4 | 53.2 |
| curved-v4-shared | simd | 32 | uniform | 40.3 | 49.1 | 42.6 | 51.5 | 54.3 |
| curved-v4-shared | default | 128 | uniform | 38.2 | 46.0 | 43.7 | 49.3 | 49.2 |
| curved-v4-shared | simd | 128 | uniform | 39.3 | 48.9 | 42.9 | 50.8 | 50.1 |
| curved-v4-unique | simd | 128 | uniform | 41.0 | 50.5 | 44.6 | 51.6 | 51.5 |
| curved-v4-unique | default | 128 | uniform | 38.0 | 46.7 | 41.6 | 52.1 | 50.6 |
| curved-v4-unique | simd | 32 | uniform | 39.7 | 48.8 | 43.3 | 51.8 | 54.8 |
| curved-v4-unique | default | 32 | uniform | 38.2 | 46.4 | 42.3 | 51.8 | 54.1 |
| curved-v4-unique | simd | 4 | uniform | 40.3 | 49.9 | 43.1 | 52.4 | 51.4 |
| curved-v4-unique | default | 4 | uniform | 39.4 | 48.6 | 43.0 | 53.2 | 52.3 |
| curved-v6-shared | default | 4 | uniform | 38.0 | 53.7 | 42.3 | 52.7 | 51.4 |
| curved-v6-shared | simd | 4 | uniform | 41.4 | 76.0 | 68.5 | 76.2 | 66.5 |
| curved-v6-shared | default | 32 | uniform | 41.4 | 52.7 | 42.3 | 51.8 | 53.6 |
| curved-v6-shared | simd | 32 | uniform | 41.0 | 76.5 | 68.9 | 73.1 | 70.6 |
| curved-v6-shared | default | 128 | uniform | 37.9 | 53.9 | 42.7 | 50.4 | 50.6 |
| curved-v6-shared | simd | 128 | uniform | 41.2 | 75.1 | 68.6 | 76.3 | 76.6 |
| curved-v6-unique | simd | 128 | uniform | 41.1 | 75.8 | 69.3 | 77.2 | 78.0 |
| curved-v6-unique | default | 128 | uniform | 38.2 | 53.7 | 44.6 | 54.9 | 51.5 |
| curved-v6-unique | simd | 32 | uniform | 41.8 | 74.9 | 70.5 | 72.0 | 70.9 |
| curved-v6-unique | default | 32 | uniform | 38.8 | 53.7 | 43.8 | 53.5 | 55.6 |
| curved-v6-unique | simd | 4 | uniform | 41.0 | 74.7 | 69.2 | 76.6 | 67.2 |
| curved-v6-unique | default | 4 | uniform | 38.5 | 53.4 | 43.5 | 54.1 | 51.7 |
| clustered-v4-shared | default | 4 | uniform | 34.7 | 42.8 | 39.8 | 42.2 | 41.7 |
| clustered-v4-shared | simd | 4 | uniform | 36.3 | 43.7 | 39.6 | 41.8 | 41.8 |
| clustered-v4-shared | default | 32 | uniform | 34.6 | 42.2 | 38.2 | 44.7 | 45.0 |
| clustered-v4-shared | simd | 32 | uniform | 36.5 | 43.5 | 39.7 | 46.0 | 46.1 |
| clustered-v4-shared | default | 128 | uniform | 35.2 | 43.2 | 39.7 | 47.0 | 46.9 |
| clustered-v4-shared | simd | 128 | uniform | 36.0 | 43.5 | 38.9 | 48.5 | 48.7 |
| clustered-v4-unique | simd | 128 | uniform | 35.9 | 43.0 | 39.3 | 49.7 | 49.4 |
| clustered-v4-unique | default | 128 | uniform | 35.1 | 42.8 | 38.9 | 46.8 | 46.9 |
| clustered-v4-unique | simd | 32 | uniform | 36.0 | 43.5 | 38.9 | 46.8 | 46.8 |
| clustered-v4-unique | default | 32 | uniform | 34.6 | 43.2 | 39.1 | 45.4 | 45.2 |
| clustered-v4-unique | simd | 4 | uniform | 35.8 | 43.6 | 39.2 | 42.2 | 41.7 |
| clustered-v4-unique | default | 4 | uniform | 34.5 | 42.4 | 38.8 | 41.4 | 41.1 |
| clustered-v6-shared | default | 4 | uniform | 37.1 | 52.1 | 41.7 | 43.4 | 43.1 |
| clustered-v6-shared | simd | 4 | uniform | 39.7 | 70.1 | 63.0 | 64.1 | 64.1 |
| clustered-v6-shared | default | 32 | uniform | 37.3 | 52.6 | 41.8 | 48.3 | 48.2 |
| clustered-v6-shared | simd | 32 | uniform | 39.8 | 70.0 | 63.2 | 64.2 | 64.1 |
| clustered-v6-shared | default | 128 | uniform | 37.0 | 52.1 | 41.6 | 49.2 | 49.7 |
| clustered-v6-shared | simd | 128 | uniform | 39.6 | 68.9 | 62.3 | 70.3 | 70.4 |
| clustered-v6-unique | simd | 128 | uniform | 40.0 | 70.8 | 63.6 | 71.7 | 71.8 |
| clustered-v6-unique | default | 128 | uniform | 36.9 | 52.1 | 42.1 | 50.1 | 50.6 |
| clustered-v6-unique | simd | 32 | uniform | 43.2 | 74.6 | 66.0 | 66.1 | 65.3 |
| clustered-v6-unique | default | 32 | uniform | 37.9 | 52.3 | 42.6 | 48.7 | 48.5 |
| clustered-v6-unique | simd | 4 | uniform | 40.3 | 69.5 | 63.8 | 65.3 | 63.8 |
| clustered-v6-unique | default | 4 | uniform | 37.3 | 51.7 | 42.6 | 43.9 | 43.6 |

## 1M primary medians

| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | default | 4 | uniform | 84.6 | 66.5 | 68.3 | 64.0 | 66.1 |
| v4 | default | 4 | hot | 41.1 | 49.4 | 46.9 | 44.3 | 44.8 |
| v4 | simd | 4 | uniform | 63.4 | 68.2 | 70.0 | 65.3 | 65.4 |
| v4 | simd | 4 | hot | 37.3 | 48.1 | 46.6 | 44.7 | 44.5 |
| v4 | default | 32 | uniform | 81.1 | 64.9 | 68.3 | 67.4 | 68.2 |
| v4 | default | 32 | hot | 39.3 | 48.4 | 45.5 | 46.0 | 45.9 |
| v4 | simd | 32 | uniform | 59.9 | 66.2 | 67.1 | 70.5 | 70.7 |
| v4 | simd | 32 | hot | 37.5 | 47.7 | 45.7 | 46.0 | 46.0 |
| v4 | default | 128 | uniform | 81.5 | 72.3 | 69.5 | 73.7 | 77.4 |
| v4 | default | 128 | hot | 43.0 | 50.1 | 48.0 | 49.2 | 51.1 |
| v4 | simd | 128 | uniform | 61.8 | 69.4 | 69.2 | 79.9 | 80.0 |
| v4 | simd | 128 | hot | 38.9 | 47.9 | 46.1 | 52.7 | 52.1 |
| v6 | simd | 128 | uniform | 107.4 | 78.2 | 73.6 | 82.1 | 84.4 |
| v6 | simd | 128 | hot | 41.3 | 57.7 | 49.6 | 54.2 | 54.0 |
| v6 | default | 128 | uniform | 118.0 | 74.1 | 72.8 | 75.9 | 76.9 |
| v6 | default | 128 | hot | 39.9 | 58.5 | 51.2 | 53.4 | 50.6 |
| v6 | simd | 32 | uniform | 108.0 | 78.4 | 73.7 | 73.8 | 74.0 |
| v6 | simd | 32 | hot | 40.2 | 57.4 | 49.6 | 48.7 | 48.6 |
| v6 | default | 32 | uniform | 124.3 | 79.6 | 73.1 | 73.2 | 71.9 |
| v6 | default | 32 | hot | 40.5 | 61.6 | 50.7 | 50.6 | 50.3 |
| v6 | simd | 4 | uniform | 107.7 | 78.7 | 77.6 | 64.9 | 64.8 |
| v6 | simd | 4 | hot | 40.7 | 58.1 | 51.1 | 45.8 | 46.0 |
| v6 | default | 4 | uniform | 122.6 | 77.8 | 72.1 | 65.1 | 63.9 |
| v6 | default | 4 | hot | 40.1 | 57.8 | 48.7 | 45.1 | 45.4 |
| asn | default | 4 | uniform | 52.9 | 46.2 | 56.1 | 22.0 | 20.7 |
| asn | default | 4 | hot | 17.6 | 32.9 | 18.8 | 12.0 | 11.9 |
| asn | simd | 4 | uniform | 58.7 | 48.3 | 59.5 | 21.6 | 21.8 |
| asn | simd | 4 | hot | 17.8 | 32.7 | 19.1 | 12.2 | 12.3 |
| asn | default | 32 | uniform | 54.1 | 47.2 | 56.4 | 26.0 | 24.4 |
| asn | default | 32 | hot | 17.7 | 32.8 | 18.3 | 14.9 | 15.6 |
| asn | simd | 32 | uniform | 54.7 | 46.6 | 53.0 | 25.2 | 24.8 |
| asn | simd | 32 | hot | 17.8 | 33.1 | 17.9 | 14.7 | 14.6 |
| asn | default | 128 | uniform | 54.1 | 46.3 | 55.0 | 31.2 | 32.5 |
| asn | default | 128 | hot | 18.5 | 33.3 | 19.2 | 17.1 | 17.2 |
| asn | simd | 128 | uniform | 56.5 | 47.0 | 56.6 | 34.2 | 34.3 |
| asn | simd | 128 | hot | 17.5 | 32.9 | 18.0 | 17.2 | 17.1 |

## Original shape medians

| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| dense-asn-shared | default | 4 | uniform | 19.0 | 31.4 | 12.5 | 7.3 | 7.3 |
| dense-asn-shared | simd | 4 | uniform | 19.2 | 31.3 | 12.3 | 7.3 | 7.4 |
| dense-asn-shared | default | 32 | uniform | 20.2 | 31.8 | 12.4 | 9.0 | 8.9 |
| dense-asn-shared | simd | 32 | uniform | 20.3 | 32.1 | 19.9 | 13.9 | 13.9 |
| dense-asn-shared | default | 128 | uniform | 19.0 | 31.4 | 12.3 | 10.7 | 10.6 |
| dense-asn-shared | simd | 128 | uniform | 19.0 | 31.7 | 12.3 | 10.2 | 10.2 |
| dense-asn-unique | simd | 128 | uniform | 20.7 | 31.8 | 14.1 | 11.4 | 11.3 |
| dense-asn-unique | default | 128 | uniform | 20.9 | 32.0 | 13.9 | 11.3 | 11.5 |
| dense-asn-unique | simd | 32 | uniform | 20.8 | 31.7 | 14.0 | 9.4 | 9.4 |
| dense-asn-unique | default | 32 | uniform | 21.0 | 31.9 | 13.6 | 9.6 | 9.6 |
| dense-asn-unique | simd | 4 | uniform | 19.6 | 31.6 | 13.9 | 7.8 | 8.0 |
| dense-asn-unique | default | 4 | uniform | 31.1 | 34.2 | 19.1 | 11.0 | 10.1 |
| sparse-asn-shared | default | 4 | uniform | 21.3 | 33.9 | 20.5 | 12.1 | 12.1 |
| sparse-asn-shared | simd | 4 | uniform | 21.8 | 33.7 | 21.7 | 11.8 | 12.1 |
| sparse-asn-shared | default | 32 | uniform | 22.6 | 34.4 | 20.9 | 15.7 | 15.8 |
| sparse-asn-shared | simd | 32 | uniform | 21.2 | 34.0 | 21.0 | 15.3 | 15.5 |
| sparse-asn-shared | default | 128 | uniform | 21.5 | 34.4 | 21.0 | 17.2 | 18.4 |
| sparse-asn-shared | simd | 128 | uniform | 21.7 | 34.1 | 20.8 | 17.4 | 17.1 |
| sparse-asn-unique | simd | 128 | uniform | 21.4 | 33.9 | 23.4 | 18.1 | 18.4 |
| sparse-asn-unique | default | 128 | uniform | 23.8 | 34.1 | 23.5 | 18.5 | 18.3 |
| sparse-asn-unique | simd | 32 | uniform | 25.0 | 33.9 | 23.4 | 16.3 | 16.4 |
| sparse-asn-unique | default | 32 | uniform | 23.0 | 34.0 | 23.7 | 16.5 | 16.5 |
| sparse-asn-unique | simd | 4 | uniform | 23.5 | 33.7 | 23.4 | 12.9 | 12.8 |
| sparse-asn-unique | default | 4 | uniform | 20.9 | 33.5 | 23.3 | 12.8 | 12.7 |
| sparse-v6-shared | default | 4 | uniform | 40.6 | 55.7 | 40.8 | 40.6 | 40.7 |
| sparse-v6-shared | simd | 4 | uniform | 40.7 | 55.9 | 41.2 | 40.8 | 40.9 |
| sparse-v6-shared | default | 32 | uniform | 40.0 | 56.4 | 40.7 | 40.1 | 40.3 |
| sparse-v6-shared | simd | 32 | uniform | 40.9 | 56.0 | 40.8 | 40.9 | 40.9 |
| sparse-v6-shared | default | 128 | uniform | 40.6 | 57.0 | 41.9 | 41.8 | 42.1 |
| sparse-v6-shared | simd | 128 | uniform | 40.9 | 56.5 | 41.0 | 40.5 | 41.2 |
| sparse-v6-unique | simd | 128 | uniform | 41.5 | 57.2 | 41.2 | 41.9 | 42.1 |
| sparse-v6-unique | default | 128 | uniform | 41.1 | 56.5 | 41.4 | 41.4 | 41.5 |
| sparse-v6-unique | simd | 32 | uniform | 43.4 | 58.2 | 43.1 | 42.6 | 43.4 |
| sparse-v6-unique | default | 32 | uniform | 55.0 | 68.0 | 48.5 | 48.4 | 42.9 |
| sparse-v6-unique | simd | 4 | uniform | 56.5 | 71.1 | 56.5 | 57.6 | 57.6 |
| sparse-v6-unique | default | 4 | uniform | 41.8 | 57.7 | 42.1 | 41.8 | 41.6 |
| deep-v6-shared | default | 4 | uniform | 1165.0 | 1132.8 | 1116.6 | 1154.3 | 1125.5 |
| deep-v6-shared | simd | 4 | uniform | 1117.7 | 1104.4 | 1122.6 | 1113.4 | 1121.0 |
| deep-v6-shared | default | 32 | uniform | 1102.5 | 1127.6 | 1158.4 | 1118.8 | 1138.6 |
| deep-v6-shared | simd | 32 | uniform | 1077.1 | 1086.5 | 1076.8 | 1078.6 | 1074.7 |
| deep-v6-shared | default | 128 | uniform | 1103.6 | 1132.9 | 1107.0 | 1154.9 | 1122.7 |
| deep-v6-shared | simd | 128 | uniform | 1078.3 | 1084.9 | 1113.1 | 1088.2 | 1123.7 |
| deep-v6-unique | simd | 128 | uniform | 1099.6 | 1076.3 | 1100.3 | 1098.2 | 1093.6 |
| deep-v6-unique | default | 128 | uniform | 1149.9 | 1151.7 | 1137.1 | 1141.3 | 1146.7 |
| deep-v6-unique | simd | 32 | uniform | 1135.8 | 1160.4 | 1131.5 | 1154.7 | 1136.6 |
| deep-v6-unique | default | 32 | uniform | 1228.8 | 1194.4 | 1289.5 | 1292.9 | 1182.2 |
| deep-v6-unique | simd | 4 | uniform | 1186.2 | 1178.9 | 1154.4 | 1200.7 | 1189.0 |
| deep-v6-unique | default | 4 | uniform | 1170.1 | 1138.1 | 1176.7 | 1137.8 | 1142.7 |
| duplicates-v4-shared | default | 4 | uniform | 866.6 | 862.7 | 859.8 | 857.9 | 861.9 |
| duplicates-v4-shared | simd | 4 | uniform | 864.8 | 863.4 | 856.7 | 858.1 | 856.5 |
| duplicates-v4-shared | default | 32 | uniform | 883.5 | 888.3 | 875.5 | 874.5 | 882.0 |
| duplicates-v4-shared | simd | 32 | uniform | 859.7 | 865.8 | 860.4 | 856.9 | 858.1 |
| duplicates-v4-shared | default | 128 | uniform | 911.5 | 870.2 | 894.6 | 898.5 | 899.8 |
| duplicates-v4-shared | simd | 128 | uniform | 914.1 | 875.7 | 873.9 | 872.8 | 865.8 |
| duplicates-v4-unique | simd | 128 | uniform | 870.2 | 873.8 | 877.1 | 876.6 | 869.0 |
| duplicates-v4-unique | default | 128 | uniform | 898.8 | 892.3 | 897.8 | 887.3 | 883.9 |
| duplicates-v4-unique | simd | 32 | uniform | 885.8 | 889.1 | 881.3 | 888.1 | 884.0 |
| duplicates-v4-unique | default | 32 | uniform | 873.8 | 865.8 | 875.9 | 877.1 | 871.8 |
| duplicates-v4-unique | simd | 4 | uniform | 892.3 | 876.7 | 886.7 | 891.8 | 897.8 |
| duplicates-v4-unique | default | 4 | uniform | 893.0 | 886.8 | 902.4 | 894.9 | 891.2 |

## Nonlinear shape medians

| Family / shape | Build | Error | Traffic | 0 | 108 | 700 | 701 | 702 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| curved-v4-shared | default | 4 | uniform | 38.5 | 52.2 | 42.5 | 52.2 | 51.6 |
| curved-v4-shared | simd | 4 | uniform | 40.7 | 49.0 | 44.6 | 53.1 | 52.1 |
| curved-v4-shared | default | 32 | uniform | 38.8 | 49.3 | 42.5 | 51.7 | 54.6 |
| curved-v4-shared | simd | 32 | uniform | 40.9 | 49.4 | 44.6 | 55.8 | 58.0 |
| curved-v4-shared | default | 128 | uniform | 39.0 | 47.1 | 43.1 | 50.9 | 49.8 |
| curved-v4-shared | simd | 128 | uniform | 40.5 | 49.3 | 44.1 | 51.8 | 51.6 |
| curved-v4-unique | simd | 128 | uniform | 40.1 | 48.9 | 44.1 | 52.3 | 51.8 |
| curved-v4-unique | default | 128 | uniform | 38.5 | 46.8 | 42.1 | 50.7 | 50.4 |
| curved-v4-unique | simd | 32 | uniform | 40.2 | 50.0 | 44.7 | 54.7 | 56.9 |
| curved-v4-unique | default | 32 | uniform | 38.0 | 47.4 | 42.4 | 52.0 | 54.1 |
| curved-v4-unique | simd | 4 | uniform | 41.1 | 49.3 | 44.0 | 53.5 | 52.4 |
| curved-v4-unique | default | 4 | uniform | 38.0 | 46.8 | 42.1 | 52.0 | 50.8 |
| curved-v6-shared | default | 4 | uniform | 38.9 | 53.5 | 42.8 | 52.4 | 51.5 |
| curved-v6-shared | simd | 4 | uniform | 40.6 | 74.1 | 67.4 | 75.4 | 66.1 |
| curved-v6-shared | default | 32 | uniform | 38.5 | 53.2 | 45.2 | 51.1 | 54.1 |
| curved-v6-shared | simd | 32 | uniform | 41.2 | 75.5 | 68.6 | 72.4 | 69.9 |
| curved-v6-shared | default | 128 | uniform | 37.9 | 52.9 | 42.5 | 50.4 | 50.8 |
| curved-v6-shared | simd | 128 | uniform | 41.0 | 76.0 | 69.5 | 77.6 | 77.5 |
| curved-v6-unique | simd | 128 | uniform | 41.2 | 74.9 | 68.9 | 77.0 | 76.8 |
| curved-v6-unique | default | 128 | uniform | 37.9 | 53.6 | 44.0 | 51.5 | 50.8 |
| curved-v6-unique | simd | 32 | uniform | 41.3 | 75.1 | 68.7 | 72.0 | 70.6 |
| curved-v6-unique | default | 32 | uniform | 38.2 | 53.5 | 43.9 | 52.6 | 54.8 |
| curved-v6-unique | simd | 4 | uniform | 41.7 | 75.9 | 70.1 | 78.6 | 68.4 |
| curved-v6-unique | default | 4 | uniform | 38.5 | 53.5 | 43.3 | 52.3 | 51.5 |
| clustered-v4-shared | default | 4 | uniform | 35.2 | 43.0 | 49.4 | 51.1 | 52.2 |
| clustered-v4-shared | simd | 4 | uniform | 36.7 | 44.2 | 40.1 | 42.5 | 42.5 |
| clustered-v4-shared | default | 32 | uniform | 34.8 | 43.1 | 39.0 | 45.5 | 46.6 |
| clustered-v4-shared | simd | 32 | uniform | 36.5 | 46.0 | 40.0 | 48.4 | 48.0 |
| clustered-v4-shared | default | 128 | uniform | 36.1 | 43.4 | 39.1 | 48.3 | 48.0 |
| clustered-v4-shared | simd | 128 | uniform | 36.5 | 43.5 | 40.2 | 50.1 | 50.0 |
| clustered-v4-unique | simd | 128 | uniform | 36.4 | 43.7 | 40.5 | 51.7 | 51.7 |
| clustered-v4-unique | default | 128 | uniform | 36.1 | 46.2 | 40.2 | 48.5 | 48.3 |
| clustered-v4-unique | simd | 32 | uniform | 36.0 | 43.3 | 40.0 | 47.3 | 47.3 |
| clustered-v4-unique | default | 32 | uniform | 35.2 | 42.8 | 39.7 | 45.8 | 45.8 |
| clustered-v4-unique | simd | 4 | uniform | 36.1 | 43.3 | 40.0 | 42.5 | 42.5 |
| clustered-v4-unique | default | 4 | uniform | 34.7 | 42.9 | 39.1 | 41.5 | 41.4 |
| clustered-v6-shared | default | 4 | uniform | 37.0 | 51.6 | 41.0 | 43.0 | 42.9 |
| clustered-v6-shared | simd | 4 | uniform | 40.1 | 70.1 | 63.3 | 64.1 | 64.0 |
| clustered-v6-shared | default | 32 | uniform | 37.0 | 51.9 | 42.3 | 47.8 | 47.4 |
| clustered-v6-shared | simd | 32 | uniform | 40.0 | 69.5 | 63.0 | 64.2 | 63.9 |
| clustered-v6-shared | default | 128 | uniform | 38.9 | 54.3 | 41.5 | 49.5 | 49.7 |
| clustered-v6-shared | simd | 128 | uniform | 39.5 | 70.0 | 62.6 | 70.5 | 70.5 |
| clustered-v6-unique | simd | 128 | uniform | 40.0 | 70.7 | 62.8 | 71.3 | 71.4 |
| clustered-v6-unique | default | 128 | uniform | 37.0 | 52.8 | 42.3 | 50.3 | 50.2 |
| clustered-v6-unique | simd | 32 | uniform | 40.1 | 70.4 | 63.3 | 65.3 | 64.6 |
| clustered-v6-unique | default | 32 | uniform | 36.9 | 51.9 | 43.0 | 48.4 | 48.5 |
| clustered-v6-unique | simd | 4 | uniform | 40.4 | 70.3 | 63.6 | 65.0 | 65.1 |
| clustered-v6-unique | default | 4 | uniform | 38.3 | 53.7 | 43.9 | 45.5 | 45.3 |

## Follow-through

Keep the guarded model as a research candidate. Before production adoption,
measure large nonlinear/native-provider inputs, startup amortization, zero-copy
correction, cold/>RAM/RSS behavior, fallback-heavy traffic and other hosts.
Do not infer a timing-trained error setting or automatic selector. No full
RadixSpline/PGM competitor comparison or universal learned-index win follows.
Next in priority order: P1-08 AVX-512/NEON qualification on actual hardware;
compile-only architecture evidence cannot establish performance. P2 counters/
PGO/build qualification remains a separate task.
