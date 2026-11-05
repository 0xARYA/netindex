# Performance work queue

Work in priority order. Update this file with commands, immutable-input hashes,
measurements, correctness results, and remaining limitations. A prototype may be
rejected; completion means a recorded decision, not necessarily adoption.

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
- [ ] **P0-09 Competitors.** Equivalent geography with MMDB, prefix selection
  with routing indexes, and membership with bitmap formats.
- [-] **P0-10 Evidence.** Standardize hashes, pinned-core serial runs, build
  provenance, repeated trials and independent correctness oracles for new harnesses.

## P1: lookup architecture

- [ ] **P1-01 Interleaving.** Overlap independent query memory accesses.
- [ ] **P1-02 SIMD across queries.** Compare with existing SIMD tail search.
- [ ] **P1-03 Eytzinger.** Compare fence layouts with the sorted baseline.
- [ ] **P1-04 Flat B+ tree.** Prototype PlanB-inspired node search; preserve the
  declared query contract rather than importing single-result LPM assumptions.
- [ ] **P1-05 Traversal.** Measure prefetching and branchless traversal separately
  and together.
- [ ] **P1-06 Adaptive search.** Choose routing/search strategies under an explicit
  memory budget, extending the current packed/interval split.
- [ ] **P1-07 Learned fences.** Start with IPv4/64-bit keys and exact bounded
  search; evaluate RadixSpline-style prediction with a correctness fallback.
- [ ] **P1-08 Architectures.** Evaluate AVX-512 and NEON on actual hardware while
  retaining portable fallbacks. Compile-only checks do not establish performance.

## P1: storage and construction

- [ ] **P1-09 Payload distribution.** Cardinality, lengths, duplicate bytes and
  repeat distance on real inputs.
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
