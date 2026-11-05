# Traversal, prefetch and alignment results - 2026-11-04

Decision: retain the measured candidates as research for memory-budgeted
selection. No automatic cache, production default, public API or format
change is adopted. These results qualify the tested implementations and
workloads; they do not establish universal dominance.

[Reproduction](README.md), [primary matrix](../results/2026-11-04-traversal.json),
[fresh-process repeats](../results/2026-11-04-traversal-repeat.json),
[assembly/counter probe](../results/2026-11-04-traversal-asm.json),
[integrity verification](../results/2026-11-04-traversal-verification.json).

## Scope and correctness

32 primary processes and six fresh-process repeats yield 3,808 timings.
All original assertions and complete independent original-input oracles
pass, including ordered targets/IDs, duplicates, borrowed payload bytes/
pointers, partial/empty/mixed batches, broad-route fallback and typed
callback-error prefixes. Forced scalar paths and 17 generated-core tests
pass. Independent linear bounds cover all eight Eytzinger combinations,
specialized B+ traversal, incomplete trees through 4097, duplicates and
unsigned boundaries. Preparation reproduces in a shadow checkout; hashes,
raw logs, header-derived capacities, alignment and timing coordinates agree.

Four-trial median ns/query is aggregate warm throughput cost, not query/
batch latency percentiles. One warmup corpus precedes every timing, with
forward/reverse trial ordering. All comparators use the same executable,
input and resident caches within each build. Default/SIMD builds change
existing packed tails; explicit AVX2 methods exist in both builds.

New Eytzinger methods share identical 64-byte group accessors. Factors:
**N/64** natural/guaranteed 64-byte alignment; **C/A** candidate/arithmetic child
selection; **P** guarded T0 prefetch. Columns 500..507 cover the complete
2x2x2 design. Natural base alignment is recorded, not assumed.
Prefetch targets one descendant group four ASN/two IP levels ahead,
clipped before pointer formation. Only group-start line is requested;
natural groups can straddle lines. No distance/hint sweep is measured.

All mirrors search global fences. Original IPv4 ordinary/prior methods
retain their directory, changing routing scope and key density in those
comparisons. Existing packed tails, decoding and borrowed emission follow.
Interval/broad-route readers retain their ordinary visitor; their shape
measurements are fallback controls rather than prefetched interval trees.

## Primary 20M Eytzinger factorial

| Family / shape | Build | Traffic | Ordinary | N C500 | N A501 | N C+P502 | N A+P503 | 64 C504 | 64 A505 | 64 C+P506 | 64 A+P507 |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 37.6 | 47.1 | 68.9 | 50.0 | 70.4 | 44.4 | 67.0 | 49.7 | 70.2 |
| v4 | simd | uniform | 104.1 | 205.2 | 208.4 | 211.8 | 201.9 | 255.9 | 233.4 | 193.1 | 179.8 |
| v4 | default | hot | 38.1 | 44.3 | 64.6 | 47.5 | 67.5 | 42.1 | 64.0 | 44.0 | 67.1 |
| v4 | default | uniform | 198.7 | 196.5 | 212.3 | 190.1 | 149.6 | 240.8 | 198.2 | 237.9 | 176.2 |
| v6 | default | hot | 39.6 | 44.0 | 63.7 | 47.5 | 66.8 | 42.4 | 64.8 | 44.1 | 67.4 |
| v6 | default | uniform | 199.6 | 155.9 | 161.0 | 157.4 | 120.8 | 192.5 | 170.4 | 192.2 | 122.3 |
| v6 | simd | hot | 42.5 | 47.7 | 67.4 | 50.9 | 70.3 | 45.2 | 70.3 | 50.8 | 70.3 |
| v6 | simd | uniform | 181.1 | 157.0 | 175.5 | 153.3 | 138.1 | 170.5 | 173.3 | 152.0 | 137.7 |
| asn | simd | hot | 22.1 | 27.3 | 54.3 | 33.3 | 55.8 | 49.4 | 55.3 | 26.2 | 56.0 |
| asn | simd | uniform | 181.7 | 187.0 | 375.9 | 309.0 | 378.9 | 210.1 | 449.2 | 323.1 | 372.6 |
| asn | default | hot | 21.9 | 24.6 | 48.0 | 30.5 | 54.1 | 17.3 | 49.5 | 24.9 | 54.7 |
| asn | default | uniform | 176.1 | 206.3 | 474.8 | 360.5 | 461.8 | 226.6 | 352.3 | 296.6 | 434.5 |

