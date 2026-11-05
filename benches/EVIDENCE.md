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

[P1-03 Eytzinger layouts](eytzinger/RESULTS.md) adds 38 processes / 1,792
post-recovery measurements, sorted-copy layout controls, heap/rank capacity and
combined-cache build costs. All original-result/borrowed-pointer/error checks,
linear tree-boundary oracles, fresh source reconstruction and header-derived
capacity accounting pass. Interrupted pre-recovery timings are not merged into
accepted receipts. Two linked key-width search helpers are inspected; hardware
counters remain unavailable. Research is retained without default adoption.

[P1-04 flat B+ nodes](flat_btree/RESULTS.md) adds 38 processes / 2,240
measurements, four node-search methods, sorted/Eytzinger/prior SIMD controls
and a full rerun after adding an assembly-motivated ordered-mask variant.
Exploratory timings are not mixed with final receipts. Fifteen generated-core
tests, complete oracle/borrowed-pointer/error checks, fresh source reconstruction
and header-derived allocation accounting qualify the prototype. All comparator
caches coexist; capacity/build measurements are distinct from RSS/candidate-only
startup. Linked node helpers and counter availability are recorded separately.

[P1-05 traversal/prefetch/alignment](traversal/RESULTS.md) adds 38 processes /
3,808 timings across the full 2x2x2 Eytzinger design and specialized B+ walks.
Every process preserves original assertion/query/payload/error checks; 17 core
tests, fresh prepared-source reconstruction, header-derived capacities and actual
pointer alignment qualify the evidence. Both comparison mirrors coexist within
a 512 MiB logical budget; capacity/build costs are distinct from RSS/candidate-only
startup. Linked factor walks verify eight prefetched specializations per build;
specialized AVX2 walks inline node comparisons. Hardware counters are unavailable.
Mixed workload results retain research candidates without a default-policy change.

[P1-06 budgeted adaptive search](adaptive/RESULTS.md) adds 114 processes / 6,624
timings with candidate-only 0/1/128 MiB budgets. Native IPv4/directory, global
caches, prior SIMD and ordinary controls qualify whole/request32 prefix policies.
Complete oracle/payload/error checks, 18 core tests, fresh prepared-source
reconstruction, header-derived aggregate admission/capacity and full-request
policy traces pass. Actual cache/build costs exclude ordinary reader/allocator/
RSS memory. Linked chooser/planner/ASN bodies and inlined-search summaries and unavailable hardware
counters are recorded. Reject the tested cache-presence/prefix selector for
production; larger budgets did not imply faster queries. Earlier frozen studies
and production source/API/encoding remain unchanged.

[P1-07 learned fences](learned/RESULTS.md) adds 210 processes / 5,280 timings for
guarded native u32/u64 affine rank prediction with binary/radix routing and
errors 4/32/128. All original assertion/query/payload/error checks, forced bad
predictions, NaN-slope recovery, 15 core tests, fresh source reconstruction,
header-derived model/cache dimensions/budgets and probes pass. Eight additional
curved/clustered files reproduce byte-for-byte; all large and nonlinear cases
repeat. Actual native-array/model costs exclude ordinary reader/allocator/RSS.
Retain promising favorable-distribution results as research; hot/small nonlinear
regressions and missing large nonlinear/provider/cold qualification prevent
production adoption. Linked learned-estimate/guarded-search bodies retain
floating and bounds work; counters remain unavailable. Earlier studies remain
frozen and production source/API/encoding is unchanged.
