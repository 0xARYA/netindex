# Performance work queue

Work in priority order. Update this file with commands, immutable-input hashes,
measurements, correctness results, and remaining limitations. Queued optimization
work should deliver a focused production implementation with tests and measured
benefits. Stop at rejection only when the evidence justifies it; a successful
experiment alone does not complete an implementation task.

Historical P1-01 through P1-08 completions record qualification, not shipped
features. Their source is [archived](benches/ARCHIVE.md); their measurements and
decisions remain below. P1-10 should implement and qualify compact references.

Statuses: `[ ]` queued; `[-]` active; `[x]` completed; `[!]` waiting on a named
dependency. Existing 2.1.0 results are in [PERFORMANCE.md](PERFORMANCE.md); they do
not complete the broader experiments below.

## P0: comparison suite

- [x] **P0-01 Exact City rebuild.** Compare the retained fixture with 2.1.0:
  output size, construction time, peak RAM, temporary disk, opening and parity.
  First isolate external construction by replaying identical opaque assertions
  in original insertion order. Keep this separate from full native conversion.
  Native conversion, full normalized validation and standalone serving qualified;
  [native results](benches/results/2026-11-04-native-city.json).
- [x] **P0-02 Scale.** Added 1K, 100K, 1M and 20M assertion datasets for IPv4,
  IPv6 and ASN; two measured passes and independent query prechecks completed.
  [Results](benches/results/2026-11-04-scale.json).
- [x] **P0-03 Traffic.** Mostly misses/hits, uniform, hot-address skew and locality.
  Two passes at 1M and 20M assertions across IPv4/IPv6/ASN;
  [results](benches/results/2026-11-04-traffic.json).
- [x] **P0-04 Shapes.** Dense/sparse ASNs, sparse IPv6, deep overlaps, duplicates
  and mostly-unique payloads.
  Shared/100% unique payload cases and complete original-target query oracles;
  [results](benches/results/2026-11-04-shapes-batches.json).
- [x] **P0-05 Batches.** Exercise 1, 8, 32 and 256 queries.
  All five shapes and both payload distributions; same rotating query corpus.
  These use the existing scalar-query visitor API, not cross-query SIMD.
- [x] **P0-06 Memory.** Warm cache, cold mappings and inputs larger than RAM.
  Qualified on Linux/WSL using private files, zero-residency cold controls and
  32 MiB cgroup memory with 82.5/200 MB inputs; host/device cache is uncontrolled.
  [Results](benches/results/2026-11-04-memory.json).
- [x] **P0-07 Concurrency.** Throughput and sampled p50/p95/p99 at 1/2/4/8/16
  readers, two reversed-order trials across 1M/20M IPv4/IPv6/ASN inputs.
  [Results](benches/results/2026-11-04-concurrent.json).
- [x] **P0-08 Replacement.** 1/4/16 readers, coarse/fine routing, two trials;
  600 publications, 120 rejected candidates, exact query and mapping-lifetime checks.
  [Results](benches/results/2026-11-04-replacement.json).
- [x] **P0-09 Competitors.** Equivalent geography with MMDB, prefix selection
  with ip_network_table, and membership with default/optimized Roaring.
  42 Linux processes, 1,032 timed measurements; independent original-input checks.
  [Results and decisions](benches/competitors/RESULTS.md).
- [x] **P0-10 Evidence.** Standardized hashes, pinned-core serial runs, build
  provenance, repeated trials and independent correctness oracles for the P0 suite.
  [Evidence standard and inventory](benches/EVIDENCE.md); apply to every new prototype.

## P1: lookup architecture

- [x] **P1-01 Interleaving.** Measured fixed-group prototype; retained as research,
  with large-uniform gains and hot/small regressions. [Results](benches/interleaved/RESULTS.md).
- [x] **P1-02 SIMD across queries.** Qualified AVX2 ASN/fence prototypes, native
  eight-lane u32 ASN, scalar fallbacks and existing tail-SIMD controls. Research
  retained without default adoption. [Results](benches/cross_query/RESULTS.md).
- [x] **P1-03 Eytzinger.** Qualified sorted/Eytzinger RAM mirrors with rank maps,
  startup/memory accounting and prior interleaving/SIMD controls. Research only.
  [Results](benches/eytzinger/RESULTS.md).
- [x] **P1-04 Flat B+ tree.** Qualified flat B+ binary/count/AVX2-count/ordered-mask
  node prototypes under the full query contract, with RAM/build and linked-code
  accounting. Research retained without default adoption.
  [Results](benches/flat_btree/RESULTS.md).
- [x] **P1-05 Traversal.** Qualified 2x2x2 arithmetic-child/prefetch/alignment
  factors and specialized B+ count/AVX2 walks, with physical-alignment, capacity
  and linked-code checks. Research candidates retained without default adoption.
  [Results](benches/traversal/RESULTS.md).
- [x] **P1-06 Adaptive search.** Qualified candidate-only cache admission under
  0/1/128 MiB budgets, native IPv4/directory routing and two sampled policies.
  Rejected the tested selector for production after measured regressions; budget
  enforcement and negative evidence remain reproducible research.
  [Results](benches/adaptive/RESULTS.md).
- [x] **P1-07 Learned fences.** Qualified guarded affine/radix models for native
  IPv4/ASN and eligible normalized 64-bit IPv6 keys, with exact correction and
  full-search recovery. Retain workload-specific research; no default adoption.
  [Results](benches/learned/RESULTS.md).
- [x] **P1-08 Architectures.** Qualified AVX-512F/BW on Ryzen 9 9950X3D2
  and NEON on native Neoverse-N2 hardware. Keep workload-dependent tails as
  research; packed NEON is promising for hot IP lookups. Portable fallbacks pass.
  [Results](benches/architectures/RESULTS.md).

## P1: storage and construction

- [x] **P1-09 Payload distribution.** Exact cardinality, lengths, physical/logical
  duplicate bytes, insertion-order gaps/distinct-byte reuse and cache admission
  qualified on retained real City and independent IPtoASN snapshots.
  [Results](benches/payload_distribution/RESULTS.md).
- [ ] **P1-10 Compact references.** Payload IDs plus a distinct span directory;
  establish the break-even point for variable-length records.
- [ ] **P1-11 Shared aliases.** Design logical target/ID mappings that preserve
  duplicate assertions, overlaps and all-match results.
- [ ] **P1-12 Disk-table cache.** Benchmark a bounded page cache and account its RAM.
- [ ] **P1-13 Grouped metadata.** Reduce probe work while keeping exact equality.
- [ ] **P1-14 Partitioned dedup.** Compare sequential spill/merge against random I/O.
- [ ] **P1-15 Bitpacking.** Measure block decoding versus direct packed access.
- [ ] **P1-16 Compatibility.** Record format and migration decisions before
  adopting encoding changes; required by P1-10, P1-11 and P1-15 if layout changes.

## P2: query capabilities

- [ ] **P2-01 Batch API.** Depends on useful P1-01/P1-02 results and defined errors,
  ordering, output ownership and partial-batch behavior.
- [ ] **P2-02 Membership/early stop.** Explicit termination and error handling.
- [ ] **P2-03 Range/CIDR intersection.** Define boundaries, ordering and duplicates.
- [ ] **P2-04 Inspection.** Explain representation choices, sizes and resource use.

## P2: qualification (apply relevant checks to every earlier prototype)

- [ ] **P2-05 Assembly.** Inspect generated and linked code for promising variants.
- [ ] **P2-06 Counters.** Cycles, instructions, branch and cache misses where supported.
- [ ] **P2-07 Application PGO.** Train on representative traffic and validate on an
  independent mix, outside library release policy.
