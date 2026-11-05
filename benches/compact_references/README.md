# P1-10: production compact references

This qualifies the actual reader and both public builders, with opaque payloads
and unchanged input semantics. See [results](RESULTS.md), [format](../../FORMAT.md),
and the [raw receipt](../results/2026-11-05-compact-references.json).

Build the production examples with Rust 1.95.0, default release settings and all
features. No PGO, LTO override or custom CPU flags were used:

```sh
cargo +1.95.0 build --locked --release --all-features --example rebuild --example reference_lookup
mkdir -p /tmp/netindex-build
taskset -c 20 target/release/examples/rebuild INPUT OUTPUT /tmp/netindex-build external
taskset -c 20 target/release/examples/reference_lookup OUTPUT
```

`rebuild` also accepts `external-legacy`, `memory`, and `memory-legacy`. Output
must not exist. It retains original insertion IDs, compares every rebuilt
assertion, target, payload and metadata value, and checks 100,005 all-match
queries. Its preparation retains an assertion vector; process peak RSS therefore
includes that vector, mapped inputs and verification. Build timing excludes
preparation, native decoding and fsync. It is not isolated builder heap usage.

`reference_lookup` measures full mmap validation, then warm scalar query loops
with runtime SIMD search enabled. It reports nine timing samples per workload,
each with 131,072 queries on these inputs. Callbacks consume ID, payload length,
first and last payload bytes. Queries use 16,384 sampled target starts or a fixed
uniform mixed IPv4/IPv6 sequence. Application payload decoding is excluded.
Files must remain immutable for the lifetime of either mapped reader.

Qualification used two independent build processes per input/mode and three
lookup processes per input/reader/encoding. Lookup order alternated; the file
cache was warmed outside each timed process. Timing results are medians of
process medians, with raw samples retained. Comparisons include the unchanged
2.1.0 source at `552ab08` using the same lookup example, and the new reader on
both old and compact files. Hash the actual sources, executables and inputs.

Retained local files are in `/root/.cache/netindex-compact-20261105/` in WSL:

- `source/`, `baseline/`: actual build sources and binaries.
- `outputs/`: files for all modes; final build rounds are 4 and 5.
- `logs/`: JSON lines, `/usr/bin/time -v`, full disassembly and fuzz logs;
  final lookup rounds are 6, 7 and 8. Earlier rounds are superseded local evidence.

The unchanged inputs are in
`/root/.cache/netindex-perf-20261105/payload-distribution-final/inputs/`:
`city-current.nidx` and `asn.nidx`. Their provenance is recorded by P1-09.
Provider data is not committed.

Run the independent verifier and receipt collector against those files:

```sh
python3 benches/compact_references/verify.py \
  /root/.cache/netindex-compact-20261105 \
  /root/.cache/netindex-perf-20261105/payload-distribution-final/inputs \
  benches/results/2026-11-05-compact-references.json
```

It independently parses both encodings, checks exact target/ID/metadata/pool
bytes and every insertion ID's selected span, verifies first-seen ordinals,
and requires all modes and repeats to produce byte-identical files within each
encoding. It also requires identical query digests across both readers and
encodings. Original 2.1.0 rejection of format 4 is checked explicitly.
