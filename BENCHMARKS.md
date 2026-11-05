# Benchmarks

The ordered [performance work queue](PERF_TASKS.md) tracks experiments, acceptance
criteria, measurements and decisions. New results should update that queue.

## Matched-contract competitors (2026-11-04)

The [comparison harness](benches/competitors/README.md) measures longest-prefix
selection against ip_network_table and unique-ASN Boolean membership against
Roaring, with default/SIMD netindex builds at 1K/100K/1M leaves or keys.
[Results and decisions](benches/competitors/RESULTS.md) retain wins, regressions,
serialized sizes, scope and limitations. The complete matrix has 42 processes
and 1,032 timed measurements with independently verified corpora and checksums.
The existing native City adapter supplies equivalent owned geography results
separately; Windows and Linux observations are not pooled.

Specialized indexes win important cases; netindex wins others. A smaller Boolean
bitmap or faster longest-prefix lookup does not establish preservation of the
full assertion contract. These results change no algorithm, format or default
build policy. The [evidence standard](benches/EVIDENCE.md) inventories the P0 suite
and defines the qualification needed for subsequent optimization prototypes.

## Concurrent readers and replacement (2026-11-04)

The [Linux harness](benches/concurrent/README.md) records 1/2/4/8/16 shared
readers on 1M/20M IPv4/IPv6/ASN inputs, including sampled latency histograms,
clock controls, two reversed-order trials and exact-query prechecks.
[Scaling results](benches/results/2026-11-04-concurrent.json) show substantial
throughput growth and some superlinear cases on WSL. Shared caches and host
scheduling prevent interpreting these as per-core service capacity.

[Replacement results](benches/results/2026-11-04-replacement.json) cover coarse
and fine routing at 1/4/16 workers: 600 successful publications and 120 rejected
candidates, with original-target result checks and mapping reclamation proofs.
Median candidate construction is 4.59–6.24 us coarse versus 4.22–4.56 ms fine
for this 196,672-assertion fixture. Mapping and validation are preloaded; fine
routing rebuilds routing data. Batch percentiles include 32 queries and caller
snapshot acquisition; publication metrics include lock waiting, with only fifty
samples per case. See the queue and receipts for ranges and scheduling outliers.
No library publication API or lookup optimization is introduced by this study.

## City replay and size scaling (2026-11-04)

The [City replay](benches/results/2026-11-04-city-replay.json) reproduces the exact
750,594,180-byte retained component with 2.0.2, then rebuilds the same assertions
with 2.1.0 into 263,944,912 bytes. Both pairs preserve every ID, target, payload,
metadata byte and sampled all-match query result. Building takes about 2.2x as
long; logical temporary use falls from 2,447,925,271 to 1,961,276,003 bytes.
This is opaque assertion replay, not complete native-provider conversion.

```powershell
cargo +1.95.0 build --locked --release --features external-sort,mmap --example rebuild
& target/release/examples/rebuild.exe INPUT.nidx NEW-OUTPUT.nidx EXISTING-TEMP-DIRECTORY
```

Keep input and output immutable while mapped. The output must not exist. The
example reconstructs original insertion order and verifies all assertions plus
100,005 edge/random IP query results. Options are 16,384-row runs, a 4 MiB key
cache and an 8 GiB temporary budget. Hash cached inputs and outputs externally.
To reproduce the old version, compile the unchanged example in a standalone
crate using netindex `=2.0.2`, the same dependency versions/features, and release
codegen-units=1. Pin and run baseline/candidate serially in alternating order.
The result file records the resolved baseline dependencies. Whole-process memory
includes the replay oracle and both mappings; it is not service or builder-only RAM.

The [native adapter](benches/native_city/README.md) separately measures native
validation, normalization/encoding, construction, normalized payload validation
and owned-record serving. [Results](benches/results/2026-11-04-native-city.json)
qualify the old path against the original artifact hash and compare 108,123
normalized answers per version. Complete conversion takes about 1.7x as long
with 2.1.0 and uses about 60% less peak process memory. Native MMDB still wins
file size, opening and memory here; see the queue for phase timings and limits.

The [size baseline](benches/results/2026-11-04-scale.json) covers 1K through 20M
exact-address/ASN assertions, with 50% hits and gaps and four-byte pooled payloads.
The default small cases are suitable for correctness checks:

