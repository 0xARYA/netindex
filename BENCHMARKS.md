# Benchmarks

Run the maintained Criterion harness from the crate directory:

```sh
cargo bench --bench index
```

It uses 65,536 deterministic assertions per scenario and batches of 1,024 queries.
IPv4, IPv6, overlapping ranges, and exact ASN keys are checked against original
input before timing. Failed operations fail the benchmark process.

| Group | Includes |
| --- | --- |
| `lookup` | Visiting all matches and reading their IDs, targets, and payload lengths. |
| `lookup-json` | Lookup, eight-byte scalar decoding, collecting `{id, value}` records, and JSON serialization. |
| `open` | Complete index validation over warm backing bytes. |
| `build` | Ingestion, sorting, payload deduplication, and complete file encoding with shared or unique payloads. |
| `predecessor-model` | An address-directory experiment, separate from the public reader. |

`lookup-json` includes misses and all overlapping matches. Its response schema is
a benchmark fixture; applications choose their own payload codec. It excludes
HTTP, network transfer, and async task coordination.

## Compare changes

```sh
cargo bench --bench index -- --save-baseline before
cargo bench --bench index -- --baseline before
```

Use the same toolchain, release settings, machine load, and CPU affinity. Criterion
reports batch times; divide by query count for per-query averages. These are warm
measurements, not cold-page timings or request p95/p99.

The earlier MMDB comparisons used experimental alias routing that is not in the
public converter. Those figures are omitted here. No current comparative speed
or memory claim is made for MMDB conversion.
