# Flat B+ node-search results - 2026-11-04

Decision: retain the qualified prototype as research. No automatic RAM
cache, public API, encoded layout or default-search change is adopted.
The measurements below qualify this implementation and workload, rather
than establish a universal layout winner.

[Methodology/reproduction](README.md),
[primary matrix](../results/2026-11-04-flat-btree.json),
[fresh-process repeats](../results/2026-11-04-flat-btree-repeat.json),
[assembly/counter probe](../results/2026-11-04-flat-btree-asm.json),
[integrity verification](../results/2026-11-04-flat-btree-verification.json).

## Measurement boundary

32 primary processes and six fresh-process repeats produce 2,240 timings.
All original assertions and complete independent original-input query
oracles pass, including ordered targets/IDs, duplicates, borrowed payload
bytes/pointers, mixed families, empty/partial batches, broad-route
fallbacks and typed callback-error prefixes. Forced scalar controls also
pass. Fifteen generated-core tests include independent B+ lower/upper
bounds, incomplete levels through 4097 keys, unsigned half boundaries
and alignment/partial-node SIMD parity. Prepared sources reproduce in a
shadow checkout; header-derived capacities and receipt totals agree.

Four-trial median ns/query is aggregate warm throughput cost, not latency
percentiles. Forward/reverse ordering and one warmup corpus precede each
timing. Prior SIMD and Eyt columns report the minimum median of two
methods after measurement, with the winning ID; these are comparison
references, not dispatch policies. All trials remain in receipts.
IP112 duplicates IP108; default/SIMD builds change existing packed tails,
while explicit cross-query and node AVX2 controls exist in both builds.

Every mirror searches global fences with identical key widths. Original
IPv4 methods retain the directory, so comparisons with ordinary/prior
also change routing scope, RAM copies and encoded-key density. B+ leaves
preserve sorted positions without per-key rank maps. Packed tails,
target decoding, payload access and interval fallbacks are unchanged.
All comparator caches coexist; construction is outside lookup timing.
PlanB motivates the layout; this is not its LPM transformation, batching,
prefetch or AVX-512 implementation. See the primary reference in README.

## Primary 20M cases

| Family / shape | Build | Traffic | Ordinary | Prior SIMD (ID) | Sorted | Eyt (ID) | B+ binary | B+ count | B+ AVX2 count | B+ ordered mask |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 39.0 | 49.3 (108) | 42.4 | 42.4 (301) | 102.1 | 72.0 | 87.1 | 81.2 |
| v4 | simd | uniform | 113.2 | 106.0 (112) | 220.3 | 179.4 (302) | 254.4 | 235.4 | 241.1 | 273.6 |
| v4 | default | hot | 39.2 | 54.5 (112) | 43.0 | 40.2 (301) | 100.5 | 71.2 | 106.8 | 85.4 |
| v4 | default | uniform | 203.7 | 139.8 (108) | 256.3 | 210.0 (302) | 286.0 | 201.8 | 265.4 | 295.9 |
| v6 | default | hot | 40.6 | 64.6 (108) | 43.4 | 40.0 (301) | 100.2 | 72.1 | 105.5 | 83.2 |
| v6 | default | uniform | 210.6 | 121.4 (108) | 204.3 | 166.3 (302) | 209.3 | 171.7 | 229.5 | 288.7 |
| v6 | simd | hot | 43.0 | 65.1 (112) | 42.4 | 42.1 (301) | 102.0 | 70.4 | 85.0 | 79.1 |
| v6 | simd | uniform | 258.9 | 167.8 (112) | 222.0 | 187.4 (302) | 298.5 | 275.4 | 340.7 | 393.7 |
| asn | simd | hot | 22.1 | 33.9 (112) | 22.7 | 19.0 (301) | 47.9 | 35.5 | 37.4 | 33.6 |
| asn | simd | uniform | 185.4 | 92.2 (112) | 202.7 | 203.5 (301) | 181.3 | 147.0 | 173.1 | 238.9 |
| asn | default | hot | 22.4 | 34.1 (112) | 23.4 | 19.0 (301) | 47.9 | 35.6 | 38.2 | 33.0 |
| asn | default | uniform | 233.5 | 102.9 (108) | 235.0 | 161.0 (301) | 125.2 | 124.1 | 155.4 | 136.2 |

