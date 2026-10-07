# Recorded benchmarks

These measurements come from a local comparison harness that is not shipped
with the crate or repository. They describe the recorded experiment, not a
performance guarantee. For the maintained Criterion harness, see
[Contributing](CONTRIBUTING.md#benchmarks).

Measured 7 October 2026 against Rust maxminddb 0.32.0 using cached DB-IP ASN Lite
and City Lite. Both readers map their files.

Results below use **benchmark-only alias routing**, fixed-size payloads and shared
strings. The public converter does not provide alias routing.

## Lookup and borrowed fields

Both readers return the same selected fields; serialization is excluded.

| Dataset / queries | MMDB ns/query | Routed netindex ns/query |
| --- | ---: | ---: |
| ASN / IPv4 hits | 105.6 | 75.3 |
| ASN / IPv6 hits | 160.9 | 71.8 |
| ASN / uniform IPv4 | 78.8 | 70.5 |
| ASN / uniform IPv6 | 15.8 | 9.2 |
| ASN / IPv6 alias hits | 242.8 | 98.6 |
| City / IPv4 hits | 417.3 | 129.5 |
| City / IPv6 hits | 458.7 | 144.1 |
| City / uniform IPv4 | 355.7 | 122.8 |
| City / uniform IPv6 | 250.3 | 73.0 |
| City / IPv6 alias hits | 539.2 | 141.5 |

## Serving memory

| Dataset | MMDB warm MiB | Routed netindex warm MiB |
| --- | ---: | ---: |
| ASN | 14.55 | 11.61 |
| City | 126.38 | 107.04 |

Working set includes resident mapped pages, heap and runtime memory. Expanded
conversion without alias routing measures about 21.9 MiB for ASN and 221.0 MiB
for City, exceeding MMDB.

## Method

Windows 11, Ryzen 9 9950X3D2, Rust 1.98.1; scalar release builds with one codegen
unit, no LTO or target-cpu override, pinned to logical CPU 2. Lookup figures are
medians of three process medians, each with a discarded warmup and nine timed
batches. Memory runs warm saved queries for sixteen passes.

Fields and matched networks are checked against MMDB before timing. These are
cached-file batch measurements; cold-disk behavior, concurrent throughput,
p95/p99 and replacement peaks remain unmeasured.