## Fresh-process 20M Eytzinger factorial

Same frozen executable/input/corpus with rebuilt caches in new processes.
Trial medians cannot isolate WSL host load/frequency/cache drift or prove
statistical significance. Compare both primary and repeat evidence.

| Family / shape | Build | Traffic | Ordinary | N C500 | N A501 | N C+P502 | N A+P503 | 64 C504 | 64 A505 | 64 C+P506 | 64 A+P507 |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 40.1 | 49.7 | 67.4 | 52.8 | 71.2 | 46.7 | 73.4 | 52.2 | 73.8 |
| v4 | simd | uniform | 155.0 | 177.8 | 202.2 | 173.9 | 166.8 | 209.6 | 193.5 | 177.1 | 199.4 |
| v4 | default | hot | 38.0 | 44.1 | 64.0 | 47.0 | 67.3 | 41.8 | 66.9 | 43.7 | 66.6 |
| v4 | default | uniform | 163.4 | 170.2 | 171.3 | 158.5 | 140.7 | 202.6 | 174.3 | 203.5 | 137.9 |
| v6 | default | hot | 39.8 | 44.6 | 64.5 | 47.9 | 67.4 | 42.1 | 64.0 | 44.4 | 67.3 |
| v6 | default | uniform | 200.9 | 176.1 | 193.7 | 161.0 | 139.3 | 229.8 | 189.4 | 203.5 | 122.4 |
| v6 | simd | hot | 43.2 | 47.3 | 67.9 | 50.3 | 71.2 | 44.3 | 71.5 | 49.9 | 70.6 |
| v6 | simd | uniform | 206.5 | 154.4 | 180.2 | 147.4 | 139.3 | 179.9 | 176.1 | 150.5 | 137.2 |
| asn | simd | hot | 22.1 | 27.5 | 54.2 | 32.9 | 54.9 | 48.1 | 51.3 | 25.2 | 54.9 |
| asn | simd | uniform | 159.1 | 221.5 | 455.0 | 317.9 | 391.3 | 226.8 | 452.0 | 331.3 | 333.6 |
| asn | default | hot | 22.1 | 26.1 | 50.9 | 30.6 | 54.6 | 17.4 | 49.2 | 24.5 | 54.6 |
| asn | default | uniform | 259.0 | 196.5 | 422.0 | 319.2 | 373.7 | 215.9 | 443.5 | 335.6 | 339.5 |

Fresh default-build paired arithmetic comparisons (ns/query):

- v4 uniform: natural/noP501 171.3, aligned/noP505 174.3, aligned/P507 137.9; ordinary 163.4.
- v4 hot: natural/noP501 64.0, aligned/noP505 66.9, aligned/P507 66.6; ordinary 38.0.
- v6 uniform: natural/noP501 193.7, aligned/noP505 189.4, aligned/P507 122.4; ordinary 200.9.
- v6 hot: natural/noP501 64.5, aligned/noP505 64.0, aligned/P507 67.3; ordinary 39.8.
- asn uniform: natural/noP501 422.0, aligned/noP505 443.5, aligned/P507 339.5; ordinary 259.0.
- asn hot: natural/noP501 50.9, aligned/noP505 49.2, aligned/P507 54.6; ordinary 22.1.

## B+ dispatch/specialization intervention