```powershell
cargo +1.95.0 bench --locked --bench scale -- --test
$env:NETINDEX_BENCH_SIZES = '1000,100000,1000000,20000000'
cargo +1.95.0 bench --locked --bench scale -- --save-baseline NAME
Remove-Item Env:NETINDEX_BENCH_SIZES
```

Generated files are retained under ignored `target/scale-fixtures/` for hashing.
This repeats a fixed warm 1,024-query corpus; it is a dataset-size baseline,
not a cold-memory, concurrency, overlap, provider-decoding or JSON comparison.

## Traffic, shapes and caller batches (2026-11-04)

```powershell
cargo +1.95.0 bench --locked --bench traffic -- --test
cargo +1.95.0 bench --locked --bench shapes -- --test
cargo +1.95.0 bench --locked --features simd --bench traffic -- --test
cargo +1.95.0 bench --locked --features simd --bench shapes -- --test
$env:NETINDEX_BENCH_SIZES = '1000000,20000000'
cargo +1.95.0 bench --locked --bench traffic -- --save-baseline NAME
Remove-Item Env:NETINDEX_BENCH_SIZES
cargo +1.95.0 bench --locked --bench shapes -- --save-baseline NAME
```

Traffic defaults to 100K assertions. It uses 16,384 queries: uniform, mostly
hits/misses, approximately 90% on 16 hot IDs, and 32-query locality runs. Shape
cases use 65,536 assertions: dense/sparse ASN, full-width sparse IPv6, up to 65
nested IPv6 prefixes, and 64 duplicate IPv4 assertions per key. Shared and fully
unique eight-byte payloads are checked against original-target scans. Caller
batches 1/8/32/256 rotate through the same corpus; `NETINDEX_BENCH_BATCHES` can
select a subset. This is the existing scalar-query visitor API, not a new batch
API. Timed loops exclude JSON and allocation.

Paired [traffic](benches/results/2026-11-04-traffic.json) and
[shape/batch](benches/results/2026-11-04-shapes-batches.json) results retain
original samples, mean confidence intervals, executable and fixture hashes.
Use the process controls in `benches/native_city/run.ps1 -TextOutput` for serial
pinned executables. Run `python benches/collect_matrix.py traffic|shapes
RUN_DIRECTORY OUTPUT_JSON` after both `KIND-20261104-1/2` baselines are saved.
These baseline measurements use default dispatch, not optional SIMD; SIMD
correctness is checked separately. Repeated warm corpora do not establish
cold-file performance, concurrent tails or deployed-service capacity.

The [Linux memory package](benches/memory/README.md) qualifies warm residency,
private-file cold cache and inputs exceeding a 32 MiB cgroup limit. Its
[18-process result](benches/results/2026-11-04-memory.json) records page faults
and memory events. It requires a private Linux filesystem and an already enabled
cgroup-v2 memory controller; it does not flush global caches. WSL host/device
caching remains outside its controls, and its timed correctness assertions and
platform differ from the Windows Criterion suite.

## External payload deduplication

Measured on 4 November 2026: Ryzen 9 9950X3D2, Windows x86_64 MSVC, Rust 1.95.0,
release builds pinned to logical CPU 2 (affinity mask 4). Three alternating,
sequential runs compare revision `f8719cd` with disk-backed deduplication on the
same generated input: 100,000 ASN assertions, 4,096 distinct variable-length
payloads, 1,024-row runs and a 4 KiB payload cache.

| Measurement | Cache-only baseline | Complete deduplication |
| --- | ---: | ---: |
| Artifact bytes | 9,536,345 | 1,926,943 |
| Peak logical temporary bytes | 17,536,257 | 9,926,855 |
| Retained payload cache bytes | 4,086 | 4,086 |
| Build elapsed, observed range (ms) | 149â€“176 | 553â€“1,135 |

Every assertion's lookup is checked against its generated target, ID and complete
payload. Complete deduplication also produces byte-for-byte identical output to
the in-memory builder. The baseline retains the same lookup semantics but writes
repeated uncached payload bytes. Disk-backed matching reduces this fixture's file
size by 79.8%, at the cost of more build I/O and elapsed time. The cache continues
to avoid disk-table work for retained values.

The timer covers pushes and finalization, excluding builder creation, validation
and the oracle. Logical buffer and temporary sizes do not measure RSS, allocator
overhead or filesystem cache. These three trials are descriptive; they do not
establish general build throughput, City conversion savings, or lookup latency.
Mostly unique inputs also pay for the disk table without a size benefit and may
need a larger temporary-disk budget. No provider schema is involved.