## Fresh-process 20M repeats

Same frozen executable/input/corpus, rebuilt caches in a new process.
Four trials cannot isolate host load, frequency or cache state, or
establish statistical significance. Compare both runs before adopting.

| Family / shape | Build | Traffic | Ordinary | Prior SIMD (ID) | Sorted | Eyt (ID) | B+ binary | B+ count | B+ AVX2 count | B+ ordered mask |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 37.9 | 49.5 (112) | 42.2 | 42.0 (301) | 101.9 | 71.8 | 87.0 | 80.1 |
| v4 | simd | uniform | 105.0 | 105.2 (108) | 198.3 | 204.1 (302) | 231.7 | 233.1 | 296.5 | 343.0 |
| v4 | default | hot | 38.6 | 49.4 (112) | 41.6 | 39.6 (301) | 100.7 | 71.1 | 105.1 | 83.0 |
| v4 | default | uniform | 144.0 | 98.1 (108) | 209.8 | 179.7 (302) | 232.2 | 195.7 | 278.1 | 318.7 |
| v6 | default | hot | 41.2 | 65.8 (108) | 45.9 | 43.4 (301) | 103.6 | 75.2 | 109.9 | 84.8 |
| v6 | default | uniform | 289.9 | 155.1 (112) | 264.7 | 180.5 (302) | 269.6 | 238.3 | 283.1 | 366.8 |
| v6 | simd | hot | 43.7 | 63.9 (112) | 41.9 | 42.0 (301) | 102.2 | 70.8 | 84.7 | 78.7 |
| v6 | simd | uniform | 226.2 | 152.3 (108) | 233.3 | 223.0 (302) | 278.1 | 222.3 | 262.4 | 347.8 |
| asn | simd | hot | 22.2 | 33.6 (112) | 23.3 | 19.1 (301) | 47.6 | 35.1 | 37.4 | 32.8 |
| asn | simd | uniform | 285.3 | 127.1 (108) | 267.3 | 205.7 (301) | 216.9 | 175.9 | 190.3 | 158.5 |
| asn | default | hot | 22.9 | 34.1 (112) | 23.4 | 21.7 (301) | 49.0 | 36.4 | 37.3 | 32.5 |
| asn | default | uniform | 171.2 | 99.7 (108) | 259.7 | 164.4 (301) | 228.9 | 218.1 | 274.3 | 242.8 |

Fresh default-build comparisons (ns/query):

- v4 uniform: B+ 401 195.7; ordinary 144.0; sorted 209.8; cross-query108 98.1.
- v4 hot: B+ 401 71.1; ordinary 38.6; sorted 41.6; cross-query108 49.8.
- v6 uniform: B+ 401 238.3; ordinary 289.9; sorted 264.7; cross-query108 165.9.
- v6 hot: B+ 401 75.2; ordinary 41.2; sorted 45.9; cross-query108 65.8.
- asn uniform: B+ 401 218.1; ordinary 171.2; sorted 259.7; cross-query108 99.7.
- asn hot: B+ 404 32.5; ordinary 22.9; sorted 23.4; cross-query108 39.9.

## Construction and memory

Capacity bytes include leaf/internal-key arrays and level-range elements;
they exclude Vec/Cache objects, allocator overhead, mapping/validation
memory and RSS. B+ leaves are separate from the sorted control. All
layouts coexist; construction time includes all layouts, not an isolated
B+ build or original mapping/validation. The 512 MiB combined logical
allocation budget is checked before construction.