401/404 retain legacy count/ordered visitors. 520 specializes count;
521 specializes the AVX2 walk, makes direct helpers inline-capable and
detects AVX2 outside node loops. Unsupported/disabled cases use count.
These are combined implementation interventions, not independent causal
estimates of inlining, dispatch and feature detection. Arrays are shared.

Primary 20M:

| Family / shape | Build | Traffic | Ordinary | Prior108 | Sorted300 | Eyt301 | Eyt302 | Old count401 | Old ordered404 | Count520 | Ordered521 |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 37.6 | 49.6 | 42.2 | 41.5 | 59.5 | 71.1 | 81.6 | 55.3 | 79.3 |
| v4 | simd | uniform | 104.1 | 103.8 | 220.6 | 198.9 | 166.9 | 175.6 | 289.1 | 201.2 | 285.9 |
| v4 | default | hot | 38.1 | 49.5 | 42.0 | 40.0 | 61.3 | 70.5 | 83.0 | 53.8 | 76.7 |
| v4 | default | uniform | 198.7 | 121.2 | 242.9 | 257.7 | 218.7 | 184.4 | 317.1 | 176.8 | 303.9 |
| v6 | default | hot | 39.6 | 64.8 | 42.4 | 40.5 | 58.7 | 70.4 | 83.7 | 53.0 | 74.9 |
| v6 | default | uniform | 199.6 | 106.8 | 200.0 | 188.0 | 155.8 | 155.2 | 247.9 | 126.7 | 242.6 |
| v6 | simd | hot | 42.5 | 64.1 | 42.0 | 41.9 | 59.5 | 71.5 | 79.7 | 55.3 | 78.2 |
| v6 | simd | uniform | 181.1 | 111.0 | 187.8 | 182.3 | 171.9 | 168.7 | 253.9 | 140.2 | 203.0 |
| asn | simd | hot | 22.1 | 39.5 | 22.5 | 19.6 | 27.8 | 35.0 | 32.5 | 34.1 | 28.7 |
| asn | simd | uniform | 181.7 | 98.7 | 224.2 | 161.9 | 217.2 | 131.6 | 156.8 | 181.1 | 138.1 |
| asn | default | hot | 21.9 | 40.0 | 22.3 | 19.0 | 28.5 | 35.0 | 32.6 | 34.6 | 29.6 |
| asn | default | uniform | 176.1 | 93.1 | 243.3 | 175.2 | 219.1 | 138.8 | 169.9 | 203.9 | 127.2 |

Fresh 20M repeats:

| Family / shape | Build | Traffic | Ordinary | Prior108 | Sorted300 | Eyt301 | Eyt302 | Old count401 | Old ordered404 | Count520 | Ordered521 |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 40.1 | 49.6 | 42.5 | 42.4 | 61.8 | 74.8 | 81.9 | 57.3 | 79.5 |
| v4 | simd | uniform | 155.0 | 136.5 | 294.3 | 254.0 | 198.2 | 218.4 | 312.3 | 223.4 | 295.7 |
| v4 | default | hot | 38.0 | 50.1 | 42.0 | 40.1 | 61.8 | 71.3 | 84.5 | 52.8 | 75.8 |
| v4 | default | uniform | 163.4 | 127.9 | 267.1 | 237.5 | 187.8 | 174.2 | 266.6 | 154.8 | 270.4 |
| v6 | default | hot | 39.8 | 64.8 | 42.3 | 40.5 | 62.7 | 71.2 | 84.0 | 53.6 | 75.0 |
| v6 | default | uniform | 200.9 | 105.7 | 210.5 | 202.5 | 165.6 | 192.9 | 265.3 | 140.5 | 254.8 |
| v6 | simd | hot | 43.2 | 63.8 | 42.2 | 41.9 | 59.9 | 72.0 | 79.5 | 55.0 | 78.0 |
| v6 | simd | uniform | 206.5 | 112.2 | 192.6 | 178.4 | 167.0 | 166.4 | 255.8 | 145.8 | 218.0 |
| asn | simd | hot | 22.1 | 39.5 | 22.1 | 18.2 | 28.3 | 35.0 | 32.3 | 33.8 | 28.3 |
| asn | simd | uniform | 159.1 | 90.9 | 219.9 | 167.7 | 306.5 | 179.4 | 160.6 | 183.2 | 140.4 |
| asn | default | hot | 22.1 | 39.4 | 22.7 | 20.7 | 29.5 | 35.2 | 32.3 | 34.4 | 29.0 |
| asn | default | uniform | 259.0 | 105.7 | 299.9 | 196.9 | 333.4 | 217.7 | 156.0 | 125.4 | 102.8 |

