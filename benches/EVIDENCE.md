# Performance evidence

P0 establishes workload baselines. An optimization needs an unchanged-input,
unchanged-contract baseline/candidate comparison in addition to this inventory.
Neither a faster projection nor a compile-only architecture check establishes
a general lookup improvement.

For new experiments, record:

1. The owning source revision and SHA-256 of harness sources, manifest/lock,
   executable, immutable inputs, generated corpora and raw logs where retained.
   Keep the full dependency lock and build commands beside standalone harnesses.
2. Toolchain, target/build settings, enabled features, CPU/OS, affinity/topology
   and controlled versus uncontrolled cache, scheduling and memory conditions.
3. The exact output contract and an independent oracle derived from original
   inputs. Check complete IDs, targets, payloads and multiplicity when those
   belong to the contract. Declare any projection such as Boolean membership
   or longest-prefix selection. Check every timed corpus before measurement
   and its aggregate output afterward; do not substitute hit counts alone for
   complete output equivalence.
4. Setup/opening/validation, conversion, lookup/decoding/serialization, network
   and publication boundaries. Label allocated versus borrowed outputs, sampled
   versus aggregate timings, clock overhead, corpus reuse and warmup.
5. Serial, pinned-core repeated trials, reversing/rotating comparison order
   where practical. Concurrent scaling pins each worker explicitly. Preserve
   observed regressions/outliers and distinguish descriptive ranges from a
   statistical conclusion or a production latency/capacity promise.
6. Resource measures with their accounting boundary: encoded bytes versus heap,
   process peak versus builder-only RAM, logical temporary bytes versus device
   allocation, guest page cache versus physical-media cache.
7. The adoption/rejection decision, affected workloads and remaining limitations.
   Keep provider schemas in application/benchmark adapters. Do not commit provider
   datasets. Record missing hardware or permissions explicitly when relevant.

## P0 result inventory (2026-11-04)

| Evidence | Reproduction/source | Main boundary |
| --- | --- | --- |
| [City replay](results/2026-11-04-city-replay.json) | [rebuild example](../examples/rebuild.rs), [instructions](../BENCHMARKS.md) | Exact opaque assertions through released external builders; complete assertion parity |
| [Native City](results/2026-11-04-native-city.json) | [adapter](native_city/README.md) | External application schema, normalization, complete native conversion, owned-record serving and sampled query parity |
| [Scale](results/2026-11-04-scale.json) | [scale.rs](scale.rs), [instructions](../BENCHMARKS.md) | Four sizes, three key families, small fixed warm corpus |
| [Traffic](results/2026-11-04-traffic.json) | [traffic.rs](traffic.rs), [instructions](../BENCHMARKS.md) | Hit/miss/skew/locality distributions at 1M/20M assertions |
| [Shapes and batches](results/2026-11-04-shapes-batches.json) | [shapes.rs](shapes.rs), [instructions](../BENCHMARKS.md) | Independent original-target scans; duplicates/overlaps; caller batches |
| [Memory](results/2026-11-04-memory.json) | [Linux harness](memory/README.md) | Guest residency and cgroup pressure, including zero-residency controls; host device cache uncontrolled |
| [Concurrency](results/2026-11-04-concurrent.json) | [Linux harness](concurrent/README.md) | Shared warm reader, per-worker pinning, full timed-corpus prechecks and sampled query latency |
| [Replacement](results/2026-11-04-replacement.json) | [Linux harness](concurrent/README.md) | Exact results, borrowed lifetimes, failed candidates, generation coherence and mapping reclamation |
| [Competitors](results/2026-11-04-competitors.json) | [Linux harness](competitors/README.md) | Matched LPM/membership projections; default/SIMD byte-identical fixtures; separate native geography evidence |

All receipts include their own scope and limitations. The supplementary
[replacement exploratory receipt](results/2026-11-04-replacement-exploratory.json)
retains scheduling outliers but lacks the earlier exact build hashes; it is
explicitly weaker evidence. The primary replacement matrix has full hashes.
Windows, WSL, warm, cold and memory-pressure observations remain separate.

The P0 inventory does not finish P1/P2 qualification. Promising algorithm/build
variants still require complete correctness checks, generated-code inspection,
available hardware counters, representative held-out PGO evaluation and an
explicit adoption decision under the same workload contract.

## P1 result inventory (2026-11-04)

[P1-01 results](interleaved/RESULTS.md) link the primary matrix, fresh-process
repeats, assembly inspection and local integrity verification. The study preserves
ordinary scalar controls and original-input checks in an isolated generated core.
Large uniform gains coexist with hot/small regressions; no default or public API
change is adopted. Later variants still require their own qualification.

[P1-02 cross-query SIMD](cross_query/RESULTS.md) adds 38 processes and 1,792
measurements across ordinary scalar, P1-01 interleaving, four/eight-query AVX2
and native eight-lane u32 ASN kernels. Complete oracle/borrowed-pointer/error
checks include forced scalar fallback; shadow-checkout preparation reproduces
the final source snapshot. Linked gathers/comparisons are inspected; matching
WSL perf tooling is unavailable, so no hardware-counter claim is made.
