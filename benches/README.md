# Benchmarks and performance evidence

Production benchmarks remain active: `index`, `search`, `scale`, `traffic`,
`shapes` and optional `codec`. Run them through the root Cargo manifest as
described in [BENCHMARKS.md](../BENCHMARKS.md).

| Supporting tools | Purpose |
| --- | --- |
| `competitors/` | Matched-contract external baselines |
| `concurrent/` | Reader concurrency and snapshot behavior |
| `memory/` | Memory-pressure and working-set qualification |
| `native_city/` | Application-owned native/conversion comparison |
| [Payload distribution](payload_distribution/README.md) | Evidence and verification for compact-reference implementation |
| [Compact references](compact_references/README.md) | Production format-4 qualification, independent verification and migration tradeoffs |
| [Shared aliases](shared_aliases/README.md) | Exact City replay and warm format-5 qualification |
| `results/` | Immutable measurement and verification receipts |

The completed P1-01–P1-08 prototype code is [archived](ARCHIVE.md). Their report
directories remain as evidence and recovery pointers; their harnesses are not
part of the current maintained tree. No production code depends on them.
