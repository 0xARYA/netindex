# Contributing to netindex

Keep the library independent of provider schemas, payload codecs, runtimes, and
release policy. Use rustfmt and idiomatic Rust; keep functions focused, visibility
narrow, and comments limited to contracts and non-obvious constraints. Separate
setup, validation, conversion, and output into readable paragraphs.

Group imports as standard library, dependencies, then local items, before module
declarations. Keep types beside their implementations, constructors first, and
entry points before helpers. Put private unit tests last. Apply the same paragraph
grouping to test setup, operations, assertions, and helpers; review it manually
because rustfmt does not choose these boundaries.

Preserve underlying error causes and return typed failures rather than panicking
in library code. Bound external input and allocation. Document unsafe operations
with their safety requirements. Tests should exercise observable behavior against
independent expectations, including failure paths.

## Checks

Run from the crate directory:

```sh
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
cargo doc --all-features --no-deps
cargo package --all-features
```

Documentation examples are tested with the crate. Use `?` for fallible operations
and assertions for observable results. Put ownership and failure contracts on the
relevant API; link related examples instead of repeating them on every method.

The tests cover overlaps, duplicates, original targets, borrowed payloads,
validation, resource limits, failed I/O, and scalar/SIMD parity. Compare query
results with independent original-input oracles. Preserve fixed fixtures for
the encoded layout. The [fuzz package](fuzz/README.md) runs reader and writer
campaigns under AddressSanitizer on Linux; it is excluded from the library package.

## Benchmarks

```sh
cargo bench --bench index -- --save-baseline NAME
cargo bench --bench index -- --baseline NAME
```

Capture the baseline before editing. The harness measures warm lookup, complete
validation, and building, using 65,536 assertions per scenario and 1,024 queries
per lookup batch. Run comparisons serially on a fixed CPU core with consistent
build settings and machine load. Record input hashes, executable, toolchain,
hardware, and measurement scope. Warm library timings do not establish HTTP
capacity or storage-cold behavior.

## Format changes

Update [FORMAT.md](FORMAT.md) when encoded bytes or validation rules change.
The reader and writers implement one layout. Update both and the independent
fixtures together when changing it. Keep record IDs, original targets, and every
assertion intact when optimizing encoding.

Rust 1.95 is the supported toolchain; a lower MSRV needs independent verification.
Attach dated measurements and input provenance to changes that make performance
claims.
