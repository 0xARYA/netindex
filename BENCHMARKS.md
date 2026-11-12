# Benchmarks

Netindex is designed for periodically rebuilt IP and ASN datasets where a query
may need **every matching record**. A policy check can return a matching CIDR,
an exception range, and two records with the same target, each with its original
target, ID, and borrowed payload. A visitor processes them without allocating
a result vector.

| What you need | What the measurements say |
| --- | --- |
| Keep all matches in a smaller index | Netindex 3.0's compact references saved **38%** on retained City data and **9%** on IPtoASN versus the previous netindex encoding. Validation took about half as long; warm lookups took 1–5% longer. |
| Serve decoded City records | In the historical DB-IP Lite comparison, Netindex decoded the same selected City fields faster; native MMDB used less disk and opened faster. |
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
longer to build. [Compact-reference results and reproduction](https://github.com/0xARYA/netindex/blob/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches/compact_references/RESULTS.md).

If your data repeats the same IPv4 table under explicit IPv6 windows, opt-in
[shared aliases](https://github.com/0xARYA/netindex/blob/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches/shared_aliases/README.md) can save more. Folding four
windows reduced the same City file from 164.8 to 62.4 MB while keeping all
20.3M logical assertions. On the same reader, warm mapped open fell from
422 to 161 ms; uniform mixed lookup rose from 55.18 to 56.34 ns/query. The
saving depends on the repeated windows.

## Serve selected City fields: Netindex vs. MMDB

The [DB-IP Lite comparison](https://github.com/0xARYA/netindex/blob/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches/BASELINES.md#mmdb-comparison) used
the public Netindex codec and checked equivalent selected fields, matched
networks, and JSON on both readers. In that October 2026 run, IPv4 City hits
took **110 ns/query for lookup plus field decoding**, versus **359 ns** with
native MMDB. JSON took **383 ns vs. 699 ns**. Bare lookup was nearly tied
(46 vs. 49 ns), and native MMDB won bare lookup on the uniform City workloads.
Native MMDB also used less disk (121 vs. 269 MiB) and opened faster
(602 vs. 1,061 ms).

These are warm batch means from the tested fixture and historical reader
versions. They do not measure per-query tails, cold opens, or service latency.

## When to use a specialized index

- **Longest prefix only:** On the 100K-leaf warm synthetic workloads, the trie
  took 42 ns/query versus netindex's 211 ns for uniform overlapping IPv4.
  Netindex took 52 ns versus the trie's 109 ns for hot disjoint IPv6.
  [Direct longest-prefix results](https://github.com/0xARYA/netindex/blob/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches/competitors/P3_RESULTS.md).
- **ASN membership only:** On the 1M-key warm synthetic workloads, Roaring took
  4.5 ns/query versus netindex's 46 ns for uniform dense keys. Netindex took
  35 ns versus Roaring's 45 ns for hot sparse keys. Choose based on your key
  distribution and whether you need records or just membership.
  [ASN membership results and encoded sizes](https://github.com/0xARYA/netindex/blob/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches/competitors/RESULTS.md#asn-membership).

These results compare equivalent answers for each narrow task. They don't
measure the cost of preserving arbitrary ranges, overlaps, duplicate targets,
or multiple payloads in a specialized index.

## Run benchmarks

See the [benchmark suite](benches/README.md) for commands, workloads and how to
compare revisions. The dated results above link to their original reports.
