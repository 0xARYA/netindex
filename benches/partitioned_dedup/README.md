# P1-14: partitioned payload deduplication

**Adopt an explicit opt-in constructor, `ExternalBuilder::new_partitioned`.**
Keep ordinary constructors and their defaults. Partitioning reduces the need
for random temporary-table I/O when a separate table cache is unavailable. It
adds sequential passes, CPU work and temporary disk, and is not a general
replacement for the cached builder.

The constructor takes a power-of-two partition count from 1 through 64. It
stages complete payloads by hash, deduplicates one partition at a time, then
merges values and references back into first-seen order. Original IDs, targets,
metadata, payload bytes, all matches and complete artifact bytes are preserved.
The existing formats require first-seen payload order; simply concatenating
partitions would violate validation rules.

The configured payload-key budget is reused across partitions. Exact disk-backed
hash/length/full-byte comparisons handle keys that do not fit, including values
larger than the fixed comparison buffer. After releasing keys, the same budget
can hold a partition's resolved-span vector; otherwise references use positioned
reads. Fixed buffers and open-file counts grow with the partition count outside
the key budget. These are logical resource limits, not an RSS cap. Partitioned
builds reject nonzero table-cache budgets and may report cumulative artifact-byte
limits at finalization, before writing output. Failed pushes poison the builder.

## Same-key-budget controls

Both paths configure a 4 MiB payload-key cap and no additional table-cache pages.
Three fresh processes per input/variant, alternating order in the second round.
Times cover construction, excluding preparation, verification and fsync.

| Platform/input | Ordinary median (range), ms | 16 partitions median (range), ms |
| --- | ---: | ---: |
| WSL2/ext4 IPtoASN | 591.68 (368.06-629.52) | 420.45 (349.82-558.22) |
| WSL2/ext4 canonical City base | 5,606.74 (4,511.19-6,102.12) | 5,338.40 (5,178.99-5,896.00) |
| WSL2/ext4 materialized City | 23,271.76 (20,957.63-23,291.55) | 21,783.03 (21,425.16-24,865.03) |
| Native Windows/NTFS IPtoASN | 2,399.49 (2,037.32-2,654.87) | 1,078.17 (872.23-2,103.45) |

The native Windows comparison reduced the median by 55%; two of three paired
rounds favored partitioning. WSL City and canonical ranges overlap substantially.
Host contention produced wide ranges, so these observations justify an optional
resource strategy, not a universal speed claim. Linux runs and native Windows
runs in this follow-up were serialized, with no overlapping timed builds.

## Cached controls and costs

The initial grid compares the prior executable, the candidate's ordinary path,
and 16/64 partitions. The ordinary path requests an 8 MiB table cache, allocating
4.125 MiB for pages and tags in addition to its 4 MiB payload-key cap. Partitioned
variants allocate no table-cache pages and reuse their 4 MiB key cap.

| WSL2/ext4 input | Prior cached median (range), ms | Candidate ordinary median (range), ms | 16 partitions median (range), ms | 64 partitions median (range), ms |
| --- | ---: | ---: | ---: | ---: |
| IPtoASN | 460.68 (314.87-531.77) | 449.25 (310.81-481.37) | 441.79 (357.09-583.30) | 415.21 (370.40-656.62) |
| Canonical City base | 4,646.18 (3,473.82-7,610.39) | 5,572.61 (4,427.24-7,316.82) | 5,063.98 (3,596.28-5,234.56) | 4,864.16 (3,556.69-9,636.77) |
| Materialized City | 15,379.15 (13,596.07-39,948.58) | 18,917.16 (15,727.90-80,427.99) | 25,218.44 (16,073.77-49,257.49) | 26,219.99 (17,442.92-28,166.43) |

These noisy timings do not establish a consistent advantage over the cached
builder. The initial native Windows IPtoASN grid also favored its cached
control: medians 2,171.97 ms versus 3,690.97/8,280.42 ms for 16/64 partitions,
with respective ranges 858.02-3,160.97, 1,111.99-6,244.36 and
1,214.15-8,610.78 ms. Do not compare native medians across the two grids as a
controlled cache-budget experiment; machine load changed between grids.

Maximum observed temporary peaks across the cached-grid rounds:

| Input | Cached ordinary temporary peak, bytes | 16 partitions temporary peak, bytes | Increase |
| --- | ---: | ---: | ---: |
| IPtoASN | 74,594,886 | 74,834,742 | 0.3% |
| Canonical City base | 570,638,099 | 682,548,271 | 19.6% |
| Materialized City | 1,965,616,403 | 2,671,943,208 | 35.9% |

