# P2: shipped query capabilities and qualification

Measured 2026-11-05 on Ryzen 9 9950X3D2, 16 cores/32 logical CPUs. Baseline:
`8d4f5b1fb33994ced4ea39e7478fc4e3697c8081`. Production source hashes match between
the Linux and Windows candidates. The library remains unpublished 3.0.0; this
change adds APIs without changing any encoded bytes or release build defaults.

## Implementation decisions

| Task | Adopted behavior and cost |
| --- | --- |
| P2-01 | Borrowed `Query` batches on `Reader` and `ReaderSet`; input order, normal per-query result order, duplicate queries independent, indexed original errors and exact partial delivery. No result buffers or speculative later queries. |
| P2-02 | `contains_ip/asn` stop at the first match. Typed `ControlFlow` visitors distinguish a successful break from a reader/visitor error; batch breaks return the query index. Existing visitors remain compatible. |
| P2-03 | Inclusive IP/CIDR/range intersection; same address family, whole original targets, duplicates and `(start,end,ID)` order. Interval trees prune by subtree maximum/start; packed columns seek a predecessor block and scan only relevant entries. Aliases project clipped source queries and merge in native IPv6 order. Reader sets visit all sources because point-routing masks cannot safely prune ranges. |
| P2-04 | Allocation-free inspection of actual formats, physical/logical counts, section/reference/payload bytes, packed/bitpacked blocks, aliases and selected reader heap capacities. O(packed blocks); no payload/target scan. Reports capacities and released validation bitmap separately from backing ownership and RSS. |

The batch API deliberately uses the established point kernels. Earlier
[cross-query SIMD](../cross_query/RESULTS.md) and
[interleaving](../interleaved/RESULTS.md) results included hot/small regressions;
these results do not justify choosing one of those kernels globally. Batch
ordering, borrowing and failure semantics are useful independently of SIMD.
There is no new persistent per-reader allocation or reader field for P2-01..03.

## Method and provenance

The [reproduction runner](README.md) builds the same standalone application with
Rust 1.95.0, one codegen unit, portable target settings and runtime SIMD dispatch.
Each configuration has nine samples per timing cell: three serial processes,
three internal rounds, with reversed/rotated orders. Setup, validation and oracle
checks are excluded from query timing. Results consume IDs, original targets and
borrowed payloads. All 452 Windows timing cells passed input identity and 43
normalized equivalent-result checksum group checks. Full result-vector checks
precede timings; tests also check original-input assertions independently.

Eight synthetic shapes have 32,768 physical assertions; aliases project to
65,536 logical assertions. Retained IPtoASN has 719,149 IP assertions and
27,840,796 bytes, SHA-256
`7e5095968be43d8537ca76a9039fede4227c48fd7ec215e55d31f9f921cb9acf`.
Linux also measured canonical City: 5,724,698 assertions, 62,361,502 bytes,
SHA-256 `1fc75070b34d5c3e8050e19c189371fee77582ba3b3dbe73ea4d4da5bc2f8917`.
Provider files are not committed. These are warm repeated queries over at most
4,096 sampled assertion starts, with misses and locality. They do not establish
uniform full-keyspace, cold-file, network-service or competing-format results.

The final Windows PGO application trains with a different seed **and distribution**
from validation: 25% versus 10% hot selection, one in seven versus one in three
attempted misses. Coverage determines actual misses. Training excludes oracles,
full-scan comparison controls and manual membership controls. Separate fresh
profiles train normal and ThinLTO builds; generation/use share all other settings,
an explicit target and compiler-matched LLVM tools. No missing-function profile
warnings appeared. This follows the [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).
It qualifies this benchmark application, not a consumer's production traffic.

## API measurements

Windows medians, default release, ns/query unless specified. Full distributions
and every batch size (1/8/32/256) are in [summary.json](raw/windows/summary.json).

