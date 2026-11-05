# Learned fences with exact correction (P1-07)

This isolated generated core compares guarded learned rank prediction with
ordinary visitors, prior cross-query AVX2 and a native sorted-key cache.
[Results](RESULTS.md). Production source, public APIs and encoding are unchanged.
Frozen earlier studies remain intact.

## Reference and implementation

[RadixSpline](https://github.com/learnedsystems/RadixSpline) combines a learned
piecewise-linear CDF with radix routing. Its [builder](https://github.com/learnedsystems/RadixSpline/blob/ab96aa59d429e7423beba2350bdcdf88952df282/include/rs/builder.h)
and [lookup](https://github.com/learnedsystems/RadixSpline/blob/ab96aa59d429e7423beba2350bdcdf88952df282/include/rs/radix_spline.h)
are the reference for this independently written Rust experiment. This is **not
an upstream port or upstream benchmark**. It uses disjoint anchored affine
segments rather than connected spline vertices, precomputed slopes, segment-end
clamps, and a correctness guard. No upstream source is vendored.

Models retain native u32 ASN keys/u32 IPv4 packed fences, or normalized u64 IPv6
packed fences when the entire fence span fits 64 bits. Wider IPv6, intervals,
broad routes and unaffordable key arrays keep ordinary fallback. Model construction
scans sorted keys once, merging duplicate keys into their last-occurrence upper
CDF rank. A segment maintains a feasible slope corridor for requested error
4/32/128; a violated corridor starts a new segment. Binary or radix routing
selects the preceding segment. Integer query-anchor subtraction precedes f64
conversion, retaining small deltas near u64::MAX. Extrapolation past a segment's
last training key returns its exact terminal rank. A one-segment model bypasses
routing and allocates no radix table. Otherwise radix bits adapt to segment
count, capped at 12. This fixed design is not trained from benchmark timings.

Prediction searches at most `2*epsilon+5` native keys. It then checks the two
**global neighbors**: the preceding key must be <=query and the next key >query.
If either condition fails, full native-key binary search supplies the exact
upper bound. This proves correctness independently of floating-point precision,
duplicate jumps, gaps or the model's requested error. Error settings define a
candidate correction window, not an unconditional prediction guarantee. ASN
lower bounds use upper_bound(query-1), with zero handled explicitly; all original
duplicates, IDs, targets, borrowed payloads and callback errors remain intact.
No query-result projection is substituted. The prototype retains non-inlined
estimate/search boundaries for linked-body inspection; their call/result-record
costs remain in every timing. Inlining/specialization is not a measured factor
here. Both sorted-control and learned search return rank/window/recovery state;
the timed reader consumes the rank. No mutable per-query counters are used.

| JSON `lanes` | Method |
| --- | --- |
| 0 | Ordinary visitor |
| 108 | Prior eight-query AVX2 with runtime dispatch and scalar fallback |
| 700 | Native sorted cache, global binary search |
| 701 | Learned estimate, binary segment routing, bounded correction/guard |
| 702 | Learned estimate, radix segment routing, bounded correction/guard |

Correctness-only variant 703 deliberately predicts rank zero to exercise recovery;
204/208/212 force prior scalar kernels. Domain fast paths are shared by 700/701/702.
Both learned variants correct with scalar/native binary search; this study adds
no new explicit SIMD kernel. Packed block decoding retains the existing reader
search behavior. Compiler-generated portable SIMD is permitted.

## Memory, inputs and protocol

One error setting per fresh process; comparison methods share that setting's
native arrays and model metadata. The 128 MiB aggregate vector-element-capacity
budget covers keys, 40-byte segment elements (including unused capacity), and
u32 radix entries. Candidate priority is IPv4, eligible IPv6, then ASN. Empty or
unaffordable key arrays allocate nothing. Model/array allocation failure is a
typed error. Segment arrays grow geometrically within the remaining budget;
actual aggregate capacity is checked before publication. Same-error preparation
is idempotent; changing the error is rejected. Mapping, validation, the ordinary
reader directory, objects, allocator overhead and RSS are outside this bound.
Construction follows ordinary validation and is measured outside query timing.
Native key mirrors remain resident even for ordinary controls; these are RAM
layout comparisons, not a zero-copy or total-process-memory result.

Reuse the six P0 exact fixtures (IPv4/IPv6/ASN, 1M/20M) and ten original shared/
unique shape fixtures unchanged. Add eight 65,536-record curved/clustered IPv4/
IPv6 fixtures. Curved raw keys are `id*(id+1)`; clustered keys are
`(id/1024)*2^26+2*(id%1024)`. IPv6 scales raw keys by `2^32+15` under a high 64-bit
base of u64::MAX, exercising nearly full-width u64 differences. Separate fixture
and oracle implementations express the formulas independently. Original assertion
checks and full original-target scan oracles verify every new timed corpus.
Clustered corpora add 63 midpoint-in-gap queries. The verifier regenerates all
eight additional files byte-for-byte with the retained generator.

144 primary processes plus 66 fresh repeats (all 20M and all nonlinear shapes)
yield 5,280 timings. Each process has four forward/reverse trials with one warmup
before every timing. Exact uniform/hot corpora contain 65,536 queries plus
boundaries, 16 passes; shapes 256 plus boundaries (and 63 clustered gaps), 256 passes.
Error and default/SIMD build orders alternate. CPU 20 is pinned in WSL, portable
release/codegen-units=1, no PGO/LTO/custom Rust flags. Both builds detect AVX2 at
runtime. Do not compile or run another benchmark during timings.

Untimed probes report `[eligible model queries, full-search fallbacks, total
local-window elements, largest local window]`; they use the same guarded search
as timing without mutable counters. These include model domain fast paths and
are diagnostics, not hardware counters. Binary/radix probe results must agree.
A forced bad-prediction variant and corrupted/NaN-slope unit tests establish
recovery even if ordinary corpora trigger no fallback. Full/partial/empty/mixed
batches, broad-route fallbacks, borrowed-pointer parity and typed error prefixes
remain checked. Unit tests cover duplicates, gaps, u32/u64 limits, sortedness,
allocation budgets and cache identity.

These are warm aggregate throughput/startup medians on one WSL host. They do not
establish statistical significance, latency percentiles, cold/>RAM behavior,
native-provider results, optimum error/radix settings or universal dominance.
The largest original fixtures have evenly spaced keys and can fit one segment;
nonlinear cases are smaller and do not qualify large nonlinear/native-provider
traffic. A production decision needs those cases plus startup amortization,
zero-copy correction economics, resident memory and other hosts.

## Reproduce

Use Rust 1.95.0, Python 3, GNU objdump/nm, Linux CPU 20, and the unchanged P0 fixture
instructions. Preparation refuses an existing `target/learned`; preserve earlier
evidence before choosing a fresh ignored target. Run serially:

```sh
python3 benches/learned/prepare.py
cargo +1.95.0 fmt --manifest-path benches/learned/Cargo.toml -- --check
CARGO_TARGET_DIR=/path/default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/learned/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/path/default cargo +1.95.0 build --locked --release --manifest-path benches/learned/Cargo.toml
CARGO_TARGET_DIR=/path/simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/learned/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/path/simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/learned/Cargo.toml
/path/default/release/netindex-learned-fixtures target/learned-fixtures
python3 benches/learned/run.py /path/default/release/netindex-learned-study /path/simd/release/netindex-learned-study /path/new-work benches/results/2026-11-04-learned.json
python3 benches/learned/repeat.py benches/results/2026-11-04-learned.json /path/new-repeat benches/results/2026-11-04-learned-repeat.json
python3 benches/learned/inspect_asm.py benches/results/2026-11-04-learned.json /path/new-work benches/results/2026-11-04-learned-asm.json
python3 benches/learned/verify.py benches/results/2026-11-04-learned.json benches/results/2026-11-04-learned-repeat.json benches/results/2026-11-04-learned-asm.json /path/new-work /path/new-repeat benches/results/2026-11-04-learned-verification.json
python3 benches/learned/report.py benches/results/2026-11-04-learned.json benches/results/2026-11-04-learned-repeat.json benches/results/2026-11-04-learned-asm.json benches/learned/RESULTS.md
```

Test/lint the generated core separately (its lock is build auxiliary output):

```sh
cargo +1.95.0 generate-lockfile --manifest-path target/learned/library/Cargo.toml
cargo +1.95.0 update --manifest-path target/learned/library/Cargo.toml -p libc --precise 0.2.190
cargo +1.95.0 update --manifest-path target/learned/library/Cargo.toml -p memmap2 --precise 0.9.11
cargo +1.95.0 test --locked --lib --features mmap,simd --manifest-path target/learned/library/Cargo.toml -- --test-threads=1
cargo +1.95.0 clippy --locked --lib --tests --features mmap,simd --manifest-path target/learned/library/Cargo.toml -- -D warnings
```

Receipts retain generated/original source, lock, toolchain, executable, input and
raw-log hashes. Verification reproduces preparation, header-derived eligibility/
native widths/segment capacities/radix dimensions/budgets, measurement coordinates,
probe parity, original-result checksums and repeat/assembly linkage. Full local
raw logs, binaries, fixture copies and disassembly dumps stay on the measurement
host. Extracted hot model bodies and generic dispatch summaries are in JSON.
LF-normalized source hashes support comparisons across checkout line endings.
