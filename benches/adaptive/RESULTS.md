# Adaptive search results - 2026-11-04

Decision: reject the tested cache-presence/prefix heuristic for production.
Retain candidate-only budget enforcement and the failed selector as
reproducible research. No production policy, cache default, public API or
format change is adopted. A larger budget does not imply faster queries.

[Protocol/reproduction](README.md), [primary matrix](../results/2026-11-04-adaptive.json),
[fresh-process repeats](../results/2026-11-04-adaptive-repeat.json),
[assembly/counter probe](../results/2026-11-04-adaptive-asm.json),
[integrity verification](../results/2026-11-04-adaptive-verification.json).

## Contract and measurement scope

96 primary processes and 18 fresh 20M repeats yield 6,624 timings. Each process
holds only its admitted candidate caches. All original assertions and the
complete independent original-input query oracle pass, including ordered
targets/IDs, duplicate multiplicity, borrowed payload bytes/pointers, partial/
empty/mixed batches, broad-route fallback and typed callback-error prefixes.
Forced scalar paths and 18 generated-core tests pass. Preparation reproduces
in a shadow checkout; source/input/executable/raw-log hashes, header-derived
aggregate capacity budgets, policy traces and measurement coordinates agree.

Every number below is a four-trial median warm aggregate ns/query. One warmup
precedes each timing; forward/reverse trial ordering, default/SIMD build order
and budget order are controlled. Both builds use runtime AVX2 detection.
This is one WSL host with fixed synthetic corpora. These are throughput costs,
not latency percentiles, statistical-significance claims or cold/>RAM results.
No PGO/LTO/custom target flags are used. Different budgets run in different
processes, so ratios across budgets include process drift.

IDs: 0 ordinary; 108 prior cross-query AVX2; 600 global admitted caches/ordered
ASN; 601 directory-aware IPv4/count ASN; 602 full-request prefix policy; 603
the same policy in requests of at most 32 queries. Sampling, dispatch and
request splitting/rebasing are timed. Cache construction is outside lookup
timing. Full-request route traces correspond to 602, not every 603 subrequest.

## Budget and startup

Admission order is native u32 IPv4 fences, u128 IPv6 fences, then ASN flat B+
leaves/internal keys/level ranges. Empty/unaffordable candidates allocate no
arrays. The bound covers actual additional vector element capacities, not
RSS, mapped input, ordinary directory/validation, object or allocator memory.
Fixed admission is not an optimized allocation policy. Startup below includes
only optional-cache construction after ordinary reader validation.

| 20M family | Budget MiB | Additional bytes | Build ms range, four processes |
| --- | ---: | ---: | ---: |
| v4 | 0 | 0 | 0.004-0.014 |
| v4 | 1 | 312,500 | 0.223-0.234 |
| v4 | 128 | 312,500 | 0.221-0.252 |
| v6 | 0 | 0 | 0.005-0.006 |
| v6 | 1 | 0 | 0.005-0.006 |
| v6 | 128 | 1,250,000 | 0.401-0.505 |
| asn | 0 | 0 | 0.005-0.006 |
| asn | 1 | 0 | 0.004-0.006 |
| asn | 128 | 85,333,440 | 36.734-64.844 |

## Selector qualification

For the 144 measured 20M workload/build/budget/process combinations, compare
each policy with the fastest observed fixed-control median among 0/108/600/601.
The minimum of several measured controls is an optimistic comparator, not
a deployable oracle. Counts and extrema do not establish significance.

| Policy | Within 10% of fastest fixed | Slower than ordinary | Worst ratio to fastest fixed | Worst case |
| --- | ---: | ---: | ---: | --- |
| 602 | 73/144 | 81/144 | 3.11x | primary asn simd b128 late-wide |
| 603 | 111/144 | 85/144 | 2.48x | primary v4 default b1 uniform |

Late-wide and late-hot deliberately reverse locality after the first 32
queries. Full-request 602 necessarily retains the decision from that prefix.
Request 32 variant 603 resamples every request, but pays more decision/dispatch
overhead; its first request can still be unrepresentative. Span is only a
proxy for locality. No claim that the heuristic is robust to arbitrary order,
distribution shifts, mixed-family suffixes or batch sizes follows.

Read same-process fixed controls before attributing a difference to policy.
601 changes both IPv4 directory use and ASN node search; it is not one global
factorial contrast. Native-width IPv4 arrays plus the existing directory are
qualified as a combined candidate; this study does not isolate width alone.

## Linked code and counters

Extracted symbol bodies and static instruction counts are retained in the
assembly receipt for the policy chooser, request planners and specialized ASN
scalar/AVX2 walks. Generic execute summaries retain scaled-access examples;
IP searches are inlined there alongside other variants, so those examples
do not isolate the IPv4 loop or width-only effects. Bounds, fallback and emission
remain in timed generic readers. Static counts are not runtime cost or branch/cache
causality. Counter probe status: unavailable.

Both builds emit the same 64-byte chooser with four conditional moves, no
conditional jumps and no calls. The 733-byte count walk already contains
four SSE comparison sites; the 1064-byte ordered walk contains 14 AVX2
comparison sites. Both walks have no calls before their first return.
Portable/scalar fallback means no explicit AVX2 dispatch; compiler-generated
SSE remains permitted. More explicit SIMD does not establish a faster path.

## 20M primary medians

