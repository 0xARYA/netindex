# netindex

Read and write immutable IP-range and exact-ASN indexes. Queries return every
matching assertion, including overlaps and duplicates, with its target and
borrowed payload bytes. Use a membership query when you only need a yes/no
answer, or longest-prefix lookup when you need one address or CIDR.

## Use cases

Use netindex for datasets built periodically and queried repeatedly: membership
feeds, network policy, and IP intelligence. It accepts unsorted IPv4 and IPv6
addresses, canonical CIDRs, inclusive ranges, and nonzero ASN keys. Payloads and
metadata are opaque; applications choose their schema or use the optional codec.

Readers use owned bytes, borrowed slices, or immutable mappings. Writers build
in memory or with bounded external sorting. Disjoint networks use compact blocks;
overlaps use an interval index. Identical payloads share bytes without merging
assertions.

Licensed under [MIT](https://github.com/0xARYA/netindex/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/0xARYA/netindex/blob/main/LICENSE-APACHE), at your option.

## Quick start

This guide covers the **unreleased 3.0 API**. To try it now, point Cargo at a
checkout of this repository (adjust the path for your project):

```toml
[dependencies]
netindex = { path = "../netindex" }
```

For the published 2.x release, use `netindex = "2"` and its
[versioned API documentation](https://docs.rs/netindex/2.1.0/netindex/).
For this checkout's API reference, run `cargo doc --all-features --no-deps --open`.
See [upgrading from 2.x](#upgrading-from-2x) before replacing deployed readers
or rebuilding files.

```rust
use netindex::{Builder, Limits, Reader, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Builder::new(Limits::default());
    builder.push(
        Target::Network {
            address: "203.0.113.0".parse()?,
            prefix: 24,
        },
        b"evidence",
    )?;

    let bytes = builder.into_bytes(b"release metadata")?;
    let reader = Reader::open(bytes, Limits::default())?;

    let matches = reader.lookup_ip("203.0.113.42".parse()?)?;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].payload, b"evidence");

    Ok(())
}
```

Index files use `.nidx`, for example `membership.nidx`.

## Reading

Open an index with `Reader::open`. Opening validates the entire file. Keep
the backing immutable; payload bytes remain opaque until your application
decodes them.

| I want to… | Use | Result |
| --- | --- | --- |
| Find every match | `lookup_ip`, `lookup_asn` | A vector of borrowed matches; empty on a miss. |
| Test whether any match exists | `contains_ip`, `contains_asn` | A boolean; does not resolve payloads. |
| Find the most specific address or CIDR | `longest_prefix` | One borrowed match or `None`; ignores ranges. |
| Process matches without a result vector | `visit_ip`, `visit_asn`, `visit_all` | A callback for each match; a callback error stops delivery. |
| Stop successfully when a callback finds what it needs | `try_visit_ip`, `try_visit_asn` | `ControlFlow::Break(value)` carries the answer. |
| Find records that intersect an IP range | `visit_intersecting` | A callback for each whole original target that intersects the query. |
| Process a mix of queries in input order | `visit_batch`, `try_visit_batch` | Callbacks with the input query position. |

ASN methods match an exact number, with zero treated as a miss. They do not
resolve an IP's ASN. IDs reflect insertion order, but matches within a query
need not arrive in that order.

A visitor can decode its payload and return an application error. Its error
type must implement `From<netindex::Error>`. If the callback itself cannot
fail, return `Ok::<_, netindex::Error>(())`.

```rust
use netindex::{Builder, Limits, Reader, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = "203.0.113.42".parse()?;
    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Address(address), b"evidence")?;
    let reader = Reader::open(builder.into_bytes(b"")?, Limits::default())?;

    reader.visit_ip(address, |matched| {
        let text = std::str::from_utf8(matched.payload)?;
        assert_eq!(text, "evidence");

        Ok::<_, Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}
```

## Batches, membership and intersections

`Reader::longest_prefix` returns the most specific matching CIDR or address
(treated as a host prefix). It ignores arbitrary ranges and breaks equal-prefix
ties by lowest logical assertion ID. Use `visit_ip` when you need all matching
assertions, including ranges.

`Reader::visit_batch` accepts borrowed `Query` values and calls the visitor with
its input query position and each borrowed match. Delivery follows input order,
then normal per-query ordering; duplicate queries run independently. There is no
output buffer, reordering, thread pool or automatic cross-query SIMD policy.
`BatchError` identifies the failed query and retains the original error. Earlier
callbacks have already run; later queries do not run. Callers own output storage
and any rollback policy.

The `try_visit_*` methods preserve visitor errors separately from a successful
break. `visit_intersecting` accepts an address, canonical CIDR, or inclusive
range. It searches
only that address family, preserves duplicates and sorted target order, and
includes shared aliases. Invalid queries fail before delivery.

```rust
use std::{net::Ipv4Addr, ops::ControlFlow};
use netindex::{Builder, Error, Limits, Query, Reader, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = Ipv4Addr::new(203, 0, 113, 42).into();
    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Address(address), b"example")?;
    let reader = Reader::open(builder.into_bytes(b"")?, Limits::default())?;
    let queries = [Query::Ip(address), Query::Intersecting(Target::Network {
        address: Ipv4Addr::new(203, 0, 113, 0).into(), prefix: 24,
    })];
    let mut results = Vec::new();
    reader.visit_batch(&queries, |query, row| {
        results.push((query, row));
        Ok::<_, Error>(())
    })?;
    assert_eq!(results.len(), 2);
    assert!(reader.contains_ip(address)?);
    let first = reader.try_visit_ip(address, |row| {
        Ok::<_, Error>(ControlFlow::Break(row.id))
    })?;
    assert_eq!(first, ControlFlow::Break(0));
    assert_eq!(reader.inspect()?.records, 1);
    Ok(())
}
```

`Reader::inspect` reports physical/logical counts, section encodings, exact region
sizes, bitpacked block counts and allocated routing/alias capacities. It scans
packed descriptors, allocates nothing and never decodes payloads. Backing length,
reader object size, auxiliary allocations and released validation scratch are
separate measures; they are not an estimate of process RSS or mapped residency.

## Separate source files

`ReaderSet` also supports batch, membership, intersection and early-stop methods.
Batch callback positions are query then source. Each query visits sources in
source order; interval queries search all sources because point coverage masks
cannot safely exclude a whole range.

`ReaderSet` shares up to 64 readers and uses IP coverage to skip files that
cannot match. It returns every matching assertion with its source position and
source-local record ID. ASN queries search every reader. Schema decoding and
provider precedence remain application decisions.

The default router uses 64 KiB; each reader retains a 1 KiB coverage summary.
`ReaderSet::with_fine_routing` uses a 1 MiB table and can skip more files; measure
its extra cache footprint against your workload. Both preserve all matches.

`replaced(position, reader)` creates a new set sharing unchanged readers. Coarse
routing reuses coverage cached during validation; fine routing scans only the
replacement. Old sets and borrowed matches remain usable; the application publishes
the new set. Use `CoverageRouter` to manage readers separately.

## Memory-mapped files

Add `features = ["mmap"]` to your dependency for `MappedReader::map_file`.
It checks file-size limits before mapping, validates the index, and owns the
mapping independently of the file handle.

```rust,no_run
# #[cfg(feature = "mmap")]
# {
use std::fs::File;

use netindex::{Limits, MappedReader};

let file = File::open("membership.nidx")?;
// SAFETY: No process modifies or truncates this file while the reader is alive.
let reader = unsafe { MappedReader::map_file(&file, Limits::default()) }?;
let matches = reader.lookup_ip("203.0.113.42".parse()?)?;
println!("{} matching entries", matches.len());
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

Mapping is unsafe because changes to the file by any process can invalidate
borrowed memory. Publish replacements as separate files and retain old readers
until their requests finish. `mapping()` exposes the read-only mapping for
platform access hints; no preloading or advice is applied by default.

For broad, disjoint networks, `Reader::open_with_broad_routes` or
`MappedReader::map_file_with_broad_routes` adds direct routes during validation.
This uses up to 32 KiB per eligible address family and preserves original IDs and
payloads. Other queries use ordinary lookup; overlaps are never bypassed. Routing
is opt-in because its additional check can slow narrow-network queries.

## Building

`Builder::into_bytes` returns owned bytes; `Builder::write_to` writes to a
caller-owned output. Metadata is stored once as opaque bytes.

Ordinary builders choose compact shared-payload references when they save
space. If a deployed reader is still on 2.x, see [upgrading from 2.x](#upgrading-from-2x)
before writing new artifacts.

Enable `external-sort` for `ExternalBuilder` when buffering all assertions would
exceed your budget. `ExternalOptions` bounds sorted runs, the payload cache, and
temporary-disk usage. Insertion IDs are preserved. Identical payloads share their
first-seen span, including when the memory cache is full or disabled. Uncached
values use a disk-backed hash table with full byte comparisons in bounded chunks;
the cache only accelerates matching. This trades random temporary-file I/O for
complete deduplication. Table growth charges both old and new tables against the
temporary-byte limit; builds may require a larger disk budget than earlier versions.
Discard the builder after a failed push.

Call `ExternalBuilder::set_disk_table_cache(bytes)` before pushing rows to
optionally cache the temporary table. This budget includes cache pages and
tags, separate from the payload-key and disk limits. The cache is off by
default; `disk_table_cache_bytes()` reports what was allocated. It does not
change output bytes or skip complete payload comparisons.

`ExternalBuilder::new_partitioned(directory, limits, options, partitions)` is an
opt-in alternative when extra table-cache memory is unavailable. It stages values
into 1-64 power-of-two partitions, reuses the payload-key budget for one partition
at a time, and restores first-seen payload order and insertion IDs. Complete byte
comparisons remain the fallback when a partition exceeds the key budget. Extra
sequential passes and temporary disk can make this slower than the ordinary
builder; measure both. Fixed I/O buffers and open-file counts grow with the
partition count outside the key budget. Nonzero table caches are unsupported,
and cumulative artifact-byte limits can fail during finalization before output.
See [partitioned-build measurements](https://github.com/0xARYA/netindex/blob/main/benches/partitioned_dedup/README.md).

The external writer requires empty, seekable output at position zero. Either
writer can leave partial output on failure. Stage output, check success, and
validate your payloads before publishing. File synchronization and release
activation belong to your application.

### Exact-bit compaction

`Reader::write_bitpacked_to(output)` optionally compacts packed IP deltas into
format 6. It accepts the output of either builder, including shared-alias files,
and preserves every assertion, ID, target, payload and metadata byte. It uses
fixed block buffers and returns the written size. When no space is saved, it
copies the original bytes. Ordinary builders continue producing formats 3/4.

Upgrade readers before using this output: readers without format-6 support
reject it, including inside alias envelopes. Exact-bit access trades some
validation and lookup work for smaller files; measure your workload. See
[FORMAT.md](https://github.com/0xARYA/netindex/blob/main/FORMAT.md) and
[bitpacking measurements](https://github.com/0xARYA/netindex/blob/main/benches/bitpacking/RESULTS.md).
Stage output and check success before publishing; output errors can leave partial
bytes.

### Shared IPv4 aliases

For an index whose complete IPv4 network section is repeated under explicit
IPv6 prefixes, `Reader::write_shared_aliases` can wrap the base in a format-5
artifact. The reader projects targets, shares borrowed payloads and preserves
logical insertion IDs without copying the IPv4 rows. Windows must be disjoint
from each other and from native IPv6 targets; overlaps and duplicates among
source IPv4 networks still produce separate matches. The source IPv4 IDs must
start at zero in sorted order. See
[the format rules](https://github.com/0xARYA/netindex/blob/main/FORMAT.md#shared-ipv4-aliases-format-5).

This is opt-in. Ordinary builders still write formats 3/4, and readers that
predate format 5 cannot open the shared artifact. The `share_aliases` example
folds explicitly declared windows from an existing file only after verifying
the projected targets, IDs, and payloads against the originals.

## Shared values

Enable `shared-values` to share strings or byte values across records.
`ValuePoolBuilder::intern` returns an ID for your record layout to store. Keep the
encoded pool in metadata or a separate file and activate it with its records.
Schemas, null handling, and numeric encoding remain yours.

`ValuePool` borrows arbitrary bytes. `StringPool` validates UTF-8 once and borrows
strings without rescanning. Both pool readers allocate no heap memory. IDs are
local to a pool and must not be reused with another release.

## MMDB conversion

Enable `mmdb` for `mmdb::visit_networks`. It validates the MMDB and visits one
network at a time. Your callback selects and encodes fields for either builder,
or skips a record. Input needs random-access bytes or a mapping; a sequential
download stream is insufficient.

Iteration options control aliases, empty records, and networks without data.
Include aliases to preserve native alias targets. The canonical `::/96` subtree
is emitted as IPv4; native-equivalent IPv6 lookups there need additional IPv6
targets or application routing. Discard the partial build on conversion failure;
earlier pushes are not rolled back. See `mmdb::visit_networks` for examples.

## Memory and concurrency

Default limits are one million assertions and 128 MiB per file. These bound
logical records and file bytes, not RSS. Validation state, allocator overhead,
caller buffers, and the OS file cache use additional memory. Large packed IPv4
sections add a lookup directory of up to 32 KiB per reader. Each reader retains
a 1 KiB coverage summary collected during validation.

`Reader<B>` is `Send + Sync` when its backing is. Share a reader or reader set
through `Arc`; this does not copy database bytes. Each caller owns its visitor
and result storage. The crate is synchronous and creates no threads.

Parallelize independent queries or bounded batches. Opening, building, lengthy
lookups, and mapped-page faults can block runtime workers. The
[Tokio](https://github.com/0xARYA/netindex/blob/main/examples/tokio.rs) and [compio](https://github.com/0xARYA/netindex/blob/main/examples/compio.rs) examples use bounded
blocking workers and finish admitted work before returning a batch failure:

```sh
cargo run --example tokio
cargo run --example compio
```

## Record codec

Add `features = ["codec"]` to your dependency for flat typed records with
shared strings and bytes. Declare fields once, then encode records with that
schema:

```rust
# #[cfg(feature = "codec")]
# {
use netindex::{
    Builder, Error, Limits, Reader, Target,
    codec::{Decoder, Encoder, Field, Kind, Schema, Value},
};

let limits = Limits::default();
let schema = Schema::new(
    &[Field::Required(Kind::U32), Field::Optional(Kind::String)],
    limits,
)?;
let mut encoder = Encoder::new(schema, limits);
let payload = encoder.encode(&[Value::U32(64512), Value::String("example network")])?;
let metadata = encoder.into_metadata()?;

let mut builder = Builder::new(limits);
builder.push(Target::Asn(64512), &payload)?;
let reader = Reader::open(builder.into_bytes(&metadata)?, limits)?;
let decoder = Decoder::open(reader.metadata()?, limits)?;

reader.visit_asn(64512, |matched| {
    let record = decoder.record(matched.payload)?;

    assert_eq!(record.string(1)?, Some("example network"));

    Ok::<_, Error>(())
})?;
# }
# Ok::<(), netindex::Error>(())
```

Pass encoded records to the index builder as payloads and store codec metadata
with the index. `Field::Absent` omits a field known to be missing throughout the
dataset; `Field::Optional` preserves missing versus empty or zero. Scalars retain
their declared width; strings and bytes use four-byte pool IDs. The schema has no
provider-specific fields, nested objects, or implicit defaults.

Opening validates metadata and shared strings. `record()` checks the row shape;
field access checks values and references. Use `validate()` to check every field
before publishing. Consumers must also check that the stored schema matches their
expected field declarations. Records and their metadata must belong to the same
dataset; in-range IDs from another pool cannot be detected automatically.

## Upgrading from 2.x

Upgrade **readers before writers**. 3.0 readers accept existing format-3
files. The ordinary 3.0 writer can emit format 4; older readers that only
support format 3 reject it. A small test artifact may still use format 3, so
use explicit legacy output while any older reader must open your files.

| Need | Use |
| --- | --- |
| Write files for a mixed 2.x/3.0 fleet | `Builder::into_legacy_bytes`, `Builder::write_legacy_to`, or `ExternalBuilder::new_legacy` |
| Use the 3.0 defaults | Upgrade readers, then rebuild with `Builder` or `ExternalBuilder` |
| Publish shared aliases or exact-bit compaction | Confirm all readers support the chosen encoding, including format 6 inside an alias envelope |

Stage a new artifact and validate it before activation. Keep the old files
if older readers need to roll back. `Reader::inspect` reports the outer and
base formats. For layout and validation details, see
[FORMAT.md](https://github.com/0xARYA/netindex/blob/main/FORMAT.md).

## Features

| Feature | Behavior |
| --- | --- |
| `mmap` | Validated file-backed opening through `MappedReader`. |
| `external-sort` | Disk-backed building with buffer and temporary-file limits. |
| `mmdb` | Validated network iteration for caller-defined MMDB conversion. |
| `shared-values` | Shared byte/string pools with caller-defined record schemas. |
| `codec` | Optional typed records and pooled values; enables `shared-values`. |
| `simd-utf8` | SIMD string validation with scalar index search; enable `shared-values` or `codec` to use string pools. |
| `simd` | Runtime AVX2 search on x86-64 and SIMD string validation, with scalar fallbacks. |

All features are disabled by default. Global `target-cpu=native` is not required.
Measure search on your workload: SIMD search can be slower than scalar for small
packed blocks. `simd-utf8` enables accelerated string-pool opening independently;
`simd` includes it and also enables AVX2 search.
Rust 1.95 is supported. The public error enum is non-exhaustive; messages are
diagnostics, not a stable wire format.

See the [file specification](https://github.com/0xARYA/netindex/blob/main/FORMAT.md) for encoding and validation, and the
[contributor guide](https://github.com/0xARYA/netindex/blob/main/CONTRIBUTING.md) for checks and measurement practices.

## Performance

Costs vary with index shape, payload decoding, and startup requirements.
In the Byway City deployment replay, netindex improved lookup tail latency
while native MMDB opened and reloaded faster. See the
[benchmark guide](https://github.com/0xARYA/netindex/blob/main/BENCHMARKS.md)
for the workloads, tradeoffs, and reproduction instructions.