Materialized City's temporary peak grows from 1.83 to 2.49 GiB. Partitioning
spills every occurrence before deduplicating, so repeated payload bytes still
consume temporary disk. Charged retained keys fall from approximately 4 MiB
to 2.05 MiB for City or 0.59 MiB for IPtoASN at 16 partitions. These are key
charges only; fixed buffers and later span vectors also consume memory. The
replay's process RSS includes its fully prepared input and is not builder RSS.

## Profiling

Separate perf runs count the whole process, including preparation and full
verification. Positioned reads/writes are syscall counts, not physical storage
operations. The prior cached executable is the control in this table.

| Input | Positioned reads, control to 16 partitions | Positioned writes, control to 16 partitions | Instructions, control to 16 partitions |
| --- | ---: | ---: | ---: |
| IPtoASN | 79,380 to 4 | 8,185 to 0 | 6.238 to 6.940 billion |
| Canonical City base | 2,123,114 to 4 | 259,929 to 0 | 53.411 to 55.023 billion |
| Materialized City | 8,121,114 to 4 | 260,364 to 0 | 219.470 to 225.065 billion |

Sequential read/write calls increase: City reads rise from 1,395,345 to
1,671,025 and writes from 1,216,457 to 1,494,565. Extra staging and first-seen
restoration add work even when partition keys fit. The candidate's ordinary
path is within 0.1% of the prior executable's whole-process instruction count
on all three inputs. No SIMD kernel or PGO change is adopted here.

The binary hashes and release x86-64 disassembly are retained locally. These
profiles do not establish behavior on cold disks, network storage, or a
distribution whose partitions exceed their key budgets. Collision and skewed
fallback correctness are tested; adversarial throughput is not qualified.

## Reproduction and qualification

Use the retained materialized City and IPtoASN inputs from
[P1-10](../compact_references/RESULTS.md) and the canonical City base from
[P1-12](../disk_table_cache/README.md). Input hashes and every replay are in the
[raw receipt](../results/2026-11-05-partitioned-dedup.json). Hardware is the
Ryzen 9 9950X3D2, logical CPU 20, Rust 1.95.0, release/all features. The replay
uses 16,384-row sort runs, a 4 MiB payload-key cap and an 8 GiB temporary limit.

The prior source is `0810171`. The exact Linux measured prototype is
[07e35f5](https://github.com/0xARYA/netindex/tree/07e35f5b54c5b0854c97b9b208a29f5292d0472e);
[8da791b](https://github.com/0xARYA/netindex/tree/8da791befb713c869241db12e29c62502ca4678b)
adds comparison-span bounds and collision tests, and is the qualified production
implementation before documentation-only clarifications. Both remain on
`codex/p1-14-partitioned-dedup-study` for exact reproduction.

```sh
cargo +1.95.0 build --locked --release --all-features --example rebuild
cat INPUT.nidx >/dev/null
/usr/bin/time -v taskset -c 20 target/release/examples/rebuild \
  INPUT.nidx OUTPUT.nidx TEMP_DIR external 0
cmp INPUT.nidx OUTPUT.nidx
# Use a fresh output path for each process:
/usr/bin/time -v taskset -c 20 target/release/examples/rebuild \
  INPUT.nidx PARTITIONED.nidx TEMP_DIR partitioned 16
cmp INPUT.nidx PARTITIONED.nidx
```

Use `external 8388608` for the cached control, or `partitioned 64` for the
larger partition count. Native Windows uses the same arguments with
`target\release\examples\rebuild.exe`, NTFS scratch/output directories, and
process affinity set to logical CPU 20. Preserve warm-input preparation and
fresh output paths. The receipt includes executable/input hashes and native
process CPU/working-set measurements.

All 81 replays checked every assertion plus 100,005 queries and produced
byte-identical output. Focused tests cover empty input, invalid partition counts,
mixed targets/overlaps, repeated/empty/large payloads, disabled and saturated
caches, forced full-hash collisions and last-byte differences, failed staging,
poisoning, delayed limits, cleanup and underlying I/O causes. Linux
AddressSanitizer passed 16 external unit tests and 3 partitioned integration
tests. The documented repository checks include default/all-feature tests,
warnings-denied Clippy/rustdoc, formatting and local package verification.

Next priority: **P1-15 bitpacking**. Keep partitioning opt-in; measure the
application's CPU, memory and temporary-disk budget before selecting it.