| Shape | Point caller loop | Batch 32 | Visit-all membership | Explicit membership | Narrow intersection | Full-scan/filter alternative |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Packed IPv4 | 38.92 | 35.96 | 36.51 | 36.80 | 117.38 | 849,194 |
| Bitpacked IPv6 | 46.83 | 41.51 | 44.77 | 45.45 | 80.91 | 1,197,913 |
| Overlapping ranges | 949.31 | 951.65 | 837.82 | 88.52 | 1,449.32 | 723,544 |
| Duplicate ASNs | 111.91 | 111.32 | 104.54 | 19.24 | — | — |
| Shared aliases | 46.09 | 41.07 | 42.11 | 41.72 | 81.01 | 1,700,738 |
| Retained IPtoASN | 292.42 | 288.73 | 287.02 | 297.19 | 266.41 | 18,701,806 |

Membership gains come from avoiding remaining matches: 9.46× on overlaps and
5.43× on duplicate ASNs, versus a caller that visits every match. An existing
caller can already stop using a private typed visitor error: 89.74/19.01 ns on
those shapes, essentially the same benefit. Single-match membership is not a
general speedup. Batch results are close to a point loop; no cross-query SIMD
speedup is claimed.

Indexed intersections provide a substantial new capability versus the existing
full-scan/filter workaround. They are not a comparison against another format
or an optimal bespoke scanner. Both paths consume identical complete matches;
indexed narrow queries repeat more often and have warmer local state. Whole-domain
queries still scale with returned assertions: default 0.637 ms for packed IPv4,
0.662 ms for bitpacked IPv6 and 13.706 ms for retained IPtoASN. The scan control
also recomputes generic bounds for every row, so whole-domain ratios include that
extra work.

Inspection medians are 600 ns for 128 packed IPv4 blocks, 669.53 ns for 128
bitpacked IPv6 blocks, and 23.44 ns for the ASN-only shape. On Linux, canonical
City inspection took 116.43 microseconds, versus 130.89 ns for a narrow intersection
and 180.21 ms for its full-scan workaround. Inspection is not constant-time when
the packed descriptor count grows. Its measured path also returns/copies the
inspection value; it is not solely the descriptor loop.

## P2-05/06: linked code and counters

Reviewed actual linked Windows default/ThinLTO+PGO kernels and named Linux
default kernels: point, batch, membership, intersection and inspection. Windows
RVAs plus PE unwind entries locate exact function bodies; ELF names locate Linux
bodies. [Archived assembly](raw/windows/asm-default-point.txt) shows bounds/error
branches, stack temporaries and ordinary scalar traversal; XMM moves also copy
values and do not prove cross-query SIMD. Existing dispatched search kernels
remain available. No new ISA requirement or unsafe library operation was added.
Subsequent import-order cleanup was rebuilt in all four configurations; linked
`.text`, `.data`, `.pdata` and `.reloc` sections match the measured executables
byte-for-byte. Read-only metadata/constants differ; measured executable hashes
and the exact compiled harness snapshot are retained.

The combined build increases the linked point body from 7,497 to 12,515 bytes and
batch body from 9,572 to 14,518 bytes. Point stack reservation increases from
0x438 to 0x4d8 bytes. Inspection's loop remains a call to the reader inspector,
with result copies; its wrapper is 260 bytes in both builds. These mixed-query
application bodies include multiple formats/error paths, not isolated dedicated
membership function sizes. Larger bodies and stack spills remain costs; the
measurements do not justify forced inlining everywhere or hand-written SIMD.

Linux `perf` counters ran on CPU 20 in two event groups, with enable/disable
acknowledgement around the kernel after setup and oracle work. All events report
100% scheduling. One counter sample per condition is supporting evidence, not
repeatable latency qualification. Linux uses the earlier 25%-hot, one-in-seven
attempted-miss corpus; its compiled harness snapshot is retained. Do not compare
these cycle counts directly with the revised Windows timing corpus.

