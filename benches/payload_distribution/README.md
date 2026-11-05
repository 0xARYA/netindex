# P1-09: opaque payload distribution

The original P1-09 measurements used the unmodified 2.1.0 library. The current
analyzer also accepts format-4 files; the historical Python verifiers remain
pinned to the original format-3 snapshots. Use
[the compact-reference verifier](../compact_references/README.md) for new outputs.
Provider schemas live only in this adapter. Raw provider data stays in ignored/private
directories; only aggregate statistics and provenance are committed.

Measure the retained real City conversion before/after full deduplication and
an independent real IPtoASN combined IPv4/IPv6 snapshot:

```sh
python3 benches/payload_distribution/fetch_asn.py target/payload-asn-source
python3 benches/payload_distribution/run.py --output /tmp/payload-distribution --asn-source target/payload-asn-source --cpu 20
```

The default City paths are the P0 retained `target/city-study/original.ipidx` and
`rebuilt-2.1.0-1.nidx`; override with `--city-original` / `--city-current`. Output
and download directories must be fresh. Select an allowed Linux CPU explicitly.
The runner hashes private copies before/after two fresh-process passes, reverses
case order, captures maximum RSS and wall time, and pins the analyzer to one CPU.
Rust 1.95.0 portable release, one codegen unit, no PGO/LTO/custom CPU flags.

The analyzer first validates the complete index using the production reader.
It interprets insertion-order payload references independently, crosschecking
every reader payload and borrowed address. Hash-map equality compares complete
payload bytes; a hash collision cannot merge different values. Both old repeated
physical spans and new shared spans are accepted.

Reported quantities:

- Exact distinct payload count, reference-weighted and distinct-value-weighted
  lengths, frequency distribution, singleton values, logical/unique/physically
  stored byte counts, and per-family sharing. Empty payloads are valid.
- Insertion-ID repeat gap; number of distinct intervening values; and distinct
  intervening payload bytes plus the repeated payload's length. These are
  construction-stream observations, not serving-traffic locality.
- Logarithmic histograms report inclusive bounds, not exact reuse quantiles.
  Counts within five payload-byte footprint thresholds are exact. They exclude
  cache entry/table overhead and are not a cache hit-rate simulation.
- A separate exact first-seen, non-evicting 4 MiB cache admission model charges
  `length + 64`, matching the current external builder's key accounting. First
  occurrences miss; uncached repeats miss; disk fallback hits are not modeled.
- A P1-10 arithmetic reference-size estimate uses minimum whole-byte IDs and an
  eight-byte directory per distinct value, excluding headers, validation state,
  lookup cost and compatibility. This is not an implemented format proposal.

The reuse tree has count and byte-weight arrays of `N + 1` elements, 12 bytes per
record. Borrowed payload keys avoid copying the pool. The 512 MiB workspace limit
covers a conservative modeled table allowance and exact array capacities; it
does not bound mapped-file residency, allocator overhead, histograms, reader
validation state or total RSS. Inputs are capped at 30M records / 2 GiB.

IPtoASN is downloaded from its [official site](https://iptoasn.com/), which lists
PDDL 1.0. The adapter preserves all source ranges, including ASN zero, and frames
each payload as version byte, ASN u32, country-length u16, country UTF-8 and the
remaining description UTF-8. It verifies every built target/ID/payload against
the original framed source. No schema is installed in the library. Downloaded
snapshots change; retain the archive, framed rows and receipt to reproduce a run.

Native runs and distributions are documented in [RESULTS.md](RESULTS.md).
For retained output:

```sh
python3 benches/payload_distribution/verify.py /tmp/payload-distribution --output /tmp/distribution.json
python3 benches/payload_distribution/verify_sources.py /tmp/payload-distribution
python3 benches/payload_distribution/verify_independent.py /tmp/payload-distribution --output /tmp/independent.json
```

The independent Python verifier scans every reference with its own slot parser,
byte-keyed counters, reference-gap histogram and payload-only LRU implementation.
It checks LRU equivalence only after requiring individual payloads to fit 4 MiB.
The distinct-intervening tree's independent naive oracle is in the Rust tests.
Source verification permits Windows/Linux line-ending differences only. Use the
measurement revision recorded in the results if later library sources change.
