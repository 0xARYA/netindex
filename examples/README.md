# Examples and artifact tools

Run these commands from the repository root. No downloads or input files are
needed for the first three examples.

- [README quick start](../README.md#quick-start): build an index in memory and
  retrieve both records that match the same IP.
- [Mapped file](mapped.rs): write a temporary `.nidx`, map it, and query an IP
  range and an exact ASN. It prints `203.0.113.42: maintenance` and removes the
  temporary file on exit.
- [Tokio](tokio.rs) and [compio](compio.rs): open and query off the async
  executor, share one reader, and finish every started batch before returning
  an error. Each example allows two outstanding jobs and exits silently on
  success.

```sh
cargo run --locked --features mmap --example mapped
cargo run --locked --example tokio
cargo run --locked --example compio
```

## Artifact tools

These commands use only synthetic inputs or checked-in fixtures. They print
JSON with timings and verification results. Use release builds for measurements.

```sh
cargo run --locked --release --features external-sort --example dedup
cargo run --locked --release --features mmap --example reference_lookup -- tests/fixtures/interval.nidx
mkdir -p target/example-artifacts
cargo run --locked --release --features 'external-sort,mmap' --example rebuild -- tests/fixtures/shared-alias.nidx target/example-artifacts/materialized.nidx target/example-artifacts
cargo run --locked --release --features mmap --example share_aliases -- target/example-artifacts/materialized.nidx target/example-artifacts/shared.nidx '2002::/16,1,1'
```

In PowerShell, use `New-Item -ItemType Directory -Force target/example-artifacts`
in place of `mkdir -p`. Both output paths must be new; choose fresh names on a
second run. Keep input and output files unchanged while the tools map them.
The last two commands expand the fixture's IPv6 aliases and fold them back
into a shared window. The tiny fixtures demonstrate the tools; use the
[benchmark guide](../BENCHMARKS.md) for representative measurements.