| Family / shape | Build | Budget MiB | Traffic | Policy 602 route | 0 | 108 | 600 | 601 | 602 | 603 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 128 | uniform | 601 | 178.6 | 152.0 | 253.1 | 202.5 | 156.0 | 154.5 |
| v4 | simd | 128 | hot | 0 | 38.0 | 49.3 | 42.5 | 38.5 | 38.2 | 38.8 |
| v4 | simd | 128 | late-wide | 0 | 100.3 | 99.2 | 178.6 | 98.1 | 104.6 | 105.1 |
| v4 | simd | 128 | late-hot | 601 | 37.8 | 49.3 | 42.1 | 38.2 | 38.0 | 38.2 |
| v4 | default | 128 | uniform | 601 | 205.5 | 115.9 | 157.5 | 116.6 | 136.1 | 212.1 |
| v4 | default | 128 | hot | 0 | 39.8 | 49.7 | 44.1 | 44.6 | 40.2 | 40.6 |
| v4 | default | 128 | late-wide | 0 | 149.7 | 93.4 | 159.4 | 117.7 | 163.6 | 145.4 |
| v4 | default | 128 | late-hot | 601 | 40.0 | 50.0 | 44.2 | 44.9 | 45.6 | 40.6 |
| v4 | simd | 1 | uniform | 601 | 101.4 | 99.1 | 187.0 | 109.4 | 115.8 | 149.8 |
| v4 | simd | 1 | hot | 0 | 37.6 | 49.1 | 42.0 | 37.9 | 37.6 | 38.4 |
| v4 | simd | 1 | late-wide | 0 | 102.4 | 100.4 | 178.8 | 119.2 | 104.9 | 105.0 |
| v4 | simd | 1 | late-hot | 601 | 37.7 | 49.5 | 42.2 | 38.2 | 39.1 | 38.9 |
| v4 | default | 1 | uniform | 601 | 220.1 | 119.2 | 191.9 | 111.5 | 133.8 | 276.8 |
| v4 | default | 1 | hot | 0 | 41.3 | 50.0 | 43.7 | 45.6 | 40.6 | 41.7 |
| v4 | default | 1 | late-wide | 0 | 173.5 | 97.5 | 149.9 | 100.2 | 152.3 | 149.2 |
| v4 | default | 1 | late-hot | 601 | 40.4 | 49.9 | 43.8 | 45.2 | 45.0 | 40.8 |
| v4 | simd | 0 | uniform | 108 | 97.7 | 97.3 | 101.1 | 99.9 | 99.0 | 100.0 |
| v4 | simd | 0 | hot | 0 | 37.6 | 48.5 | 38.2 | 38.2 | 37.8 | 38.4 |
| v4 | simd | 0 | late-wide | 0 | 98.9 | 96.7 | 100.8 | 100.9 | 100.0 | 98.9 |
| v4 | simd | 0 | late-hot | 108 | 37.9 | 48.7 | 38.3 | 38.5 | 48.6 | 39.0 |
| v4 | default | 0 | uniform | 108 | 159.9 | 101.9 | 163.2 | 154.0 | 97.9 | 96.3 |
| v4 | default | 0 | hot | 0 | 39.9 | 54.1 | 40.6 | 40.5 | 40.2 | 40.7 |
| v4 | default | 0 | late-wide | 0 | 150.4 | 100.6 | 157.9 | 154.8 | 152.4 | 95.5 |
| v4 | default | 0 | late-hot | 108 | 40.5 | 54.4 | 41.2 | 41.3 | 54.1 | 40.7 |
| v6 | default | 0 | uniform | 108 | 288.6 | 171.4 | 288.5 | 273.9 | 165.4 | 162.1 |
| v6 | default | 0 | hot | 0 | 41.8 | 65.7 | 42.8 | 42.4 | 41.8 | 42.9 |
| v6 | default | 0 | late-wide | 0 | 249.6 | 140.3 | 243.8 | 255.3 | 234.3 | 137.6 |
| v6 | default | 0 | late-hot | 108 | 41.6 | 65.8 | 43.4 | 43.4 | 65.8 | 43.5 |
| v6 | simd | 0 | uniform | 108 | 323.1 | 224.6 | 332.4 | 330.2 | 186.5 | 188.7 |
| v6 | simd | 0 | hot | 0 | 42.8 | 64.9 | 43.1 | 44.3 | 42.8 | 43.6 |
| v6 | simd | 0 | late-wide | 0 | 224.3 | 142.4 | 234.7 | 238.7 | 227.1 | 142.9 |
| v6 | simd | 0 | late-hot | 108 | 42.3 | 64.6 | 42.8 | 42.9 | 64.3 | 43.4 |
| v6 | default | 1 | uniform | 108 | 244.4 | 130.7 | 229.5 | 230.4 | 135.6 | 126.5 |
| v6 | default | 1 | hot | 0 | 41.1 | 65.2 | 42.3 | 41.5 | 40.8 | 42.2 |
| v6 | default | 1 | late-wide | 0 | 230.0 | 130.3 | 230.8 | 235.0 | 228.8 | 129.5 |
| v6 | default | 1 | late-hot | 108 | 41.0 | 64.8 | 41.9 | 41.6 | 65.1 | 42.5 |
| v6 | simd | 1 | uniform | 108 | 298.4 | 184.8 | 301.0 | 284.5 | 168.9 | 162.0 |
| v6 | simd | 1 | hot | 0 | 42.4 | 63.6 | 43.0 | 43.0 | 42.6 | 43.3 |
| v6 | simd | 1 | late-wide | 0 | 214.8 | 144.0 | 230.5 | 232.3 | 223.1 | 138.0 |
| v6 | simd | 1 | late-hot | 108 | 42.6 | 63.7 | 43.2 | 43.5 | 63.9 | 43.6 |
| v6 | default | 128 | uniform | 600 | 289.6 | 166.9 | 302.2 | 326.7 | 311.6 | 303.9 |
| v6 | default | 128 | hot | 0 | 41.3 | 65.3 | 42.8 | 41.8 | 41.2 | 43.1 |
| v6 | default | 128 | late-wide | 0 | 235.6 | 133.1 | 228.4 | 230.8 | 237.6 | 227.3 |
| v6 | default | 128 | late-hot | 600 | 41.0 | 64.9 | 42.5 | 41.5 | 42.6 | 43.4 |
| v6 | simd | 128 | uniform | 600 | 282.0 | 195.2 | 289.5 | 284.9 | 287.8 | 299.3 |
| v6 | simd | 128 | hot | 0 | 42.2 | 66.6 | 42.1 | 42.4 | 42.6 | 44.0 |
| v6 | simd | 128 | late-wide | 0 | 224.1 | 142.3 | 220.1 | 222.1 | 228.5 | 220.8 |
| v6 | simd | 128 | late-hot | 600 | 42.4 | 66.9 | 42.8 | 42.8 | 44.3 | 43.8 |
| asn | simd | 128 | uniform | 600 | 350.4 | 162.1 | 173.4 | 256.6 | 139.5 | 181.5 |
| asn | simd | 128 | hot | 0 | 22.2 | 39.4 | 28.8 | 34.5 | 22.7 | 23.2 |
| asn | simd | 128 | late-wide | 0 | 191.5 | 103.6 | 112.9 | 161.8 | 321.9 | 161.5 |
| asn | simd | 128 | late-hot | 600 | 22.2 | 39.3 | 28.8 | 34.8 | 28.8 | 23.2 |
| asn | default | 128 | uniform | 600 | 201.5 | 103.1 | 131.7 | 166.2 | 129.4 | 133.9 |
| asn | default | 128 | hot | 0 | 21.9 | 39.3 | 29.0 | 34.6 | 22.5 | 23.2 |
| asn | default | 128 | late-wide | 0 | 303.3 | 115.2 | 150.0 | 215.7 | 292.3 | 150.4 |
| asn | default | 128 | late-hot | 600 | 21.9 | 39.0 | 28.9 | 34.7 | 28.9 | 23.0 |
| asn | simd | 1 | uniform | 108 | 158.9 | 98.2 | 185.5 | 229.3 | 119.5 | 128.4 |
| asn | simd | 1 | hot | 0 | 22.3 | 39.3 | 22.8 | 22.8 | 22.5 | 23.0 |
| asn | simd | 1 | late-wide | 0 | 172.0 | 94.2 | 160.5 | 164.4 | 159.7 | 94.9 |
| asn | simd | 1 | late-hot | 108 | 22.1 | 39.2 | 22.7 | 22.7 | 39.2 | 23.0 |
| asn | default | 1 | uniform | 108 | 165.7 | 94.5 | 154.8 | 163.4 | 88.0 | 90.9 |
| asn | default | 1 | hot | 0 | 22.1 | 39.8 | 23.5 | 23.1 | 22.4 | 23.0 |
| asn | default | 1 | late-wide | 0 | 373.7 | 158.7 | 357.6 | 359.3 | 352.8 | 164.0 |
| asn | default | 1 | late-hot | 108 | 22.0 | 39.2 | 22.6 | 22.5 | 39.3 | 23.2 |
| asn | simd | 0 | uniform | 108 | 196.6 | 97.3 | 215.1 | 191.8 | 111.2 | 111.9 |
| asn | simd | 0 | hot | 0 | 22.7 | 39.3 | 22.9 | 25.2 | 23.0 | 23.1 |
| asn | simd | 0 | late-wide | 0 | 329.8 | 155.1 | 379.4 | 377.5 | 382.2 | 173.8 |
| asn | simd | 0 | late-hot | 108 | 23.4 | 39.9 | 23.2 | 23.2 | 39.3 | 23.2 |
| asn | default | 0 | uniform | 108 | 212.7 | 101.6 | 205.3 | 170.9 | 95.6 | 118.8 |
| asn | default | 0 | hot | 0 | 23.7 | 40.3 | 24.4 | 23.2 | 22.7 | 23.6 |
| asn | default | 0 | late-wide | 0 | 381.0 | 152.0 | 362.5 | 296.7 | 390.9 | 177.4 |
| asn | default | 0 | late-hot | 108 | 22.1 | 39.6 | 22.7 | 22.8 | 39.7 | 23.2 |