Reproduce with `cargo +1.95.0 run --locked --release --features external-sort --example dedup`,
pinning both baseline and candidate to the same CPU. Copy the unchanged example
into a checkout of the baseline revision for comparison.
[Input and executable hashes, settings and raw trials](benches/results/2026-11-04-external-dedup.json).

## MMDB comparison

Measured on 7 October 2026: Ryzen 9 9950X3D2, Windows 11, Rust 1.95.0,
one pinned logical CPU. Both readers map cached DB-IP Lite October 2026 data.
The native reader is `maxminddb 0.32.0`. Netindex uses the public index and
`codec` APIs; the benchmark adapter only selects provider fields. SIMD was disabled.

**Times are ns/query; lower is better.** These are Criterion batch means
divided by query count, with 30 samples and warm pages.

| Dataset / workload | Reader | Lookup | Fields | JSON |
| --- | --- | ---: | ---: | ---: |
| ASN / ipv4-hit | MMDB | 55 | 105 | 212 |
| ASN / ipv4-hit | netindex | 47 | 77 | 188 |
| ASN / ipv6-hit | MMDB | 127 | 170 | 311 |
| ASN / ipv6-hit | netindex | 50 | 78 | 231 |
| ASN / ipv4-uniform | MMDB | 30 | 80 | 170 |
| ASN / ipv4-uniform | netindex | 46 | 83 | 187 |
| ASN / ipv6-uniform | MMDB | 15 | 34 | 72 |
| ASN / ipv6-uniform | netindex | 13 | 23 | 53 |
| City / ipv4-hit | MMDB | 49 | 359 | 699 |
| City / ipv4-hit | netindex | 46 | 110 | 383 |
| City / ipv6-hit | MMDB | 111 | 401 | 788 |
| City / ipv6-hit | netindex | 51 | 93 | 464 |
| City / ipv4-uniform | MMDB | 42 | 389 | 657 |
| City / ipv4-uniform | netindex | 54 | 124 | 362 |
| City / ipv6-uniform | MMDB | 26 | 272 | 542 |
| City / ipv6-uniform | netindex | 56 | 88 | 315 |

JSON includes lookup, borrowed decoding, response construction, allocation, and
serialization. Every timed query is checked for identical fields, network, and JSON.
Lookup returns the same optional network on both readers; JSON misses are `null`.
Warm batch means do not establish cold or concurrent tail latency.

| Dataset | Reader | File size (MiB) | Validated opening (ms) |
| --- | --- | ---: | ---: |
| ASN | MMDB | 9.12 | 31.9 |
| ASN | netindex | 20.36 | 72.8 |
| City | MMDB | 121.11 | 602.3 |
| City | netindex | 268.51 | 1060.9 |

Conversion uses the public codec and shared strings: ASN number
and organization; City country, continent, first region, city, postal code,
coordinates, accuracy radius, and time zone, using English names. Aliases are
expanded, including IPv6 targets for canonical `::/96`. Converted files remain
about **2.2 times the MMDB file size**. Opening includes native `verify()` or index
validation plus `Decoder::open()`, without decoding every payload.

Optional broad routes reduced City uniform IPv6 lookup from 56 to 32 ns, fields
from 88 to 59 ns, and JSON from 315 to 286 ns. They add up to 32 KiB per eligible
family and can slow other queries; default reading leaves them disabled.

Hit workloads sample every 997th native network, capped at 1,024 IPs per family.
Uniform workloads use 1,024 deterministic addresses including misses; four alias
addresses are checked too. Cold queries, concurrency, HTTP, and p95/p99 are unmeasured.

[Measurements, confidence intervals, hashes, and configuration](benches/results/2026-10-07-mmdb.json).
The JSON also retains earlier fixed-codec build and replacement observations,
explicitly marked historical; they were not rerun with the public codec.

## Payload codec comparison

`netindex::codec` is the published codec used in the MMDB comparison above.
The fixed baseline is an older, hand-written codec kept only in the benchmarks.
Both use shared strings; the public codec omits fields absent from DB-IP Lite.
This separate warm payload benchmark samples every 997th native network, capped
at 2,048 records: 705 ASN and 2,048 City records. It uses the same machine and
30 samples, one-second warmup, and three-second measurements. Fields and JSON
are checked before timing. Other Rust workloads were active; the unchanged
control varied between trials, so these numbers do not establish a latency gain.