| Family | Build | B+ leaves MiB | Internal keys MiB | Level bytes | B+ total MiB | All caches MiB | Primary build ms | Fresh build ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 1.192 | 0.397 | 128 | 1.590 | 4.272 | 1.290 | 1.133 |
| v4 | default | 1.192 | 0.397 | 128 | 1.590 | 4.272 | 1.212 | 1.123 |
| v6 | default | 1.192 | 0.397 | 128 | 1.590 | 4.272 | 1.726 | 1.202 |
| v6 | simd | 1.192 | 0.397 | 128 | 1.590 | 4.272 | 1.188 | 1.211 |
| asn | simd | 76.294 | 5.086 | 96 | 81.380 | 310.262 | 239.408 | 168.241 |
| asn | default | 76.294 | 5.086 | 96 | 81.380 | 310.262 | 124.115 | 119.829 |

No encoded file bytes change. Candidate-only deployment might discard
the comparison caches, but its startup/RSS and API/resource policy need
separate measurements. Natural Vec alignment and unpadded internal-level
starts do not guarantee a node fits within one physical cache line.

## Primary 1M cases

| Family / shape | Build | Traffic | Ordinary | Prior SIMD (ID) | Sorted | Eyt (ID) | B+ binary | B+ count | B+ AVX2 count | B+ ordered mask |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | default | hot | 38.5 | 48.2 (112) | 39.0 | 39.0 (301) | 76.6 | 57.6 | 65.6 | 63.5 |
| v4 | default | uniform | 77.4 | 62.6 (112) | 119.9 | 72.6 (302) | 104.3 | 88.1 | 99.6 | 133.5 |
| v4 | simd | hot | 37.3 | 48.5 (112) | 38.3 | 38.1 (301) | 79.8 | 59.7 | 65.1 | 63.6 |
| v4 | simd | uniform | 59.4 | 66.9 (112) | 111.7 | 75.4 (302) | 110.0 | 90.6 | 98.7 | 120.0 |
| v6 | simd | hot | 41.2 | 57.2 (112) | 39.3 | 39.1 (301) | 82.0 | 59.8 | 67.9 | 67.1 |
| v6 | simd | uniform | 109.0 | 77.0 (112) | 113.8 | 75.8 (302) | 112.3 | 93.7 | 98.9 | 122.0 |
| v6 | default | hot | 40.2 | 57.7 (112) | 39.4 | 39.3 (301) | 77.7 | 56.7 | 68.3 | 64.8 |
| v6 | default | uniform | 123.5 | 74.4 (112) | 124.8 | 77.0 (302) | 107.6 | 91.8 | 103.7 | 137.6 |
| asn | default | hot | 17.8 | 28.0 (112) | 18.3 | 11.4 (301) | 35.5 | 28.0 | 25.7 | 22.3 |
| asn | default | uniform | 53.0 | 41.9 (112) | 57.1 | 59.4 (302) | 50.8 | 46.7 | 48.9 | 46.5 |
| asn | simd | hot | 17.6 | 27.9 (112) | 18.2 | 10.7 (301) | 35.1 | 27.4 | 25.5 | 21.7 |
| asn | simd | uniform | 55.3 | 41.8 (112) | 58.0 | 59.3 (302) | 51.3 | 47.5 | 48.4 | 44.4 |

## Shape controls (65,536 assertions)

The 256-query warm corpus can favor predictable branches/cache residency.
Deep-prefix/duplicate-IP interval cases measure existing fallback overhead,
not B+ interval traversal. Shared/unique payloads remain separate.

