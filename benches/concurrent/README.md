# Shared readers and replacement

Standalone Linux/WSL benchmarks use Rust 1.95.0, release codegen-units=1 and mmap,
without optional SIMD, LTO or PGO. They leave library dependencies unchanged.
Run serially on an otherwise quiet machine. The runners require guest CPUs
0,2,...,30 for workers and 31 for coordination; validate topology before adapting
these constants. At sixteen workers the coordinator shares an SMT core.

```sh
cargo +1.95.0 fmt --manifest-path benches/concurrent/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/netindex-concurrent cargo +1.95.0 build --locked --release --manifest-path benches/concurrent/Cargo.toml
CARGO_TARGET_DIR=/tmp/netindex-concurrent cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/concurrent/Cargo.toml -- -D warnings
python3 benches/concurrent/run.py /tmp/netindex-concurrent/release/netindex-concurrent-study /tmp/FRESH-SCALING-DIRECTORY /tmp/scaling.json
python3 benches/concurrent/run_replacement.py /tmp/netindex-concurrent/release/replacement /tmp/FRESH-REPLACEMENT-DIRECTORY /tmp/replacement.json
```

Generate the 1M/20M `target/traffic-fixtures/` inputs with the traffic benchmark
first, as documented in [BENCHMARKS.md](../../BENCHMARKS.md). Scaling copies them
into a fresh private Linux filesystem directory, verifies hashes, validates and
warms the mapping, then shares an immutable reader. Every timed query is checked
before measurement against its independent ID/target/payload oracle; checksums
are checked afterward. Each worker executes two million mixed hit/miss queries
over a rotating corpus. Two trials reverse the order of 1/2/4/8/16 workers.
Sampled query latency includes clock overhead; histograms and clock controls
are retained. Throughput excludes setup, thread startup, joining and aggregation.

Replacement generates two 196,672-assertion files and one unchanged reader.
Each case uses 51 separately mapped, validated readers over those immutable
files. The application publishes a coherent generation plus `ReaderSet` through
`RwLock<Arc<Snapshot>>`. Candidate construction happens outside the lock.
At 1/4/16 workers and both coarse/fine routing, two trials perform fifty swaps
per case. Every result is checked against original target scans, including
source position, ID, target, bytes and multiplicity. Workers retain old requests
and borrowed payloads. Invalid files and replacement positions leave publication
unchanged. Weak references verify retired mapping reclamation, and eventual
release of all 51 mappings after snapshot owners drop.

Replacement latency samples cover a complete 32-query batch including snapshot
acquisition and correctness checking. Publication timings include lock waiting
and swapping, excluding destruction. Candidate construction excludes file mapping
and validation. This measures one caller publication policy, not an activation
API or a comparison with ArcSwap/RCU. The portable regression is
[tests/replacement.rs](../../tests/replacement.rs).

These are warm, closed-loop library workloads on a live virtualized host. Guest
CPU pinning cannot control host scheduling. Two trials and sampled percentiles
do not establish service capacity, queueing latency or production SLOs. Scaling
can appear superlinear because of shared caches and VM scheduling; do not
extrapolate per-core capacity. Replacement files are small and prevalidated;
deployment, pathname replacement and physical disk failures are outside scope.

Results: [scaling](../results/2026-11-04-concurrent.json),
[replacement](../results/2026-11-04-replacement.json). Each receipt records source,
executable and input hashes, build settings, histograms and limitations.