- [ ] **P2-08 Build settings.** Compare normal optimization, ThinLTO, PGO and their
  combination, including binary size and startup.
- [ ] **P2-09 Correctness.** Extend independent tests/fuzzing for layouts, batches,
  architectures, malformed data and resource failures.
- [ ] **P2-10 Decisions.** Record adoption/rejection, gains, regressions, resource
  costs and the workloads supported by evidence for each experiment.

## Acceptance

Preserve the declared query and validation contracts, original targets, insertion
IDs, duplicates and all matches. Use unchanged input and compare complete results.
Adopt optimizations only with repeatable measured benefits and explicit costs.
Keep provider schemas in benchmark/application adapters, out of library modules.
Do not commit provider datasets. Hardware/data blockers must name the missing
resource; continue independent work without silently skipping priority items.

## Work log

### 2026-11-04: P0-01 started

- Starting revision: `6efec832d3f41874db9f999e13f891e550c3fd9f` (2.1.0).
- Retained source: GeoLite2-City, native SHA-256
  `d6ff9368ab19a13b096fd6392f752aa6c0890f2c92752d09253fab9e50501e46`.
- Retained converted artifact: 750,594,180 bytes, 20,255,722 assertions; SHA-256
  `83d775f419696a224b5f57a114ec42ea5f1289b5ea86d4ae171d00218b3d2f18`.
- Copied both immutable files to ignored `target/city-study/`; both SHA-256 hashes
  match the retained evidence. Production files and deployment remain untouched.
- Planned first comparison: opaque assertion replay through external builders
  2.0.2 and 2.1.0 with identical insertion order, metadata and build budgets.
  This isolates building; it does not measure provider decoding/native conversion.
- Added schema-neutral `examples/rebuild.rs`: validates the input, reconstructs
  insertion order, rebuilds with unchanged metadata and verifies every output
  assertion's unique ID, target and payload plus 100,005 deterministic IP queries
  (complete match lists and ordering). It creates a new output rather than
  replacing an existing file. Both versions pass the five-assertion mixed fixture.
- Runs in progress: baseline 2.0.2 then candidate 2.1.0, followed by the reverse
  order, pinned to logical CPU 20. Rust 1.95.0 release, codegen-units=1, no SIMD;
  all shared resolved dependency versions match. Options: 16,384-row sort runs,
  4 MiB key cache, 8 GiB temporary budget. Construction excludes preparation,
  validation, oracle and fsync; opening is netindex validation, not full application
  payload validation. Whole-process peak working set includes mapped input/output,
  the assertion-order oracle and verification storage, not just builder memory.

### 2026-11-04: P0-01 initial result; P0-02 started

Both replay pairs passed: every one of 20,255,722 assertions retained its unique
ID, target and payload; metadata matches exactly; 100,005 IP queries per run
returned identical full match lists and ordering. Both 2.0.2 outputs have the
original artifact's SHA-256, proving byte-for-byte baseline reproduction. Both
2.1.0 outputs have SHA-256
`1be3a0505fd8eb3dd7ce2b7f807d1744541a86e553cb7c1be78bc95ad65c72a7`.

| Measurement | 2.0.2 | 2.1.0 |
| --- | ---: | ---: |
| Output bytes | 750,594,180 | 263,944,912 |
| Construction seconds, pair 1 | 22.515 | 48.403 |
| Construction seconds, pair 2 | 20.891 | 46.901 |
| Logical peak temporary bytes | 2,447,925,271 | 1,961,276,003 |
| Whole-process peak working set, MiB | 2,539.3 | 2,075.2 |

Output is 64.8% smaller; construction is about 2.2x slower. These are two paired,
descriptive runs on a live Windows desktop, not a general latency guarantee.
The new component is still about 5.29x the 49,899,266-byte native MMDB.
P0-01 remains active: full native conversion costs and application-level opening,
payload validation, standalone serving RSS and native lookup comparison remain.
Replay memory includes oracle storage and both mappings; it is not builder-only
or service RSS. Neither run changes Byway or deploys the result.

P0-02 now has `benches/scale.rs`: exact-key IPv4, IPv6 and ASN fixtures, 50% hits
and gaps, 1,024 queries per batch. Set `NETINDEX_BENCH_SIZES` to a comma-separated
list up to 20,000,000; small defaults keep correctness checks inexpensive. It
saves generated files to ignored `target/scale-fixtures/` for hashing. Two serial
timed passes at 1K/100K/1M/20M completed. This covers size only, not the
traffic/overlap/cold-memory dimensions in later P0 tasks.

### 2026-11-04: recorded evidence and next work

[City replay results](benches/results/2026-11-04-city-replay.json) retain both
pairs, phase timings, process peaks, source/executable/input/output hashes,
dependency versions and resource settings. Payload bytes fell from 503,375,959
to 16,726,691; the 162,045,776-byte reference section and both address indexes
are unchanged in size. Compact references and alias sharing therefore remain
substantial structural opportunities; no minimum-size claim follows.

[Scale results](benches/results/2026-11-04-scale.json) retain fixture hashes and
Criterion mean confidence intervals for both passes. Mean ns/query ranges:

| Assertions | IPv4 | IPv6 | ASN |
| ---: | ---: | ---: | ---: |
| 1,000 | 37.0–51.9 | 36.7–38.3 | 9.9–10.2 |
| 100,000 | 38.6–39.9 | 38.7–41.3 | 20.5–22.5 |
| 1,000,000 | 40.3–40.6 | 41.4–41.9 | 36.2–45.8 |
| 20,000,000 | 48.7–48.8 | 52.1–52.6 | 79.6–80.0 |

The fixed, repeated 1,024-query corpus gives a warm working set; this does not
measure whole-file cache pressure. Variation in the unchanged 1K IPv4 control
and 1M ASN case cautions against small comparative claims. These establish a
baseline, not an optimization gain. P0-10's protocol is applied to both new
harnesses; broader workload/hardware coverage is still pending.

That checkpoint's native-conversion and traffic follow-ups are recorded below.
Keep the opaque replay and size baseline separate from those measurements.

Verification passed: rustfmt; default/all-feature all-target Clippy with warnings
denied; default/all-feature tests including ignored cases; scale benchmark smoke;
documentation with warnings denied; and local all-feature packaging. Replay also
passed the mixed fixture and seven packed/interval/fixed/strided golden fixtures.
Metadata and address-index bytes hash identically before and after the real replay.
CI now includes scale correctness; remote CI for these additions is not yet run.

### 2026-11-04: native qualification, traffic, shapes and caller batches

P0-01's [standalone adapter](benches/native_city/README.md) snapshots the external
application's normalizer, payload encoder, full native validator, format reader
and patched maxminddb. Both baseline outputs reproduce the original component
byte for byte; both candidates reproduce the already verified replay hash.
Shared dependencies and application sources match across versions.
[Results](benches/results/2026-11-04-native-city.json) retain source/dependency and
executable provenance, phase timings and all process receipts.

| Native conversion observation | 2.0.2 | 2.1.0 |
| --- | ---: | ---: |
| Assertions | 20,255,722 | 20,255,722 |
| Output bytes | 750,594,180 | 263,944,912 |
| Total seconds, two runs | 35.65–35.98 | 60.02–60.58 |
| Peak process working set, MiB | 773.3 | 309.2–310.1 |
| Logical peak temporary bytes | 2,447,925,271 | 1,961,276,003 |
| Warm application opening, seconds | 4.22–4.31 | 4.07–4.32 |
| Separate mapped serving process peak, MiB | 727.8 | 263.7 |

