# P1-12: bounded disk-table cache

The external builder can cache pages of its private payload table. The
cache is opt-in so existing builds retain their memory budgets. The
payload-key cache, the table-page cache and the temporary-disk limit
are separate. The output format and exact payload equality checks are
unchanged.

The cache uses direct mapping and write-back eviction. A page holds 16
table cells (384 bytes for legacy cells or 512 bytes for compact cells).
Each in-memory slot includes a page number, dirty flag, and space for the
larger cell layout. On the measured 64-bit build, each slot charges 528
bytes. Capacity rounds down to a power of two slots. Allocation is deferred
until the first disk table is needed. Growth flushes the old table's dirty
pages before sequential rehashing, then reuses the cache for the new table.
Disk accounting still charges both tables while they coexist. Finalization
drops the temporary table and its cache after the last payload insertion.

## Build measurements

Each cell is the median of three fresh processes, followed by the observed
range in parentheses. Budgets ran in ascending, descending, then ascending
order. The page charge includes tags and any padding.

| Table-cache budget | Charged RAM | Materialized City (s) | Canonical City base (s) | IPtoASN (ms) |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 0 | 17.86 (15.88-19.82) | 3.48 (3.46-3.86) | 362.86 (348.22-439.66) |
| 1 MiB | 0.516 MiB | 14.88 (14.33-19.94) | - | 336.20 (331.53-351.50) |
| 8 MiB | 4.125 MiB | 15.18 (12.84-15.34) | 3.09 (3.04-3.13) | 311.63 (304.83-344.15) |
| 32 MiB | 16.50 MiB | 14.73 (12.98-15.18) | 2.96 (2.95-3.14) | 314.67 (306.36-383.57) |

The 8 MiB budget cut median build time by 15.0% for materialized City,
11.3% for the canonical City base, and 14.1% for IPtoASN. The 1 MiB
materialized City median was also lower, but its range spans the
uncached median. Increasing the budget to 32 MiB quadrupled the
page charge and did not yield a clear time advantage. Keep the cache
optional; 8 MiB is a useful starting budget for inputs like these.

Materialized City and IPtoASN retained exactly 164,836,510 and 27,840,796
output bytes, respectively. Their temporary peaks stayed at 1,965,616,403
and 74,594,886 bytes. The canonical base retained 62,361,502 output bytes
and a 570,638,099-byte temporary peak. The prior executable's baseline checks were 17.25 s
and 378.22 ms. The three-round grid uses one candidate binary for all
budgets. The canonical grid and counter replays use the final build,
which drops the provisional allocation if the allocator reports excess
capacity. See the [raw receipt](../results/2026-11-05-disk-table-cache.json)
for input and executable hashes, per-run phase times, peak RSS, and counters.

## Positioned I/O and assembly

These are whole-process syscall counts from separate `perf stat` runs of
the final binary. They include all positioned I/O, including payload-byte
comparisons. They do not count physical storage operations.

| Input | Budget | Positioned reads | Positioned writes |
| --- | ---: | ---: | ---: |
| Materialized City | 0 | 18,271,867 | 500,002 |
| Materialized City | 8 MiB | 8,120,985 | 260,300 |
| Materialized City | 32 MiB | 6,911,324 | 32,750 |
| IPtoASN | 0 | 417,703 | 114,313 |
| IPtoASN | 8 MiB | 79,378 | 8,187 |
| IPtoASN | 32 MiB | 79,382 | 8,188 |

At 8 MiB this is 56% fewer positioned reads and 48% fewer writes on
materialized City, and 81% fewer reads and 93% fewer writes on IPtoASN.
The same City counter replay took 15.66 s without the cache and 15.03 s
with it, illustrating that fewer calls do not translate directly into the
same percentage saved in wall time. The prior and final uncached binaries
issued almost the same counts. Release x86-64 disassembly confirms that
the cache slot probe uses a shift, mask, stride and tag comparison with no
division or positioned I/O on a tag hit.

## Reproduction

Build the replay example with Rust 1.95.0. For the materialized inputs,
use the unchanged [P1-10 output files](../compact_references/RESULTS.md).
For the canonical City input, use the embedded format-4 file from
[P1-11](../shared_aliases/README.md): the validated four-window envelope
is 112 bytes, and the rest is the standalone format-4 file.

```sh
cargo +1.95.0 build --locked --release --all-features --example rebuild
dd if=city-shared.nidx of=city-canonical.nidx bs=112 skip=1
cat INPUT.nidx >/dev/null
/usr/bin/time -v taskset -c 20 target/release/examples/rebuild \
  INPUT.nidx OUTPUT.nidx TEMP_DIR external 8388608
cmp INPUT.nidx OUTPUT.nidx
```

Use a fresh output path for each run. The final argument sets the table
cache budget in bytes; use zero for the control. To reproduce the counters
separately, prepend
`perf stat -e task-clock,cycles,instructions,syscalls:sys_enter_pread64,syscalls:sys_enter_pwrite64 --`
to the pinned replay command.

The replay verifies metadata, every original ID, target and payload,
and 100,005 all-match queries. The separate byte comparison requires
identical artifacts. Timings cover external construction after input
preparation, through writing the output; they exclude verification
and fsync. The replay holds a sorted input list in memory; its peak RSS
is not the external builder's working set.

Runs used WSL2 Ubuntu 24.04 with ext4, pinned to logical CPU 20 on a Ryzen 9
9950X3D2. Input pages were warmed before each process. Settings were
16,384 rows per sort run, a 4 MiB payload-key budget and an 8 GiB temporary
limit. Randomized hash placement varies between processes. Results cover
this filesystem and its guest cache behavior; the host and storage caches
were not reset. They do not establish results under OS cache pressure, on
native Windows, or on other devices.

## Verification

All 43 replays matched their input bytes, including every assertion and
100,005 queries per replay. The Windows checks cover both cell widths,
forced collisions, dirty eviction and write errors, table growth, and the
public budget and configuration rules. Formatting, Clippy, default and
all-feature tests, docs and packaging passed.
