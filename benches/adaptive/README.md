# Adaptive search with an additional-cache budget (P1-06)

This isolated study adds candidate-only caches and a deterministic selector to a
copy of the library. [Results](RESULTS.md). Production code, public APIs and
encoded bytes are unchanged. Frozen P1-01 through P1-05 studies remain intact.

## Admission and policy

Each fresh process receives an explicit `NETINDEX_STUDY_BUDGET`: 0, 1 MiB or
128 MiB. Reject budgets above 128 MiB. Greedily admit, in order, nonempty native
u32 IPv4 packed fences, u128 IPv6 packed fences, then flat B+ ASN leaves/internal
keys/level ranges. Skip unaffordable candidates before allocating their arrays.
The sum of actual vector element capacities is checked before publishing the
cache. Repeating preparation with the same budget is idempotent; changing it is
a typed error. Failed preparation does not publish a partial cache.

This bounds **additional cache vector element capacity**, not total reader
memory, allocator overhead, vector/cache objects, mapped input, validation or
process RSS. The ordinary reader's existing IPv4 directory remains available
and outside this additional-cache budget. No competing layout mirrors are
resident. Construction follows ordinary reader validation and is timed
separately from queries. Allocation failures remain typed errors.

The selector uses only batch length, family, numeric span of the first 32
queries and admitted cache presence. Batches shorter than 32, mixed sample
families or sample spans at most 4096 use ordinary visitors. Otherwise use an
admitted cache (IPv4 directory-aware, ASN ordered AVX2, IPv6 global sorted
fences), or the existing eight-query cross-query kernel without a cache.
These constants are explicit research heuristics, not a trained or optimal
policy. It reads neither scenario/traffic labels nor benchmark timings.
Sampling and dispatch are included in timed queries. Short batches avoid the
sample scan. Interval/broad-route visitors and unavailable caches preserve
ordinary fallback behavior and borrowed query-major results.

| JSON `lanes` | Method |
| --- | --- |
| 0 | Ordinary visitor |
| 108 | Prior eight-query AVX2, runtime feature detection/scalar fallback |
| 600 | Admitted global sorted fences; specialized ordered AVX2 ASN B+ walk |
| 601 | Existing IPv4 directory plus native fences; specialized count ASN walk |
| 602 | Prefix-sampled policy for each complete corpus request |
| 603 | Same policy for consecutive requests of at most 32 queries |

Correctness-only variants204/208/212/604 force scalar kernels. The32-query
control includes repeated policy decisions and request splitting/rebasing.
Its throughput is not a latency measurement. Full-corpus policy traces are
logged outside timing; the verifier reconstructs them from generated samples.
The32-query variant applies the same selector separately in each request.

## Protocol and limits

Use unchanged P0 fixtures and the original independent original-input oracle.
Exact IPv4/IPv6/ASN have 1M/20M entries; five shape families each have shared/
unique payloads. All three budgets run in separate processes, alternating
budget and default/SIMD build order. Both builds detect AVX2 at runtime; the
SIMD feature does not itself enable the research AVX2 kernels.

Exact cases use uniform, hot, late-wide and late-hot corpora: 65,536 queries plus
boundaries, 16 passes. Hot IDs address at most 1024 records. Late-wide begins
with 32 hot queries then becomes uniform; late-hot begins uniform then becomes
hot. These controls expose unrepresentative prefix samples. Shapes retain 256
queries plus boundaries, 256 passes. Four forward/reverse trials and one warmup
corpus precede every timing. CPU 20 is pinned in WSL; portable release with
codegen-units=1, no custom Rust flags/PGO/LTO. Setup and input hashing are outside
timing. Do not run compilation or another benchmark during measurements.

All methods retain full IDs, targets, multiplicity, borrowed payload bytes and
pointers, typed callback-error prefixes, partial/empty/mixed batches and broad
route fallbacks. Partial checks include 31/32/33 to exercise the policy boundary.
Generated-core tests cover exact budget admission, unsigned bounds, duplicates,
incomplete trees, specialized traversal and forced scalar paths.

The matrix has 96 primary processes plus 18 fresh 20M process repeats and 6,624
individual timings. Medians are warm aggregate ns/query; the study does not
establish statistical significance, cold/>RAM performance, hardware-counter
causality, native-provider economics, optimal admission or universal dominance.

## Reproduce

Rust 1.95.0, Python 3, GNU objdump/nm and Linux CPU 20 are required. Generate the
unchanged traffic/shape fixtures using their owning benchmark instructions.
Preparation refuses to overwrite `target/adaptive`; preserve earlier receipts
before using a fresh ignored study directory. Build and run serially:

```sh
python3 benches/adaptive/prepare.py
cargo +1.95.0 fmt --manifest-path benches/adaptive/Cargo.toml -- --check
CARGO_TARGET_DIR=/path/default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/adaptive/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/path/default cargo +1.95.0 build --locked --release --manifest-path benches/adaptive/Cargo.toml
CARGO_TARGET_DIR=/path/simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/adaptive/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/path/simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/adaptive/Cargo.toml
python3 benches/adaptive/run.py /path/default/release/netindex-adaptive-study /path/simd/release/netindex-adaptive-study /path/new-work benches/results/2026-11-04-adaptive.json
python3 benches/adaptive/repeat.py benches/results/2026-11-04-adaptive.json /path/new-repeat benches/results/2026-11-04-adaptive-repeat.json
python3 benches/adaptive/inspect_asm.py benches/results/2026-11-04-adaptive.json /path/new-work benches/results/2026-11-04-adaptive-asm.json
python3 benches/adaptive/verify.py benches/results/2026-11-04-adaptive.json benches/results/2026-11-04-adaptive-repeat.json benches/results/2026-11-04-adaptive-asm.json /path/new-work /path/new-repeat benches/results/2026-11-04-adaptive-verification.json
python3 benches/adaptive/report.py benches/results/2026-11-04-adaptive.json benches/results/2026-11-04-adaptive-repeat.json benches/results/2026-11-04-adaptive-asm.json benches/adaptive/RESULTS.md
```

Test and lint the generated core separately; its standalone dependency lock is
build auxiliary output. On a fresh prepared tree:

```sh
cargo +1.95.0 generate-lockfile --manifest-path target/adaptive/library/Cargo.toml
cargo +1.95.0 update --manifest-path target/adaptive/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/adaptive/library/Cargo.toml -p memmap2 --precise 0.9.11
cargo +1.95.0 test --locked --lib --features mmap,simd --manifest-path target/adaptive/library/Cargo.toml -- --test-threads=1
cargo +1.95.0 clippy --locked --lib --tests --features mmap,simd --manifest-path target/adaptive/library/Cargo.toml -- -D warnings
```

Primary receipts retain source/generated-source/toolchain/executable/input/raw
log hashes, cache bytes/build time, full-corpus selected methods and results.
Verification reproduces preparation in a shadow checkout, checks aggregate
admission against file headers, reconstructs full-corpus policy choices, checks
all timing coordinates/results and links repeat/assembly receipts. Raw logs,
fixture copies, binaries and full disassemblies remain on the measurement host;
JSON includes hashes and extracted linked-code bodies. LF-normalized source
hashes account for checkout newline conversion.