Native MMDB alone is 49,899,266 bytes, opens with complete native validation in
2.61–2.64 seconds, and has an 88.4 MiB standalone process peak. Thus normalized
2.1.0 saves 64.8% of artifact bytes and about 60% of conversion peak memory,
while complete conversion takes about 1.7x as long. It does not beat native MMDB
on size, startup or memory here. Output validation still walks every assertion;
lower artifact size did not establish a repeatable opening-time gain.

Both versions matched serialized owned location answers for 108,123 queries:
fixed edge/random cases, sampled canonical endpoints, and compatible, mapped
and 6to4 IPv4 endpoints. The separate 100,005-query serving corpus had 42,802
hits in every run. Recorded p95/p99 values have a 100 ns clock quantum; two
trials cannot establish broad latency gains. Serving peaks include opening,
not deployed-service RAM. Conversion excludes download, gzip, orchestration,
explicit fsync and atomic publication. No collector deployment was changed.

P0-03 [traffic results](benches/results/2026-11-04-traffic.json) cover 1M and 20M
keys in IPv4/IPv6/ASN using 16,384 repeated queries, rather than the earlier
1,024-query corpus. Independent prechecks verify every count, ID, original target
and payload. Two 20M-run means in ns/query:

| Traffic | IPv4 | IPv6 | ASN |
| --- | ---: | ---: | ---: |
| Uniform 50% hits | 109.3–109.4 | 161.5–162.3 | 110.4–115.1 |
| Mostly hits, 16,221/16,384 | 116.6–117.4 | 172.8–172.9 | 117.3–119.7 |
| Mostly misses, 163/16,384 hits | 98.8–99.1 | 153.6–153.6 | 157.0–171.2 |
| Approximately 90% on 16 hot IDs | 40.8–41.0 | 47.3–47.6 | 33.0–33.4 |
| 32-query adjacent-ID runs | 58.9–59.1 | 56.9–57.1 | 25.8–26.2 |

Hot-skew/locality cases are also 50% hits. The larger corpus exposes materially
more variation than the earlier warm scale corpus; these are workload baselines,
not algorithm gains. ASN mostly-miss results remain noisier across trials.

P0-04/P0-05 [shape/batch results](benches/results/2026-11-04-shapes-batches.json)
cover 65,536 assertions, five shapes, shared/100% unique eight-byte payloads,
and caller batches 1/8/32/256. Every query is compared with a brute-force scan of
original targets and complete ID/target/payload lists. Batches rotate through the
same 256 queries, including single-query batches. Shared payload means, ns/query:

| Shape | Batch 1, two runs | Batch 256, two runs |
| --- | ---: | ---: |
| Dense ASN | 23.6–23.8 | 17.9–17.9 |
| Sparse ASN | 24.2–24.3 | 20.1–20.2 |
| Sparse IPv6, 16-byte deltas | 43.6–44.1 | 42.7–42.8 |
| Up to 65 nested IPv6 prefixes | 1037.6–1039.8 | 1034.7–1036.9 |
| 64 duplicate IPv4 assertions/key | 818.6–823.8 | 817.3–822.2 |

Caller batching amortizes loop overhead for cheap lookups; it does not introduce
interleaving or cross-query SIMD. Nested/duplicate hits must emit all assertions,
unlike a single-result LPM comparison. Unique payloads retain every assertion and
increase artifact size; complete hashes/confidence intervals are in the results.

All Windows trials used Rust 1.95.0, release codegen-units=1, CPU 20, serial
processes, two trials, unchanged semantics and no PGO/LTO/custom RUSTFLAGS.
New workload baselines use scalar dispatch; CI adds correctness runs with and
without optional SIMD. These results do not imply remote CI, other hardware,
cold files, concurrent load or production capacity.

Next: P0-06 isolated Linux/WSL warm/cold/32 MiB cgroup memory qualification;
then P0-07 concurrent readers. Do not compare Linux and Windows timings directly.

### 2026-11-04: memory qualification

P0-06's [separate Linux package](benches/memory/README.md) qualified all 18
processes: three families, warm/cold/pressure modes, two trials with reversed
mode order. Every cold process started queries with zero resident file pages;
every warm process had complete residency; pressure processes registered memory
limit events without OOM kills. Private copies prevent page-cache charges from
escaping to another cgroup. No global cache flush or sibling limit was changed.
[Results](benches/results/2026-11-04-memory.json) retain per-pass faults, residency,
cgroup memory counters, input/executable/source/lock hashes and kernel details.

| Family | Input MB | Warm ns/query, all passes | First cold pass, microseconds/query | 32 MiB pressure, microseconds/query |
| --- | ---: | ---: | ---: | ---: |
| IPv4 | 82.5 | 285–306 | 31.4–32.4 | 406–451 |
| IPv6 | 82.5 | 334–429 | 31.9–34.2 | 408–439 |
| ASN | 200.0 | 436–583 | 74.7–79.4 | 963–1337 |

All modes validate first and use four disjoint 8,192-query passes, half hits.
Timed loops retain per-query count/ID/payload assertions. Warm runs had zero
major faults; cold runs incurred about 1,636–3,638 and warmed on later passes.
Pressure incurred 46,208–131,369 major faults per process with memory peaks
32.0–32.24 MiB and swap disabled. Default mmap readahead was left unchanged.

These are WSL2/ext4 guest-cache and bounded-memory results. Host VHD/device caches
are uncontrolled: they do not establish physical-media cold latency. The file
exceeds the child's memory budget, not the physical desktop's installed RAM.
Neither these checked Linux loops nor their timer should be compared directly
with native Windows Criterion values. The large pressure penalty justifies a
later P1-12 investigation of access advice/page-cache behavior; no mapping-policy
change is adopted from this baseline.

Collector Worker independently qualified the same artifacts through the complete
production converter on Linux. Its owning report is Byway's
`notes/2026-11-04-ip-intel-network-audit.md`, section "Netindex 2.1.0: real City
qualification": matching output hashes, all 20,255,722 assertions, 449,189 native
queries, cross-reader compatibility, and 286 consumer tests. Its 69/77-second
conversion observations include archive extraction and syncing, on different
hardware/build settings. Keep those results separate from the local 36/60-second
adapter observations. No adoption/deployment change follows from this queue.

Next in priority order: **P0-07 concurrent readers**, then snapshot replacement
and matched-contract competitors. P0-10 remains active for the remaining matrix.

Local verification for this checkpoint passed: Rust 1.95 formatting; default and
all-feature all-target Clippy with warnings denied; default and all-feature tests
including ignored cases; traffic/shape correctness with default and SIMD dispatch;
documentation with warnings denied; all-feature package verification; standalone
Linux memory release build/Clippy; Python parsing; and source/fixture receipt
integrity checks. Native adapters built for both released versions and passed
full output validation and parity. Remote checks are tracked in GitHub Actions
for the publishing commit; the earlier checkpoint's CI limitation is historical.


## 2026-11-04 concurrent readers and snapshot replacement

P0-07 and P0-08 completed with the [standalone Linux harness](benches/concurrent/README.md).
No lookup implementation or encoded format changed. This baseline uses mmap
without optional SIMD, LTO or PGO; optimization comparisons remain separate.

At 20M assertions, one-worker throughput spans 3.43�3.85M IPv4, 3.26�3.28M IPv6,
and 2.59�2.73M ASN queries/s. Sixteen workers span 90.11�95.30M, 70.40�71.84M
and 53.49�56.04M respectively. Every timed query passes its exact independent
oracle before measurement; timed checksums match. Histograms include sampled
query p50/p95/p99 and clock controls. Guest CPU pinning does not isolate host
scheduling, and superlinear results must not be extrapolated as service capacity.

