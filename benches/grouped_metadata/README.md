# P1-13: grouped payload-table metadata

**Decision: reject both tested candidates for production.** Retain P1-12's
page cache. At the same 8 MiB budget, the revised metadata filter increased
median build time by 1.7-4.1% and consumed an additional 128-512 KiB. Separate
profiles showed less than 0.6% fewer positioned reads and no consistent
instruction reduction. The archive holds the candidate source; the maintained
library has no grouped-fingerprint implementation.

The study qualifies resident one-byte fingerprints over the private disk-backed
payload table. The control array uses space within the existing configured
table-cache budget, in addition to the P1-12 pages and tags. The array is
discarded before allocating a replacement at growth. If the replacement
does not fit alongside the pages, ordinary cell probing resumes.

Fingerprint groups contain 16 entries. The high seven hash bits identify
candidate cells and a separate high bit distinguishes occupied entries from
empty ones. A fingerprint hit still checks the stored full hash, payload
length, and complete payload bytes. Linear probe positions and first-empty
termination remain unchanged. The control array is not serialized, so there
is no artifact version or temporary-file layout change. The candidates use
ordinary probes when the table-cache budget is zero.

This adapts the resident metadata filter described in the
[Abseil Swiss Tables design note](https://abseil.io/about/design/swisstables)
to the existing disk table. It retains the existing load factor, payload
comparison, and disk layout rather than replacing the table design.
The optional `simd` feature compares 16 control bytes using baseline SSE2
on x86-64; other builds use a portable lane scan.

## Results

Three fresh processes per input and variant, with paired old/new order
alternating across rounds. The configured budget is 8 MiB in every row.

| Input | P1-12 median (range), ms | Revised median (range), ms | Change | Charged RAM, P1-12 to revised |
| --- | ---: | ---: | ---: | ---: |
| Materialized City | 14,196 (13,727-14,403) | 14,778 (14,685-14,837) | +4.1% | 4.125 to 4.625 MiB |
| Canonical City base | 3,132 (3,068-3,185) | 3,185 (3,167-3,272) | +1.7% | 4.125 to 4.625 MiB |
| IPtoASN | 282.74 (276.88-294.95) | 293.72 (288.28-325.85) | +3.9% | 4.125 to 4.250 MiB |

The initial candidate always scanned a group. Its cached medians were
14,934.92 ms for materialized City, 3,317.21 ms for canonical City and
300.54 ms for IPtoASN, versus paired controls of 14,851.39, 3,311.70 and
300.56 ms. It did not establish a time benefit. The revised candidate
checks the home fingerprint first and calls the group scanner only for
an unrelated occupied slot; grouped lookup stays outside the ordinary
inlined probe path. This revision also failed to establish a benefit.

The additional metadata stayed within the configured budget, and the
recorded temporary-disk peaks and output bytes were unchanged. Resource
correctness does not justify adopting a slower implementation.

## Profiling

Whole-process counts from separate final-build perf replays:

| Input | Positioned reads, P1-12 | Positioned reads, revised | Reduction | Instructions, P1-12 to revised |
| --- | ---: | ---: | ---: | ---: |
| Materialized City | 8,124,584 | 8,101,497 | 0.28% | 218.912 to 219.406 billion |
| Canonical City base | 2,121,823 | 2,111,077 | 0.51% | 53.309 to 53.341 billion |
| IPtoASN | 79,381 | 79,373 | 0.01% | 6.231 to 6.220 billion |

These counts include payload comparisons and the replay's verification.
They are syscall counts, not physical storage operations. Adjacent probes
already share cached pages. The resident filter saves cell inspection,
but avoids very little positioned I/O on these inputs. This is an inference
from the counts and the cache layout, not a measured universal property
of grouped metadata.

Release x86-64 disassembly is retained locally with the binary hashes.
The SIMD candidate uses packed byte equality and byte masks for the 16-byte
group. SIMD-off replays check the portable lane scan against identical input
and output bytes. This does not qualify NEON hardware or higher table load
factors.

[Raw reports, input and executable hashes](../results/2026-11-05-grouped-metadata.json)
include the initial and revised grids, separate perf runs, no-cache controls,
and portable-branch replays.

## Reproduction and scope

Use the retained materialized City and IPtoASN inputs from
[P1-10](../compact_references/RESULTS.md) and the canonical embedded City
base from [P1-12](../disk_table_cache/README.md). The prior executable is built
from `c7b8530` with the same settings. Candidate sources are archived at
[initial e7c448c](https://github.com/0xARYA/netindex/tree/e7c448ca797b7052f9b12ac21d926c84fda4667f)
and [revised b335c0d](https://github.com/0xARYA/netindex/tree/b335c0d6312e078161990bb6b78756970521fb02)
on `codex/p1-13-grouped-metadata-study`. Use a separate checkout to build them;
the current production tree intentionally retains P1-12:

```sh
cargo +1.95.0 build --locked --release --all-features --example rebuild
cat INPUT.nidx >/dev/null
/usr/bin/time -v taskset -c 20 target/release/examples/rebuild \
  INPUT.nidx OUTPUT.nidx TEMP_DIR external 8388608
cmp INPUT.nidx OUTPUT.nidx
```

Use fresh output paths. Each replay checks all IDs, targets, metadata and
payloads, and 100,005 all-match queries. Compare prior and current builds
at the same configured budget, including zero as a control. Construction
time excludes preparing the input assertions, verification, and fsync.
The benchmark process holds an in-memory replay list; its peak RSS is not
the external builder's working set.

Warm-input, fresh-process runs were pinned to logical CPU 20 on a Ryzen 9
9950X3D2 in WSL2 Ubuntu 24.04, ext4. Settings were 16,384 sort-run records,
a 4 MiB payload-key budget and an 8 GiB temporary limit. Hash placement
varies between processes. Guest, host and storage caches were not reset.
These results do not qualify native Windows performance, cold I/O, or
inputs larger than the available OS cache.


## Verification and disposition

All 70 replays matched their input bytes and verified every assertion plus
100,005 queries. The revised candidate passed default and all-feature tests,
formatting, both Clippy checks, docs and packaging on Windows. Linux
AddressSanitizer runs passed all 17 external-library tests with the SIMD
feature enabled and with the portable branch selected.

The tests cover all byte values and tags in every SIMD lane, first-empty
termination, occupied slots before a home position, full-hash and fingerprint
collisions, wraparound, table growth, dirty-page eviction and I/O failures,
and the budget threshold at which fingerprints are dropped.

The maintained library, examples, tests and package metadata are identical
to P1-12. Both candidate implementations and their tests remain recoverable
from the archive links above. P1-14 will investigate sequential spill/merge,
which addresses a different source of random temporary-file I/O.
