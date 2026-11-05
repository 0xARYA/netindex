# Competitor results — 2026-11-04

[Reproduction and contracts](README.md), [complete hashed receipt](../results/2026-11-04-competitors.json).

All 42 processes and 1,032 timing measurements passed their independent result
checks. Default and SIMD builds produce byte-identical fixtures and corpora.
This is a warm Linux/WSL baseline at 1K/100K/1M leaves or keys; no core algorithm
or encoded layout changed. Source, lock, executable, input and corpus hashes are
retained with raw-log hashes. Four prefix and six membership trials balance
engine order. The tables show median aggregate ns/query at 1M, including corpus
iteration and output accumulation. Smaller is faster. They do not show individual
query latency or establish statistical significance.

## Longest-prefix selection

| Family and shape | Traffic | netindex default | netindex SIMD | ip_network_table |
| --- | --- | ---: | ---: | ---: |
| v4 disjoint | uniform | 82.1 | 67.4 | 45.3 |
| v4 disjoint | hot | 39.5 | 41.6 | 38.4 |
| v4 overlap | uniform | 606.7 | 647.4 | 51.5 |
| v4 overlap | hot | 312.3 | 314.4 | 40.3 |
| v6 disjoint | uniform | 121.1 | 112.9 | 122.6 |
| v6 disjoint | hot | 41.6 | 45.4 | 108.2 |
| v6 overlap | uniform | 952.2 | 1068.1 | 126.6 |
| v6 overlap | hot | 430.7 | 428.7 | 112.2 |

The trie column is measured in the SIMD comparison executable; its implementation
and compiler target stay unchanged. Raw receipts also retain its default-build
timings. Each result includes network address, prefix and insertion ID. Netindex
visits all matches and selects the longest prefix; the trie directly selects one.
Uniform means generated leaf IDs, with half leaf/gap queries. Overlap parents
cover the gaps, making those cases 100% hits. Hot repeats the first 1,024 leaves.

The specialized trie wins substantially for overlapping-prefix selection. Netindex
wins the hot disjoint IPv6 cases here and is competitive for uniform disjoint
IPv6. This result does not establish general trie or netindex superiority.

## ASN membership

| Shape | Traffic | netindex default | netindex SIMD | Roaring default | Roaring optimized |
| --- | --- | ---: | ---: | ---: | ---: |
| dense | uniform | 46.01 | 49.19 | 4.53 | 4.76 |
| dense | hot | 20.85 | 20.45 | 4.83 | 5.45 |
| sparse | uniform | 51.05 | 47.57 | 51.91 | 52.19 |
| sparse | hot | 35.29 | 35.31 | 44.66 | 45.44 |
| clustered | uniform | 51.08 | 49.20 | 6.31 | 8.43 |
| clustered | hot | 20.87 | 20.94 | 12.83 | 8.90 |

The Roaring columns use the SIMD comparison executable; complete results include
both builds. The contract is Boolean membership over unique positive u32 ASNs,
with empty netindex payloads and half hits. There is no record decoding or JSON.

Roaring is much faster on dense/clustered keys. Netindex is faster on this sparse
permutation. Run compression saves space but can slow lookup: optimization is a
size/speed choice, not an unconditional win.

| 1M keys | netindex encoded bytes | Roaring default serialized bytes | Roaring optimized serialized bytes |
| --- | ---: | ---: | ---: |
| dense | 8,000,109 | 131,208 | 230 |
| sparse | 8,000,109 | 2,524,296 | 2,524,296 |
| clustered | 8,000,109 | 501,368 | 4,540 |

These are serialized sizes, not heap or resident memory. A dense run can encode
membership extremely compactly. Netindex retains assertion identities and its
general payload/reference layout even with empty payloads. Dropping IDs or
duplicate assertions would violate its contract. This comparison uses unique
inputs and makes no equivalent-record-storage claim for bitmaps.

## Equivalent geography

The separately measured [native City evidence](../results/2026-11-04-native-city.json)
already qualifies the application-normalized owned-record contract with 108,123
parity queries, including aliases. Native MMDB is 49,899,266 bytes versus
263,944,912 bytes for the 2.1.0 component. Native serving process peak is about
92.7 MB versus 276.5 MB for the component; opening/validation costs differ and
are retained in that receipt. Native owns its bytes while the component is mapped.

Both report a 100 ns median on the mixed 100,005-query corpus. The component's
900/1,100 ns p95/p99 versus native 1,000–1,100/1,200–1,300 ns are descriptive
two-run observations with 100 ns clock quantization, not a significant speedup
claim. Windows geography timings are not pooled with Linux synthetic timings.

## Decisions and next experiments

- Retain these as comparison baselines; no implementation or default build policy
  is changed. There is no demonstrated universal winner.
- Evaluate interleaving and cross-query SIMD next (P1-01/P1-02) under the full
  all-match contract. The existing SIMD flag is workload-dependent in this suite;
  it is not a blanket improvement, and ASN lookup has no explicit SIMD search.
- Use the trie result to motivate routing/layout experiments (P1-03–P1-06),
  without replacing all-match or duplicate semantics with longest-prefix rules.
- Carry Roaring's density-dependent containers and run compression into adaptive
  layout/bitpacking investigations (P1-06/P1-15). Preserve original IDs and
  payload references; the 230-byte dense membership file is a narrower contract.
- Keep shared-alias and compact-reference investigations (P1-10/P1-11) open.
  Native MMDB still has a substantial geography file-size advantage.
- This suite covers one trie and one Rust bitmap implementation. Broader libraries,
  real routing tables, cold/cache-pressure operation and hardware-specific build
  variants need their own qualified experiments.