Reading and JSON timings cover one record. Encoding covers 705 ASN records
or 2,048 City records, including metadata finalization.

| Dataset | Codec | Bytes / record | Read fields (ns / record) | JSON (ns / record) | Encode sample (us) |
| --- | --- | ---: | ---: | ---: | ---: |
| ASN | Fixed baseline | 12 | 14 | 83 | 100.4 |
| ASN | netindex::codec | 9 | 17 | 95 | 50.2 |
| City | Fixed baseline | 51 | 24 | 270 | 653.4 |
| City | netindex::codec | 37 | 28 | 247 | 427.7 |

Opening metadata
took 3.4 us (fixed) and 2.8 us (public) for ASN; 12.0 us and 11.8 us for City.
The public codec produced smaller records in these samples. Read and encoding timings varied;
the results above do not establish a general performance advantage.
These results exclude index lookup and full-file conversion; they do not replace
the MMDB comparison above. [Measurements and hashes](benches/results/2026-10-07-codec.json).

## Reproduce

Download and decompress the two inputs identified in the results file once.
Set `NETINDEX_BENCH_ASN` and `NETINDEX_BENCH_CITY` to their immutable cached paths:

```sh
cargo +1.95.0 bench --bench mmdb --features codec,mmdb,mmap,external-sort
```

Pin to one CPU for comparable results. Conversion uses a 64 MiB payload cache
and an 8 GiB temporary-disk ceiling; ensure free disk can accommodate the build.
Private `.nidx` outputs under `target/mmdb-bench` are removed after the run.
No download occurs.

With the same cached input paths, run the codec comparison with:

```sh
cargo +1.95.0 bench --bench codec --features codec,mmdb,mmap
```

## Index regression benchmarks

```sh
cargo bench --bench index -- --save-baseline before
cargo bench --bench index -- --baseline before
```

This harness uses 65,536 assertions and 1,024 queries per scenario for IPv4,
IPv6 (including uncompressed 128-bit deltas), overlaps, and ASN keys. It measures visitors, scalar payloads serialized
as `{id, value}`, validation, building, and an address-directory experiment.
Failed operations fail the benchmark process.

## SIMD and validation follow-up (2026-10-08)

On the same CPU, AVX2 variants did not consistently beat scalar search across
cached ASN and City hit workloads. The optional packed-search SIMD path remained unchanged.
A 128-bit comparison prototype and width-specialized searches were discarded.

An opening profile instead found discarded target construction during validation.
Checking numeric bounds directly improved ordinary IPv4/IPv6 network opening by
7-13% across two paired trials of 65,536 assertions. Wide IPv6 address results
were mixed: 21% faster in one pair and 6% slower in the other. These are warm
synthetic validation measurements, not full-database startup measurements.

[Results and profiling scope](benches/results/2026-10-08-simd.json).

## String-pool opening

Batched validation checks the shared UTF-8 body and every value boundary.
The `simd-utf8` feature (also enabled by `simd`) uses `simdutf8`; malformed inputs
retain value-relative errors.
The same cached ASN and City inputs produced 73,355 and 177,408 unique strings.

| Dataset | Feature | Previous per-string validation (us) | Batched validation (us) |
| --- | --- | ---: | ---: |
| ASN | Default | 1,405 | 106 |
| ASN | SIMD | 1,252 | 72 |
| City | Default | 4,949 | 894 |
| City | SIMD | 3,127 | 148 |

These are paired means over 30 samples, including directory validation and boundary
checks. Host activity caused timing variation; this measures string-pool opening,
not complete database startup or lookup. Run the codec benchmark with `^utf8/`,
with and without `simd`, to reproduce. [Measurements](benches/results/2026-10-08-simd-followup.json).

At that checkpoint, AVX-512 searches and an additional vectorized ordering pass
did not establish repeatable gains. The NEON search prototype was compile-only;
the optional UTF-8 dependency supplies ARM64 acceleration independently. The
later [P1-08 qualification](benches/architectures/RESULTS.md) executes new packed
search prototypes on native AVX-512 and NEON hardware.

## External I/O, all packed widths, and application build tuning (2026-11-04)

The [performance audit](PERFORMANCE.md) records ETW profiling, assembly inspection,
paired scalar/AVX2 trials across every packed width, positioned-I/O measurements,
and application PGO versus ThinLTO. It includes reproduction instructions,
related-project research, and limits on the conclusions.

