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

## Deployment qualification

On November 6, 2026, the standalone Byway City adapter ran on the production
collector host (KVM, Xeon Gold 5315Y, Ubuntu 24.04, ext4), pinned to virtual
CPU 3 at reduced scheduling priority. It used the same executable and three
immutable City artifacts as the Linux profile above. It did not load these
files into the running service.

The trace contains 3,278 chronologically ordered production sessions from
November 4–6 UTC with persisted IP-intel evidence and an observed socket
peer. It preserves repeated addresses: 1,101 peers were distinct, and 2,158
queries were IPv4. Each layout hit on all 3,278 trace entries. Both converted
layouts matched native MMDB on every normalized answer, including 8,118
additional boundary and alias checks. The raw trace stayed on the host and
was removed after the run.

| Measurement | Native MMDB | Shared aliases and compact references | Also bitpacked |
| --- | ---: | ---: | ---: |
| Cold startup and validate, s (range) | 5.53 (5.39–5.56) | 7.57 (7.42–7.63) | 7.86 (7.64–7.93) |
| Warm-file startup and validate, s | 5.38 | 7.44 | 7.71 |
| Reload with cold replacement, s (range) | 5.43 (5.40–5.53) | 7.53 (7.47–7.54) | 7.76 (7.74–7.89) |
| Peak reload RSS, MiB | 120.2 | 126.6 | 115.6 |
| Warm-file trace p95 / p99, µs | 2.38 / 3.02 | 2.06 / 2.55 | 1.93 / 2.30 |

Medians of three fresh processes per condition. Cold startup verified zero
resident Linux file pages before opening. Reload primed the old file, retained
its reader, and verified zero resident pages for a distinct, byte-identical
replacement. Hypervisor and device caches were not controlled. These times
cover the City adapter's file open and full validation, not service restart,
the entire provider set, or a concurrent production rollout. Lookup timings
include owned decoding and clock overhead; this all-hit trace does not
qualify miss latency.

Native MMDB opens and reloads faster. Netindex answered the observed trace
faster at the tail, and bitpacking had the lowest reload RSS. The warm/cold
startup gap was much smaller than the gap between formats, so profile
validation before pursuing another storage encoding. Keep bitpacking optional.

[Reproduction](README.md) · [Aggregate samples](../results/2026-11-06-p3-city-deployment.json)

## Validation follow-up

The follow-up compared netindex 3.0.0 at `19ac3e9` with the same netindex
revision in both builds. The only functional difference was in Byway's
native component loader, based on Byway `824205c3`. The original loader
decodes and validates every logical assertion, including aliases and shared
payloads. The candidate uses a temporary cache bounded to 65,536 entries
(1 MiB on these hosts). A hit requires the same immutable slice, including
its length. Network family, ordering, overlaps, and record count are still
checked for every row. Payloads with a network override are revalidated for
every target. A collision or allocation failure only causes more validation.
Neither file format nor lookup changed.

On Windows, pinned to CPU 20 of the Ryzen 9 9950X3D2, fresh processes
alternated baseline and candidate with already-read files. The medians
below cover three samples per cell with identical Rust 1.95.0 release
settings. The phase timers were placed around netindex's mapped open,
Byway metadata validation, and the logical record visit in both builds.

| Phase | Shared baseline | Shared candidate | Bitpacked baseline | Bitpacked candidate |
| --- | ---: | ---: | ---: | ---: |
| netindex mapped open and structure, ms | 140.67 | 140.09 | 181.25 | 183.07 |
| Byway metadata, ms | 1.16 | 1.15 | 1.18 | 1.14 |
| Byway logical record validation, ms | 3,341.68 | 983.66 | 3,568.03 | 1,153.90 |
| Total open, ms | 3,483.68 | 1,123.82 | 3,751.07 | 1,338.48 |

In a separate WSL2 baseline profile with frame pointers, `perf record`
sampled user CPU time during a bitpacked open followed by the standard
lookup workload. Flat samples put 22.68% in compact text decoding,
10.72% in UTF-8 validation, and 10.10% in the compact payload decoder
itself. Those are exclusive samples from one process, not elapsed phase
percentages. On Windows, both candidate artifacts matched native MMDB on
108,123 normalized lookups each.

On the same Ryzen host under Ubuntu 24.04 WSL2, both builds used identical
release flags and immutable copies on the Linux filesystem. Each cell is
the median of three fresh processes pinned to CPU 20, alternating the two
builds. The cold trials verified zero resident Linux file pages. Reload
kept the old reader alive and opened a distinct, hash-verified cold copy.
Host and device caches were not controlled.

| Measurement | Native MMDB | Shared baseline → candidate | Bitpacked baseline → candidate |
| --- | ---: | ---: | ---: |
| Warm open and validate, s | 2.64 | 3.71 → 1.18 | 3.96 → 1.34 |
| Cold open and validate, s | 2.68 | 3.78 → 1.18 | 3.88 → 1.35 |
| Cold replacement reload, s | 2.67 | 3.80 → 1.24 | 3.95 → 1.40 |
| Peak reload RSS, MiB | 120.1 | 126.4 → 126.8 | 115.4 → 115.8 |

