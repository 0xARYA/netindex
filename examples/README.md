# Examples and artifact tools

Start with the [README quick start](../README.md) for building and querying
an index. The Tokio and compio examples show how to share one validated
reader with blocking workers while keeping at most two batches outstanding:

```sh
cargo run --locked --example tokio
cargo run --locked --example compio
```

The other Cargo examples are reproducible artifact tools. They use release
builds for measurement and print JSON to standard output.

| Tool | Required features | Purpose |
| --- | --- | --- |
| `dedup` | `external-sort` | Compare external and in-memory output for synthetic duplicate payloads. |
| `rebuild` | `external-sort,mmap` | Replay an index and check metadata, assertions, and sampled queries. |
| `reference_lookup` | `mmap` | Measure validated opening and warm lookup for sampled targets and uniform IPs. |
| `share_aliases` | `mmap` | Fold declared, fully materialized IPv6 alias windows and verify the result. |

For example, run `cargo run --locked --release --features mmap --example
reference_lookup -- INPUT.nidx`. Keep files immutable while mapped; use
private, stable copies. Rebuild and alias folding require a new output
path. See [BENCHMARKS.md](../BENCHMARKS.md) and the
[alias study](../benches/shared_aliases/README.md) for measurement commands.