## 20M fresh-process repeat medians

| Family / shape | Build | Budget MiB | Traffic | Policy 602 route | 0 | 108 | 600 | 601 | 602 | 603 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | simd | 128 | uniform | 601 | 101.7 | 102.6 | 183.6 | 94.3 | 93.4 | 99.5 |
| v4 | simd | 128 | hot | 0 | 37.4 | 49.7 | 41.5 | 38.0 | 38.0 | 38.3 |
| v4 | simd | 128 | late-wide | 0 | 118.8 | 110.2 | 224.6 | 129.5 | 102.4 | 119.2 |
| v4 | simd | 128 | late-hot | 601 | 37.8 | 49.2 | 41.8 | 37.8 | 37.9 | 38.1 |
| v4 | default | 128 | uniform | 601 | 189.2 | 115.3 | 180.4 | 123.3 | 110.1 | 238.5 |
| v4 | default | 128 | hot | 0 | 39.9 | 51.1 | 46.2 | 45.2 | 40.7 | 40.6 |
| v4 | default | 128 | late-wide | 0 | 182.9 | 97.7 | 149.5 | 102.9 | 169.8 | 184.7 |
| v4 | default | 128 | late-hot | 601 | 40.4 | 49.3 | 44.0 | 44.9 | 44.9 | 40.7 |
| v4 | simd | 1 | uniform | 601 | 154.0 | 137.3 | 241.0 | 156.2 | 127.1 | 145.9 |
| v4 | simd | 1 | hot | 0 | 38.6 | 49.4 | 43.2 | 38.8 | 38.2 | 39.1 |
| v4 | simd | 1 | late-wide | 0 | 133.7 | 126.4 | 259.7 | 153.6 | 190.4 | 163.8 |
| v4 | simd | 1 | late-hot | 601 | 38.4 | 49.3 | 43.2 | 38.9 | 38.7 | 38.7 |
| v4 | default | 1 | uniform | 601 | 259.9 | 167.9 | 201.5 | 162.2 | 166.7 | 240.8 |
| v4 | default | 1 | hot | 0 | 40.8 | 49.7 | 43.9 | 44.8 | 40.8 | 41.8 |
| v4 | default | 1 | late-wide | 0 | 164.4 | 95.5 | 142.4 | 100.6 | 160.7 | 164.5 |
| v4 | default | 1 | late-hot | 601 | 42.1 | 50.9 | 43.9 | 44.4 | 44.7 | 41.0 |
| v4 | simd | 0 | uniform | 108 | 178.6 | 166.1 | 176.5 | 184.0 | 156.4 | 158.6 |
| v4 | simd | 0 | hot | 0 | 38.0 | 49.7 | 38.0 | 38.2 | 37.9 | 38.4 |
| v4 | simd | 0 | late-wide | 0 | 124.5 | 134.9 | 128.9 | 126.6 | 120.0 | 157.3 |
| v4 | simd | 0 | late-hot | 108 | 37.6 | 50.2 | 38.2 | 38.3 | 49.8 | 38.3 |
| v4 | default | 0 | uniform | 108 | 222.4 | 133.3 | 213.0 | 224.1 | 134.0 | 139.1 |
| v4 | default | 0 | hot | 0 | 40.4 | 50.2 | 41.6 | 41.9 | 41.0 | 41.4 |
| v4 | default | 0 | late-wide | 0 | 247.7 | 125.6 | 204.3 | 203.9 | 213.3 | 123.4 |
| v4 | default | 0 | late-hot | 108 | 40.1 | 49.9 | 40.9 | 40.8 | 49.3 | 40.7 |
| v6 | default | 0 | uniform | 108 | 259.0 | 150.2 | 285.0 | 281.5 | 179.6 | 168.3 |
| v6 | default | 0 | hot | 0 | 41.2 | 64.5 | 42.7 | 42.8 | 41.0 | 42.8 |
| v6 | default | 0 | late-wide | 0 | 269.2 | 162.9 | 280.7 | 301.3 | 273.1 | 152.1 |
| v6 | default | 0 | late-hot | 108 | 41.7 | 65.9 | 44.3 | 46.0 | 65.9 | 42.9 |
| v6 | simd | 0 | uniform | 108 | 305.2 | 224.9 | 329.1 | 333.5 | 218.3 | 236.7 |
| v6 | simd | 0 | hot | 0 | 42.3 | 64.6 | 43.1 | 43.5 | 42.7 | 43.8 |
| v6 | simd | 0 | late-wide | 0 | 272.1 | 177.4 | 271.5 | 263.9 | 244.8 | 141.9 |
| v6 | simd | 0 | late-hot | 108 | 42.9 | 65.2 | 43.5 | 43.7 | 64.7 | 43.9 |
| v6 | default | 1 | uniform | 108 | 287.0 | 181.5 | 310.6 | 300.2 | 174.5 | 172.8 |
| v6 | default | 1 | hot | 0 | 41.1 | 65.1 | 42.9 | 42.8 | 41.4 | 43.1 |
| v6 | default | 1 | late-wide | 0 | 238.0 | 126.7 | 236.7 | 287.3 | 262.6 | 139.9 |
| v6 | default | 1 | late-hot | 108 | 42.8 | 67.0 | 44.8 | 45.2 | 66.9 | 44.6 |
| v6 | simd | 1 | uniform | 108 | 313.2 | 212.3 | 298.1 | 309.9 | 173.7 | 154.4 |
| v6 | simd | 1 | hot | 0 | 43.5 | 65.4 | 44.2 | 43.2 | 42.9 | 44.1 |
| v6 | simd | 1 | late-wide | 0 | 340.7 | 268.4 | 357.3 | 377.6 | 307.5 | 179.0 |
| v6 | simd | 1 | late-hot | 108 | 42.3 | 65.1 | 42.8 | 42.9 | 64.5 | 43.3 |
| v6 | default | 128 | uniform | 600 | 261.1 | 167.4 | 273.5 | 261.9 | 265.9 | 259.7 |
| v6 | default | 128 | hot | 0 | 40.8 | 64.9 | 41.9 | 42.0 | 40.8 | 43.1 |
| v6 | default | 128 | late-wide | 0 | 312.7 | 156.6 | 281.2 | 279.6 | 308.7 | 308.6 |
| v6 | default | 128 | late-hot | 600 | 40.7 | 66.0 | 42.4 | 41.5 | 42.0 | 43.9 |
| v6 | simd | 128 | uniform | 600 | 295.4 | 190.4 | 318.2 | 331.0 | 311.2 | 334.8 |
| v6 | simd | 128 | hot | 0 | 43.1 | 64.6 | 43.1 | 42.5 | 42.8 | 43.3 |
| v6 | simd | 128 | late-wide | 0 | 328.1 | 228.3 | 332.1 | 324.1 | 293.7 | 324.0 |
| v6 | simd | 128 | late-hot | 600 | 42.8 | 65.1 | 43.2 | 43.5 | 43.3 | 43.9 |
| asn | simd | 128 | uniform | 600 | 324.5 | 138.8 | 142.8 | 204.6 | 198.6 | 157.9 |
| asn | simd | 128 | hot | 0 | 22.9 | 39.4 | 29.6 | 34.9 | 22.4 | 23.1 |
| asn | simd | 128 | late-wide | 0 | 394.7 | 169.0 | 196.1 | 255.9 | 316.0 | 154.5 |
| asn | simd | 128 | late-hot | 600 | 22.6 | 39.3 | 29.5 | 34.8 | 29.3 | 23.3 |
| asn | default | 128 | uniform | 600 | 391.3 | 171.0 | 236.1 | 314.0 | 219.9 | 206.5 |
| asn | default | 128 | hot | 0 | 22.1 | 39.7 | 29.2 | 34.4 | 22.3 | 23.3 |
| asn | default | 128 | late-wide | 0 | 406.2 | 161.2 | 236.2 | 253.5 | 388.1 | 226.3 |
| asn | default | 128 | late-hot | 600 | 22.3 | 40.1 | 29.2 | 34.6 | 29.2 | 23.3 |
| asn | simd | 1 | uniform | 108 | 185.5 | 113.8 | 254.6 | 165.1 | 102.3 | 103.7 |
| asn | simd | 1 | hot | 0 | 22.7 | 39.5 | 22.6 | 22.5 | 22.4 | 23.0 |
| asn | simd | 1 | late-wide | 0 | 185.6 | 103.2 | 186.9 | 167.5 | 167.7 | 91.0 |
| asn | simd | 1 | late-hot | 108 | 22.8 | 39.3 | 23.2 | 22.8 | 39.2 | 23.0 |
| asn | default | 1 | uniform | 108 | 443.2 | 181.2 | 444.0 | 451.9 | 146.5 | 142.9 |
| asn | default | 1 | hot | 0 | 22.4 | 39.6 | 22.7 | 22.8 | 22.4 | 23.1 |
| asn | default | 1 | late-wide | 0 | 377.6 | 162.8 | 361.9 | 310.4 | 330.9 | 160.3 |
| asn | default | 1 | late-hot | 108 | 22.3 | 39.6 | 22.7 | 22.6 | 40.0 | 23.2 |
| asn | simd | 0 | uniform | 108 | 158.7 | 100.4 | 160.1 | 157.0 | 93.7 | 96.4 |
| asn | simd | 0 | hot | 0 | 22.7 | 39.2 | 22.6 | 22.7 | 22.1 | 23.2 |
| asn | simd | 0 | late-wide | 0 | 187.1 | 96.5 | 160.3 | 148.6 | 154.7 | 96.9 |
| asn | simd | 0 | late-hot | 108 | 22.8 | 39.3 | 22.6 | 22.6 | 39.4 | 23.1 |
| asn | default | 0 | uniform | 108 | 413.4 | 168.0 | 383.1 | 397.5 | 169.2 | 134.4 |
| asn | default | 0 | hot | 0 | 22.1 | 39.5 | 22.8 | 22.6 | 22.3 | 23.0 |
| asn | default | 0 | late-wide | 0 | 154.1 | 93.8 | 157.9 | 159.8 | 157.3 | 101.2 |
| asn | default | 0 | late-hot | 108 | 22.3 | 39.2 | 22.7 | 22.5 | 39.5 | 23.0 |