Both candidates again matched all 108,123 normalized answers. In this
local benchmark, the change cut netindex cold validation and reload
time by 65–69% and put both layouts ahead of native MMDB at those
boundaries. This uses the fixed qualification corpus, not the private
production trace or deployment host; it does not establish whole-service
restart or rollout performance.

[Samples, cache checks, and provenance](../results/2026-11-06-city-validation.json).

### Reducing work in the record walk

With the payload cache in place, a second WSL2 sample showed that packed
delta reconstruction and target projection were also hot. Two changes
preserve the existing checks and artifact bytes:

- Byway compares the canonical network start against the prior network's
  saved end as a same-family integer, computing each end once.
- netindex slices each exact-bit delta column once per block when visiting
  entries, rather than taking the same checked slice for every row. Point
  lookup still uses its existing search path.

In three paired Windows runs per layout, keeping the previous 1 MiB cache,
a Byway prototype using `IpAddr` comparisons reduced median warm open from
1.121 to 0.977 s (shared) and 1.332 to 1.159 s (bitpacked). A separate
comparison of just the netindex
change reduced bitpacked warm open from 1.327 to 1.242 s; the structural
validation phase went from 181.5 to 138.2 ms. A 4 MiB cache alone was about
as fast as the Byway change, but did not consistently help when combined
with it. Keep the 1 MiB limit.

The `IpAddr` comparison did not consistently improve the Linux runs.
The final Byway code compares IPv4 as `u32` and IPv6 as `u128`.
In a focused Linux check with the earlier Byway code held constant,
slicing the column once reduced the bitpacked structural phase from
171 to 135 ms and total warm open from 1.373 to 1.297 s (five paired
processes on CPU 6). With the same-family integer comparison and the
column change together, a separate three-pair check on CPU 4 took
1.198 s versus 1.332 s for the original cached loader. These two
focused checks use different CPU affinities and cannot be subtracted
to estimate each change's contribution.

The final pair used the same Linux cache checks, files, CPU 20, and three
fresh processes per cell as the validation comparison above. Its baseline
already includes the 1 MiB Byway cache.

| Measurement | Native MMDB | Shared cached → final | Bitpacked cached → final |
| --- | ---: | ---: | ---: |
| Warm open and validate, s | 2.80 | 1.22 → 1.16 | 1.36 → 1.25 |
| Cold open and validate, s | 2.72 | 1.19 → 1.16 | 1.43 → 1.29 |
| Cold replacement reload, s | 2.77 | 1.24 → 1.17 | 1.43 → 1.31 |

The shared cold-open change was small in this run (2%); bitpacked cold
open improved by 10%. Both layouts matched native MMDB on all 108,123
queries each. An earlier prototype run was excluded when the required
cold cache check failed. The final run used a new Linux directory and
verified residency before all 45 trials.

## Collector host validation follow-up

On November 6, 2026, the standalone adapter compared released netindex
3.0.0 plus the original Byway loader against the netindex 3.0.1 code
change plus Byway's cache and overlap validation changes. Both builds
used the same Byway source revision as the local validation follow-up
and the same three artifact hashes as the earlier deployment qualification.
This measures the combined changes. The isolated netindex result is
reported in [Reducing work in the record walk](#reducing-work-in-the-record-walk).

The original trace file was deleted after its run. Requerying the same
production session window returned 1,965 eligible sessions (802 distinct
peers, 1,347 IPv4 queries), versus 3,278 originally. Both builds replayed
the same reconstructed trace, in order, preserving repeat addresses.
The addresses stayed on the collector host. The running collector service
did not load the test artifacts.

| Median on collector host | Native MMDB | Shared baseline → candidate | Bitpacked baseline → candidate |
| --- | ---: | ---: | ---: |
| Warm open and validate, s | 5.480 | 7.618 → 2.246 | 7.884 → 2.509 |
| Cold open and validate, s | 5.496 | 7.594 → 2.388 | 7.952 → 2.535 |
| Cold replacement reload, s | 5.562 | 7.715 → 2.349 | 7.953 → 2.508 |
| Warm trace lookup p95 / p99, µs | 2.72 / 3.45 | 2.23 / 2.67 → 2.22 / 2.64 | 2.25 / 2.71 → 2.18 / 2.60 |

Each cell is the median of three fresh processes on CPU 3 of the Xeon
Gold 5315Y host, at nice +10. The runner alternated converted builds
and verified Linux page residency before all 45 trials. Reload retained
the old reader while opening a distinct, byte-identical cold replacement.
Both builds and both layouts matched all 1,965 trace results and 8,118
additional alias and boundary queries. All trace lookups hit, so the
latencies do not establish miss or service-level performance.

The candidate reduced cold open and reload time by 68–70% compared with
the original converted loader, and finished both phases ahead of native
MMDB on this host. Most of the gain comes from the Byway changes; upgrading
the netindex crate alone will not deliver the combined result.

[Aggregate samples and provenance](../results/2026-11-06-city-validation-collector.json).