Replacement covers changes to IPv4/IPv6/ASN targets and payloads, duplicates,
unchanged source identity, retained old requests and borrowed payloads. Each
worker observes both generations; all query results preserve source positions,
IDs, targets, bytes and multiplicity. All twelve cases release 49 retired mappings
while initial/final snapshots remain alive and all 51 once those owners drop.
The portable regression repeats publication while workers hold borrowed answers.

Median candidate construction spans 4.59�6.24 us for coarse routing versus
4.22�4.56 ms for fine routing on 196,672 assertions. Fine routing rebuilds its
routing information; mapping and validation are excluded. Publication p99 spans
0.06�4.26 us coarse and 0.12�3.29 us fine, including lock waits; each case has
only fifty publications, so this percentile includes the slowest event and is
sensitive to scheduling. Sampled checked 32-query batch p99 spans 4.30�9.35 us.
An [earlier exploratory repeat](benches/results/2026-11-04-replacement-exploratory.json)
observed a 270 us publication outlier. Its exact build hashes were not retained;
the receipt preserves metrics and the raw-output hash as supplemental evidence.
This reinforces that the final two trials do not bound scheduling tails.
These are small warm synthetic files and one application-owned RwLock publication
policy, not per-query latency, deployment atomicity or a production SLO.

Next in priority order: **P0-09 matched-contract competitors**. P0-10 remains
active while the comparison suite is being extended.


Verification passed for this checkpoint: Rust 1.95 formatting, default/all-feature
all-target Clippy with warnings denied, default/all-feature tests including
ignored cases, documentation with warnings denied and all-feature package
verification. The new publication regression passed on Windows and Linux.
Both standalone release binaries passed Linux Clippy and the final full matrices;
source/executable/fixture hashes and result counts were checked. No version bump
is needed for benchmark, documentation and test additions. Remote CI is tracked
on the publishing commit separately from these local checks.


## 2026-11-04 matched-contract competitors and P0 evidence completion

P0-09 completed with [reproduction](benches/competitors/README.md),
[measurements/decisions](benches/competitors/RESULTS.md) and the
[hashed receipt](benches/results/2026-11-04-competitors.json). P0-10 now has a
shared [evidence standard and result inventory](benches/EVIDENCE.md).

New dependencies remain in a standalone benchmark workspace: ip_network_table
0.2.0 (treebitmap backend), ip_network 0.4.1 and Roaring 0.12.0. No library, format,
public API, application schema or default build setting changed. Geography uses
existing owned-record City comparisons and 108,123-query parity, under its own
Windows timing/memory boundary; it is not pooled with the new Linux timings.

The Linux matrix has 1K/100K/1M leaves or keys, IPv4/IPv6 disjoint/overlapping
prefixes, dense/sparse/clustered ASN membership and uniform/hot corpora. Every
case runs default and SIMD executables with byte-identical serialized inputs and
corpora. Four prefix trials alternate engine order; six membership trials cover
all engine-order permutations. All 42 processes and 1,032 timing measurements
passed. All original prefix boundaries/endpoints and neighbouring addresses, or
keys and neighbouring integers, pass the independent HashMap/HashSet oracles;
each complete timed corpus is prechecked and every timed checksum matches.

At 1M uniform IPv4 leaves, default/SIMD netindex is 82.1/67.4 ns/query versus
45.3 ns for the trie. With overlapping IPv4 prefixes it is 606.7/647.4 versus
51.5 ns. With hot disjoint IPv6 it is 41.6/45.4 versus 108.2 ns. At 1M ASN keys,
dense membership is 46.0/49.2 ns versus 4.5�4.8 ns for Roaring. Sparse SIMD
membership is 47.6 versus 51.9�52.2 ns; default is 51.1 versus 53.5�54.6 ns. These are
median aggregate observations, not significant speedup claims or query tails.
The complete table retains both builds, traffic patterns and Roaring variants.

Dense membership encodes to 8,000,109 bytes in netindex, 131,208 bytes in default
Roaring and 230 bytes with run compression. Sparse bitmap bytes are 2,524,296;
clustered optimized bytes are 4,540. These narrower Boolean representations do
not preserve insertion IDs, duplicates or payloads. Their sizes are serialized
bytes, not heap/RSS. Run compression can trade lookup speed for size. Full
netindex result semantics must survive any adoption of density-dependent layouts.

Decision: retain the specialized baselines; adopt no algorithm or blanket SIMD
policy from these observations. Prioritize P1-01/P1-02 interleaving/cross-query
SIMD under the full all-match contract, followed by the queued routing/layout
experiments. Carry density/run ideas into P1-06/P1-15 and retain the native-MMDB
size gap as motivation for P1-10/P1-11. No universal winner is demonstrated.
Two rejected fixture-development passes (zero ASN and insufficient parent-prefix
budget) produced no accepted result receipt. The final positive-ASN fixture uses
a two-million-assertion budget to include overlapping parents and was rebuilt
and remeasured in full.

Next in priority order: **P1-10 compact references**. P1-09 found variable-length
payload references dominate current City storage; its arithmetic estimate remains
unimplemented and excludes decoding/validation/compatibility costs.
P2 assembly/counters/PGO/build qualification still applies to promising variants;
P0 completion does not complete the full optimization queue.


Verification passed: Rust 1.95 formatting, default/all-feature all-target Clippy
with warnings denied, default/all-feature tests including ignored cases,
documentation with warnings denied and all-feature package verification. Both
standalone release profiles passed Clippy and the entire matrix. The receipt
verifier checked all source/executable/input/raw-log hashes, trial coordinates,
checksums and histogram percentiles; it retains measured working-copy hashes and
adds LF-normalized source hashes for Git checkout comparisons. Python parsing and
local documentation-link checks passed. The preceding publishing commit's Windows,
Linux and fuzz CI completed successfully; this checkpoint's CI is tracked on its
own publishing commit. No version bump or registry publication is needed.

### P1-01 completed: fixed-group query interleaving (2026-11-04)

[Study and decision](benches/interleaved/RESULTS.md): 32 primary processes plus two
fresh-process repeats, 960 measurements, original-input/full-output/borrowed-pointer
checks and typed visitor failures. Research methods live only in an ignored core
copy. No tracked library source, encoding, public API or release policy changes.

Four-lane through sixteen-lane ASN lower bounds and packed-IP fence probes interleave
independent searches; packed tails, decoding and interval fallbacks remain serial.
One lane controls for kernel/setup changes. Hot/small regressions rule out blanket
adoption; aggregate costs are not per-query tail latency or production capacity.
Linked-code inspection records the timed dispatchers and their stack reservations,
without attributing effects to unmeasured hardware counters. P2-05 remains open
for future promising variants; P2-07/P2-08 still own representative application PGO
and build-setting evaluation. Follow-through: P1-02 cross-query SIMD and cheaper lane state.

Verification: root formatting, default/all-feature all-target Clippy with warnings
denied, default tests, all-feature tests including ignored cases, warnings-denied
documentation and local packaging passed. Standalone default/SIMD release Clippy,
fragment/harness formatting, ten generated-core unit tests, generated-core Clippy,
Python parsing, JSON parsing and local documentation links passed. The integrity
receipt verifies all 34 local processes and 960 measurements against retained
source/executable/input/log hashes and checked totals. CI is tracked separately
on the publishing commit. No version bump or registry publication is needed.

### P1-02 completed: cross-query SIMD (2026-11-04)

