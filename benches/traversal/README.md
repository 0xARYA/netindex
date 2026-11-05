# Traversal, prefetch and alignment study (P1-05)

An isolated generated core measures eight Eytzinger traversal combinations and
two specialized B+ walks alongside existing controls. [Results](RESULTS.md).
Production source, public APIs and encoded bytes are unchanged.

[Khuong and Morin](https://arxiv.org/abs/1509.05053) motivate the arithmetic
traversal/prefetch question. This independent Rust experiment retains original
rank maps, typed errors and full-result semantics rather than copying their
tuned implementation or assuming its results transfer to this workload.

## Factors and controls

The eight Eytzinger methods vary three factors independently:

- Candidate tracking vs arithmetic child selection and last-left-turn recovery.
- No software prefetch vs one guarded T0 descendant-group prefetch per node.
- Natural alignment vs guaranteed 64-byte group alignment.

Both mirrors store the identical one-based heap in identical 64-byte groups:
16 u32 ASNs or four u128 fences, with an unused index-zero sentinel and final
group padding. The natural array type has normal key alignment; the aligned
wrapper uses `repr(align(64))`. Both use the same group/index accessor. Cache
receipts record actual base alignment modulo64, including naturally aligned
controls that could happen to be cache-line aligned. No pure alignment claim
follows if the recorded control happens to be aligned too.

At node k, prefetch targets group k: keys at `k*LANES..(k+1)*LANES`, the possible
descendants four levels ahead for ASN/two for IP. Target multiplication is
checked and clipped to the valid heap before forming a pointer. Every group is
fully allocated; no out-of-allocation pointer or uninitialized key read occurs.
The hint fetches the line containing group start. An unaligned group can straddle
two physical lines; only the first is explicitly requested. No prefetch distance
or hint sweep is measured. Interval and broad-route visitors retain ordinary
fallbacks, not prefetched interval trees.

Variant IDs in the inherited JSON `lanes` field:

| ID | Method |
| --- | --- |
| 0 / 108 | Ordinary visitor / prior eight-query AVX2 |
| 300 / 301 / 302 | Sorted mirror / legacy candidate / legacy arithmetic |
| 401 / 404 | Legacy B+ count / ordered AVX2 |
| 500 + bits | New grouped Eytzinger: bit0 arithmetic, bit1 prefetch, bit2 aligned |
| 520 | Specialized B+ count walk; no per-node method selection |
| 521 | Specialized B+ ordered AVX2 walk; detection outside node loop |
| 204 / 208 / 212 / 403 / 522 | Forced scalar controls, correctness only |

The new Eytzinger factorial comparisons hold key density, accessor structure,
sorted-rank lookup, global search scope and all resident arrays fixed. Comparing
them with legacy Eytzinger also changes grouping/specialization, so that is a
control rather than a single-factor ablation. Arithmetic source does not prove
the compiled method is fully branch-free: loops, rank checks and prefetch guards
remain. Inspect the retained assembly.

B+520 specializes the scalar count walk. B+521 uses a dedicated AVX2 walk and
inline-capable direct rank helpers, detecting AVX2 once per query outside node
loops; unsupported/disabled paths use count. These combine dispatch/inlining/
ISA-hoisting changes. They are measured interventions, not separate causal
estimates for each change. B+ arrays retain natural alignment and original level
starts. No extra B+ prefetch is added.

All mirrors search global packed fences with original u128 widths. Ordinary/prior
IPv4 visitors retain their directory, so those comparisons also change routing
scope and key density. Original packed tails, decoding, IDs/targets and borrowed
payloads follow. Duplicates and original query-major callback/error prefixes
remain intact. The default/SIMD builds vary existing packed-tail search; explicit
AVX2 methods exist in both builds. There is no new cross-query batch kernel.

## Allocation boundary

Cache rows list 24 capacity-byte values. First come P1-04's 18 arrays (ASN/v4/v6
sorted/heap/rank, then B+ leaves/internal keys/level ranges); last come six new
mirror arrays (ASN/v4/v6, natural then aligned). Actual base mod 64 values follow
that last order. All arrays coexist in all comparisons, including ordinary
baseline timings. Logical requested bytes are checked against 512 MiB before
fallible allocation. Alignment padding and level-range elements are included;
allocator/object overhead, RSS and mapping/validation memory are excluded.
Construction elapsed time covers all retained layouts together, not candidate-only
startup, and occurs after ordinary validation outside timed lookup.