| Default kernel | Cycles/query | Instructions/query | Branch-miss rate | Generic cache-miss rate |
| --- | ---: | ---: | ---: | ---: |
| Packed IPv4 point | 270.57 | 903.34 | 0.569% | 1.810% |
| Bitpacked IPv6 point | 361.81 | 1,089.26 | 0.565% | 3.470% |
| Overlaps point | 7,456.08 | 28,558.44 | 0.127% | 9.551% |
| Overlaps membership | 696.00 | 2,081.72 | 0.775% | 7.994% |
| Duplicate ASN point | 1,170.13 | 3,448.17 | 0.135% | 4.169% |
| Duplicate ASN membership | 132.26 | 273.08 | 0.284% | 0.310% |
| Canonical City point | 355.38 | 1,039.57 | 0.435% | 13.926% |
| Canonical City narrow intersection | 655.69 | 2,010.07 | 0.103% | 12.000% |

The unchanged baseline's point instruction counts differ by approximately
0.68% packed IPv4, 1.36% bitpacked IPv6, 0% overlap/ASN and 0.65% canonical City.
Point source methods were untouched. Timing is noisier: the single default ASN
counter sample is 232.44 ms versus baseline 171.11 ms despite nearly identical
instructions; the nine-sample default timing median is 168.15 ns/query.
No blanket claim of zero point regression follows. Generic cache counters do not
identify a cache level or establish memory bandwidth. Raw supported events,
checksums, identities and CPU information are in [raw/linux](raw/linux).

## P2-07/08: application build settings

Held-out Windows point medians in ns/query. The combined build improves all nine
declared point workloads, by 10.0–54.7%; this is a workload-specific result.

| Workload | Default | ThinLTO | PGO | ThinLTO+PGO |
| --- | ---: | ---: | ---: | ---: |
| Packed IPv4 | 38.92 | 38.57 | 29.57 | 28.85 |
| Bitpacked IPv6 | 46.83 | 46.87 | 48.05 | 42.17 |
| Overlapping ranges | 949.31 | 898.71 | 918.52 | 684.41 |
| Duplicate ASNs | 111.91 | 82.01 | 92.60 | 56.60 |
| Shared aliases | 46.09 | 44.58 | 35.83 | 32.37 |
| Retained IPtoASN | 292.42 | 281.22 | 266.47 | 132.59 |

| Build | Executable bytes | Main-to-ready median ms | Parent process wall median ms |
| --- | ---: | ---: | ---: |
| Baseline | 473,088 | 4.376 | 15.085 |
| Default | 507,904 | 4.325 | 15.263 |
| ThinLTO | 541,696 | 4.232 | 14.934 |
| PGO | 502,272 | 3.724 | 14.600 |
| ThinLTO+PGO | 557,056 | 3.298 | 14.278 |

Ten startup trials/build, reversed order, warm filesystem, one packed IPv4
fixture. Main-to-ready includes fixture building, hashing, validation and query
preparation; parent wall includes process creation/loader, output and exit.
Neither is a cold production startup measurement. PE executable sizes exclude
debug/profile artifacts. New capabilities linked by this harness add 7.36% versus
its baseline executable; combined optimization adds 9.68% versus default.

Adopt the reproducible **application** PGO/ThinLTO workflow as an option, with
held-out validation and size/startup accounting. Keep library release defaults.
PGO alone regresses bitpacked IPv6 point latency by 2.6%; full-scan overlap controls
also regress (723.54 to 827.80 microseconds/query). Combined optimization does not
improve every method: bitpacked IPv6 inspection is only modestly better and some
full-scan controls regress. Re-train on a consumer's real traffic and deployment
target before selecting flags or shipping a profile. Actual production traces,+cold mappings, other CPUs and cross-format comparisons were not qualified here.
Earlier Linux PGO profiles included comparison controls and are superseded by
the revised Windows protocol; they are not used to recommend build settings.

## P2-09/10: correctness and completion

Eight focused integration tests cover original IDs/targets/payloads, overlaps,
duplicate and absent keys, tails, address extremes, invalid queries, callback
errors, exact early-stop prefixes, borrowed lifetimes/pointers, default/legacy/
compact/bitpacked files, independent format-6 fixtures, alias prefixes and
interleaved IDs, native/alias ordering, broad routing, reader-set routing/order,
inspection accounting and truncated backing. Both fuzz targets now check batch,
membership, intersection, early stopping and inspection against independent
expectations; the encoder uses original input assertions for both output modes.

