# Benchmarks

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

## SIMD and validation follow-up

On the same CPU, AVX2 variants did not consistently beat scalar search across
cached ASN and City hit workloads. The optional packed-search SIMD path remains unchanged.
A 128-bit comparison prototype and width-specialized searches were discarded.

An opening profile instead found discarded target construction during validation.
Checking numeric bounds directly improved ordinary IPv4/IPv6 network opening by
7-13% across two paired trials of 65,536 assertions. Wide IPv6 address results
were mixed: 21% faster in one pair and 6% slower in the other. These are warm
synthetic validation measurements, not full-database startup measurements.

[Results and profiling scope](benches/results/2026-10-08-simd.json).

## String-pool opening

Batched validation checks the shared UTF-8 body and every value boundary.
The `simd` feature uses `simdutf8`; malformed inputs retain value-relative errors.
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

AVX-512 searches and an additional vectorized ordering pass did not establish
repeatable gains. A NEON search prototype compiled for ARM64 but was not run;
the optional UTF-8 dependency supplies ARM64 acceleration independently.
