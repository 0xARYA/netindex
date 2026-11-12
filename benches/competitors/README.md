# Trie and bitmap comparison

This optional Linux runner compares three narrow operations on identical
synthetic inputs: IPv4 and IPv6 longest prefix against `ip_network_table`,
and exact ASN membership against Roaring. It uses 100K leaves or keys per
case. The executable can also run an individual case on Windows.

```sh
CARGO_TARGET_DIR=target/bench-default cargo +1.95.0 build --locked --release --manifest-path benches/competitors/Cargo.toml
CARGO_TARGET_DIR=target/bench-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/competitors/Cargo.toml
python3 benches/competitors/run.py target/bench-default/release/netindex-competitor-study target/bench-simd/release/netindex-competitor-study /tmp/netindex-comparison /tmp/netindex-comparison.json --cpu 0
python3 benches/competitors/verify.py /tmp/netindex-comparison.json
```

Choose an available CPU and unused output paths on Linux storage. Both
builds use the same inputs; the runner records executable, fixture, and
log hashes. Individual runs accept `FRESH_DIRECTORY FAMILY SHAPE RECORDS`.
Valid cases are `v4` or `v6` with `disjoint` or `overlap`, and `asn`
with `dense`, `sparse`, or `clustered`. On Windows use the
[process runner](../run.ps1) to apply a single-CPU affinity mask.

An independent map or set checks answers before timing. Prefix answers include
the matched network and insertion ID. Membership returns a Boolean. These
benchmarks exclude opening, building and serialization. They use warm synthetic
corpora and do not compare preservation of all overlapping and duplicate records.
See the [public guide](../../BENCHMARKS.md#when-to-use-a-specialized-index) for
historical results.