All seven documented local checks passed: formatting, both Clippy configurations,
default/all-feature tests (including ignored tests), warning-free documentation,
and package verification. AArch64 Windows all-feature library cross-check passed;
this is compilation, not execution or a new NEON performance claim. Linux scalar
application correctness passed all eight shapes. Linux AddressSanitizer passed
29 unit + 4 bitpacking + 8 query tests. Separate 61-second fuzz campaigns completed
10,276,652 read and 9,700 encode runs without findings. These are smoke campaigns,
not exhaustive malformed-input coverage. Existing native AVX-512/NEON performance
evidence remains in [P1-08](../architectures/RESULTS.md).

WSL stopped responding after the completed Linux qualification. Text evidence
and exact binaries were recovered read-only from its stopped disk; the revised
PGO matrix therefore ran on Windows. No Linux rerun of that revised profile is
claimed. All ten P2 tasks are complete for the implemented library and declared
benchmark application. Costs, negative results and consumer-owned production
qualification remain explicit above.

## Search-only membership

Adopted direct membership for readers and reader sets. It no longer constructs
matches or resolves references or payloads. Packed, interval, bitpacked and
aliased IP searches use the existing index and still report malformed index
data. There are no file changes or new memory allocations.

Median ns/query from nine warm samples per cell (three fresh processes per
build, alternating order; 1,048,576 queries per sample; Ryzen 9 9950X3D2,
Windows, CPU 20). Point lookup is the unchanged-work control:

| Input | Membership before / after | Point before / after |
| --- | ---: | ---: |
| Packed IPv4 | 35.10 / 26.84 | 37.30 / 37.84 |
| Packed IPv6 | 36.20 / 28.89 | 38.77 / 39.74 |
| Bitpacked IPv6 | 43.15 / 35.93 | 45.46 / 46.24 |
| Overlapping IPv4 | 85.52 / 58.89 | 928.08 / 928.47 |
| Exact ASN (duplicate keys) | 18.88 / 19.49 | 107.23 / 107.60 |
| Mixed | 30.64 / 24.81 | 34.21 / 34.67 |
| Shared aliases | 38.19 / 30.05 | 43.97 / 42.41 |
| Retained City | 53.42 / 40.36 | 58.93 / 57.05 |
| Retained IP-to-ASN | 271.09 / 185.51 | 275.40 / 272.44 |

All input digests and result checksums matched. IP membership took
17-32% less time; exact ASN was effectively flat on this small-key workload.
Point controls moved by at most 3.6%. A dense bitmap is not adopted: the ASN
workload does not establish value for a real consumer or a default budget.
[Raw samples and provenance](../results/2026-11-06-p3-membership.json).

## Standards cleanup regression check

Compared `8be3593` and `30a9d37` with the same
[query harness](README.md) on Windows,
Rust 1.95.0 release builds, pinned to logical CPU 20 on a Ryzen 9 9950X3D2.
Artifact hashes and query checksums matched across all nine workloads.
The initial retained City run measured 59.27 to 57.73 ns for point lookup
and 44.13 to 41.23 ns for membership; this was a local benchmark.

The focused rerun used five pairs of alternating processes, each with three
samples of 1,048,576 queries per method. Median nanoseconds per query:

| Workload | Point, before to after | Membership, before to after |
| --- | ---: | ---: |
| Packed IPv6 | 39.35 to 42.11 (+7.0%) | 28.96 to 29.28 (+1.1%) |
| Packed IPv4 | 38.21 to 38.77 (+1.4%) | 27.03 to 27.00 (-0.1%) |
| ASN | 106.71 to 107.12 (+0.4%) | 19.21 to 19.21 |

The synthetic packed IPv6 point regression remains open. The benchmark's
point loop had the same instructions apart from address displacements;
that comparison does not cover every callee or establish a cause. An
isolated build with the original packed source still measured 42.64 ns
(nine samples). Profile the full packed IPv6 lookup path on this host and
check a second native build before changing search code or alignment.