## Capacity, construction and physical alignment

All arrays coexist even for ordinary timings. Capacities include sentinel/
group padding and level-range elements, exclude allocator/object overhead,
mapping/validation memory and RSS. Build time covers all layouts together,
not candidate-only startup; the requested logical budget is 512 MiB.

| Family | Build | New mirrors MiB | All caches MiB | Natural/64 base mod 64 | Primary build ms | Fresh build ms |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| v4 | simd | 2.385 | 6.656 | [32, 0] | 2.114 | 1.778 |
| v4 | default | 2.385 | 6.656 | [32, 0] | 1.653 | 1.764 |
| v6 | default | 2.385 | 6.656 | [0, 0] | 1.892 | 2.240 |
| v6 | simd | 2.385 | 6.656 | [0, 0] | 1.645 | 1.971 |
| asn | simd | 152.588 | 462.850 | [16, 0] | 341.219 | 263.227 |
| asn | default | 152.588 | 462.850 | [16, 0] | 156.697 | 156.190 |

Per-process actual alignments and each array capacity remain in receipts.
The grouped mirror vs legacy Eytzinger comparison changes accessors and
specialization as well as storage. Use the factorial columns for paired
alignment/prefetch/traversal comparisons. Candidate-only memory/startup
would differ if comparison caches were removed; it is not measured here.

## 1M and shape controls

Below, Eyt best is the minimum median over 500..507 after measurement,
with its ID. This is a comparison summary, not a measured dispatch policy.
Full per-method trials remain in the primary receipt. Shapes reuse a
256-query corpus and can favor predictable branches/cache residency.

