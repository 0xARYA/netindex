# Interleaving prototype

This experiment adds research methods to an ignored, generated copy of the core.
Tracked library source, format, ordinary entry points and validation stay intact.
It measures ordinary scalar visitors, a one-lane kernel/setup control, and
round-robin groups of 4/8/16 queries. This is not a public batch API.

The prototype interleaves ASN lower-bound steps and packed-IP block-fence probes.
IPv4 keeps its existing directory; packed delta search/decoding and payload access
are serial. Interval/range/overlap and optional broad-route readers retain the
existing scalar visitor. Results emit in query-major order, preserving each
query's sorted target order, insertion IDs, duplicates and borrowed payloads.
Errors from callbacks preserve their caller-defined type and stop emission.
Backing bytes must remain immutable, as required by the ordinary reader.

Grouping independent searches to overlap latency is described by
[Lemire](https://lemire.me/blog/2019/09/14/speeding-up-independent-binary-searches-by-interleaving-them/).
[AMAC](https://www.vldb.org/pvldb/vol9/p252-kocberber.pdf) maintains per-lookup
state and can refill work independently. This study uses synchronous fixed groups;
it does not implement AMAC, explicit prefetching or cross-query SIMD.

## Reproduction

Start from a fresh checkout/output directory. The preparer refuses to overwrite
`target/interleaved/library`. It copies core sources and README, constructs an
isolated manifest from the core dependencies/features, inserts the research
fragments, and records original/prepared source hashes. Core tests remain intact.
Generate the traffic/shape fixtures as described in [BENCHMARKS.md](../../BENCHMARKS.md)
before running the full matrix.

```sh
rustfmt +1.95.0 --edition 2024 --check benches/interleaved/reader.inc.rs benches/interleaved/packed.inc.rs
python3 benches/interleaved/prepare.py
cargo +1.95.0 fmt --manifest-path benches/interleaved/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/interleaved-default cargo +1.95.0 build --locked --release --manifest-path benches/interleaved/Cargo.toml
CARGO_TARGET_DIR=/tmp/interleaved-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/interleaved/Cargo.toml
CARGO_TARGET_DIR=/tmp/interleaved-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/interleaved/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/interleaved-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/interleaved/Cargo.toml -- -D warnings
/tmp/interleaved-default/release/netindex-interleaved-study target/traffic-fixtures/v4-1000000.nidx v4 1000000 check
python3 benches/interleaved/run.py /tmp/interleaved-default/release/netindex-interleaved-study /tmp/interleaved-simd/release/netindex-interleaved-study /tmp/FRESH-LINUX-DIRECTORY /tmp/interleaved.json
```

Use Linux/WSL and run serially; CPU 20 must be available. Release uses
codegen-units=1, portable compiler target, mmap, optional SIMD, no LTO/PGO/custom
Rust flags. Both scalar and candidate run in the same executable/core snapshot,
with identical dependencies, validated immutable input and query corpus.
The one-lane control differs from ordinary `partition_point`: it exposes setup
and search-kernel overhead before attributing a gain to interleaving.

Every assertion's ID, target and payload is checked against its original fixture
generator. Independent oracles check complete query match sequences, followed by
differential scalar/prototype checks and borrowed-pointer equality. Additional
checks cover mixed families, ranges, /0 networks, duplicates, empty readers,
empty/partial batches, optional broad routes and typed visitor failures.

The matrix uses 1M/20M exact IPv4/IPv6/ASN assertions, uniform/hot corpora, and
65,536-assertion dense/sparse ASN, sparse IPv6, deep-prefix and duplicate-address
shapes with shared/unique payloads. Exact corpora contain 65,536 queries plus
boundary/wrong-family queries, repeated sixteen times per timing. Shape corpora
contain 256 queries plus boundaries, repeated 256 times. Each measurement has one
warmup pass; four trials reverse scalar/1/4/8/16 order. Timed hit counts and
ID/query-index sums must match. Targets and borrowed payloads are consumed through
`black_box`; timing includes batch setup and visitors, without output allocation.

Aggregate ns/query describes throughput cost. It does not describe query/batch
tail latency, queueing, HTTP, cold files, >RAM operation or native provider decoding.
Small reused shape corpora are warm controls. Guest CPU affinity cannot isolate
host scheduling. A best-case gain does not justify a default or public API change.
Use the [evidence standard](../EVIDENCE.md) for subsequent variants.

## Results and supplementary checks

[Results and adoption decision](RESULTS.md) distinguish the primary matrix from
fresh-process confirmation and static code inspection. No library API is added.
After the full run, repeat the selected default-build cases using its frozen inputs:

```sh
python3 benches/interleaved/repeat.py /tmp/interleaved.json /tmp/FRESH-REPEAT-DIRECTORY /tmp/interleaved-repeat.json
```

Inspect the linked executor call inside the timed repetition loop in `main`,
distinguishing its hit/checksum accumulator arguments from warmup/capture calls.
Pass the observed default and SIMD symbol addresses to the inspector. The dated
receipt used `0x32800` and `0x3f1a0`; rediscover them for different builds.

```sh
python3 benches/interleaved/inspect.py /tmp/interleaved.json /tmp/FRESH-LINUX-DIRECTORY DEFAULT_HEX SIMD_HEX /tmp/interleaved-asm.json
python3 benches/interleaved/verify.py /tmp/interleaved.json /tmp/interleaved-repeat.json /tmp/interleaved-asm.json /tmp/FRESH-LINUX-DIRECTORY /tmp/FRESH-REPEAT-DIRECTORY /tmp/interleaved-verification.json
```

Full disassembly stays in the private work directory; the small receipt records
hashes, prologue excerpts and qualified static observations. Verification checks
retained local sources, dependency lock, executables, inputs, raw logs, coordinate
coverage, derived timings and checked match/checksum totals. It records canonical
LF source hashes for Git checkout comparison; it does not rerun the experiment.
