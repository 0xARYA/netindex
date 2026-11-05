# Eytzinger RAM-layout results - 2026-11-04

Decision: retain the measured research layouts; do not adopt a library default
or format change. Candidate tracking helps these ASN controls, but global IP
heaps and arithmetic traversal regress elsewhere, with added RAM/startup costs.

[Reproduction and methodology](README.md), [primary matrix](../results/2026-11-04-eytzinger.json),
[fresh repeats](../results/2026-11-04-eytzinger-repeat.json),
[linked-code/counter probe](../results/2026-11-04-eytzinger-asm.json),
[integrity verification](../results/2026-11-04-eytzinger-verification.json).

## Measurement boundary

An earlier matrix was interrupted by a filesystem I/O failure and WSL shutdown.
Its timings were not accepted. After storage/VM recovery, the entire primary
matrix and all fresh-process cases were rerun; receipts below are from that
post-recovery run only. No partial pre-recovery timing is merged into them.

32 primary processes plus six fresh repeats produced 1,792 timing measurements.
All original assertions, complete original-target oracle matches, scalar output,
borrowed pointers, duplicates, mixed families, empty/partial batches, broad-route
fallbacks and typed callback-error prefixes pass. Thirteen generated-core tests
include linear lower/upper-bound oracles for incomplete trees and unsigned
u32/u128 extremes. Final prepared sources reproduce in a shadow checkout.

Four-trial median aggregate ns/query follows; these are throughput costs, not
query/batch latency percentiles. One warmup corpus precedes each timing.
Best prior is the minimum median across interleaving 8/16 and cross-query SIMD
108/112 for that row. It is a post-hoc comparison reference, not a dispatch policy.
All individual trials and methods remain in receipts. IP112 duplicates IP108.

Ordinary/prior IPv4 paths keep their directory. Sorted mirror and both Eytzinger
methods search all packed fences, using identical key widths. Comparing with
the sorted mirror assesses the layout/rank-mapping tradeoff; comparing with
ordinary also changes key density, RAM copying and IPv4 routing scope.
Packed tail search, decoding/payload access and interval fallbacks are unchanged.
All mirrors stay resident during every comparator. Lookups exclude cache build.
No on-disk layout or production library code changes.

## Primary 20M cases

| Family / shape | Build | Traffic | Ordinary | Best prior (ID) | Sorted mirror | Eyt candidate | Eyt path |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 38.2 | 49.8 (108) | 42.1 | 42.2 | 59.5 |
| v4 | simd | uniform | 118.8 | 102.1 (108) | 259.1 | 224.1 | 195.5 |
| v4 | default | hot | 39.5 | 50.7 (108) | 46.1 | 41.7 | 61.1 |
| v4 | default | uniform | 195.0 | 121.2 (16) | 298.1 | 261.6 | 196.8 |
| v6 | default | hot | 40.8 | 65.7 (112) | 43.1 | 40.4 | 60.8 |
| v6 | default | uniform | 261.6 | 133.7 (112) | 266.6 | 268.9 | 221.3 |
| v6 | simd | hot | 44.8 | 66.9 (108) | 44.5 | 44.1 | 62.9 |
| v6 | simd | uniform | 300.6 | 198.7 (108) | 312.8 | 274.6 | 211.1 |
| asn | simd | hot | 22.0 | 34.2 (112) | 24.1 | 20.2 | 28.1 |
| asn | simd | uniform | 261.8 | 114.6 (108) | 336.5 | 210.9 | 398.9 |
| asn | default | hot | 22.3 | 33.9 (112) | 22.5 | 18.3 | 28.1 |
| asn | default | uniform | 324.8 | 134.1 (8) | 444.1 | 227.4 | 395.5 |

## Fresh-process 20M repeats

Every family/build repeats in a fresh process with the same executable, input
and corpus. Startup rebuilds all RAM mirrors. Four trials per method cannot
control host load/frequency/cache state or establish statistical significance.

