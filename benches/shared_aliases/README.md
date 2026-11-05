# P1-11: shared aliases

The production reader supports opt-in [format 5](../../FORMAT.md#shared-ipv4-aliases-format-5).
On the same retained City assertions used for [P1-10](../compact_references/RESULTS.md),
sharing saved **102,474,896 bytes (97.73 MiB, 62.17%)**. Four descriptors
replace 14,531,024 physical rows. The output contains 5,724,698 physical
and 20,255,722 logical assertions. The envelope costs 112 bytes.

| Input and reader | Artifact bytes | Mmap open (ms) | RSS at open (KiB) | Sampled hits (ns/query) | Uniform mixed IP (ns/query) |
| --- | ---: | ---: | ---: | ---: | ---: |
| City format 4, prior reader | 164,836,510 | 418.76 | 145,452 | 128.08 | 54.04 |
| City format 4, current reader | 164,836,510 | 422.45 | 145,472 | 130.81 | 55.18 |
| City format 5, current reader | 62,361,614 | 160.50 | 45,448 | 127.11 | 56.34 |
| IPtoASN format 4, prior reader | 27,840,796 | 15.80 | 25,496 | 251.04 | 258.66 |
| IPtoASN format 4, current reader | 27,840,796 | 15.75 | 25,512 | 265.06 | 259.95 |

For City, shared aliases cut open time by 62% and open RSS by 69%. Warm
sampled hits were roughly unchanged; uniform mixed lookup was 4.3%
slower than the prior reader and 2.1% slower than the same reader on the
materialized file. Plain format-4 City was about 2% slower on both warm
patterns. Plain IPtoASN sampled hits were 5.6% slower in the median, with
substantial spread across current-reader runs (259 to 316 ns/query).
These short runs do not isolate the effect of dispatch from code generation
and measurement variability. Format 5 is useful when size and open cost matter.

Three fresh processes per variant, alternating order, pinned to logical
CPU 20 on a Ryzen 9 9950X3D2 in WSL2 Ubuntu 24.04. Rust 1.95.0,
`--release --all-features`, without PGO. File pages were warmed before
each open. Lookup results are medians of three process medians, each over
nine samples of 131,072 queries. RSS includes resident mmap pages and is
measured at the end of open, before query setup. City processes returned
131,072 and 56,704 matches per sample on the two patterns; IPtoASN returned
131,072 and 123,000. Counts and aggregate digests agreed across variants.
This measures opaque lookups on one host, without native provider decoding
or a cold I/O comparison. See the [raw samples and SHA-256 hashes](../results/2026-11-05-shared-aliases.json).

## Replay and migration

The replay checks every materialized ID, target and payload before replacing
the aliases, and verifies all output IDs, targets and payloads, metadata and
150,000 lookup results. Both replays produced identical bytes. The example's
build phase (after original open, before final verification, without fsync)
took 1.71 to 1.96 seconds. Peak RSS for the full replay was about 848 MiB.
This is an in-memory conversion example; the production wrapper can write
over an eligible mapped base made by either builder without rebuilding it.

With the retained City input and prior reader from P1-10:

```sh
cargo +1.95.0 build --locked --release --all-features \
  --example share_aliases --example reference_lookup

target/release/examples/share_aliases CITY_FORMAT4.nidx CITY_FORMAT5.nidx \
  ::/96,1,2 ::ffff:0:0/96,7265512,1 2001::/32,10898268,1 2002::/16,14704189,1

cat CITY_FORMAT5.nidx >/dev/null
taskset -c 20 target/release/examples/reference_lookup CITY_FORMAT5.nidx
```

Repeat on the same format-4 input with both builds of `reference_lookup`,
and on format 5 with the new reader. Each window specifies the IPv6 prefix,
first logical ID and ID stride. Windows project the next 32 address bits
and ignore subsequent bits. The four projections reproduce these retained
assertions. The library does not infer provider conventions from addresses.
It rejects windows that intersect independent IPv6 assertions, and requires
the IPv4 source to consist of networks with IDs in sorted row order.
Duplicates and overlaps within the source are preserved.

The previous reader rejects format 5 with `Version(5)`; deploy the new
reader before emitting this format. Standard builders still emit formats 3/4,
and the new reader still accepts both. The feature is independent of the
format-4 compact reference default.

## Verification

Default and all-feature tests, formatting, Clippy, docs and package checks
passed. Four integration tests cover exact sorted visits and lookup order,
overlaps, duplicates, payload borrowing, independent format bytes and
malformed input. The Linux AddressSanitizer read fuzzer completed 21,163,205
executions in 61 seconds with the new format-5 seed and no failure.
