# Contributing to netindex

Keep provider schemas, application codecs, runtimes, and release policy outside the
library. Use rustfmt, focused functions, and narrow visibility. Comments explain
contracts or non-obvious constraints; names and code should explain the rest.

Group imports as standard library, dependencies, then local items, before module
declarations. Keep types beside their implementations, constructors first, entry
points before helpers, and private unit tests last.

Separate setup, validation, conversion, and output into readable paragraphs.
Apply the same grouping to tests. Review these breaks manually: rustfmt handles
wrapping and indentation, not paragraph structure.

Preserve underlying error causes and return typed failures rather than panicking
in library code. Bound external input and allocation. Document unsafe operations
with their safety requirements. Tests should exercise observable behavior against
independent expectations, including failure paths.

## Checks

Run from the crate directory:

```sh
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 clippy --locked --all-targets -- -D warnings
cargo +1.95.0 clippy --locked --all-targets --all-features -- -D warnings
cargo +1.95.0 test --locked -- --test-threads=1
cargo +1.95.0 test --locked --all-features -- --test-threads=1
cargo +1.95.0 doc --locked --all-features --no-deps
cargo +1.95.0 package --locked --all-features
```

Set `RUSTDOCFLAGS="-D warnings"` when checking documentation. Packaging requires
a clean working tree; use `--allow-dirty` only for local verification.

Use `?` in documentation examples and assert observable results. Put ownership
and failure contracts on the relevant API; link examples rather than repeating
them on every method.

Check overlaps, duplicates, original targets, borrowed payloads, resource limits,
failed I/O, and scalar/SIMD parity. Compare queries with independent expectations
from the original input. Keep fixed format fixtures independent of the writer.
The [fuzz package](fuzz/README.md) runs on Linux with AddressSanitizer.

## Benchmarks

See [benchmarks](BENCHMARKS.md) for workloads and reproduction.
The Criterion harness below tracks changes within netindex:

```sh
cargo bench --bench index -- --save-baseline NAME
cargo bench --bench index -- --baseline NAME
cargo bench --bench index -- --test
```

`--test` runs the correctness prechecks and each benchmark once; CI uses it
without collecting timings.

Capture the baseline before editing. The harness measures warm lookup, complete
validation, and building, using 65,536 assertions per scenario and 1,024 queries
per lookup batch. `lookup-json` includes lookup, decoding each eight-byte scalar
payload, allocating all matching records, and serializing `{id, value}` objects
with serde_json. It includes misses and overlaps, but excludes HTTP and networking;
the schema is a benchmark fixture, not a library codec.

Run comparisons serially on a fixed CPU core with consistent
build settings and machine load. Record input hashes, executable, toolchain,
hardware, and measurement scope. Warm library timings do not establish HTTP
capacity or cold-file behavior.

## Format changes

Update [FORMAT.md](FORMAT.md) when encoded bytes or validation rules change.
The reader and writers implement one layout. Update both and the independent
fixtures together when changing it. Keep record IDs, original targets, and every
assertion intact when optimizing encoding.

Rust 1.95 is the supported toolchain; a lower MSRV needs independent verification.
Attach dated measurements and input provenance to changes that make performance
claims.
