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