Run `cargo bench --bench search -- --test` for all-width correctness checks,
then repeat with `--features simd` before the benchmark separator. The timed
workload uses 65,536 address assertions and 1,024 queries per batch, half hits
and half misses. It checks original targets, IDs, payload bytes, and the actual
encoded width before timing. CI runs both variants without collecting timings.

The following P1-01 through P1-08 search studies are historical qualification.
Their harness source and one-off architecture workflow are retired from the
current tree; [archive/recovery instructions](benches/ARCHIVE.md) identify the
complete frozen checkout. Reports and JSON evidence remain unchanged. Active
production benchmarks and the payload profiler are listed in [benches](benches/README.md).

## Fixed-group query interleaving (2026-11-04)

The [P1-01 study](benches/interleaved/README.md) compares ordinary visitors with
1/4/8/16-lane research kernels in an isolated core copy. Its
[results](benches/interleaved/RESULTS.md) include fresh-process repeats, linked-code
inspection and an explicit decision to retain the prototype without adoption.

## Cross-query SIMD (2026-11-04)

The [P1-02 study](benches/cross_query/README.md) compares ordinary visitors,
P1-01 interleaving and AVX2 gathers across independent ASN/packed-IP queries,
with existing tail SIMD independently enabled/disabled.
[Results and decision](benches/cross_query/RESULTS.md) include all six large-case
fresh-process repeats, native eight-lane u32 ASN, generated-code inspection and
qualified hardware-counter availability. No default/API change is adopted.

## Eytzinger RAM layouts (2026-11-04)

The [P1-03 study](benches/eytzinger/README.md) compares key-only sorted and
Eytzinger heap/rank mirrors with ordinary encoded visits and prior interleaving/
SIMD. [Results](benches/eytzinger/RESULTS.md) report lookup, construction and
allocated-capacity costs separately, including global-fence versus IPv4-directory
scope. Complete post-recovery runs replace an interrupted I/O-failed matrix.
No default or format change is adopted.

### Flat B+ node search (P1-04)

[Study and reproduction](benches/flat_btree/README.md) compare original/prior
SIMD, sorted/Eytzinger mirrors and four flat B+ node methods: binary, count,
AVX2 count and ordered AVX2 mask rank. [Results](benches/flat_btree/RESULTS.md)
include fresh-process repeats, cache capacities/build costs, complete original
query/payload/error checks, linked assembly and retained integrity receipts.
These warm RAM-layout measurements do not establish cold-file behavior, universal
performance or a library-default decision. P1-05 owns traversal/prefetch ablations.

### Traversal, prefetch and alignment (P1-05)

[Study/reproduction](benches/traversal/README.md) measures the complete 2x2x2
arithmetic-child / software-prefetch / physical-alignment design with matched
group accessors, plus specialized B+ count/ordered walks.
[Results](benches/traversal/RESULTS.md) retain primary/fresh-process evidence,
actual pointer alignment, capacity/build accounting and bounded hot-path assembly.
These warm RAM experiments preserve full result/payload/error semantics; optional
memory-budgeted deployment and directory-aware routing remain P1-06 work.

### Adaptive search with an explicit additional-cache budget (P1-06)

[Study/reproduction](benches/adaptive/README.md) runs candidate-only caches under
0/1/128 MiB limits, with native IPv4/directory routing and whole/request32 policy
controls. [Results](benches/adaptive/RESULTS.md) retain 114 processes / 6,624
timings, fresh-process repeats, actual capacities/build costs, complete output/
error checks and linked policy/native-search assembly. The tested cache-presence/
prefix selector is rejected for production after measured regressions. The budget
covers additional vector element capacity, not total reader memory or RSS.

### Learned fences with guarded exact correction (P1-07)

[Study/reproduction](benches/learned/README.md) compares native sorted caches,
binary/radix-routed affine prediction, prior SIMD and ordinary visitors at
errors 4/32/128. [Results](benches/learned/RESULTS.md) retain 210 processes / 5,280
timings, fresh large/nonlinear repeats, exact-result/guard recovery checks,
actual cache/build costs, byte-for-byte new fixture reproduction and linked
model assembly. Large uniformly spaced keys benefit; small nonlinear/hot cases
and unmeasured large nonlinear/provider behavior prevent default adoption.
The 128 MiB bound covers additional vector capacities, not total reader/RSS.

