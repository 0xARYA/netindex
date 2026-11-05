# Interleaving results — 2026-11-04

Decision: retain this research prototype; do not adopt it as a library default
or public batch API. Large uniform cases show useful gains, while hot/small
cases regress. P1-02 must investigate lower-overhead cross-query kernels next.

[Reproduction](README.md), [primary receipt](../results/2026-11-04-interleaved.json),
[fresh-process repeats](../results/2026-11-04-interleaved-repeat.json),
[generated-code inspection](../results/2026-11-04-interleaved-asm.json),
and [integrity verification](../results/2026-11-04-interleaved-verification.json).

## Primary matrix

All 32 processes and 880 timing measurements passed. Every original assertion
was checked; independent original-target oracles and full scalar/prototype match
sequences agree, including query order, ID, target, payload, multiplicity and
borrowed pointers. Mixed-family, range, empty, partial-batch, broad-route and
typed visitor-failure checks passed. Encoded inputs and ordinary validation
remain unchanged. The generated core exists under ignored target output only.

The table shows median aggregate ns/query over four trials at 20M assertions.
Zero lanes means the ordinary scalar visitor; one lane controls for the new
search kernel and batch setup. All columns use the same executable/input/corpus.
Numbers are descriptive throughput costs, not query tail latencies.

| Family | Build | Traffic | Scalar | 1 lane | 4 lanes | 8 lanes | 16 lanes |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | uniform | 109.3 | 141.8 | 123.1 | 111.0 | 140.5 |
| v4 | simd | hot | 37.8 | 60.3 | 55.7 | 50.4 | 51.0 |
| v4 | default | uniform | 152.3 | 144.1 | 160.6 | 147.9 | 133.7 |
| v4 | default | hot | 37.4 | 57.8 | 55.2 | 52.9 | 51.6 |
| v6 | default | uniform | 232.5 | 222.8 | 185.8 | 192.4 | 161.4 |
| v6 | default | hot | 39.6 | 82.0 | 76.7 | 75.1 | 66.4 |
| v6 | simd | uniform | 249.8 | 257.0 | 234.7 | 238.7 | 256.7 |
| v6 | simd | hot | 42.9 | 80.1 | 76.1 | 71.3 | 69.5 |
| asn | simd | uniform | 242.1 | 474.1 | 181.7 | 118.3 | 136.4 |
| asn | simd | hot | 22.0 | 46.1 | 78.3 | 44.2 | 43.9 |
| asn | default | uniform | 138.6 | 304.1 | 140.2 | 121.1 | 123.6 |
| asn | default | hot | 22.5 | 47.4 | 50.0 | 44.8 | 44.3 |

Four in-process trials do not make scheduling/cache/frequency conditions constant.
For example, primary default ASN scalar uniform timing shifts from 153.4 to
137.4 ns/query across the four trials. This variation must not be hidden by a median
or interpreted as a stable absolute speedup. Default/SIMD executables also differ
in code layout: ASN has no explicit SIMD search in either build.

The 65K shape controls mostly regress or remain close to scalar. Deep-prefix and
duplicate-address cases use the ordinary interval visitor, so they demonstrate
fallback overhead/correctness, not interleaved interval traversal. Sparse IPv6
covers 16-byte deltas, and shared/unique payload cases remain distinct.

## Fresh-process confirmation

Two additional processes repeat the 20M default-build IPv6 and ASN cases without
changing source, executable, immutable input, oracle, corpus or timing protocol.
All eighty additional timing measurements pass. Four-trial medians follow:

| Family | Traffic | Scalar | 1 lane | 4 lanes | 8 lanes | 16 lanes |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| v6 | uniform | 228.1 | 220.0 | 182.4 | 196.1 | 168.1 |
| v6 | hot | 39.9 | 84.1 | 79.3 | 75.8 | 67.0 |
| asn | uniform | 139.0 | 535.2 | 134.4 | 118.2 | 118.8 |
| asn | hot | 22.4 | 46.4 | 50.6 | 45.7 | 44.4 |

Sixteen lanes reduce median aggregate cost by about 26% for IPv6 uniform and
15% for ASN uniform in the fresh processes. The corresponding hot cases are
about 1.7x and 2.0x slower. Positive large-uniform results therefore persist,
but they do not justify broad adoption or an adaptive threshold from this dataset.

## Generated-code inspection

GNU objdump/nm inspection identifies the measured executor by its call inside
the timed repetition loop, with hit/checksum accumulator pointers. The default
executor is 25,488 bytes; SIMD is 25,759 bytes. Both reserve 6,920 stack bytes
before saved registers. All variant paths share that dispatcher/frame once per
entire corpus call; this is not the stack cost of the scalar library method.

Lane state, stack accesses, conditionals and remaining packed-helper calls are
visible in the linked executable. Whole-function static counts in the receipt
include inactive variants and do not measure runtime spills, hotspots, cache
misses or achieved memory-level parallelism. They motivate investigating compact,
family-specific kernels; they do not prove the cause of a regression. Packed
delta/tail search and payload emission remain serial in this prototype.

The comparison uses generic compiler targets, optional existing within-query SIMD,
no explicit prefetch, no cross-query SIMD and no PGO/LTO. Assembly qualification
is scoped to these executables; P2-05 remains open for subsequent variants.

## Follow-through

- P1-01 is complete as a measured fixed-group experiment. Keep its controls and
  correctness harness; add no library API or default-path change.
- P1-02 should compare cross-query SIMD and lower-overhead lane state against
  ordinary scalar and the current fixed-group baseline, including hot/partial
  batches and fallback semantics.
- Later traversal/adaptive experiments can evaluate deeper interleaving or AMAC
  scheduling; this result does not reject those approaches.
- Application PGO and build-policy evaluation remain P2-07/P2-08 work. Do not use
  PGO to conceal algorithm/setup regressions or change library release policy.
- Native-provider traffic, cold/>RAM operation, more architectures and counters
  need separate evidence before production adoption.
