# Comparisons under equivalent query contracts

This standalone study compares netindex with
[ip_network_table 0.2.0](https://docs.rs/ip_network_table/0.2.0/ip_network_table/struct.IpNetworkTable.html)
and [Roaring 0.12.0](https://docs.rs/roaring/0.12.0/roaring/bitmap/struct.RoaringBitmap.html).
These dependencies belong to the benchmark, not the library. The trie provides
longest-prefix lookup; Roaring provides integer membership and optional run
compression through `optimize()`.

The historical P0 receipt uses the harness at `ab28f32`; use that revision to
verify its original 1,032-measurement matrix. The current harness adds
`netindex-direct` to the prefix comparison. On Windows, pin externally (see
the [native City process runner](../native_city/README.md)); Linux still pins
internally.

```sh
cargo +1.95.0 fmt --manifest-path benches/competitors/Cargo.toml -- --check
CARGO_TARGET_DIR=/tmp/competitor-default cargo +1.95.0 build --locked --release --manifest-path benches/competitors/Cargo.toml
CARGO_TARGET_DIR=/tmp/competitor-simd cargo +1.95.0 build --locked --release --features simd --manifest-path benches/competitors/Cargo.toml
CARGO_TARGET_DIR=/tmp/competitor-default cargo +1.95.0 clippy --locked --release --all-targets --manifest-path benches/competitors/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/competitor-simd cargo +1.95.0 clippy --locked --release --all-targets --features simd --manifest-path benches/competitors/Cargo.toml -- -D warnings
python3 benches/competitors/run.py /tmp/competitor-default/release/netindex-competitor-study /tmp/competitor-simd/release/netindex-competitor-study /tmp/FRESH-LINUX-DIRECTORY /tmp/competitors.json
python3 benches/competitors/verify.py /tmp/competitors.json
```

Run serially with an otherwise quiet host. The executable pins itself to guest
CPU 20; adapt that constant and record topology on other machines. Builds use
Rust 1.95.0, release codegen-units=1, portable target and no PGO/LTO/custom flags.
The runner verifies byte-identical default/SIMD fixtures and corpora, records
source/manifest/lock/runner/executable/input hashes, and retains raw logs.
The verifier checks hashes, trial coordinates, checksums and histogram percentiles,
then adds LF-normalized source hashes for comparison across Git newline settings.
Measured working-copy source hashes remain intact; binary hashes are never normalized.
Each case owns a fresh directory and exclusively created immutable files.

The matrix contains 1K, 100K and 1M leaves/keys (the builder budget allows two million
assertions to include parent prefixes in overlapping cases):

- IPv4 and IPv6, disjoint or overlapping prefixes. IPv4 leaves are /24s at
  512-address intervals; overlaps add /20 and /16 parents. IPv6 leaves are /64s
  at alternate /64 intervals under `2001:db8::/32`; parents are /56 and /48.
  The result is `Option<(network address, prefix, insertion ID)>`.
  Netindex reduces its all-match visitor to the longest prefix. Duplicate
  prefixes are excluded because trie insertion replaces their previous value.
- ASN membership over unique positive u32 keys, with empty netindex payloads.
  Dense keys are consecutive, sparse keys use a modular permutation over the
  nonzero u32 domain, and clustered keys occupy groups of 1,024 with gaps.
  Results are Boolean. Default and optimized Roaring are separate measurements;
  both serialized representations must deserialize to the original bitmap.

Before timing, independent standard HashMap masked-key equality or HashSet
membership checks all timed queries and every original boundary/key plus its
neighbours. Prefix endpoints are included. A 65,536-query corpus repeats sixteen
times per measurement, after one warmup pass; four prefix trials alternate engine
order and six membership trials cover all engine-order permutations. Every timed fingerprint/checksum must match the independent oracle.
Uniform traffic spreads across generated leaf IDs; hot traffic uses the first
1,024 IDs. Disjoint and membership cases have 50% hits. Overlap parents cover
gaps, so those cases have 100% hits. Persisted corpora use little-endian u128
addresses or u32 ASNs, without a header.

All structures reside together: netindex owns its encoded Vec, and the trie and
bitmaps own heap structures. Serialized file sizes do not measure heap/RSS.
Lookup timing excludes construction, serialization and validation. Sampled
latencies include clock overhead; aggregate time also includes corpus iteration
and output fingerprint/Boolean accumulation. These fixed warm synthetic corpora
do not establish global-uniform IP performance, cold I/O, concurrency, service
capacity, or an exhaustive ranking of routing/bitmap implementations. Neither
specialized projection preserves netindex's full assertion contract.

Equivalent geography evidence is reused from the
[native City adapter](../native_city/README.md) and its
[hashed result](../results/2026-11-04-native-city.json): identical normalized owned
records and 108,123-query parity, with serving measured separately on Windows.
Its timings are not pooled with the new Linux synthetic measurements.

Read the [results and decisions](RESULTS.md) and the updated
[performance queue](../../PERF_TASKS.md).