| Family / shape | Build | Traffic | Ordinary | Prior SIMD (ID) | Sorted | Eyt (ID) | B+ binary | B+ count | B+ AVX2 count | B+ ordered mask |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense-asn-shared | default | uniform | 19.0 | 27.7 (112) | 17.4 | 6.9 (301) | 16.9 | 14.3 | 14.1 | 14.5 |
| dense-asn-shared | simd | uniform | 20.0 | 28.0 (112) | 17.7 | 8.7 (301) | 17.1 | 14.4 | 13.9 | 14.3 |
| dense-asn-unique | simd | uniform | 21.6 | 28.4 (112) | 20.1 | 9.8 (301) | 20.0 | 15.5 | 15.8 | 15.5 |
| dense-asn-unique | default | uniform | 20.9 | 27.9 (112) | 19.7 | 7.2 (301) | 19.0 | 15.1 | 15.3 | 14.9 |
| sparse-asn-shared | default | uniform | 21.3 | 30.9 (112) | 21.7 | 9.0 (301) | 26.9 | 24.4 | 23.2 | 20.3 |
| sparse-asn-shared | simd | uniform | 21.5 | 30.1 (112) | 21.1 | 9.0 (301) | 27.0 | 24.2 | 21.7 | 20.1 |
| sparse-asn-unique | simd | uniform | 23.5 | 30.5 (112) | 23.4 | 9.7 (301) | 28.6 | 25.2 | 22.6 | 20.9 |
| sparse-asn-unique | default | uniform | 23.5 | 31.1 (112) | 25.7 | 9.0 (301) | 28.1 | 24.8 | 22.3 | 20.3 |
| sparse-v6-shared | default | uniform | 40.9 | 57.5 (108) | 41.7 | 41.6 (301) | 63.7 | 52.2 | 58.9 | 52.8 |
| sparse-v6-shared | simd | uniform | 43.0 | 58.5 (112) | 42.8 | 45.8 (301) | 71.8 | 55.8 | 62.1 | 55.4 |
| sparse-v6-unique | simd | uniform | 41.6 | 56.5 (112) | 41.0 | 40.9 (301) | 64.9 | 52.2 | 60.4 | 53.2 |
| sparse-v6-unique | default | uniform | 50.4 | 60.3 (112) | 44.0 | 46.0 (301) | 65.5 | 52.1 | 58.6 | 53.1 |
| deep-v6-shared | default | uniform | 1076.2 | 1089.2 (112) | 1085.6 | 1083.5 (301) | 1086.1 | 1077.1 | 1078.1 | 1104.0 |
| deep-v6-shared | simd | uniform | 1081.2 | 1090.0 (112) | 1085.3 | 1084.7 (301) | 1096.0 | 1090.5 | 1099.3 | 1089.4 |
| deep-v6-unique | simd | uniform | 1083.9 | 1076.8 (108) | 1090.2 | 1080.5 (302) | 1092.7 | 1078.6 | 1075.8 | 1068.4 |
| deep-v6-unique | default | uniform | 1072.1 | 1085.6 (112) | 1072.2 | 1068.3 (302) | 1064.6 | 1069.1 | 1064.4 | 1072.1 |
| duplicates-v4-shared | default | uniform | 861.5 | 865.3 (108) | 863.8 | 860.8 (302) | 858.3 | 862.8 | 860.5 | 858.4 |
| duplicates-v4-shared | simd | uniform | 868.7 | 861.2 (112) | 870.9 | 861.6 (301) | 867.0 | 863.2 | 859.7 | 858.4 |
| duplicates-v4-unique | simd | uniform | 867.8 | 867.5 (112) | 876.2 | 870.0 (301) | 875.9 | 874.8 | 868.8 | 880.8 |
| duplicates-v4-unique | default | uniform | 883.1 | 881.8 (108) | 876.8 | 868.1 (302) | 882.2 | 881.8 | 872.2 | 885.5 |

## Linked code and limitations

Complete B+ bound specializations and explicit AVX2 helpers are retained
in the assembly receipt. Static whole-function instruction/branch counts
include dispatch/fallback/bounds-failure paths; they are not runtime
counts or causal explanations of measured changes. Count401 allows
compiler auto-vectorization; it is not a forced scalar instruction test.

The exploratory implementation motivated an additional ordered-mask
variant: portable AVX2 count32 emitted software population-count
arithmetic. Final receipts rerun the entire matrix/repeat set with404
included; exploratory timings are not merged. Ordered404 exploits the
sorted-prefix invariant and can stop after the first partial vector.
It changes both mask reduction and early-exit behavior, so differences
from402 do not isolate a single instruction or prove causality.

