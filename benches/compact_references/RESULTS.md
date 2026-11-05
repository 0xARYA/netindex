# P1-10 results — 2026-11-05

**Adopted in production, preparing the breaking 3.0.0 release.** Both builders
choose indexed shared-payload references when strictly smaller than existing
representations. The reader supports formats 3 and 4. Explicit legacy-output
methods retain format-3 compatibility for 2.x consumers. Registry publication
has not occurred.

The implementation preserves every assertion, insertion ID, original target,
metadata byte and opaque payload byte. Directory entries are borrowed from the
file; validation needs no separate distinct-value directory. The in-memory
writer reuses its deduplication table allocation for span-to-ordinal lookup.
The external builder embeds value IDs in its existing slot spill, keeps the
directory on disk, and charges all temporary bytes against the configured limit.

## File size

| Input | Assertions | Shared values | Format 3 bytes | Format 4 bytes | Saving |
| --- | ---: | ---: | ---: | ---: | ---: |
| Retained real City | 20,255,722 | 271,275 | 263,944,912 | 164,836,510 | 99,108,402 B / 94.52 MiB / 37.55% |
| Independent IPtoASN | 719,149 | 86,902 | 30,741,317 | 27,840,796 | 2,900,521 B / 2.77 MiB / 9.44% |

Both use three-byte IDs. Reference storage falls from 162,045,776 to 62,937,374
bytes for City and 5,753,192 to 2,852,671 for IPtoASN. These totals include the
new eight-byte descriptor and complete span directory. Payload pools and
metadata/target/index bytes are identical.

For `N` assertions and `U` directory entries, the new cost is `8 + 8*U + w*N`.
Selection includes the descriptor, directory, and minimum whole-byte ID width.
With three-byte IDs, the exact break-even test against eight-byte spans is
`5*N > 8*U + 8`. Uniform lengths keep the smaller fixed-length encoding;
unshared values and pools beyond `u32::MAX` retain existing span encodings.
This is an implemented saving, not a minimum-size bound or an MMDB-size win.

## Opening and warm lookup

AMD Ryzen 9 9950X3D2, WSL Ubuntu 24.04, pinned CPU 20, Rust 1.95.0, all features,
one release codegen unit, no PGO/LTO/custom CPU flags. File cache warmed outside
opening; no system cache dropping. Three processes per cell, nine lookup samples
per process. Entries below are medians of process medians.

| Input / observation | 2.1.0 reader, format 3 | 3.0.0 reader, format 3 | 3.0.0 reader, format 4 |
| --- | ---: | ---: | ---: |
| City full validation, ms | 835.84 | 827.63 | 418.52 |
| City RSS immediately after open, KiB | 244,284 | 244,316 | 145,448 |
| City sampled target hits, ns/query | 124.90 | 125.70 | 131.39 |
| City uniform mixed IP, ns/query | 54.60 | 54.52 | 55.38 |
| IPtoASN full validation, ms | 33.74 | 36.52 | 16.01 |
| IPtoASN RSS immediately after open, KiB | 28,356 | 28,368 | 25,504 |
| IPtoASN sampled target hits, ns/query | 249.57 | 246.89 | 254.87 |
| IPtoASN uniform mixed IP, ns/query | 261.51 | 259.19 | 265.27 |

Compact opening is about 50% faster for City and 53% for IPtoASN against 2.1.0.
City open RSS falls by 96.52 MiB; IPtoASN by 2.79 MiB. These are process RSS
observations with mapped input, not isolated allocator measurements.

**Warm lookup is slower:** roughly 1.4–5.2% against the unchanged 2.1.0 reader
on these workloads. The extra dependent directory access has a measurable cost.
The new reader on format 3 is included to separate encoding and reader changes;
its IPtoASN opening also regressed by about 2.8 ms in this sample. Raw process
ranges and all timings are retained; these small samples do not establish
universal latency or tail behavior. Legacy output remains available when that
tradeoff matters more than file size and opening costs.

## Construction

Two independent processes per mode, identical insertion stream and metadata.
Build seconds exclude preparation, source decoding, fsync and verification.

| Input / writer | Legacy build seconds | Compact build seconds | Legacy temporary peak bytes | Compact temporary peak bytes |
| --- | --- | --- | ---: | ---: |
| City external | 14.931, 14.904 | 14.920, 14.997 | 1,961,276,003 | 1,965,616,403 |
| City in-memory | 2.734, 2.356 | 2.680, 2.537 | — | — |
| IPtoASN external | 0.330, 0.329 | 0.452, 0.457 | 73,204,454 | 74,594,886 |
| IPtoASN in-memory | 0.057, 0.062 | 0.074, 0.068 | — | — |

City construction is similar within these observed ranges; IPtoASN construction
is slower, particularly external building. Sorting runs remain 16,384 records,
the logical payload cache is 4 MiB, and the temporary limit is 8 GiB. Additional
directory storage is sixteen bytes per distinct value. Disk-table cells grow
from 24 to 32 bytes to retain value IDs; growth charges both live tables. Large
individual lengths use an eight-byte escape in the existing temporary slots.

The receipt also retains `/usr/bin/time` peak RSS. The replay harness holds all
input assertions and maps both files during verification, so those peaks are
**not isolated external-builder RAM**. The production external implementation
retains bounded buffers and the configured cache; it allocates no full value
directory in memory. Native provider conversion and application decoding were
not measured here.

## Validation and assembly

All sixteen final builds matched every original assertion and 100,005 sampled
all-match queries each. Both writers and both repeats produced byte-identical
compact files; legacy files matched the retained originals byte for byte. The
independent Python parser checked all 20,974,871 insertion IDs, complete pool
bytes, metadata/target/index bytes, directory order and reference membership.
Query counts and digests matched across both readers and both encodings.
The old reader rejects format 4 with `Version(4)`.

The independent fixed format-4 fixture covers empty/shared values, malformed
descriptors, conflicting flags, invalid ordinals, unused entries, noncontiguous
spans, truncation and suffixes. Writer tests cover width boundaries, exact
break-even ties, cache admission/disabled cache, disk-table growth/collisions,
wide temporary lengths, output errors and cleanup.

Disassembly of `Layout::span` confirms three-byte IDs use one byte load and one
two-byte load plus shift/or, with no overread or `memcpy` call. The indexed path
then loads its eight-byte directory span. Bounds/overflow checks and the width
dispatch remain; lookup introduces no heap allocation or payload decoding.
The full disassembly hash is in the receipt. Existing runtime SIMD target search
is preserved; this change does not add SIMD or PGO.

All seven documented repository checks passed, including both clippy modes,
default/all-feature tests, warning-free documentation and package verification.
The updated distribution analyzer's four tests and clippy passed. Index and SIMD
search benchmark correctness prechecks passed. Linux ASan smoke campaigns ran
20,097,303 reader cases and 163,692 final-writer cases, 61 seconds each, with no
failure. These are smoke campaigns, not exhaustive format validation.

See [reproduction and scope](README.md) and the
[raw source/input/binary hashes, samples and verification receipt](../results/2026-11-05-compact-references.json).
Next priority is P1-11 shared aliases, preserving logical assertions and all-match
semantics while reducing physical target expansion.