[Study and decision](benches/cross_query/RESULTS.md): 32 primary processes plus
six fresh-process repeats, 1,792 measurements. Compare ordinary visitors, P1-01
1/4/8/16 interleaving, 4/8-query AVX2 masked 64-bit gathers, and native eight-lane
u32 ASN gathers. Existing packed-tail SIMD is independently enabled/disabled;
cross-query AVX2 is detected in both builds. IP variant 112 duplicates variant
108 as a diagnostic control; native u32 gathers apply to ASN only.

Original assertions, full ordered matches, payload bytes/pointers, partial/empty
batches and typed callback-error prefixes pass. Forced scalar versions of all
new kernels pass the same corpus checks. Twelve generated-core unit tests include
linear unsigned-boundary/alignment oracles. Preparation is independently
reproduced in a disposable shadow checkout; receipts verify final source, lock,
executable, input, raw-log, trial-grid and derived timing/count integrity.

Assembly inspection confirms five specialized AVX2 kernels per executable,
including VPGATHERQQ/VPCMPGTQ and native VPGATHERDD/VPCMPGTD. Eight-query 64-bit
state remains stack-backed; native u32 ASN supplies a compact-state alternative.
Matching perf tooling is unavailable for this WSL kernel. Static inspection is
not runtime counter evidence. Large-uniform benefits coexist with hot/small
regressions and timing variation; retain research only, with no library source,
format, API or release-policy change. No universal-win claim or adaptive dispatch
policy follows. Follow-through: P1-03 Eytzinger fence layouts; P1-06 owns
adaptive selection.

Fresh default-build four-trial medians: IPv6 vector-8 uniform 162.7 versus
scalar 270.8 ns/query; native-u32 ASN uniform 89.8 versus 155.6. Corresponding
hot costs are 68.8 versus 42.6 and 34.2 versus 22.0 ns/query. IPv4 duplicate
source-algorithm controls vary substantially, and native/64-bit ASN rankings
change across runs. These are qualified throughput observations, not stable
statistical speedups or per-query tail latencies.

Verification: documented root formatting, default/all-feature all-target Clippy
with warnings denied, default tests, all-feature tests including ignored cases,
and warnings-denied docs passed. Standalone default/SIMD release Clippy, harness/
fragment formatting, twelve final generated-core tests, generated-core lib/test
Clippy and receipt/shadow-source verification passed. Local packaging is checked
on this checkpoint; hosted CI is tracked separately on its publishing commit.

### P1-03 completed: Eytzinger RAM mirrors (2026-11-04)

[Results and decision](benches/eytzinger/RESULTS.md): 38 processes and 1,792
measurements compare ordinary sorted encoded visits, prior interleaving/SIMD,
key-only sorted mirrors, candidate-tracking Eytzinger and arithmetic/path-recovery
Eytzinger. Packed-tail/decoding semantics stay intact; sorted/heap mirrors use
global fences while original IPv4 paths retain their directory. Comparisons with
the sorted mirror distinguish layout from copying/density/routing changes.

Fresh default 20M ASN: candidate uniform 198.4 versus sorted mirror 260.8,
ordinary 277.8 and prior SIMD 109.1 ns/query; hot candidate 18.4 versus ordinary
22.5. Heap keys/rank maps cost 152.588 MiB, or 228.882 MiB with the sorted control;
combined-cache construction is about 106-215 ms across these ASN runs. Small
ASN controls favor candidate tracking but use a fixed, reused 256-query corpus.
Fresh SIMD IPv4 arithmetic global search is 265.1 versus ordinary 105.4 ns/query.
Retain research only; no default, public API, file-format or release-policy change.

Initial timings were interrupted by storage I/O failure/WSL shutdown and rejected.
After recovery, the entire primary matrix and all six large fresh-process cases
were rerun. Receipts contain post-recovery measurements only. Original assertions,
ordered targets/IDs/payloads/pointers, duplicates, empty/partial groups and typed
callback errors pass. Thirteen core tests include linear unsigned-boundary and
incomplete-tree oracles. Shadow preparation, file/input/executable/log hashes,
trial grids, derived timing/count totals and header-derived cache capacities pass.

Linked-code inspection retains both key-width helper bodies: u32 arithmetic uses
SETB/LEA child calculation and NOT/TZCNT/shift recovery, with loop/rank-check
branches. Static counts are not runtime counter evidence; matching WSL perf
tooling is unavailable. P1-05 owns prefetch/traversal/alignment experiments; P1-06
can evaluate directory-preserving forests, computed rank maps and bounded cache
selection. Follow-through: P1-04 flat B+ tree node search under the full query
contract.

Verification: post-recovery root formatting, default/all-feature all-target Clippy
with warnings denied, default tests, all-feature tests including ignored cases,
and warnings-denied docs passed. Standalone default/SIMD release Clippy and
harness/fragment formatting passed. Thirteen generated-core tests and generated
lib/test Clippy passed after recovery. Python/JSON parsing, local links and full
receipt/source reconstruction passed. Local packaging is checked on this checkpoint;
hosted CI is tracked separately on the publishing commit. No registry release.

## P1-04 flat B+ node search - 2026-11-04

Completed as a qualified research experiment, without a library-default/API/format
change. [Results](benches/flat_btree/RESULTS.md) and
[reproduction](benches/flat_btree/README.md) link the primary matrix, fresh repeats,
linked assembly and source/evidence verification receipts. No registry release.

The prototype stores sorted leaves and flat bottom-up child-maximum levels:
16 u32 ASNs or four u128 IP fences per logical 64-byte node. Leaf positions
restore original rows/blocks without per-key rank maps. It compares binary,
scalar-count, detected AVX2 count and ordered AVX2 prefix-mask search against
ordinary/prior SIMD, sorted-copy and Eytzinger controls. Full lookup semantics,
duplicates, original IDs/targets, borrowed payload pointers and callback-error
prefixes remain intact. Interval/broad-route paths retain existing visitors.
All mirrors search global fences; original IPv4 paths keep their directory.

Thirty-two final primary processes and six fresh repeats passed, producing
2,240 timings. All original assertions and complete independent query oracles,
partial/empty/mixed-family batches and forced scalar controls pass. Fifteen
generated-core tests cover lower/upper bounds, duplicate separators, incomplete
levels through 4097, unsigned u32/u128 halves, alignment and allocation accounting.
Fresh shadow preparation reproduces the frozen sources; input/executable/raw-log
hashes, header-derived capacities and exact timing/checksum coordinates agree.

Observed four-trial median ns/query in fresh 20M cases:

- Default IPv6 uniform: B+ count 238.3, ordinary 289.9, sorted 264.7,
  prior SIMD112 155.1. Hot: B+ count 75.2 versus ordinary 41.2.
- Default IPv4 uniform: B+ count 195.7 versus ordinary 144.0 and SIMD108 98.1;
  hot 71.1 versus ordinary 38.6. No directory-aware B+ claim follows.
- SIMD-build ASN uniform: ordered mask 158.5, AVX2 count 190.3, ordinary 285.3
  and SIMD108 127.1. Hot: ordered mask 32.8 versus ordinary 22.2.
  Default ASN uniform instead regresses: best B+ count 218.1 versus ordinary 171.2.

B+ capacity is 1.590 MiB for each 20M packed-IP case and 81.380 MiB for 20M ASNs,
including level-range elements. With sorted/Eytzinger comparison arrays retained,
combined capacities are 4.272/310.262 MiB, within the checked 512 MiB logical budget.
Combined-cache construction spans 1.123-1.726 ms for large IP cases and
119.829-239.408 ms for ASNs across primary/repeats. These are allocated capacities
and all-layout build times, not RSS or candidate-only startup.

