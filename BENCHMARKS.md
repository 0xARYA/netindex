# Benchmarks

## MMDB comparison

Measured on 7 October 2026: Ryzen 9 9950X3D2, Windows 11, Rust 1.95.0,
one pinned logical CPU. Both readers map the same cached DB-IP Lite October 2026
data; netindex converts it through its public APIs. The MMDB reader is unchanged
`maxminddb 0.32.0`. SIMD is disabled.

**Each cell is MMDB → netindex, in ns/query; lower is better.** These are
Criterion batch means divided by query count, with 30 samples and warm pages.

| Dataset / workload | Lookup only | Read fields | JSON response |
| --- | ---: | ---: | ---: |
| ASN / ipv4-hit | 55 → 50 | 135 → 80 | 257 → 198 |
| ASN / ipv6-hit | 127 → 58 | 170 → 79 | 314 → 228 |
| ASN / ipv4-uniform | 32 → 43 | 75 → 76 | 161 → 165 |
| ASN / ipv6-uniform | 14 → 12 | 33 → 26 | 63 → 54 |
| City / ipv4-hit | 55 → 53 | 404 → 119 | 703 → 487 |
| City / ipv6-hit | 118 → 57 | 504 → 97 | 865 → 383 |
| City / ipv4-uniform | 41 → 46 | 310 → 86 | 559 → 331 |
| City / ipv6-uniform | 25 → 69 | 269 → 99 | 702 → 394 |

JSON timings include lookup, borrowed field decoding, response construction,
allocation, and serialization. Every timed IP is checked for identical fields,
network, and JSON before measurement. Lookup-only returns the same optional
network target on both readers; misses serialize as `null` in JSON timing.

| Dataset | MMDB / nidx file size | Validated opening: MMDB / netindex |
| --- | ---: | ---: |
| ASN | 9.12 MiB / 24.84 MiB | 34.7 ms / 84.0 ms |
| City | 121.11 MiB / 323.00 MiB | 716.8 ms / 1062.1 ms |

Conversion uses a caller-owned fixed-field codec with shared strings. ASN selects
number and organization; City selects country, continent, first region, city,
postal code, coordinates, accuracy radius, and time zone, using English names.
It expands aliases and adds IPv6 targets for the canonical `::/96` subtree.
This costs storage: **the converted files are about 2.7× larger**. Opening includes
native `verify()` or netindex validation plus `StringPool::open()`. File size is
not resident memory.

Hit workloads sample every 997th native network, capped at 1,024 IPs per family;
uniform workloads contain 1,024 deterministic IPs including misses. Four additional
alias addresses are checked and timed. Cold pages, concurrent throughput, HTTP,
RSS, and request p95/p99 are outside this measurement.

[Results, confidence intervals, query counts, input hashes, and build details](benches/results/2026-10-07-mmdb.json).
The benchmark codec and conversion are in [benches/mmdb.rs](benches/mmdb.rs) and
[benches/support](benches/support).

### Reproduce

Download and decompress the two MMDBs identified in the results file once. Keep
them immutable throughout the run. Set paths to those cached inputs:

```sh
NETINDEX_BENCH_ASN=/data/dbip-asn.mmdb \
NETINDEX_BENCH_CITY=/data/dbip-city.mmdb \
cargo +1.95.0 bench --bench mmdb --features mmdb,mmap,shared-values
```

Pin to one CPU for comparable results. Derived `.nidx` files are rebuilt under
a private temporary directory under `target/mmdb-bench`, removed after the run;
no download occurs.

## Index regression benchmarks

```sh
cargo bench --bench index -- --save-baseline before
cargo bench --bench index -- --baseline before
```

This separate harness uses 65,536 assertions and 1,024 queries per scenario
for IPv4, IPv6, overlaps, and ASN keys. It measures visitors, scalar payloads
serialized as `{id, value}`, validation, building, and an address-directory
experiment. Failed operations fail either benchmark process.
