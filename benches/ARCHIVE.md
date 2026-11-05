# Retired search experiments

P1-01 through P1-08 completed qualification without becoming production defaults.
Their standalone harnesses, generated-core patches, runners and one-off NEON
workflow are retired from the current tree. Reports and immutable measurement
JSON remain in place; existing result links continue to work. Small ignore files
also remain so retained local compiler caches do not become untracked source.

| Study | Recorded disposition |
| --- | --- |
| [Interleaving](interleaved/RESULTS.md) | Workload-specific candidate; no default adoption |
| [Cross-query SIMD](cross_query/RESULTS.md) | Workload-specific candidate; no default adoption |
| [Eytzinger layouts](eytzinger/RESULTS.md) | Measured layouts; no default adoption |
| [Flat B+ search](flat_btree/RESULTS.md) | Measured layouts; no default adoption |
| [Traversal/prefetch](traversal/RESULTS.md) | Measured candidates; no default adoption |
| [Adaptive routing](adaptive/RESULTS.md) | Tested selector rejected; budget accounting retained in evidence |
| [Learned fences](learned/RESULTS.md) | Promising on some distributions; no default adoption |
| [AVX-512/NEON](architectures/RESULTS.md) | Mixed AVX-512; promising packed NEON on tested hot IP cases |

Archiving is not a claim that every candidate failed. Promising implementations
remain recoverable for a focused production change. Current work prioritizes
implementing compact references using the [payload distribution evidence](payload_distribution/RESULTS.md).

## Reproduce or recover a prototype

The complete pre-retirement source is frozen at
[`9010a9eb6bcd8e96c0a60d98b12a44db974ddf79`](https://github.com/0xARYA/netindex/tree/9010a9eb6bcd8e96c0a60d98b12a44db974ddf79).
Create a separate checkout from the repository root, choosing a fresh destination:

```sh
git worktree add --detach ../netindex-search-archive 9010a9eb6bcd8e96c0a60d98b12a44db974ddf79
```

Run the original study README there. The reports' measurement revisions and
source/input hashes remain authoritative for exact historical reproduction.
The old code and the original manual architecture workflow are also readable
directly through the snapshot link. To recover a promising kernel, extract only
the relevant implementation into a production change and qualify it against
current controls, rather than restoring the whole experimental dependency chain.

Ignored inputs/build outputs are not copied into a new checkout by Git. Point
the archived runner at the retained private data where its CLI permits; otherwise
provide the required private fixtures in that checkout as described by its
original README. Existing local input copies, generated source snapshots, raw
logs, binaries, profiles and disassembly remain untouched. No disk-space
reclamation from those artifacts is claimed.

[Retirement manifest](retired-search-studies.json) lists every removed path and
its immutable Git blob ID. It can be checked against the snapshot with `git
cat-file` without relying on an external archive or a temporary Actions artifact.

Production Criterion benchmarks, competitor/concurrency/memory/native-conversion
qualification, and the payload profiler remain active. Production code, public
APIs and encoded bytes are unchanged by this cleanup.
