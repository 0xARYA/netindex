# Flat B+ node-search study (P1-04)

An isolated research core compares flat, bottom-up B+ search with unchanged
ordinary, cross-query SIMD, sorted-mirror and Eytzinger controls.
[Measurements and decision](RESULTS.md). No tracked library source/API/format change.

[PlanB](https://github.com/nicexlab/planb) motivates the flat multiway layout and
parallel node comparisons. This is an independent rank-search prototype, not
a reproduction of its AVX-512, batching, prefetch or single-result route lookup.
Netindex retains all duplicate assertions, original targets/IDs and payloads.

## Layout and scope

Sorted leaves retain original row/block order. Each internal key is the maximum
of a child group, stored in one flat node vector, with bottom-up level ranges.
Logical nodes contain 64 bytes of keys: 16 u32 ASNs or four u128 IP fences.
Partial nodes have actual lengths; no padding sentinel can collide with a key.
Lower/upper bounds route to the first child whose maximum is >=/> the query,
respectively. Original sorted leaf positions require no per-key rank array.
Empty inputs, incomplete levels, duplicate separators and unsigned extremes are
covered by independent linear-oracle tests.

| Variant ID | Method |
| --- | --- |
| 0 | Ordinary visitor |
| 108 / 112 | Prior eight-query AVX2 / native u32 ASN; same IP control |
| 300 / 301 / 302 | Sorted mirror / Eytzinger candidate / arithmetic path recovery |
| 400 | B+ binary partition search inside each node |
| 401 | B+ count comparisons inside each node; compiler may auto-vectorize |
| 402 | B+ detected AVX2 node counts, portable count fallback |
| 404 | B+ ordered AVX2 mask rank, portable count fallback |
| 204 / 208 / 212 / 403 | Forced scalar controls, correctness only |

Explicit AVX2 uses unsigned u32 comparisons via sign-bit transforms and lexicographic
u128 comparison of high/low halves, including equality for upper bounds. Loads
cover complete eight-u32/two-u128 groups only; scalar tails handle partial nodes.
Ordered-mask404 uses the sorted-prefix invariant, trailing-one rank and early
exit after the first partially matching vector; it avoids population counting.
Unordered keys are not a valid input to that internal rank helper.
ISA detection remains in the safe node helper. These are within-node comparisons,
not a new cross-query batching implementation. Linked assembly, rather than the
source expression, determines emitted vector operations and conditional branches.

All new mirrors search global fences with the same key width. Original IPv4
ordinary/prior methods retain their directory, so those comparisons also change
routing scope and encoded-key density. B+ vs sorted/Eytzinger controls isolates
more of the layout effect. Original packed tail search, target decoding and
borrowed payload access follow. Interval and optional broad-route visitors fall
back to their existing implementation; deep/duplicate IP shapes are fallback
controls, not new interval-tree layouts. Query-major order and typed callback
error prefixes are preserved.

## Resource boundary

All comparator caches coexist during all timings. The cache receipt lists 18
allocated-capacity byte counts: first the P1-03 nine arrays (ASN/v4/v6, each
sorted/heap/ranks), then B+ ASN/v4/v6, each leaves/internal keys/level ranges.
The B+ sorted leaves are separate copies from the Eytzinger sorted control.
Array capacities include level-range elements, exclude Vec/Cache objects and
allocator overhead, and are not RSS or total validation/construction peak RAM.
Build time covers all retained layouts together, not candidate-only startup.

Requested sizes are checked against a 512 MiB combined logical allocation budget
before construction. Tree counts, size arithmetic and fallible reserves are
checked; allocation errors retain their cause. Construction occurs once after
ordinary mapping/validation and outside lookup timing. Natural Vec alignment
does not guarantee cache-line alignment, and internal level starts are not
cache-line padded. Prefetch/alignment/traversal tuning remains P1-05 work.

## Reproduction

Use x86-64 Linux/WSL with CPU20 available, Rust 1.95.0, portable release builds,
codegen-units=1, no PGO/LTO/custom Rust flags, and existing
[traffic/shape fixtures](../../BENCHMARKS.md). Preparation refuses to overwrite
`target/flat_btree`; retain the frozen P1-01/02/03 sources it composes.

```sh
rustfmt +1.95.0 --edition 2024 --check benches/flat_btree/reader.inc.rs benches/flat_btree/packed.inc.rs benches/flat_btree/study_flat_btree.rs
python3 benches/flat_btree/prepare.py
cargo +1.95.0 fmt --manifest-path benches/flat_btree/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/bt-default cargo +1.95.0 build --locked --release --manifest-path benches/flat_btree/Cargo.toml
CARGO_TARGET_DIR=/tmp/bt-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/flat_btree/Cargo.toml
CARGO_TARGET_DIR=/tmp/bt-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/flat_btree/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/bt-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/flat_btree/Cargo.toml -- -D warnings
cargo +1.95.0 update --manifest-path target/flat_btree/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/flat_btree/library/Cargo.toml -p memmap2 --precise 0.9.11
CARGO_TARGET_DIR=/tmp/bt-tests cargo +1.95.0 test --locked --manifest-path target/flat_btree/library/Cargo.toml --features mmap,simd --lib
CARGO_TARGET_DIR=/tmp/bt-tests cargo +1.95.0 clippy --locked --manifest-path target/flat_btree/library/Cargo.toml --features mmap,simd --lib --tests -- -D warnings
```

Fifteen generated-core tests include B+ independent lower/upper bound oracles,
partial-node/level boundaries through 4097, duplicates and u32/u128 extremes,
scalar/detected parity, node comparisons at every element alignment within a
256-bit vector, unsigned half boundaries and allocation-accounting/limit checks.

Run benchmarks serially after compilation/tests finish. Each process verifies
every original assertion, complete original-input query oracle outputs and scalar
outputs including targets, IDs, payload bytes/pointers, partial/empty batches,
mixed families, broad routes and typed error prefixes before timing. Four
forward/reverse trials each follow one corpus warmup. Exact inputs use 65,536
queries plus boundaries, repeated 16 times; shapes use 256 plus boundaries,
repeated 256 times. Aggregate ns/query is throughput cost, not tail latency.

```sh
python3 benches/flat_btree/run.py /tmp/bt-default/release/netindex-flat_btree-study /tmp/bt-simd/release/netindex-flat_btree-study /tmp/FRESH-BT-WORK /tmp/bt.json
python3 benches/flat_btree/repeat.py /tmp/bt.json /tmp/FRESH-BT-REPEAT /tmp/bt-repeat.json
python3 benches/flat_btree/inspect_asm.py /tmp/bt.json /tmp/FRESH-BT-WORK /tmp/bt-asm.json
python3 benches/flat_btree/verify.py /tmp/bt.json /tmp/bt-repeat.json /tmp/bt-asm.json /tmp/FRESH-BT-WORK /tmp/FRESH-BT-REPEAT /tmp/bt-verification.json
python3 benches/flat_btree/report.py /tmp/bt.json /tmp/bt-repeat.json /tmp/bt-asm.json benches/flat_btree/RESULTS.md
```

32 primary processes and six fresh-process 20M repeats yield 2,240 measurements.
Verification checks hashes, raw logs, exact coordinates/derived totals and
header-derived capacities, and reconstructs prepared sources in a shadow checkout.
Full disassembly remains local; bounded complete node-helper bodies and a counter
availability probe are retained. WSL host load, repeated warm corpora and one
machine limit generalization. No cold/>RAM, native-provider, significance,
AVX-512/NEON or universal speedup claim follows from this study.
