# Benchmarks

Two Criterion targets cover the routine regressions. Both generate their own
inputs and check complete results before timing.

| Target | Workload |
| --- | --- |
| `index` | Warm exact lookup and validation for IPv4, wide IPv6 and ASN; one in-memory IPv4 build |
| `queries` | Membership, longest prefix, intersection and mixed batch queries over overlapping prefixes and ranges, duplicate targets, exact ASNs and misses |

```sh
cargo +1.95.0 bench --locked --bench index --bench queries -- --test
cargo +1.95.0 bench --locked --bench index --bench queries -- --save-baseline before
# After the change, use the same machine and build flags:
cargo +1.95.0 bench --locked --bench index --bench queries -- --baseline before
```

CI runs the correctness command with default features and with `--features simd`.
To measure one operation, pass a name filter after `--`, for example
`cargo +1.95.0 bench --locked --bench index -- lookup`.

Pin to the same logical CPU on a quiet host. Keep inputs, feature flags, and
toolchain fixed, and repeat separate processes before calling a small change a
regression. Lookup throughput counts input queries, including misses.
Validation starts with bytes in memory; build includes dropping the encoded
artifact. Warm throughput does not measure per-query tails or cold startup.
See Criterion's [timing loops](https://bheisler.github.io/criterion.rs/book/user_guide/timing_loops.html)
for setup and teardown boundaries.

The optional [competitor harness](competitors/README.md) checks identical
longest-prefix or Boolean membership answers against a trie or bitmap. The
[public benchmark guide](../BENCHMARKS.md) explains the measured tradeoffs.
Larger studies, dataset-dependent harnesses and raw receipts are recoverable
from the [historical benchmark snapshot](https://github.com/0xARYA/netindex/tree/1ea02fa6852bf3ab2f299278fdc2faf05d7e3b47/benches).
