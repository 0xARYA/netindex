# P1-09: exact payload distribution on real inputs

Completed 2026-11-05. No library, public API or format changes. The measurements
establish distributions for two source snapshots, not a minimum achievable file
size or an improvement to lookup/construction speed.

## Inputs and verification

- Retained real City output: the original 2.0.2 conversion, 750,594,180 bytes,
  and its 2.1.0 opaque replay, 263,944,912 bytes. Each has 20,255,722 assertions.
  Input hashes and source provenance remain in the P0 receipts. These are two
  representations of the **same** dataset, not independent City samples.
- Public [IPtoASN combined IPv4/IPv6 TSV](https://iptoasn.com/), downloaded for
  this study: 719,149 IP-range assertions, including unmapped/ASN-zero rows.
  The response identifies a 2026-11-01 snapshot (`Last-Modified`); the archive is
  10,955,454 bytes, SHA-256
  `1634a0e449f3ee11088bba85493447a51007b44dff3ac18328f1f2d9127c0a9e`.
  The study-owned payload framing preserves ASN, country and description;
  endpoints remain targets. This is not the City's application schema, an exact
  ASN-key index, or a native-MMDB size comparison. The official site lists PDDL 1.0.

All analysis uses immutable private copies and exact byte equality, without
sampling. The production reader validates each file; every assertion's payload
and borrowed address are checked against independently interpreted slots. Every
IPtoASN built ID, original target and payload is also checked against the framed
source, and its complete insertion-order payload digest matches the Python
source-framing digest.

Two fresh processes per file, with case order reversed, produce identical
non-timing statistics. Original/current City payload-stream SHA-256, frequency
distributions, family sharing and reuse statistics match exactly. An independent
Python scan agrees on cardinality, lengths, frequency distribution, physical
span counts, insertion repeat gaps, complete payload digests and payload-only
4 MiB LRU counts. It shares no Rust parser or counter implementation. Four Rust
tests cover a naive reuse oracle, empty/fixed/malformed inputs, duplicate bytes
in separate physical spans, and non-evicting-cache versus byte-reuse semantics.

## Cardinality and physical duplication

| Measurement | City original | City current | IPtoASN framed |
| --- | ---: | ---: | ---: |
| Assertions | 20,255,722 | 20,255,722 | 719,149 |
| Distinct complete payload values | 271,275 | 271,275 | 86,902 |
| Logical payload bytes across assertions | 1,195,593,764 | 1,195,593,764 | 30,165,566 |
| Unique payload bytes | 16,726,691 | 16,726,691 | 4,166,150 |
| Physically stored payload bytes | 503,375,959 | 16,726,691 | 4,166,150 |
| Redundant physically stored bytes | 486,649,268 | 0 | 0 |
| Nonempty physical spans | 8,186,119 | 271,275 | 86,902 |
| Singleton values | 21,728 | 21,728 | 36,244 |

The current builders already eliminate the repeated complete City records.
The old file's redundant 464.1 MiB is a historical storage cost, not remaining
savings in 2.1.0. Logical duplicates still total 1,178,867,073 bytes for City
and 25,999,416 bytes for IPtoASN, but those bytes are already shared physically.
The largest value frequency is 1,207,077 City assertions and 139,696 IPtoASN
assertions. No provider record contents are published.

City has 230,239 values shared by IPv4 and IPv6, 41,036 IPv6-only values and no
IPv4-only values. IPtoASN has 30,224 shared, 49,173 IPv4-only and 7,505 IPv6-only.
Shared payloads do not prove shared network aliases; the City counts still
include the conversion's expanded alias assertions.

## Lengths

Both datasets have variable-length complete records, so a global fixed-length
reference encoding cannot represent them directly. City has only six lengths:

| Bytes | Assertions | Distinct values |
| ---: | ---: | ---: |
| 11 | 4,564 | 1 |
| 46 | 3,397,224 | 782 |
| 50 | 34,984 | 367 |
| 54 | 391,854 | 3,624 |
| 58 | 529,503 | 11,600 |
| 62 | 15,897,593 | 254,901 |

City reference-weighted p50/p90/p99/max are all 62 bytes. IPtoASN has 166
different lengths: reference-weighted p50/p90/p99/max are 40/66/95/222 bytes,
and distinct-value-weighted percentiles are 45/69/98/222. All are exact nearest-rank
length quantiles, rather than samples or bucket midpoint estimates. Neither real
input has empty payloads; empty payload support is tested separately.

## Insertion-order locality and cache interpretation

These describe the construction stream, not live query traffic. Cold references
are excluded from reuse quantiles. The profiler computes exact distances, then
publishes logarithmic histograms; the intervals below are **inclusive bounds**.

| Repeated-value quantity | City p50 / p90 / p99 | IPtoASN p50 / p90 / p99 |
| --- | --- | --- |
| Reference gap | 2–3 / 32,768–65,535 / 2,097,152–4,194,303 | 2–3 / 65,536–131,071 / 262,144–524,287 |
| Distinct intervening values | 1 / 4,096–8,191 / 131,072–262,143 | 2–3 / 16,384–32,767 / 65,536–131,071 |
| Intervening distinct bytes + repeated value | 64–127 B / 256–512 KiB / 8–16 MiB | 64–127 B / 512–1,024 KiB / 2–4 MiB |

The byte bounds in the last row use rounded upper endpoints for readability;
the machine-readable histograms retain the exact one-byte-less upper endpoints.
Of City's 19,984,447 repeat references, 18,890,111 (94.52%) have a payload-only
reuse footprint at most 4 MiB. All 632,247 IPtoASN repeats fit that threshold.
Both actual datasets have individual values smaller than the threshold, letting
the independent Python LRU verify these counts. **This excludes cache metadata
and hashing overhead**, and is not evidence of serving locality or cache speed.

Separately, the current external builder's first-seen, non-evicting 4 MiB key
cache charges payload length + 64. Its City model first rejects admission at
insertion ID 872,616, records 12,069,603 hits / 8,186,119 misses,
and predicts **503,375,959 miss-payload bytes**. This exactly matches the old
file's entire physical pool and its 8,186,119 nonempty spans. The current disk
fallback keeps those same opaque values complete without storing repeat bytes.
The model describes cache admission, not the disk table's measured lookup cost.
IPtoASN's model gives 552,673 hits / 166,476 misses and 8,037,392 miss bytes;
its current fully deduplicated pool is only 4,166,150 bytes.

## P1-10 implications

Both current files use eight-byte offset/length references. These consume
162,045,776 bytes (**61.4% of City**) and 5,753,192 bytes (18.7% of IPtoASN).
Both distinct-value counts require three whole bytes for an ordinal. A plain
three-byte ID array plus an eight-byte distinct span directory would cost:

| Arithmetic estimate | City | IPtoASN |
| --- | ---: | ---: |
| IDs + directory | 62,937,366 B | 2,852,663 B |
| Reduction from current references | 99,108,410 B (94.5 MiB) | 2,900,529 B (2.8 MiB) |

This excludes headers, validation memory, decoding/lookup work and compatibility.
For three-byte IDs the reference-only break-even condition is `8U + 3N < 8N`,
or `U/N < 62.5%`. Observed cardinality ratios are 1.34% for City and 12.08% for
IPtoASN. P1-10 should measure that explicit representation with complete semantics,
including its random payload access and startup costs. Few City length classes
are also relevant to later grouped-metadata/bitpacking experiments. No size
estimate is an implemented optimization or a minimum converted-size claim.

## Analysis cost, evidence and limitations

Measurements use Rust 1.95.0 portable release, one codegen unit, no PGO/LTO/custom
CPU flags, CPU 20 on the Ryzen 9 9950X3D2 under Ubuntu 24.04/WSL2. They measure
this offline profiler, not production reader throughput:

| Input | Analyzer wall time, two processes | Maximum process RSS |
| --- | --- | --- |
| City original | 6.76 / 6.66 s | 976.0 MiB |
| City current | 6.96 / 6.48 s | 530.8 MiB |
| IPtoASN | 0.193 / 0.189 s | 50.5 MiB |

`analysis_total_ms` includes reader validation, exact profiling and full payload
crosschecks but excludes copying inputs and process startup. `/usr/bin/time`
also retains whole-process wall/RSS records. The dense count/byte reuse arrays
occupy 243,068,676 bytes for City; modeled total table+tree state is 287,108,868
bytes, under the 512 MiB modeled workspace limit. That limit excludes mapped
input residency, validation state, allocator overhead and histograms; it is
explicitly **not** an RSS bound. IPtoASN modeled state is 19,639,848 bytes.

[Aggregate data](../results/2026-11-05-payload-distribution.json),
[independent verification](../results/2026-11-05-payload-distribution-independent.json)
and the committed receipt retain source/input/executable hashes. Raw files and
logs stay under `/root/.cache/netindex-perf-20261105/payload-distribution-final`.
Reproduce at measurement revision `ccd069d5b715c7ac9595c327d245aa8be84c5ac8`;
later verification/documentation additions leave measured Rust code unchanged.

Both Windows and Linux study tests and Clippy pass. Repository fmt, both Clippy
configurations, default/all-feature tests including ignored cases, warning-denied
docs and all-feature packaging pass. File hashes, accounting identities, source
snapshots and repeat parity verify. No provider datasets are committed. These
two source snapshots do not establish distributions for every provider, codec,
payload schema, build order or serving workload.

Next priority: **P1-10 compact references**.