Linked ordered kernels use vector compares/movemasks and TZCNT-encoded
prefix reduction. The count32 kernel retains software population-count
arithmetic (multiplications/shifts) in this portable build. An outlined
node-dispatch closure and four ISA-detection wrappers remain in linked
code; their complete bodies are retained too. Specializing/inlining the
dispatch and hoisting feature detection are concrete P1-05 ablations.
Instruction presence identifies mechanisms, not their runtime cost.

| Build | Symbol | Bytes | Conditional moves | Set condition | Conditional jumps |
| --- | --- | ---: | ---: | ---: | ---: |
| default | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound` | 382 | 3 | 0 | 7 |
| default | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound` | 1133 | 5 | 0 | 36 |
| default | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound::{{closure}}` | 594 | 6 | 10 | 12 |
| default | `netindex_flat_btree_core::study_flat_btree::rank32` | 414 | 0 | 0 | 9 |
| default | `netindex_flat_btree_core::study_flat_btree::count32` | 652 | 0 | 0 | 12 |
| default | `netindex_flat_btree_core::study_flat_btree::rank128` | 335 | 1 | 2 | 6 |
| default | `netindex_flat_btree_core::study_flat_btree::count128` | 318 | 1 | 2 | 6 |
| default | `<u32 as netindex_flat_btree_core::study_flat_btree::Key>::ordered_rank` | 345 | 1 | 2 | 8 |
| default | `<u32 as netindex_flat_btree_core::study_flat_btree::Key>::vector_count` | 345 | 1 | 2 | 8 |
| default | `<u128 as netindex_flat_btree_core::study_flat_btree::Key>::ordered_rank` | 353 | 3 | 6 | 7 |
| default | `<u128 as netindex_flat_btree_core::study_flat_btree::Key>::vector_count` | 353 | 3 | 6 | 7 |
| simd | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound` | 382 | 3 | 0 | 7 |
| simd | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound` | 1133 | 5 | 0 | 36 |
| simd | `netindex_flat_btree_core::study_flat_btree::Tree<K>::bound::{{closure}}` | 594 | 6 | 10 | 12 |
| simd | `netindex_flat_btree_core::study_flat_btree::rank32` | 414 | 0 | 0 | 9 |
| simd | `netindex_flat_btree_core::study_flat_btree::count32` | 652 | 0 | 0 | 12 |
| simd | `netindex_flat_btree_core::study_flat_btree::rank128` | 335 | 1 | 2 | 6 |
| simd | `netindex_flat_btree_core::study_flat_btree::count128` | 318 | 1 | 2 | 6 |
| simd | `<u32 as netindex_flat_btree_core::study_flat_btree::Key>::ordered_rank` | 345 | 1 | 2 | 8 |
| simd | `<u32 as netindex_flat_btree_core::study_flat_btree::Key>::vector_count` | 345 | 1 | 2 | 8 |
| simd | `<u128 as netindex_flat_btree_core::study_flat_btree::Key>::ordered_rank` | 353 | 3 | 6 | 7 |
| simd | `<u128 as netindex_flat_btree_core::study_flat_btree::Key>::vector_count` | 353 | 3 | 6 | 7 |

Hardware-counter probe: **unavailable**. No workload counters were collected.

This x86-64/WSL experiment has synthetic fixed warm corpora and uncontrolled
host effects. It does not establish cold/>RAM/native-provider capacity,
MMDB superiority, tail latency, significance or other-architecture performance.
No prefetch, AVX-512/NEON, new cross-query batching, cache-line alignment
guarantee or candidate-only RSS/startup result is introduced.

Next: P1-05 traversal/prefetch/alignment ablations; P1-06 owns optional
memory-budgeted selection and directory-preserving variants. Keep this
node-search evidence as a candidate, not an automatic deployment policy.