## 1M primary medians

| Family / shape | Build | Budget MiB | Traffic | Policy 602 route | 0 | 108 | 600 | 601 | 602 | 603 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| v4 | default | 0 | uniform | 108 | 86.3 | 65.3 | 83.6 | 80.6 | 64.2 | 69.5 |
| v4 | default | 0 | hot | 0 | 39.7 | 48.6 | 40.8 | 40.5 | 40.5 | 40.8 |
| v4 | default | 0 | late-wide | 0 | 79.9 | 64.8 | 82.0 | 81.7 | 81.1 | 68.7 |
| v4 | default | 0 | late-hot | 108 | 39.9 | 48.5 | 40.6 | 40.8 | 48.7 | 40.0 |
| v4 | simd | 0 | uniform | 108 | 59.1 | 66.5 | 60.4 | 60.5 | 66.7 | 68.2 |
| v4 | simd | 0 | hot | 0 | 37.8 | 47.7 | 38.1 | 37.9 | 37.9 | 38.3 |
| v4 | simd | 0 | late-wide | 0 | 59.5 | 66.7 | 60.7 | 60.7 | 59.7 | 67.7 |
| v4 | simd | 0 | late-hot | 108 | 37.4 | 48.3 | 37.9 | 38.1 | 47.4 | 38.3 |
| v4 | default | 1 | uniform | 601 | 81.0 | 63.8 | 95.3 | 68.5 | 69.3 | 85.0 |
| v4 | default | 1 | hot | 0 | 39.5 | 48.7 | 43.1 | 44.2 | 41.1 | 40.4 |
| v4 | default | 1 | late-wide | 0 | 80.4 | 64.2 | 94.6 | 68.8 | 79.7 | 85.8 |
| v4 | default | 1 | late-hot | 601 | 40.2 | 48.9 | 42.5 | 43.6 | 43.9 | 40.7 |
| v4 | simd | 1 | uniform | 601 | 59.3 | 66.3 | 105.0 | 61.2 | 61.6 | 66.5 |
| v4 | simd | 1 | hot | 0 | 37.4 | 47.7 | 38.3 | 37.5 | 37.8 | 38.1 |
| v4 | simd | 1 | late-wide | 0 | 60.2 | 66.1 | 106.1 | 62.2 | 59.8 | 66.7 |
| v4 | simd | 1 | late-hot | 601 | 37.6 | 47.5 | 38.0 | 37.6 | 37.5 | 38.0 |
| v4 | default | 128 | uniform | 601 | 79.6 | 64.9 | 94.7 | 68.6 | 69.0 | 84.5 |
| v4 | default | 128 | hot | 0 | 39.8 | 48.5 | 42.7 | 44.4 | 39.8 | 40.4 |
| v4 | default | 128 | late-wide | 0 | 80.5 | 64.4 | 96.1 | 69.3 | 80.5 | 85.8 |
| v4 | default | 128 | late-hot | 601 | 40.2 | 48.2 | 43.1 | 44.8 | 44.8 | 40.5 |
| v4 | simd | 128 | uniform | 601 | 59.4 | 65.8 | 105.1 | 61.8 | 62.0 | 67.6 |
| v4 | simd | 128 | hot | 0 | 37.6 | 47.0 | 39.0 | 38.1 | 37.5 | 38.1 |
| v4 | simd | 128 | late-wide | 0 | 59.2 | 65.8 | 106.4 | 62.9 | 60.2 | 68.8 |
| v4 | simd | 128 | late-hot | 601 | 37.7 | 47.5 | 39.3 | 38.2 | 38.0 | 38.1 |
| v6 | simd | 128 | uniform | 600 | 107.3 | 77.0 | 107.8 | 108.4 | 108.7 | 109.4 |
| v6 | simd | 128 | hot | 0 | 37.8 | 57.7 | 38.8 | 38.6 | 38.0 | 39.4 |
| v6 | simd | 128 | late-wide | 0 | 113.7 | 78.0 | 108.7 | 108.4 | 110.3 | 111.3 |
| v6 | simd | 128 | late-hot | 600 | 37.9 | 57.3 | 38.4 | 38.5 | 38.5 | 39.0 |
| v6 | default | 128 | uniform | 600 | 118.3 | 74.9 | 120.3 | 121.5 | 121.1 | 123.5 |
| v6 | default | 128 | hot | 0 | 40.1 | 57.3 | 39.6 | 39.6 | 40.7 | 40.9 |
| v6 | default | 128 | late-wide | 0 | 115.7 | 73.5 | 118.9 | 119.7 | 115.1 | 122.6 |
| v6 | default | 128 | late-hot | 600 | 40.1 | 57.5 | 40.5 | 41.2 | 40.3 | 42.8 |
| v6 | simd | 1 | uniform | 600 | 108.4 | 77.9 | 109.4 | 109.4 | 109.3 | 110.1 |
| v6 | simd | 1 | hot | 0 | 38.5 | 57.5 | 38.7 | 38.5 | 38.3 | 39.2 |
| v6 | simd | 1 | late-wide | 0 | 107.1 | 77.9 | 108.2 | 107.9 | 106.7 | 112.0 |
| v6 | simd | 1 | late-hot | 600 | 38.3 | 57.7 | 38.8 | 38.7 | 38.7 | 39.2 |
| v6 | default | 1 | uniform | 600 | 117.2 | 74.5 | 120.2 | 120.0 | 121.5 | 124.1 |
| v6 | default | 1 | hot | 0 | 40.1 | 59.0 | 42.0 | 40.3 | 40.6 | 41.9 |
| v6 | default | 1 | late-wide | 0 | 116.9 | 73.5 | 119.5 | 119.5 | 116.4 | 122.9 |
| v6 | default | 1 | late-hot | 600 | 40.2 | 57.8 | 39.6 | 39.9 | 39.5 | 41.0 |
| v6 | simd | 0 | uniform | 108 | 107.2 | 78.2 | 109.2 | 110.9 | 76.6 | 85.2 |
| v6 | simd | 0 | hot | 0 | 39.1 | 58.7 | 41.8 | 40.9 | 38.6 | 40.5 |
| v6 | simd | 0 | late-wide | 0 | 107.3 | 77.3 | 108.1 | 107.8 | 107.6 | 84.5 |
| v6 | simd | 0 | late-hot | 108 | 38.2 | 56.7 | 40.7 | 40.7 | 56.9 | 39.4 |
| v6 | default | 0 | uniform | 108 | 116.3 | 74.9 | 117.7 | 117.7 | 74.9 | 76.4 |
| v6 | default | 0 | hot | 0 | 40.2 | 57.8 | 40.5 | 40.7 | 40.7 | 40.9 |
| v6 | default | 0 | late-wide | 0 | 117.0 | 74.4 | 116.7 | 116.9 | 116.1 | 75.9 |
| v6 | default | 0 | late-hot | 108 | 40.4 | 57.6 | 40.7 | 40.5 | 57.7 | 41.0 |
| asn | default | 0 | uniform | 108 | 50.6 | 44.3 | 52.5 | 52.2 | 44.1 | 46.7 |
| asn | default | 0 | hot | 0 | 17.3 | 32.4 | 17.9 | 18.0 | 17.6 | 18.4 |
| asn | default | 0 | late-wide | 0 | 51.3 | 45.4 | 52.6 | 52.7 | 51.2 | 47.1 |
| asn | default | 0 | late-hot | 108 | 17.5 | 32.5 | 18.0 | 17.9 | 32.6 | 18.5 |
| asn | simd | 0 | uniform | 108 | 51.2 | 44.3 | 52.6 | 52.9 | 44.4 | 47.0 |
| asn | simd | 0 | hot | 0 | 17.4 | 32.5 | 18.0 | 18.0 | 17.5 | 18.5 |
| asn | simd | 0 | late-wide | 0 | 51.5 | 45.2 | 53.6 | 53.3 | 51.6 | 47.7 |
| asn | simd | 0 | late-hot | 108 | 17.4 | 32.7 | 18.0 | 18.1 | 32.7 | 18.5 |
| asn | default | 1 | uniform | 108 | 53.9 | 46.5 | 55.0 | 54.5 | 46.1 | 48.9 |
| asn | default | 1 | hot | 0 | 17.6 | 32.7 | 18.1 | 18.1 | 17.6 | 18.4 |
| asn | default | 1 | late-wide | 0 | 51.8 | 46.7 | 56.3 | 55.1 | 52.7 | 49.9 |
| asn | default | 1 | late-hot | 108 | 17.8 | 32.4 | 18.1 | 18.2 | 32.3 | 18.7 |
| asn | simd | 1 | uniform | 108 | 52.0 | 44.8 | 53.1 | 53.7 | 44.2 | 47.3 |
| asn | simd | 1 | hot | 0 | 17.5 | 32.6 | 18.1 | 18.0 | 17.6 | 18.4 |
| asn | simd | 1 | late-wide | 0 | 51.3 | 45.5 | 53.7 | 53.3 | 51.3 | 47.1 |
| asn | simd | 1 | late-hot | 108 | 17.4 | 32.5 | 18.0 | 18.1 | 32.7 | 18.4 |
| asn | default | 128 | uniform | 600 | 51.7 | 44.7 | 40.2 | 45.1 | 40.1 | 40.6 |
| asn | default | 128 | hot | 0 | 17.4 | 32.5 | 18.9 | 27.3 | 18.3 | 19.1 |
| asn | default | 128 | late-wide | 0 | 52.2 | 45.1 | 40.0 | 45.9 | 53.1 | 40.8 |
| asn | default | 128 | late-hot | 600 | 17.7 | 32.9 | 19.7 | 27.6 | 18.8 | 19.5 |
| asn | simd | 128 | uniform | 600 | 52.9 | 46.7 | 41.2 | 47.3 | 40.5 | 42.3 |
| asn | simd | 128 | hot | 0 | 18.5 | 32.7 | 19.0 | 27.2 | 18.1 | 18.9 |
| asn | simd | 128 | late-wide | 0 | 51.7 | 44.9 | 40.2 | 45.3 | 52.1 | 42.0 |
| asn | simd | 128 | late-hot | 600 | 18.0 | 32.6 | 18.6 | 27.1 | 18.5 | 18.9 |

