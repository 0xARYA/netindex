# Index fuzz checks

This separate package pins fuzz dependencies and keeps libFuzzer out of the library.

`read` validates bounded arbitrary file bytes, then compares IP and ASN searches
against a linear scan of accepted assertions. It checks assertion endpoints,
address extrema, additional queries derived from input bytes, and conservative
coverage routing. Fixed interval, packed IPv4, packed IPv6, and mixed fixtures
seed the corpus.

`encode` generates up to 128 mixed targets with arbitrary canonical prefix lengths,
IPv4/IPv6 inclusive ranges, and shared payloads. It checks insertion IDs, targets,
and bytes after a complete writer/reader roundtrip, then compares IP and ASN
lookups against original-input assertions. Both enable runtime SIMD where available;
core unit tests independently force scalar search even on AVX2-capable machines.

Run on Linux, including WSL, with cargo-fuzz 0.13.2 and the pinned nightly toolchain:

```sh
mkdir -p fuzz/corpus/read
cp tests/fixtures/*.nidx fuzz/corpus/read/
cargo +nightly-2026-09-01 fuzz run read --fuzz-dir fuzz -- -max_total_time=60 -max_len=65536
cargo +nightly-2026-09-01 fuzz run encode --fuzz-dir fuzz -- -max_total_time=60 -max_len=4096
```

These commands enable AddressSanitizer by default. Windows MSVC coverage linking
failed with this toolchain; use Linux rather than disabling instrumentation.
CI runs 60-second smoke campaigns. Use longer runs for deeper coverage.

The `values` target checks shared-pool parsing, typed UTF-8 access, bounds, and
writer deduplication. Seed and run it separately:

```sh
mkdir -p fuzz/corpus/values
cp tests/fixtures/values.bin fuzz/corpus/values/
cargo +nightly-2026-09-01 fuzz run values --fuzz-dir fuzz -- -max_total_time=60 -max_len=65536
```

All three targets run in Linux CI.