## Reproduction

Use x86-64 Linux/WSL with guest CPU 20 available, Rust 1.95.0 and existing
[traffic/shape fixtures](../../BENCHMARKS.md). Release/codegen-units=1, portable
target, no PGO/LTO/custom flags. Preparation refuses an existing `target/traversal`
and composes the frozen P1-01/02/03/04 sources with the new fragments.

```sh
rustfmt +1.95.0 --edition 2024 --check benches/traversal/reader.inc.rs benches/traversal/packed.inc.rs benches/traversal/eytzinger.inc.rs benches/traversal/btree.inc.rs
python3 benches/traversal/prepare.py
cargo +1.95.0 fmt --manifest-path benches/traversal/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/trav-default cargo +1.95.0 build --locked --release --manifest-path benches/traversal/Cargo.toml
CARGO_TARGET_DIR=/tmp/trav-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/traversal/Cargo.toml
CARGO_TARGET_DIR=/tmp/trav-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/traversal/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/trav-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/traversal/Cargo.toml -- -D warnings
cargo +1.95.0 update --manifest-path target/traversal/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/traversal/library/Cargo.toml -p memmap2 --precise 0.9.11
CARGO_TARGET_DIR=/tmp/trav-tests cargo +1.95.0 test --locked --manifest-path target/traversal/library/Cargo.toml --features mmap,simd --lib
CARGO_TARGET_DIR=/tmp/trav-tests cargo +1.95.0 clippy --locked --manifest-path target/traversal/library/Cargo.toml --features mmap,simd --lib --tests -- -D warnings
```

Seventeen generated-core tests include every traversal combination against linear
lower/upper-bound oracles, duplicate/incomplete trees through 4097, u32/u128 unsigned
boundaries, alignment/capacity guarantees and direct AVX2/scalar parity.

Run measurements serially after compilation/tests finish. Every process checks
all original assertions and complete independent query oracles: ordered targets,
IDs, duplicate results, borrowed payload bytes/pointers, empty/partial/mixed-family
batches, broad-route fallback and typed callback-error prefixes. Four forward/
reverse trials each have one corpus warmup. Exact 1M/20M cases use 65,536 plus boundary
queries times 16 passes; shapes use 256 plus boundaries times 256. Aggregate ns/query
measures warm throughput cost, not query/batch latency percentiles.

```sh
python3 benches/traversal/run.py /tmp/trav-default/release/netindex-traversal-study /tmp/trav-simd/release/netindex-traversal-study /tmp/FRESH-TRAV-WORK /tmp/traversal.json
python3 benches/traversal/repeat.py /tmp/traversal.json /tmp/FRESH-TRAV-REPEAT /tmp/traversal-repeat.json
python3 benches/traversal/inspect_asm.py /tmp/traversal.json /tmp/FRESH-TRAV-WORK /tmp/traversal-asm.json
python3 benches/traversal/verify.py /tmp/traversal.json /tmp/traversal-repeat.json /tmp/traversal-asm.json /tmp/FRESH-TRAV-WORK /tmp/FRESH-TRAV-REPEAT /tmp/traversal-verification.json
python3 benches/traversal/report.py /tmp/traversal.json /tmp/traversal-repeat.json /tmp/traversal-asm.json benches/traversal/RESULTS.md
```

32 primary processes plus six fresh-process 20M repeats yield 3,808 timings.
Verification reconstructs prepared sources in a shadow checkout and checks
source/lock/executable/input/raw-log hashes, header-derived capacities, physical
alignment, exact coordinates and derived totals. Full dumps remain local;
bounded hot-path assembly and counter availability are retained. Synthetic warm
corpora, one WSL machine and uncontrolled host effects limit generalization.
No cold/>RAM, native-provider, other-architecture or universal-win claim follows.