## Shape medians

| Family / shape | Build | Budget MiB | Traffic | Policy 602 route | 0 | 108 | 600 | 601 | 602 | 603 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| dense-asn-shared | default | 0 | uniform | 108 | 19.3 | 31.3 | 21.3 | 21.0 | 31.4 | 32.4 |
| dense-asn-shared | simd | 0 | uniform | 108 | 20.0 | 33.2 | 23.2 | 21.9 | 32.3 | 34.1 |
| dense-asn-shared | default | 1 | uniform | 600 | 19.0 | 31.6 | 10.5 | 14.2 | 10.6 | 11.0 |
| dense-asn-shared | simd | 1 | uniform | 600 | 19.1 | 31.6 | 10.6 | 14.2 | 10.8 | 11.1 |
| dense-asn-shared | default | 128 | uniform | 600 | 19.1 | 31.3 | 10.5 | 14.2 | 10.6 | 11.1 |
| dense-asn-shared | simd | 128 | uniform | 600 | 21.2 | 33.0 | 11.3 | 14.7 | 11.3 | 11.8 |
| dense-asn-unique | simd | 128 | uniform | 600 | 21.9 | 34.4 | 12.4 | 15.8 | 12.0 | 12.4 |
| dense-asn-unique | default | 128 | uniform | 600 | 21.2 | 31.8 | 11.3 | 15.0 | 11.4 | 11.9 |
| dense-asn-unique | simd | 1 | uniform | 600 | 21.0 | 31.7 | 11.7 | 14.7 | 12.1 | 12.3 |
| dense-asn-unique | default | 1 | uniform | 600 | 23.9 | 32.5 | 11.8 | 16.2 | 11.8 | 12.7 |
| dense-asn-unique | simd | 0 | uniform | 108 | 20.7 | 31.6 | 22.8 | 23.0 | 31.6 | 32.6 |
| dense-asn-unique | default | 0 | uniform | 108 | 20.7 | 31.6 | 22.7 | 22.6 | 31.5 | 32.5 |
| sparse-asn-shared | default | 0 | uniform | 108 | 21.7 | 33.3 | 22.7 | 23.6 | 34.7 | 35.9 |
| sparse-asn-shared | simd | 0 | uniform | 108 | 20.5 | 33.2 | 21.9 | 21.6 | 33.5 | 34.0 |
| sparse-asn-shared | default | 1 | uniform | 600 | 20.1 | 33.1 | 14.0 | 23.9 | 14.1 | 14.8 |
| sparse-asn-shared | simd | 1 | uniform | 600 | 20.3 | 33.0 | 14.0 | 23.2 | 13.9 | 14.7 |
| sparse-asn-shared | default | 128 | uniform | 600 | 21.5 | 33.5 | 14.0 | 23.8 | 14.2 | 14.9 |
| sparse-asn-shared | simd | 128 | uniform | 600 | 20.6 | 35.0 | 15.1 | 24.3 | 14.9 | 16.1 |
| sparse-asn-unique | simd | 128 | uniform | 600 | 21.4 | 33.5 | 14.7 | 24.1 | 14.8 | 15.8 |
| sparse-asn-unique | default | 128 | uniform | 600 | 21.0 | 33.5 | 14.7 | 24.1 | 15.0 | 15.8 |
| sparse-asn-unique | simd | 1 | uniform | 600 | 24.2 | 34.4 | 15.3 | 24.9 | 15.6 | 16.2 |
| sparse-asn-unique | default | 1 | uniform | 600 | 21.3 | 33.6 | 17.5 | 24.5 | 17.6 | 16.6 |
| sparse-asn-unique | simd | 0 | uniform | 108 | 24.7 | 33.8 | 25.8 | 25.7 | 34.0 | 35.0 |
| sparse-asn-unique | default | 0 | uniform | 108 | 21.3 | 33.5 | 22.8 | 23.1 | 33.7 | 34.5 |
| sparse-v6-shared | default | 0 | uniform | 108 | 40.3 | 55.9 | 40.9 | 40.5 | 55.9 | 55.3 |
| sparse-v6-shared | simd | 0 | uniform | 108 | 40.5 | 55.9 | 41.8 | 41.3 | 56.2 | 57.7 |
| sparse-v6-shared | default | 1 | uniform | 600 | 40.3 | 55.6 | 40.0 | 39.7 | 40.1 | 43.1 |
| sparse-v6-shared | simd | 1 | uniform | 600 | 40.3 | 56.0 | 39.8 | 40.1 | 40.0 | 41.9 |
| sparse-v6-shared | default | 128 | uniform | 600 | 40.5 | 55.1 | 40.5 | 42.1 | 40.1 | 43.6 |
| sparse-v6-shared | simd | 128 | uniform | 600 | 40.6 | 55.5 | 40.7 | 41.2 | 41.5 | 44.2 |
| sparse-v6-unique | simd | 128 | uniform | 600 | 41.1 | 57.0 | 40.8 | 40.5 | 41.1 | 43.2 |
| sparse-v6-unique | default | 128 | uniform | 600 | 43.8 | 57.0 | 40.5 | 40.2 | 40.4 | 43.6 |
| sparse-v6-unique | simd | 1 | uniform | 600 | 41.0 | 56.7 | 40.8 | 40.6 | 40.9 | 43.3 |
| sparse-v6-unique | default | 1 | uniform | 600 | 40.8 | 56.1 | 40.1 | 40.1 | 40.0 | 41.5 |
| sparse-v6-unique | simd | 0 | uniform | 108 | 40.8 | 56.1 | 41.3 | 41.2 | 56.2 | 56.3 |
| sparse-v6-unique | default | 0 | uniform | 108 | 40.6 | 55.4 | 42.2 | 40.9 | 55.6 | 55.8 |
| deep-v6-shared | default | 0 | uniform | 108 | 1076.2 | 1079.4 | 1072.3 | 1066.2 | 1082.7 | 1081.1 |
| deep-v6-shared | simd | 0 | uniform | 108 | 1067.5 | 1081.6 | 1074.6 | 1064.7 | 1097.9 | 1073.8 |
| deep-v6-shared | default | 1 | uniform | 108 | 1055.7 | 1064.4 | 1058.0 | 1053.2 | 1065.4 | 1066.1 |
| deep-v6-shared | simd | 1 | uniform | 108 | 1060.1 | 1068.3 | 1064.8 | 1061.6 | 1067.9 | 1070.0 |
| deep-v6-shared | default | 128 | uniform | 108 | 1083.3 | 1082.0 | 1083.6 | 1088.0 | 1081.6 | 1098.8 |
| deep-v6-shared | simd | 128 | uniform | 108 | 1075.1 | 1095.8 | 1074.0 | 1083.4 | 1093.8 | 1076.7 |
| deep-v6-unique | simd | 128 | uniform | 108 | 1100.2 | 1091.5 | 1103.4 | 1097.6 | 1088.3 | 1103.3 |
| deep-v6-unique | default | 128 | uniform | 108 | 1111.7 | 1100.4 | 1107.9 | 1092.3 | 1088.3 | 1108.6 |
| deep-v6-unique | simd | 1 | uniform | 108 | 1107.4 | 1099.7 | 1102.0 | 1107.0 | 1096.7 | 1114.2 |
| deep-v6-unique | default | 1 | uniform | 108 | 1126.1 | 1104.8 | 1124.6 | 1113.7 | 1111.4 | 1135.3 |
| deep-v6-unique | simd | 0 | uniform | 108 | 1113.4 | 1133.4 | 1295.8 | 1296.6 | 1112.9 | 1116.8 |
| deep-v6-unique | default | 0 | uniform | 108 | 1087.7 | 1075.0 | 1090.6 | 1092.1 | 1075.3 | 1102.5 |
| duplicates-v4-shared | default | 0 | uniform | 0 | 874.6 | 878.6 | 871.3 | 872.3 | 870.0 | 877.9 |
| duplicates-v4-shared | simd | 0 | uniform | 0 | 864.4 | 862.1 | 867.8 | 870.2 | 879.6 | 900.0 |
| duplicates-v4-shared | default | 1 | uniform | 0 | 873.3 | 890.1 | 866.0 | 880.7 | 866.0 | 879.2 |
| duplicates-v4-shared | simd | 1 | uniform | 0 | 859.2 | 862.7 | 860.9 | 859.0 | 855.7 | 861.5 |
| duplicates-v4-shared | default | 128 | uniform | 0 | 873.1 | 913.1 | 865.2 | 859.1 | 863.4 | 868.7 |
| duplicates-v4-shared | simd | 128 | uniform | 0 | 854.7 | 871.5 | 854.6 | 854.4 | 852.6 | 859.6 |
| duplicates-v4-unique | simd | 128 | uniform | 0 | 856.8 | 895.4 | 857.5 | 860.2 | 859.6 | 870.6 |
| duplicates-v4-unique | default | 128 | uniform | 0 | 856.5 | 865.6 | 854.7 | 856.8 | 856.0 | 864.4 |
| duplicates-v4-unique | simd | 1 | uniform | 0 | 858.8 | 876.6 | 864.7 | 859.8 | 863.5 | 865.5 |
| duplicates-v4-unique | default | 1 | uniform | 0 | 881.3 | 874.3 | 875.2 | 870.6 | 872.3 | 884.1 |
| duplicates-v4-unique | simd | 0 | uniform | 0 | 872.3 | 864.6 | 866.5 | 873.6 | 875.9 | 872.8 |
| duplicates-v4-unique | default | 0 | uniform | 0 | 860.4 | 863.0 | 859.7 | 860.5 | 855.7 | 869.1 |

## Follow-through

Keep the explicit budget and cheap fallback contract for future experiments.
A production selector needs representative batch-size/distribution-shift
validation, admission/eviction economics, startup amortization, RSS/cold/>RAM
qualification and evidence on more hosts. Broader sampling or per-request
routing must earn its overhead; no timing-trained tuning is adopted here.
Next in priority order: P1-07 learned fences with exact bounded search and
fallback, measured under the same complete output contract.
