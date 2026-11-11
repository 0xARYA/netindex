# Benchmarks

Netindex is designed for periodically rebuilt IP and ASN datasets where a query
may need **every matching record**. A policy check can return a matching CIDR,
an exception range, and two records with the same target, each with its original
target, ID, and borrowed payload. A visitor processes them without allocating
a result vector.

| What you need | What the measurements say |
| --- | --- |
| Keep all matches in a smaller index | Netindex 3.0's compact references saved **38%** on retained City data and **9%** on IPtoASN versus the previous netindex encoding. Validation took about half as long; warm lookups took 1–5% longer. |
| Serve decoded City records | On a Byway collector-host replay, the shared-alias netindex layout with the tuned Byway loader delivered **2.64 µs p99 vs. 3.45 µs** for native MMDB. The index was **25% larger** on disk. |
| Pick one CIDR or check ASN membership | The tested trie won overlapping-prefix selection. Roaring won dense and clustered Boolean membership. Netindex won some disjoint IPv6 and sparse ASN cases. |

## Keep all records while using less space

These comparisons rebuild the same inputs and preserve every assertion, ID,
original target, payload, and metadata byte. File sizes are decimal MB; validation
measures a mapped open with warm file pages, excluding application decoding.

| Input | File size, legacy → compact | Validation, legacy → compact |
| --- | ---: | ---: |
| Retained City, 20.3M assertions | 263.9 → 164.8 MB | 836 → 419 ms |
| IPtoASN, 719K assertions | 30.7 → 27.8 MB | 33.7 → 16.0 ms |

Both 3.0 builders choose compact references when they save space. This is a
size and opening improvement over netindex 2.1.0. The additional reference read
made warm opaque lookups about 1–5% slower on these workloads, and IPtoASN took
longer to build. [Compact-reference results and reproduction](benches/compact_references/RESULTS.md).

If your data repeats the same IPv4 table under explicit IPv6 windows, opt-in
[shared aliases](benches/shared_aliases/README.md) can save more. Folding four
windows reduced the same City file from 164.8 to 62.4 MB while keeping all
20.3M logical assertions. On the same reader, warm mapped open fell from
422 to 161 ms; uniform mixed lookup rose from 55.18 to 56.34 ns/query. The
saving depends on the repeated windows.

## Serve City data: netindex vs. native MMDB

The [collector-host replay](benches/native_city/P3_RESULTS.md#collector-host-validation-follow-up)
compares native GeoLite2 City MMDB against two netindex layouts with netindex
3.0.1 and Byway's tuned validator. All paths returned equivalent normalized,
owned records on 1,965 production-session queries and 8,118 additional
alias and boundary queries.

| Measurement | Native MMDB | Netindex, shared aliases | Netindex, also bitpacked |
| --- | ---: | ---: | ---: |
| File size | 49.9 MB | 62.4 MB | 56.6 MB |
| Warm trace p95 / p99, µs | 2.72 / 3.45 | 2.22 / 2.64 | 2.18 / 2.60 |
| Cold open and validate, s | 5.50 | 2.39 | 2.54 |
| Cold replacement reload, s | 5.56 | 2.35 | 2.51 |

Netindex answered this trace faster at the tail. Native MMDB used less disk and
opened and reloaded faster against the original Byway loader: the shared layout
took 7.59 s to open and 7.72 s to reload before tuning. Most of the startup
gain came from Byway's payload cache and overlap checks. Upgrading the crate
alone won't deliver the same result.

These are medians from three standalone processes per condition, using a warm,
all-hit trace for lookups and verifying zero resident Linux file pages before
cold opens. They don't measure miss latency, HTTP, or a live rollout.
[Samples, artifact hashes, and cache controls](benches/results/2026-11-06-city-validation-collector.json).

## When to use a specialized index

- **Longest prefix only:** On the 100K-leaf warm synthetic workloads, the trie
  took 42 ns/query versus netindex's 211 ns for uniform overlapping IPv4.
  Netindex took 52 ns versus the trie's 109 ns for hot disjoint IPv6.
  [Direct longest-prefix results](benches/competitors/P3_RESULTS.md).
- **ASN membership only:** On the 1M-key warm synthetic workloads, Roaring took
  4.5 ns/query versus netindex's 46 ns for uniform dense keys. Netindex took
  35 ns versus Roaring's 45 ns for hot sparse keys. Choose based on your key
  distribution and whether you need records or just membership.
  [ASN membership results and encoded sizes](benches/competitors/RESULTS.md#asn-membership).

These results compare equivalent answers for each narrow task. They don't
measure the cost of preserving arbitrary ranges, overlaps, duplicate targets,
or multiple payloads in a specialized index.

## Run benchmarks and inspect the evidence

- [Benchmark reports and reproduction](benches/BASELINES.md): commands for
  regression, traffic, scale, City conversion, and other maintained harnesses,
  plus historical comparisons and their limits.
- [Performance evidence index](benches/EVIDENCE.md): experimental reports, raw
  measurements, and what to record when comparing changes.
