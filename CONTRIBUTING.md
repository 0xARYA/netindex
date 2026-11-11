# Contributing to netindex

Keep provider and application schemas, runtimes, and release policy outside the
library. The optional codec supplies encoding mechanics. Use rustfmt, focused
functions, and narrow visibility. Comments explain contracts or non-obvious
constraints; names and code should explain the rest.

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

## Code organization

- Keep one public path for each API. Keep file validation, querying, and artifact
  rewriting in their respective reader modules. The physical section owns
  entry traversal; alias projection and query policy build on it.
- Separate in-memory construction from the external builder's temporary files.
  Keep spill records and disk tables private. `FORMAT.md` describes published
  bytes; temporary formats belong beside their encoders and decoders.
- Keep coverage routing across sources distinct from direct routes within one
  reader. Name modules after the behavior they own and introduce a helper when
  it removes duplication, owns a resource, or makes a real boundary clearer.
  Split files when they mix responsibilities, rather than by line count.
- Put public contract tests in `tests/`, private unit tests beside the code, and
  independent encoded fixtures in `tests/fixtures/`. Examples demonstrate public
  use; benchmark harnesses and dated evidence live in `benches/`. Raw result
  receipts under `benches/results/` stay in Git and are excluded from the crate
  package.

Review an ordinary call and a failure end to end. Check who validates, owns
resources, and reports errors. Look for repeated logic and unnecessary layers;
use rustfmt for layout mechanics and review statement grouping separately.

## Checks

Run from the crate directory:

```sh
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 clippy --locked --all-targets -- -D warnings
cargo +1.95.0 clippy --locked --all-targets --all-features -- -D warnings
cargo +1.95.0 test --locked -- --test-threads=1
cargo +1.95.0 test --locked --all-features -- --test-threads=1 --include-ignored
cargo +1.95.0 doc --locked --all-features --no-deps
cargo +1.95.0 package --locked --all-features
```

Set `RUSTDOCFLAGS="-D warnings"` when checking documentation. Packaging requires
a clean working tree; use `--allow-dirty` only for local verification.

Feature-dependent [usage guide](GUIDE.md) examples use hidden feature guards.
Ordinary all-feature doctests exercise them; the mapped-file example is compiled without
opening a local file. When publishing a new major version, update the README
quick start and its versioned documentation link with the release.

The [Cargo examples](examples/README.md) list distinguishes runnable integration
examples from the artifact tools used in benchmark reproduction. Declare each
tool's required features in Cargo so a missing feature fails before execution.

Use `?` in documentation examples and assert observable results. Put ownership
and failure contracts on the relevant API; link examples rather than repeating
them on every method.

Check overlaps, duplicates, original targets, borrowed payloads, resource limits,
failed I/O, and scalar/SIMD parity. Compare queries with independent expectations
from the original input. Keep fixed format fixtures independent of the writer.
The [fuzz package](fuzz/README.md) runs on Linux with AddressSanitizer.

## Benchmarks

See the [benchmark guide](BENCHMARKS.md) for workload comparisons and the
[benchmark reports](benches/BASELINES.md) for reproduction.
The Criterion harness below tracks changes within netindex:

```sh
cargo bench --bench index -- --save-baseline NAME
cargo bench --bench index -- --baseline NAME
cargo bench --bench index -- --test
```

`--test` runs the correctness prechecks and each benchmark once; CI uses it
without collecting timings.

Capture the baseline before editing. The harness measures warm lookup, complete
validation, and building, including IPv6 deltas requiring 16 bytes. It uses
65,536 assertions per scenario and 1,024 queries
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
The reader and writers implement one layout. Keep both and the independent
fixtures consistent when changing it. Encoded layout changes also require a
package compatibility decision; fixture updates alone do not make them compatible.
Keep record IDs, original targets, and every assertion intact when optimizing encoding.

Rust 1.95 is the supported toolchain; a lower MSRV needs independent verification.
Attach dated measurements and input provenance to changes that make performance
claims.