### Native AVX-512 and NEON qualification (P1-08)

[Study/reproduction](benches/architectures/README.md) compares runtime-guarded
AVX-512F/BW tails and two NEON mask reductions with ordinary, scalar and
interleaved controls. [Results](benches/architectures/RESULTS.md) retain native
Ryzen 9 9950X3D2 and Neoverse-N2 evidence: 68 processes, 4,384 timings, unchanged
input hashes, full result/error checks, unsigned/alignment/kernel checks and
linked disassembly. AVX-512 has gains and regressions; packed NEON improves
tested hot-IP cases 1.14-1.16x, with other workload tradeoffs. These warm,
virtualized-host results remain research and do not qualify all microarchitectures
or justify a blanket production policy.

### Real payload distribution and reuse (P1-09)

[Study/reproduction](benches/payload_distribution/README.md) scans retained
old/current City output and an independent public IPtoASN snapshot with exact
byte equality. [Results](benches/payload_distribution/RESULTS.md) retain two
fresh-process passes, independent Python crosschecks, complete payload/borrowed
pointer verification, all original ASN assertions, lengths/frequencies, physical
duplication and insertion-order reuse. Current City payload bytes are fully
deduplicated; eight-byte variable-length references consume 61.4% of that file.
The three-byte-ID plus distinct-span-directory arithmetic estimate motivates
P1-10; it is not an implemented format or measured lookup/build improvement.
Raw provider records remain private. Locality counts describe construction
order and distinguish payload-only reuse from the actual cache admission charge.

## Compact payload references - 2026-11-05

P1-10 is implemented in the production reader and both builders for 3.0.0.
[Results](benches/compact_references/RESULTS.md) and
[reproduction](benches/compact_references/README.md) record unchanged-input
qualification, source/binary hashes, exact semantic verification and migration.
City saves 94.52 MiB (37.55%), IPtoASN 2.77 MiB (9.44%). Full validation is
about twice as fast; warm opaque lookups are about 1.4-5.2% slower against
2.1.0 on the retained workloads. IPtoASN construction is also slower. Explicit
legacy output remains available. No native conversion/application decoding
or universal MMDB performance claim is included.

## Shared aliases - 2026-11-05

P1-11 adds an opt-in production format that shares an IPv4 network table
across explicit IPv6 windows. [Results and replay](benches/shared_aliases/README.md):
retained City drops from 164.8 to 62.4 MB and warm mmap open from 419 to
161 ms. Sampled hits are roughly unchanged; uniform mixed lookup is about
4% slower. The measurements compare the same logical assertions in netindex
and do not qualify native provider readers or every input shape.

## Bounded disk-table cache - 2026-11-05

[P1-12 results](benches/disk_table_cache/README.md) qualify an opt-in
write-back cache for the external builder's temporary payload table. An
8 MiB requested limit charged 4.125 MiB and reduced median build time by
11-15% on retained City and IPtoASN replays, with byte-identical output and
unchanged temporary-disk charges. The measurement includes the canonical
City base from P1-11. Separate syscall counts show fewer positioned reads
and writes. Native Windows performance and cold storage were not measured.

## Grouped payload-table metadata - 2026-11-05

[P1-13 results](benches/grouped_metadata/README.md) reject both bounded
resident fingerprint filters tested against the P1-12 page cache. The revised
filter used another 128-512 KiB within an unchanged 8 MiB limit, increased
median build time by 1.7-4.1%, and reduced positioned reads by less than 0.6%.
Candidate source remains archived; production retains P1-12. Input/output
identity, scalar/SIMD paths, CPU instructions and syscall counts are recorded.

## Partitioned payload deduplication - 2026-11-05

[P1-14 results](benches/partitioned_dedup/README.md) qualify the explicit
`ExternalBuilder::new_partitioned` constructor. At the same 4 MiB key cap with
no table-cache pages, native Windows/NTFS IPtoASN median construction changed
from 2,399.49 to 1,078.17 ms. Wide ranges and mixed cached controls keep this
opt-in. It trades extra sequential passes and temporary disk for fewer random
table operations; City temporary peak increased by 35.9%. All 81 replays retained
identical bytes and verified every assertion plus 100,005 queries. The report
contains timed ranges, native/WSL measurements, resource accounting, instruction
and syscall counters, source snapshots and limitations.
