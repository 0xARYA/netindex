# P3 consumer qualification

On November 6, 2026, the standalone adapter built and opened the retained
GeoLite2 City snapshot with the current Byway format crate, payload encoder,
normalizer, validator and lookup. The Byway snapshot was
`658d2339308cbcf20e5860daa08fc5f0e2409699`; the netindex baseline was `ab28f32`.
The comparison used the same executable for native and converted serving,
with native Windows processes pinned to CPU 20 on the Ryzen 9 9950X3D2.
This does not include the deployed service, networking, or production traffic.

| Artifact | Bytes | Warm-file open and validate, ms | Peak serving working set, MiB | Reload, ms | Peak reload working set, MiB | Lookup p95 / p99, ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Native MMDB | 49,899,266 | 2,739 | 88.2 | 3,015 | 152.3 | 1,200 / 1,500 |
| Compact references and shared aliases | 62,361,614 | 3,548 | 71.0 | 3,546 | 128.6 | 1,100 / 1,300 |
| Also bitpacked | 56,617,154 | 3,728 | 65.8 | 3,796 | 117.7 | 900 / 1,100 |

Medians of three fresh processes per case. Files were cache-primed; this is
not an OS-cache-cold result. Reload validates a distinct immutable, byte-identical
copy while retaining the previous reader. The native adapter owns its bytes and
the converted adapter maps them, so working-set differences include that choice.
These peaks are process memory observations, not system-wide cache accounting.

Each converted representation matched native MMDB on all 108,123 normalized
owned-record comparisons, including alias queries and boundaries. Serving used
the same deterministic 100,005 IPs per run, with 42,802 hits. All p50 values
were 100 ns. Individual timings include clock overhead and have visible
100 ns quantization; the observed tail differences do not establish production
latency or negate the earlier bare-kernel bitpacking regressions.

The native conversion and full validation took 71.38 s in one process, with
1,965,616,403 peak temporary bytes. The compact-reference intermediate was
164,836,510 bytes. Alias folding reduced physical assertions from 20,255,722 to
5,724,698 while preserving all logical IDs, targets, payloads and metadata.
The alias tool compared 150,000 complete queries; the bitpacking replay checked
all 20,255,722 assertions and another 100,005 complete queries. These conversion
and rewrite numbers are separate from serving and exclude durable sync.

For this consumer and fixture, format 5 is a viable integration candidate:
it is about 25% larger than native MMDB but used about 16% less process working
set during reload, at a slower open. Bitpacking reduced peak reload working
set by a further 8.5% relative to format 5 and increased warm-file open time
by about 5%. Keep bitpacking optional. Before a production rollout, replay a
real request trace and measure verified cold cache behavior on the deployment
host. The Byway validator visits logical assertions, so reducing physical rows
does not proportionally reduce consumer validation work.

[Reproduction](README.md) · [Raw receipts and provenance](../results/2026-11-06-p3-city.json)

## Storage and lookup cache profile

The same immutable files were profiled on Ubuntu 24.04 under WSL2 on the Ryzen
9 9950X3D2, with processes pinned to virtual CPU 20 and Rust 1.95.0. The Linux
adapter used the same Byway sources. Each cell is the median of three fresh
processes with rotated case order. The fixed 100,005 queries produced 42,802
hits per run. A separate parity run for each converted artifact matched all
108,123 normalized answers on Linux.

| File region | Native MMDB | Shared aliases and compact references | Also bitpacked |
| --- | ---: | ---: | ---: |
| Search tree or IP index | 29,146,299 | 24,696,954 | 18,952,494 |
| Separate payload reference region | — | 19,344,302 | 19,344,302 |
| Record data or opaque payloads | 20,752,708 | 16,726,691 | 16,726,691 |
| Header, metadata, and separator | 259 | 1,593,667 | 1,593,667 |
| **Total bytes** | **49,899,266** | **62,361,614** | **56,617,154** |

The native tree includes pointers to data and its data section can reuse
individual fields. The converted files preserve assertions and insertion IDs
and store separate references to complete normalized payloads. The regions
are therefore useful for byte accounting, not equivalent encodings of the
same fields. See the [MMDB format specification](https://maxmind.github.io/MaxMind-DB/).

| Measurement | Native MMDB | Shared aliases and compact references | Also bitpacked |
| --- | ---: | ---: | ---: |
| Open and validate with all file pages cached, s | 2.89 | 4.03 | 4.18 |
| Open and validate from zero resident file pages, s (range) | 6.66 (2.96–14.80) | 4.20 (4.08–10.00) | 20.31 (4.34–23.79) |
| Lookup-only user-space cache misses/query | 3.33 | 6.85 | 6.69 |
| Lookup-only user-space cycles/query | 1,927 | 1,847 | 1,870 |
| Lookup-only p95 / p99, ns | 1,139 / 1,439 | 1,010 / 1,309 | 1,030 / 1,310 |

`mincore` verified zero resident file pages before every cold run and all pages
resident before every warm run. The WSL host and underlying device caches were
uncontrolled; the wide cold ranges cannot rank deployment startup. Open
includes the production consumer's full validation. The Linux `perf` generic
user-space cache and cycle events ran without multiplexing during the lookup
loop. They do not attribute misses to specific file regions. The converted
paths incurred about twice the recorded misses but completed the timed loop
with slightly fewer cycles; miss counts alone did not predict lookup time.

The converted files use three bytes per ordinal for 271,275 unique payloads.
Using 19 bits for each of 5,724,698 physical references would save 3,577,936
bytes before any additional format overhead. Even with bitpacked IP columns,
that alone would leave a 3,139,952-byte gap to native. Keep the format
unchanged. Warm validation is the larger measured consumer cost; qualify it
and replay production traffic on the deployment host before trading more
decode work for storage. If the storage target justifies another prototype,
start with a schema-neutral ordinal column and require an end-to-end win.

[Linux reproduction](README.md) · [Raw profile and provenance](../results/2026-11-06-p3-city-profile.json)
