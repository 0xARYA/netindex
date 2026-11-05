# Eytzinger RAM-layout study (P1-03)

This isolated research copy compares original sorted encoded lookups with
key-only sorted RAM mirrors and one-based breadth-first (Eytzinger) mirrors.
[Results and decision](RESULTS.md). No tracked library source or format changes.

The layout experiment follows the question studied by
[Khuong and Morin](https://arxiv.org/abs/1509.05053): how array layout affects
comparison search. It does not reproduce their tuned prefetch implementation.
There is no explicit prefetch, cache-line alignment guarantee or new SIMD here;
prefetch/traversal combinations remain P1-05 work.

The cache holds three arrays per key family: sorted keys, heap keys with one
unused sentinel, and u32 original sorted ranks with one sentinel. ASN keys are
u32; packed IPv4/IPv6 fences remain u128. In-order construction supports arbitrary
tree sizes and equal keys. ASN lower bounds preserve all duplicate rows/IDs;
IP upper bounds identify original packed blocks. Original tail search, decoding
and borrowed payload access follow. Interval and optional broad-route readers
retain ordinary visitors; callback errors retain their type and emission prefix.

The sorted RAM mirror controls for key density/copying: all three new methods
use the same key width and search all fences. Original IPv4 visitors and earlier
interleaving/SIMD methods retain the existing directory. Comparing a new mirror
with the ordinary encoded reader therefore changes routing scope as well as
layout. Compare Eytzinger with the sorted mirror to assess the layout tradeoff.

Variant IDs in the inherited JSON `lanes` field:

| ID | Method |
| --- | --- |
| 0 | Ordinary visitor |
| 8/16 | P1-01 interleaving |
| 108 | P1-02 cross-query AVX2 eight-query groups |
| 112 | P1-02 native eight-lane u32 ASN / duplicate IP108 control |
| 300 | Key-only sorted mirror |
| 301 | Eytzinger with candidate tracking |
| 302 | Eytzinger with arithmetic child selection and last-left-turn recovery |
| 204/208/212 | Prior forced scalar kernels; correctness checks only |

`Tree::bound` is not inlined and dispatches the three new methods per query.
Arithmetic source is not a claim that generated traversal has no branches.
Both executables also contain earlier cross-query AVX2 controls. `simd` enables
existing within-query packed-tail SIMD; the default build has scalar tails.

## Memory and construction

Cache construction occurs once after mapping/validation, before lookup timing.
It reports elapsed build ns and nine allocated-capacity byte counts ordered as
ASN sorted/heap/ranks, IPv4 sorted/heap/ranks, IPv6 sorted/heap/ranks. These exclude
allocator overhead, metadata, mapping/validation memory and process RSS. All
mirrors coexist, including during ordinary baseline timing. A candidate-only
deployment would not necessarily retain the sorted control. No isolated
candidate-only startup time is measured.

Requested array sizes are checked against a 512 MiB logical budget before
allocation; allocation failures preserve their underlying causes. Empty trees
still allocate heap/rank sentinels. The verifier independently derives capacities
from original file counts/flags and format's 256-row packed blocks. Rank arrays
restore original sorted row/block positions in this prototype; computed rank
mappings are not evaluated.

## Reproduction

Use x86-64 Linux/WSL with CPU 20 available, portable release/codegen-units=1,
no PGO/LTO/custom Rust flags, and [traffic/shape fixtures](../../BENCHMARKS.md).
Start with a fresh `target/eytzinger`: preparation refuses to overwrite it.
It reuses frozen P1-01/P1-02 code and original-input oracle harnesses, adds the
research cache and methods to generated sources, formats them and records hashes.

```sh
rustfmt +1.95.0 --edition 2024 --check benches/eytzinger/reader.inc.rs benches/eytzinger/packed.inc.rs benches/eytzinger/study_eytzinger.rs
python3 benches/eytzinger/prepare.py
cargo +1.95.0 fmt --manifest-path benches/eytzinger/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/eyt-default cargo +1.95.0 build --locked --release --manifest-path benches/eytzinger/Cargo.toml
CARGO_TARGET_DIR=/tmp/eyt-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/eytzinger/Cargo.toml
CARGO_TARGET_DIR=/tmp/eyt-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/eytzinger/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/eyt-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/eytzinger/Cargo.toml -- -D warnings
```

Thirteen generated-core unit tests include the earlier SIMD checks plus independent
linear lower/upper-bound oracles for both key widths, duplicates, incomplete trees,
counts 0..129 and powers-of-two boundaries through 1025, and unsigned extremes.
Generated test locks are auxiliary build outputs; pin common dependencies to the
study lock when creating one.

```sh
cargo +1.95.0 update --manifest-path target/eytzinger/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/eytzinger/library/Cargo.toml -p memmap2 --precise 0.9.11
CARGO_TARGET_DIR=/tmp/eyt-tests cargo +1.95.0 test --locked --manifest-path target/eytzinger/library/Cargo.toml --features mmap,simd --lib
CARGO_TARGET_DIR=/tmp/eyt-tests cargo +1.95.0 clippy --locked --manifest-path target/eytzinger/library/Cargo.toml --features mmap,simd --lib --tests -- -D warnings
```

Run measurements serially after compilation/tests finish. All original assertions,
full ordered oracle/scalar outputs, IDs/targets/payloads/pointers, empty/partial
batches, mixed families, duplicates, broad routes and typed error prefixes are
checked before timing. Each variant has four forward/reverse-order trials,
with one warmup corpus before each timing. Aggregate ns/query is throughput cost, not tail
latency, cold/>RAM or native-provider capacity. CPU pinning cannot isolate host load.

```sh
python3 benches/eytzinger/run.py /tmp/eyt-default/release/netindex-eytzinger-study /tmp/eyt-simd/release/netindex-eytzinger-study /tmp/FRESH-EYT-WORK /tmp/eytzinger.json
python3 benches/eytzinger/repeat.py /tmp/eytzinger.json /tmp/FRESH-EYT-REPEAT /tmp/eytzinger-repeat.json
python3 benches/eytzinger/inspect_asm.py /tmp/eytzinger.json /tmp/FRESH-EYT-WORK /tmp/eytzinger-asm.json
python3 benches/eytzinger/verify.py /tmp/eytzinger.json /tmp/eytzinger-repeat.json /tmp/eytzinger-asm.json /tmp/FRESH-EYT-WORK /tmp/FRESH-EYT-REPEAT /tmp/eytzinger-verification.json
python3 benches/eytzinger/report.py /tmp/eytzinger.json /tmp/eytzinger-repeat.json /tmp/eytzinger-asm.json benches/eytzinger/RESULTS.md
```

The primary matrix has 32 processes; all six 20M family/build cases repeat in
fresh processes. Verification reproduces the prepared source tree in a disposable
shadow checkout and checks source/lock/executable/input/raw-log hashes, coordinate
coverage, capacities, budgets and derived match/checksum/timing totals. Full
disassembly stays local; the receipt retains the bounded search helpers and a
hardware-counter availability probe. Other architectures and future optimized
variants require separate evidence before adoption.
