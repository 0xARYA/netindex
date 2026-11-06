# P3 direct longest-prefix lookup

The new `Reader::longest_prefix` uses the existing disjoint packed search and
a right-first traversal of overlapping interval rows. The search drops a
subtree if its maximum end cannot cover the address; once a winner exists,
earlier starts cannot outrank it. At the same start, it still checks
specificity and insertion ID. Only the winner's payload is resolved. There is
no persistent side index, allocation, file-format change or change to
`visit_ip`. Large same-start or range-heavy overlap groups can still require
many row reads.

Windows x64, Ryzen 9 9950X3D2, CPU 20, Rust 1.95.0 portable release with
runtime SIMD. Each shape uses 100,000 generated leaves and the P0 comparison
corpus. Three processes per shape, four trials each, 1,048,576 lookups per
sample. Values are aggregate median ns/query, including the equivalent output
fingerprint. The oracle checks original masked keys, boundaries and neighbours
before each timed corpus. The trie comparison has unique CIDRs; additional
integration tests cover duplicate IDs, host addresses, arbitrary ranges,
legacy/packed/bitpacked layouts and aliases.

| Workload | Visitor selection | Direct | Trie |
| --- | ---: | ---: | ---: |
| IPv4 disjoint uniform | 53.89 | 57.87 | 38.77 |
| IPv4 disjoint hot | 46.81 | 51.64 | 37.24 |
| IPv4 overlap uniform | 420.98 | 211.21 | 42.30 |
| IPv4 overlap hot | 335.84 | 168.66 | 44.42 |
| IPv6 disjoint uniform | 78.57 | 82.29 | 112.08 |
| IPv6 disjoint hot | 47.45 | 52.23 | 109.47 |
| IPv6 overlap uniform | 682.25 | 286.42 | 118.08 |
| IPv6 overlap hot | 445.89 | 198.40 | 116.42 |

Adopt the additive direct API. On overlap it costs about 42–50% of visitor
selection latency; the specialized trie still wins those cases. On disjoint
inputs the new wrapper costs about 5–10% more than the visitor. Keep the
existing visitor for callers that need all results. These are fixed warm
corpora, not a claim about every routing table or CPU.

To reproduce a Windows case, build with
`cargo +1.95.0 build --locked --release --features simd --manifest-path benches/competitors/Cargo.toml`.
Run the resulting executable through [run.ps1](../native_city/run.ps1) with
arguments `FRESH_DIRECTORY v4 overlap 100000` (substitute `v6` and/or
`disjoint`). Use three fresh processes per shape, reversing the order of the
four shapes in the middle pass.

[Raw receipts, input hashes and ranges](../results/2026-11-06-p3-longest-prefix.json)