Assembly inspection motivated ordered-mask404 after exploratory count kernels
showed software population-count arithmetic. Every final primary/repeat process
was rerun with404 included; exploratory timings are not mixed into receipts.
Linked ordered kernels use comparisons/movemasks and TZCNT-encoded prefix rank.
Complete bound/count/rank helpers, an outlined dispatch closure and four ISA
wrappers are retained. Static code observations do not establish runtime costs;
matching WSL perf counters are unavailable. Ordered masks also add early exit,
so their comparison does not isolate a single instruction. Portable Vec alignment,
unpadded level starts, runtime dispatch and per-node feature checks remain
explicit limits; no prefetch or PlanB LPM/AVX-512 reproduction is claimed.

Decision: retain the candidate for further controlled experiments. Mixed uniform
results, repeat drift, hot-query regressions and extra memory prevent automatic
adoption. Synthetic warm fixed corpora, four trials and host uncertainty do not
prove significance, cold/>RAM/native-provider capacity or universal superiority.
Follow-through: **P1-05 traversal/prefetch/branchless ablations**, including dispatch
specialization/inlining, hoisted ISA checks and physical alignment. P1-06 owns
optional memory-budgeted selection, native-width IPv4 fences and directory-aware
forests; each needs separate measurements before deployment.

Verification: root formatting, default/all-feature all-target Clippy with warnings
denied, default tests, all-feature tests including ignored cases and warnings-denied
docs passed. Final standalone default/SIMD release Clippy, harness/fragment
formatting, 15 generated-core tests and lib/test Clippy passed. Python parsing,
JSON/source/receipt reconstruction and local documentation links passed. Packaging
is checked on the publishing checkpoint; hosted CI is tracked separately.

## P1-05 traversal, prefetch and alignment - 2026-11-04

Completed as a qualified research study; no production default, API or encoding
change. [Results](benches/traversal/RESULTS.md) and
[reproduction](benches/traversal/README.md) link primary/repeat/assembly/verification
receipts. Thirty-two primary processes and six fresh repeats pass, producing
3,808 timings across 17 methods. All original assertions, complete independent
query oracles, ordered IDs/targets, duplicates, payload bytes/pointers, mixed/
partial/empty batches, broad-route fallback and typed callback-error prefixes pass.
Seventeen generated-core tests add linear-oracle checks for every new traversal
factor and specialized scalar/AVX2 walk, incomplete trees through 4097, unsigned
boundaries and allocation/alignment guarantees. Source reconstruction, hashes,
raw-log coordinates and header-derived capacity accounting pass.

The Eytzinger factorial holds grouped accessors, key density and original-rank
mapping fixed while varying candidate tracking/arithmetic child selection,
software T0 prefetch and natural/64-byte alignment separately and together.
Prefetch targets a valid 64-byte descendant group four ASN/two IP levels ahead;
checked/clipped arithmetic precedes pointer formation. Natural controls record
actual alignment: default large ASN has base mod64=16, IPv4=32 and IPv6=0,
while all aligned arrays have mod64=0. An already-aligned natural control cannot
establish a pure physical-alignment effect. Legacy Eytzinger controls also change
accessor/specialization structure, so they are not single-factor comparisons.

Fresh default 20M medians (ns/query), with matching primary results in the tables:

- IPv4 arithmetic, natural/no-prefetch501 171.3; aligned/no-prefetch505 174.3;
  aligned/prefetch507 137.9; ordinary 163.4 and prior SIMD108 127.9.
  Hot aligned arithmetic/prefetch costs 66.6 versus ordinary 38.0.
- IPv6 arithmetic, natural/no-prefetch501 193.7; aligned/no-prefetch505 189.4;
  aligned/prefetch507 122.4; ordinary 200.9 and prior SIMD108 105.7.
  Hot aligned arithmetic/prefetch costs 67.3 versus ordinary 39.8.
- ASN aligned candidate/no-prefetch504 costs 17.4 for hot queries versus
  ordinary 22.1; enabling prefetch506 costs 24.5. Uniform candidate504 costs
  215.9, while prefetched candidate506 costs 335.6. Alignment/prefetch do not
  combine into a universal improvement.

B+520 specializes scalar counting. B+521 specializes the ordered AVX2 walk,
hoists detection outside node loops and makes direct node helpers inline-capable.
These form a combined dispatch/inlining/ISA intervention, not isolated causal
estimates. Fresh default ASN uniform521 102.8 versus legacy ordered404 156.0,
ordinary 259.0 and prior SIMD108 105.7; primary521 127.2 versus404 169.9.
Hot521 still costs 29.0 versus ordinary 22.1. Other family/build results remain
mixed; no significance or universal dominance follows from four-trial medians.

The two added group mirrors consume 2.385 MiB per large packed-IP case and
152.588 MiB for 20M ASNs. Combined comparison capacities are 6.656/462.850 MiB,
within the checked 512 MiB logical budget. All-layout construction spans
1.645-2.240 ms for large IP cases and 156.190-341.219 ms for ASNs across runs.
Capacities include array/group padding and level-range elements, exclude allocator/
object overhead and RSS; construction excludes original mapping/validation and
does not isolate candidate-only startup. All caches coexist during all timings.

Assembly retains 16 two-width Eytzinger factor walks per executable, exactly eight
with a T0 site, and four specialized B+ walks. Direct AVX2 comparisons are inlined;
all specialized B+ walks have no calls before normal return, with three remaining
call sites in bounds-failure blocks. Arithmetic source does not imply a wholly
branch-free compiled algorithm: loop/rank/prefetch guards remain. Static code
observations establish mechanisms, not causal runtime savings; matching WSL
hardware counters remain unavailable. No prefetch-distance/hint sweep, B+ prefetch,
PGO/LTO, cold/>RAM/native-provider capacity or other-architecture result is claimed.

Decision: retain prefetched arithmetic IP traversal, aligned candidate hot-ASN
and specialized B+ paths as workload-specific candidates. Regressions, extra RAM,
repeat drift, synthetic warm corpora and host uncertainty prevent automatic
adoption. Follow-through: P1-06 qualified explicit memory budgets, with
native-width IPv4 fences, directory-aware forests and measured selection policy.
Post-hoc fastest variants are comparison references, not an implemented policy.

Verification: root formatting, default/all-feature all-target Clippy with warnings
denied, default tests, all-feature tests including ignored cases and warnings-denied
docs passed. Standalone default/SIMD release Clippy, harness/fragment formatting,
17 generated-core tests and lib/test Clippy passed. Receipt reconstruction,
Python/JSON parsing and local documentation links pass. Local package verification
is checked on this checkpoint; hosted CI is tracked on the publishing commit.
No registry release.


### 2026-11-04: P1-06 adaptive search and explicit cache budgets

[Study/reproduction](benches/adaptive/README.md),
[results](benches/adaptive/RESULTS.md),
[primary](benches/results/2026-11-04-adaptive.json),
[fresh repeats](benches/results/2026-11-04-adaptive-repeat.json),
[assembly](benches/results/2026-11-04-adaptive-asm.json), and
[verification](benches/results/2026-11-04-adaptive-verification.json).

Completed the isolated budget/selector experiment: 96 primary processes plus
18 fresh 20M repeats, 6,624 timings. Budgets 0/1/128 MiB apply to aggregate
additional cache vector element capacities. Admit native u32 IPv4 fences, u128
IPv6 fences, then ASN B+ leaves/nodes/level ranges; skip zero/unaffordable
candidates before allocating. Check actual capacities before publication and
retain typed errors/idempotent same-budget preparation. No comparison mirrors
coexist. Ordinary directory/validation, mapping, object/allocator overhead and
RSS are outside this explicit additional-cache budget.

