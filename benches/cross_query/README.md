# Cross-query SIMD study (P1-02)

This research package compares ordinary visitors, the frozen P1-01 interleaving
kernels, and new cross-query AVX2 kernels. [Results and decision](RESULTS.md).
No tracked library source, encoding or public API changes.

ASN search gathers complete eight-byte rows and masks off IDs for unsigned key
comparisons. Packed-IP fence search gathers both halves of each original u128
key and compares them lexicographically as unsigned integers. One AVX2 vector
holds four queries; eight-query groups advance two independent vectors each round.
A separate ASN variant uses native eight-lane u32 gathers, loading only keys
with stride eight; row counts above i32::MAX use scalar fallback. Completed and
absent lanes are masked.
Slice/bound checks and runtime AVX2 detection precede unsafe gathers. The
[Rust intrinsic documentation](https://doc.rust-lang.org/core/arch/x86_64/fn._mm256_mask_i64gather_epi64.html)
describes the masked gather used here.

IPv4 retains its directory; packed tails/delta decoding/payload access and
interval/broad-route fallbacks remain serial. Output remains query-major, with
original targets, IDs, duplicates, borrowed payloads and typed callback errors.
ISA/bounds checks and setup are included in timings. Backing bytes stay immutable.

The `default` build retains scalar packed tails; `simd` enables existing tail
SIMD. **Cross-query variants use runtime-detected AVX2 in both builds.** This
separates cross-query work from existing within-query tail acceleration.

Variant IDs in JSON's inherited `lanes` field:

| ID | Method |
| --- | --- |
| 0 | Ordinary scalar-query visitor |
| 1 | P1-01 one-lane kernel/setup control |
| 4/8/16 | P1-01 fixed-group interleaving |
| 104/108 | New AVX2 four/eight-query groups |
| 112 | Native eight-lane u32 ASN; same IP kernel as 108 |
| 204/208/212 | New kernels with forced scalar fallback; correctness checks only |

## Reproduction

Use x86-64 Linux/WSL with guest CPU 20 available and no custom Rust flags. Generate
the traffic/shape fixtures from [BENCHMARKS.md](../../BENCHMARKS.md). Start from
a fresh `target/cross-query` directory: preparation refuses to overwrite it.
It copies the core and reuses P1-01 fragments and its independent oracle harness,
adds the new methods/kernels, formats generated code, and hashes the entire
prepared source tree. The pinned lock uses the same dependencies as P1-01.

```sh
rustfmt +1.95.0 --edition 2024 --check benches/cross_query/reader.inc.rs benches/cross_query/packed.inc.rs benches/cross_query/study_simd.rs
python3 benches/cross_query/prepare.py
cargo +1.95.0 fmt --manifest-path benches/cross_query/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/cross-default cargo +1.95.0 build --locked --release --manifest-path benches/cross_query/Cargo.toml
CARGO_TARGET_DIR=/tmp/cross-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/cross_query/Cargo.toml
CARGO_TARGET_DIR=/tmp/cross-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/cross_query/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/cross-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/cross_query/Cargo.toml -- -D warnings
```

The generated core's additional unit tests compare vector and scalar kernels with
linear oracles at all 32 byte alignments, counts 0/1/2/3/4/7/8/9/17/64/511,
unsigned 32/128-bit boundaries, inactive/partial groups and invalid bounds.
Use a separate target directory for unit tests; its generated dependency lock is
build auxiliary output rather than part of the prepared measurement snapshot.
Pin common dependencies to the study lock if resolving a fresh core-test lock.

```sh
cargo +1.95.0 update --manifest-path target/cross-query/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/cross-query/library/Cargo.toml -p memmap2 --precise 0.9.11
CARGO_TARGET_DIR=/tmp/cross-tests cargo +1.95.0 test --locked --manifest-path target/cross-query/library/Cargo.toml --features mmap,simd --lib
CARGO_TARGET_DIR=/tmp/cross-tests cargo +1.95.0 clippy --locked --manifest-path target/cross-query/library/Cargo.toml --features mmap,simd --lib --tests -- -D warnings
```

Run measurements serially after builds/tests finish. The full matrix checks every
original assertion, independent full match sequences, pointer parity, partial
batches, forced scalar kernels and typed error emission prefixes before timing.
The runner requires actual AVX2 detection for every measured corpus. Primary
matrix: 32 processes; repeat: all six 20M family/build cases in fresh processes.

```sh
python3 benches/cross_query/run.py /tmp/cross-default/release/netindex-cross-query-study /tmp/cross-simd/release/netindex-cross-query-study /tmp/FRESH-CROSS-WORK /tmp/cross-query.json
python3 benches/cross_query/repeat.py /tmp/cross-query.json /tmp/FRESH-CROSS-REPEAT /tmp/cross-query-repeat.json
python3 benches/cross_query/inspect.py /tmp/cross-query.json /tmp/FRESH-CROSS-WORK /tmp/cross-query-asm.json
python3 benches/cross_query/verify.py /tmp/cross-query.json /tmp/cross-query-repeat.json /tmp/cross-query-asm.json /tmp/FRESH-CROSS-WORK /tmp/FRESH-CROSS-REPEAT /tmp/cross-query-verification.json
```

The verifier independently reruns preparation in a disposable shadow checkout,
compares generated files, and checks retained source/lock/executable/input/log
hashes, trial coverage and derived counts/timings. Full disassembly stays local.
Receipts record canonical LF hashes as well as raw measured hashes.

Portable compiler target, release/codegen-units=1, no PGO/LTO; same executable
contains scalar and candidates. Four forward/reverse trials and one warmup pass
per measurement follow P1-01. Aggregate ns/query is throughput cost, not query
tail latency, cold-file behavior, HTTP or native-provider capacity. WSL pinning
cannot control host scheduling. Results across these two builds or P1-01's old
executables are not an isolated causal comparison. Actual other-architecture
qualification remains P1-08; representative PGO remains P2-07/P2-08.

Regenerate the descriptive tables from the frozen receipts with:

```sh
python3 benches/cross_query/report.py PRIMARY_JSON REPEAT_JSON ASM_JSON benches/cross_query/RESULTS.md
```