| Family / shape | Build | Traffic | Ordinary | Best prior (ID) | Sorted mirror | Eyt candidate | Eyt path |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | hot | 39.3 | 49.2 (112) | 43.3 | 43.5 | 62.6 |
| v4 | simd | uniform | 105.4 | 125.1 (8) | 264.5 | 345.0 | 265.1 |
| v4 | default | hot | 39.4 | 50.8 (108) | 44.6 | 40.4 | 59.5 |
| v4 | default | uniform | 162.8 | 90.1 (108) | 232.0 | 184.4 | 154.4 |
| v6 | default | hot | 40.0 | 66.2 (112) | 43.7 | 40.4 | 61.2 |
| v6 | default | uniform | 300.5 | 155.0 (108) | 345.3 | 329.5 | 271.2 |
| v6 | simd | hot | 44.7 | 65.4 (112) | 43.7 | 43.7 | 60.7 |
| v6 | simd | uniform | 190.1 | 142.5 (108) | 230.9 | 183.3 | 175.2 |
| asn | simd | hot | 23.3 | 34.5 (112) | 23.2 | 20.6 | 30.6 |
| asn | simd | uniform | 204.5 | 92.7 (108) | 223.2 | 187.7 | 302.1 |
| asn | default | hot | 22.5 | 33.9 (112) | 23.3 | 18.4 | 26.9 |
| asn | default | uniform | 277.8 | 109.1 (108) | 260.8 | 198.4 | 324.1 |

Fresh default-build comparisons (ns/query):

- v4 uniform: Eyt 302 154.4; sorted mirror 232.0; ordinary 162.8.
- v4 hot: Eyt 301 40.4; sorted mirror 44.6; ordinary 39.4.
- v6 uniform: Eyt 302 271.2; sorted mirror 345.3; ordinary 300.5.
- v6 hot: Eyt 301 40.4; sorted mirror 43.7; ordinary 40.0.
- asn uniform: Eyt 301 198.4; sorted mirror 260.8; ordinary 277.8.
- asn hot: Eyt 301 18.4; sorted mirror 23.3; ordinary 22.5.

## Cache construction and memory

Capacity bytes cover array elements only: no allocator overhead, cache metadata,
mapping, validation memory or process RSS. All mirrors coexist in these runs.
Build time includes sorted extraction, heap permutation and rank construction;
it does not isolate candidate-only startup or include original reader validation.
The 512 MiB budget bounds requested logical array sizes before allocation.

| Family | Build | Sorted MiB | Eyt keys/ranks MiB | Combined MiB | Primary build ms | Fresh build ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 1.192 | 1.490 | 2.682 | 0.879 | 0.757 |
| v4 | default | 1.192 | 1.490 | 2.682 | 0.945 | 0.780 |
| v6 | default | 1.192 | 1.490 | 2.682 | 0.937 | 0.889 |
| v6 | simd | 1.192 | 1.490 | 2.682 | 0.741 | 0.822 |
| asn | simd | 76.294 | 152.588 | 228.882 | 214.837 | 125.880 |
| asn | default | 76.294 | 152.588 | 228.882 | 108.440 | 106.230 |

Original file bytes are unchanged. Original-rank maps add to the heap cost
and restore insertion IDs/duplicate sequences and original block
positions. A computed rank mapping is not evaluated here. Shared/unique payload
files retain separate checks and measurements.

## Shape controls (65,536 assertions)

| Family / shape | Build | Traffic | Ordinary | Best prior (ID) | Sorted mirror | Eyt candidate | Eyt path |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| dense-asn-shared | default | uniform | 19.1 | 27.6 (112) | 16.4 | 6.4 | 16.1 |
| dense-asn-shared | simd | uniform | 19.1 | 27.7 (112) | 17.3 | 8.7 | 16.9 |
| dense-asn-unique | simd | uniform | 21.1 | 28.3 (112) | 18.9 | 9.5 | 19.1 |
| dense-asn-unique | default | uniform | 22.1 | 28.4 (112) | 19.0 | 6.8 | 18.7 |
| sparse-asn-shared | default | uniform | 21.5 | 27.3 (16) | 20.7 | 8.2 | 19.1 |
| sparse-asn-shared | simd | uniform | 21.0 | 29.2 (16) | 20.8 | 9.2 | 18.9 |
| sparse-asn-unique | simd | uniform | 23.7 | 30.0 (16) | 23.2 | 9.9 | 21.3 |
| sparse-asn-unique | default | uniform | 25.1 | 27.5 (16) | 22.5 | 7.3 | 20.3 |
| sparse-v6-shared | default | uniform | 40.6 | 56.2 (112) | 40.2 | 40.6 | 45.6 |
| sparse-v6-shared | simd | uniform | 41.6 | 60.9 (112) | 45.4 | 46.4 | 50.8 |
| sparse-v6-unique | simd | uniform | 41.3 | 56.6 (108) | 41.6 | 41.8 | 46.6 |
| sparse-v6-unique | default | uniform | 44.2 | 59.8 (108) | 41.3 | 43.3 | 47.1 |
| deep-v6-shared | default | uniform | 1080.3 | 1078.3 (16) | 1100.3 | 1110.5 | 1110.0 |
| deep-v6-shared | simd | uniform | 1100.9 | 1101.6 (108) | 1102.1 | 1097.5 | 1110.0 |
| deep-v6-unique | simd | uniform | 1131.9 | 1095.8 (16) | 1177.3 | 1122.0 | 1145.4 |
| deep-v6-unique | default | uniform | 1157.6 | 1124.4 (8) | 1162.8 | 1152.0 | 1099.6 |
| duplicates-v4-shared | default | uniform | 1002.6 | 997.9 (8) | 1003.7 | 991.6 | 989.7 |
| duplicates-v4-shared | simd | uniform | 873.0 | 876.0 (8) | 883.1 | 879.0 | 875.0 |
| duplicates-v4-unique | simd | uniform | 981.5 | 926.9 (108) | 933.2 | 905.7 | 925.0 |
| duplicates-v4-unique | default | uniform | 1022.8 | 1008.9 (108) | 1072.9 | 1023.4 | 1057.5 |