Controls are ordinary visitor, prior cross-query AVX2, admitted global caches,
and IPv4-directory-aware/native-width fences with count ASN nodes. A deterministic
policy uses batch length, first 32 family/span and admitted cache presence, with
ordinary/cross-query fallbacks. Measure whole-corpus decisions and requests of
at most 32 queries, including sampling/dispatch/rebasing overhead. Four-trial
forward/reverse timings preserve the original corpora and add late-wide/late-hot
controls that reverse locality after the sampled prefix. No traffic labels or
timing feedback inform selection. Early exploratory five-variant logs are
excluded; final receipts contain the complete six-variant matrix and repeats.

Decision: reject the tested cache-presence/prefix heuristic for production.
Across 144 20M workload/build/budget/process combinations, whole-request policy
was within 10% of the fastest observed fixed-control median in 73/144; request32
policy in 111/144. Worst ratios to that optimistic fixed comparator were 3.11x
(whole-request ASN late-wide) and 2.48x (request32 IPv4 uniform). Those medians
are not significance claims. Cache presence does not establish benefit; a larger
budget does not imply faster queries. Preserve the explicit budget mechanism
and failed selector as research, with no production/API/format/default change.

Large IPv4 caches use 312,500 bytes and build in 0.221-0.252 ms; large IPv6 use
1,250,000 bytes and 0.401-0.505 ms when admitted. Large ASN uses 85,333,440 bytes
(81.38 MiB) and 36.734-64.844 ms. The 1 MiB budget admits large IPv4 but skips
large IPv6/ASN; the zero budget admits no arrays. Build ranges span four
primary/repeat default/SIMD processes and exclude normal mapping/validation.
Actual capacities satisfy every budget. These are warm one-host synthetic
throughput/startup measurements, not cold/>RAM, RSS, native-provider, optimal
admission or universal-win evidence.

All original assertions and complete independent original-input oracles pass:
ordered IDs/targets, duplicates, borrowed bytes/pointers, partial/empty/mixed
batches, broad-route fallback and typed callback-error prefixes. Checks include
31/32/33-query policy boundaries and forced scalar paths. Eighteen generated-core
tests and lib/test Clippy pass. Fresh shadow preparation reproduces every
recorded generated source; header-derived aggregate admission, full-request
policy traces, coordinates, result checksums and source/input/executable/raw-log
hashes agree. Frozen earlier studies remain unchanged.

Linked assembly retains six bodies per executable: the 64-byte chooser, request
planners and specialized scalar/AVX2 ASN walks. Five generic execute summaries
retain scaled-access examples; inlined IP searches are not isolated from other
variants, so those examples do not establish width-only effects.
Static counts establish emitted mechanisms, not runtime cost or causality;
matching WSL hardware counters are unavailable. More representative sampling,
batch sizes, admission economics, startup amortization and RSS/cold qualification
are needed before a production selector. Follow-through: P1-07 qualified learned
fences under exact correction and correctness fallback. P2 profiling/PGO qualification
remains distinct from this no-PGO portable-release experiment.

Verification: Rust 1.95 root formatting, default/all-feature all-target Clippy
with warnings denied, default tests and all-feature serial tests including
ignored, warning-denied all-feature docs and all-feature package verification
passed. Both standalone release builds passed formatting/Clippy; generated-core
18 tests and lib/test Clippy passed. Python/JSON syntax, documentation links,
receipt reconstruction and diff checks passed. Local checks are distinct from
remote CI; hardware counter profiling remains unavailable in this WSL kernel.
Library version remains 2.1.0; this checkpoint publishes research through Git,
without a registry release or public behavior change.


### 2026-11-04: P1-07 learned fences with guarded exact correction

[Study/reproduction/reference](benches/learned/README.md),
[results](benches/learned/RESULTS.md),
[primary](benches/results/2026-11-04-learned.json),
[fresh repeats](benches/results/2026-11-04-learned-repeat.json),
[assembly](benches/results/2026-11-04-learned-asm.json), and
[verification/fixture reproduction](benches/results/2026-11-04-learned-verification.json).

Completed the isolated experiment: 144 primary processes plus 66 fresh repeats,
5,280 timings. Fit a greedy affine slope corridor to upper-CDF ranks for native
u32 IPv4 fences/ASN keys and normalized u64 IPv6 fences when their span fits 64
bits. Error 4/32/128 settings run in separate processes. Binary/radix segment
routing, integer subtraction before f64 conversion, end clamps and bounded local
correction are timed. A two-neighbor global check proves the exact upper bound;
invalid predictions fall back to full native search. Requested model error is
not a proof for unseen gaps, duplicate jumps or floating-point rounding.

This independently written Rust prototype is inspired by RadixSpline routing/
corridors, but uses disjoint anchored affine segments rather than connected
spline vertices. It is not an upstream port, upstream performance comparison or
full RadixSpline/PGM qualification. No upstream code is vendored. Single-segment
models bypass routing and allocate no radix; otherwise bits adapt to segment
count up to 12. Scalar/native correction retains existing packed delta search;
no new explicit SIMD kernel, trained selector or PGO is adopted.

Compare ordinary, prior cross-query AVX2, global native sorted cache, learned
binary routing and learned radix routing. Reuse all original fixtures/corpora
unchanged and add eight 65,536-record curved/clustered IPv4/IPv6 shared/unique
fixtures, with independent original-target scan oracles and 63 extra clustered
gap queries. IPv6 exercises almost full-width u64 deltas under a high 64-bit base.
All 20M exact and all nonlinear cases repeat in fresh processes. Timings use
CPU 20 portable release/codegen-units=1, four forward/reverse trials with a warmup
before each measurement; only uniform/hot exact traffic and uniform shapes are
claimed. Early lint-failed preparation attempts produce no accepted timings.

Decision: retain guarded prediction as a workload-specific research candidate,
without a production/default/API/encoding change. Fixing error 4/radix route 702
across primary/repeat and default/SIMD 20M processes, uniform ASN is 6.41-10.70x
faster than ordinary and 2.97-4.21x than the fastest observed fixed-control median
(ordinary/prior SIMD/native sorted). Hot ASN is 1.85-1.91x faster than ordinary.
Uniform IPv6 is 1.37-2.69x faster than ordinary, but hot IPv4 ratios are 0.88-0.91x
and hot IPv6 can regress. Small nonlinear cases expose routing/correction
overhead; do not infer a universal winner. Same-process medians and their
optimistic minimum comparator do not establish statistical significance.

All large original cases fit one segment because keys are evenly spaced.
Nonlinear cases have only 256 packed fences; large nonlinear/native-provider
behavior remains unmeasured. Normal probes require zero full-search recoveries
across 4,738,704 primary and 2,373,156 repeat eligible model queries; binary/radix
probe results agree. Forced rank-zero and NaN-slope tests establish recovery,
not fallback-heavy throughput or latency. Interval/broad/wider-than-u64 IPv6
cases keep ordinary fallback and do not qualify learned acceleration.

Aggregate additional cache vector capacity is bounded by 128 MiB. Actual 20M
IPv4/IPv6/ASN capacities are 312,580/625,080/80,000,080 bytes, including spare
segment capacity. Error 4 construction ranges are 0.366-0.571/0.416-0.627/
74.386-83.468 ms across four processes per family, after normal reader validation.
Native mirrors are shared by all comparators and remain resident; model-only
metadata is not the full memory cost. Mapping/validation, existing directory,
objects, allocator overhead and RSS are excluded. Before production adoption,
measure large nonlinear/provider inputs, startup amortization, zero-copy/strided
correction, cold/>RAM/RSS, fallback-heavy traffic and other hosts.