| Records | Family / shape | Build | Traffic | Ordinary | Eyt best (ID) | Count520 | Ordered521 |
| ---: | --- | --- | --- | ---: | ---: | ---: | ---: |
| 1000000 | v4 | default | hot | 37.9 | 39.4 (504) | 44.2 | 59.8 |
| 1000000 | v4 | default | uniform | 76.2 | 74.5 (507) | 69.9 | 128.1 |
| 1000000 | v4 | simd | hot | 37.4 | 40.3 (504) | 48.2 | 59.5 |
| 1000000 | v4 | simd | uniform | 59.3 | 76.5 (507) | 78.6 | 105.6 |
| 1000000 | v6 | simd | hot | 41.3 | 42.6 (500) | 48.9 | 61.0 |
| 1000000 | v6 | simd | uniform | 107.2 | 77.3 (507) | 75.9 | 104.9 |
| 1000000 | v6 | default | hot | 38.3 | 39.7 (504) | 44.1 | 60.1 |
| 1000000 | v6 | default | uniform | 116.0 | 74.0 (507) | 70.0 | 129.0 |
| 1000000 | asn | default | hot | 17.3 | 12.9 (500) | 27.0 | 18.6 |
| 1000000 | asn | default | uniform | 51.1 | 58.8 (507) | 46.4 | 42.1 |
| 1000000 | asn | simd | hot | 17.3 | 14.5 (500) | 26.2 | 17.7 |
| 1000000 | asn | simd | uniform | 51.5 | 60.1 (507) | 46.2 | 39.3 |
| 65536 | dense-asn-shared | default | uniform | 18.8 | 10.2 (504) | 14.0 | 10.3 |
| 65536 | dense-asn-shared | simd | uniform | 19.3 | 10.4 (500) | 13.5 | 9.9 |
| 65536 | dense-asn-unique | simd | uniform | 21.1 | 10.8 (500) | 14.2 | 10.7 |
| 65536 | dense-asn-unique | default | uniform | 21.0 | 10.6 (504) | 14.4 | 11.2 |
| 65536 | sparse-asn-shared | default | uniform | 20.1 | 10.4 (504) | 22.7 | 13.2 |
| 65536 | sparse-asn-shared | simd | uniform | 21.6 | 10.5 (500) | 23.0 | 13.9 |
| 65536 | sparse-asn-unique | simd | uniform | 22.3 | 11.2 (500) | 23.5 | 14.2 |
| 65536 | sparse-asn-unique | default | uniform | 22.4 | 11.2 (500) | 23.8 | 14.7 |
| 65536 | sparse-v6-shared | default | uniform | 40.3 | 40.3 (500) | 43.3 | 50.8 |
| 65536 | sparse-v6-shared | simd | uniform | 41.3 | 41.1 (500) | 44.1 | 53.3 |
| 65536 | sparse-v6-unique | simd | uniform | 41.3 | 41.4 (500) | 44.9 | 52.3 |
| 65536 | sparse-v6-unique | default | uniform | 40.5 | 41.1 (504) | 44.0 | 50.8 |
| 65536 | deep-v6-shared | default | uniform | 1076.1 | 1059.3 (506) | 1067.8 | 1063.4 |
| 65536 | deep-v6-shared | simd | uniform | 1098.9 | 1070.0 (503) | 1065.9 | 1069.8 |
| 65536 | deep-v6-unique | simd | uniform | 1105.9 | 1097.4 (501) | 1093.0 | 1111.4 |
| 65536 | deep-v6-unique | default | uniform | 1114.7 | 1098.2 (507) | 1095.3 | 1097.1 |
| 65536 | duplicates-v4-shared | default | uniform | 855.5 | 854.1 (507) | 866.1 | 858.8 |
| 65536 | duplicates-v4-shared | simd | uniform | 867.2 | 861.0 (503) | 863.8 | 860.6 |
| 65536 | duplicates-v4-unique | simd | uniform | 863.2 | 859.7 (505) | 866.4 | 864.5 |
| 65536 | duplicates-v4-unique | default | uniform | 880.6 | 875.1 (506) | 881.9 | 877.4 |

## Linked assembly and limitations

Full bounded Eytzinger factor/two-width walks and specialized B+ walks,
legacy dispatch and wrappers are retained. Static instruction/branch/
prefetch counts include all helper paths, not runtime execution counts.
Arithmetic source is not a claim of a fully branch-free algorithm.

All four specialized B+ walks per executable contain no calls before
their normal return; the three remaining call sites in each body lie
in bounds-failure blocks after that return. Both AVX2 key-width walks
contain direct vector comparisons: node helpers are inlined. ISA detection
is outside the node loop in source. These code observations establish
mechanisms, not individual runtime savings or a causal speedup split.

| Build | Eyt walks | Walks with T0 | T0 sites | B+ count/vector walks |
| --- | ---: | ---: | ---: | ---: |
| default | 16 | 8 | 8 | 4 |
| simd | 16 | 8 | 8 | 4 |

Hardware-counter probe: **unavailable**. No workload counters collected.

One x86-64 WSL host and synthetic warm corpora cannot establish cold/>RAM,
native-provider, latency-percentile, significance or cross-architecture
performance. No PGO/LTO, AVX-512/NEON, new batch kernel or on-disk
reordering is introduced. ISA/prefetch instructions identify mechanisms;
without counters they do not establish why a timing changed.

Next: P1-06 optional memory-budgeted strategy selection, including native
IPv4 key widths and directory-aware forests. Keep default/adaptive policy
decisions separate from post-hoc fastest-variant summaries.