Deep-prefix/duplicate-address cases retain ordinary interval traversal.
They measure fallback overhead/correctness rather than Eytzinger interval trees.
Dense/sparse ASN and 16-byte-delta IPv6 controls expose small warm-index costs.

## Assembly and limitations

Each executable has two key-width specializations of Tree::bound, containing
all three mirror methods behind runtime dispatch. The linked-code receipt keeps
complete bounded helper bodies and conditional jump/move/set counts. Those
counts include all paths: they are not dynamic instructions, cache misses or
causal evidence for a timing difference. Arithmetic source is not a claim of
branch-free generated search. No explicit prefetch or cache-line alignment is
added; the research paper's tuned prefetch implementation is not reproduced.

The matching WSL perf tool is unavailable; no hardware counters were measured.
Portable release/codegen-units=1, optional existing tail SIMD, no PGO/LTO/custom
target flags. These are synthetic warm fixed corpora on one live x86-64 WSL
host. No cold/>RAM, other-architecture, native-provider or production capacity
claim follows. Cross-build or old-executable comparisons do not isolate layout.

## Generated-code observations and decision

The u32 arithmetic loop uses CMP/SETB/LEA for comparison-to-child calculation,
a conditional loop back-edge, and NOT/TZCNT/shift for path recovery, followed
by original-rank lookup and a bounds-failure path. The sorted path includes a
conditional move; candidate tracking retains data-dependent branches. These
observations confirm distinct emitted strategies without assigning a measured
cost to any instruction or claiming the entire helper is branch-free.

- P1-03 is complete as a RAM-layout comparison. Retain both Eytzinger methods
  and the sorted mirror as research controls; no default/API/format change.
- Fresh default hot ASN: candidate tracking 18.4 versus ordinary 22.5 ns/query.
  Fresh uniform ASN: 198.4 versus sorted mirror 260.8 and ordinary 277.8;
  earlier cross-query SIMD is faster at 109.1. Heap keys/ranks add 152.588 MiB
  for this 20M case; keeping the sorted control totals 228.882 MiB.
- Small ASN shape gains (roughly 6-10 versus 19-25 ns/query) use only 256
  repeatedly reused synthetic queries. They can favor predictable branches;
  do not generalize them to larger/unseen corpora or application capacity.
- IPv6 arithmetic traversal can beat the matched sorted mirror, while prior
  SIMD remains faster. Fresh SIMD IPv4 global arithmetic traversal is 265.1
  versus ordinary 105.4 ns/query. Losing the directory is a separate tradeoff;
  this result does not reject directory-preserving Eytzinger designs.
- P1-05 should test prefetch, branchless traversal and alignment separately.
  P1-06 can evaluate directory-preserving forests, computed rank mappings
  and memory-budgeted selection with independent corpora. No variant here is
  assumed to be the best possible Eytzinger implementation.
- Next priority: P1-04 flat B+ tree / PlanB-inspired node search, preserving
  complete all-match semantics. Counters, other architectures and held-out
  application PGO remain separate qualification tasks. No version bump or
  registry release is needed for this research checkpoint.