All original assertion/query oracles pass with full ordered IDs/targets,
duplicates, borrowed bytes/pointers, partial/empty/mixed batches, broad routes
and typed callback-error prefixes. Fifteen generated-core tests and lib/test
Clippy pass. Fresh shadow preparation reproduces every recorded source; all
eight new inputs regenerate byte-for-byte. Header-derived native widths/domain
eligibility, segment/radix capacities, aggregate budgets, probes, coordinates,
checksums and source/input/executable/raw-log/repeat/assembly linkage pass.
Frozen earlier study sources and production code remain unchanged.

Assembly retains two native-width guarded-search bodies (498/520 bytes) and
two 605-byte estimate bodies per executable. Estimates use one floating multiply
and no floating division; bounds, conversions and routing branches remain.
Static counts do not establish runtime costs or causality. Matching WSL perf
counters remain unavailable. Root formatting, default/all-feature all-target
Clippy, serial default/all-feature tests including ignored and warning-denied
all-feature docs pass; standalone release builds and fragments pass formatting/
Clippy. Final packaging/link/integrity checks accompany the checkpoint.

At the P1-07 checkpoint, next was **P1-08 actual-hardware AVX-512/NEON qualification**, preserving portable
fallbacks. P2 profiling/counters/PGO/build work remains distinct from these
portable no-PGO warm measurements. Library version remains 2.1.0; research is
published through Git without a registry release.

Inspection retains non-inlined estimate/search boundaries, so call/result-record
costs are included. Inlining/specialization is an unmeasured follow-up; timings
do not establish the best possible implementation of the learned approach.

### P1-08 native architecture qualification - 2026-11-05 UTC

Completed native execution on Ryzen 9 9950X3D2 (WSL2, AVX-512F/BW, CPU 20)
and Neoverse-N2 (GitHub ubuntu-24.04-arm, NEON/ASIMD, CPU 3). The ARM run
[passed](https://github.com/0xARYA/netindex/actions/runs/41988692543).
Both receipts identify 707ddb1; 25 generated files and 15 original sources per
host reconstruct independently, with source line endings normalized. All 16
native input hashes also match the frozen P1 exact-key/shape fixtures.

68 final processes produced 4,384 timing rows, 88 full-contract prechecks and
25,612,800 kernel comparisons. Runtime guards, unavailable-ISA and 16-byte NEON
scalar fallbacks, unsigned boundaries, all 64 alignments, malformed columns,
full IDs/targets/borrowed payloads, callback order/errors and mixed/empty/partial
cases pass. Both hosts passed ten core unit tests and study Clippy. Repository
fmt, both Clippy configurations, default/all-feature tests including ignored,
warning-denied docs and all-feature package verification pass.

AVX-512 ordinary/candidate medians: 20M uniform IPv4 176.67/142.89 ns (1.24x),
20M uniform IPv6 291.60/318.15 ns (0.92x); the latter varies 1.01x to 0.87x by
process. Untouched ASN control variation limits exact percentage claims.
Packed NEON hot IP cases improve 1.14-1.16x with repeat agreement; byte scanning
is weaker there, but faster on 20M uniform IPv6. No universal winning policy.
Disassembly confirms unsigned vector comparisons; packed NEON reduces code
from 292-648 to 172-196 bytes using narrowing and trailing-zero reduction.

Retain research, without production/API/encoding/version changes. These are
warm virtualized-host measurements, without PMU or bare-metal isolation. Intel,
Apple, other ARM cores, SVE/SVE2 and cross-query AVX-512/NEON remain unmeasured.
[Detailed results and reproducible evidence](benches/architectures/RESULTS.md).
At the P1-08 checkpoint, next was **P1-09 payload distribution**.

### P1-09 payload distribution completed - 2026-11-05

Measurement revision ccd069d. Three files / two independent real source snapshots:
old/current retained City (20,255,722 assertions each) and public IPtoASN combined
IPv4/IPv6 (719,149 ranges, including ASN zero). Raw provider data stays local.
Two fresh processes per file, reverse case order, CPU 20, Rust 1.95 portable
release. Input/executable/source hashes, wall/RSS and all aggregate data retained.

City: 271,275 distinct complete payloads / 16,726,691 unique bytes. Original pool
503,375,959 bytes contains 486,649,268 redundant bytes; the current pool has zero
redundant bytes. Original/current insertion-order payload digests, frequencies,
family sharing and reuse agree exactly. Six record lengths (11/46/50/54/58/62),
reference-weighted p50/p90/p99/max 62 bytes. IPtoASN: 86,902 values / 4,166,150
unique bytes; 166 lengths, p50/p90/p99/max 40/66/95/222; zero redundant storage.

Exact first-seen 4 MiB key-cache model (length + 64) predicts City's original
503,375,959 pool bytes and 8,186,119 physical spans exactly. Distinguish that
non-evicting admission model from reference gaps, distinct reuse and payload-only
LRU: 94.52% of City's repeated references fit a 4 MiB payload-byte footprint,
excluding metadata/overhead and without any serving-traffic claim.

Current City references consume 162,045,776 bytes, 61.4% of the file. Three-byte
IDs + eight-byte distinct span directory estimate 62,937,366 bytes (94.5 MiB
reference reduction). The same estimate saves 2.8 MiB for IPtoASN. This is P1-10
input, not an implemented format, measured speed gain or minimum-size claim.

Four focused study tests pass on Windows/Linux. Independent Python full scans
agree on byte equality/cardinality, lengths/frequencies, physical spans, gaps,
digests and 4 MiB payload-only LRU counts. All original ASN IDs/targets/payloads
and all analyzer payload slots/borrowed pointers are checked. Six profile runs,
exact repeat parity, all retained hashes and 28 measured source/lock files pass.
Root fmt, both Clippy configurations, default/all-feature tests including ignored,
warning-denied docs and all-feature package verification pass.

Analyzer times: City ~6.5-7.0 s; IPtoASN ~0.19 s. City modeled workspace 273.8 MiB,
but maximum RSS ~976.0 MiB old / 530.8 MiB current, including mapped input. The
512 MiB model is not an RSS bound. No library/API/encoding/version change.
[Detailed results](benches/payload_distribution/RESULTS.md).
Next: **P1-10 compact references**.

### 2026-11-05: retire unadopted search study harnesses

Retired 89 tracked files / 10,103 source lines from completed P1-01 through
P1-08 studies, including the one-off architecture workflow. Preserve all result
reports and immutable JSON measurements. Short per-study READMEs now link to
the full pre-retirement snapshot 9010a9eb6bcd8e96c0a60d98b12a44db974ddf79;
the archive manifest records every removed Git blob for verification/recovery.

The adaptive selector remains rejected. Other unadopted candidates are archived
without relabeling their workload-specific gains as failures. Production
benchmarks, comparison/qualification baselines and the payload profiler remain
active. No library/API/encoding/version change. Raw local input data, generated
source snapshots, executables, profiles and disassembly remain untouched.
This is source-tree cleanup; no artifact disk reclamation is claimed.

Next: implement **P1-10 compact references**, then measure its size, lookup,
startup and validation-memory costs before deciding adoption.

Retirement verification: all 89 archive Git-blob pointers and 68 local report/
recovery links pass. Production sources, root Cargo files, FORMAT.md, the main
Rust workflow and all measurement JSON/result reports are unchanged. Documented
fmt/Clippy/tests/docs/package checks and the production SIMD packed-width
benchmark correctness smoke pass. No benchmark timings were collected.
