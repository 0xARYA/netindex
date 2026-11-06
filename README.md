# netindex

Read and write immutable IP-range and exact-ASN indexes. Queries return every
matching assertion, including overlaps and duplicates, with its original target
and borrowed payload bytes.

## Performance

Warm DB-IP Lite City hit queries measured **3.3–4.3× faster lookup and field
decoding** and **1.7–1.8× faster lookup through JSON serialization** than the Rust
MMDB reader, with identical fields, networks, and JSON output.

| Query | netindex (ns) | MMDB (ns) | Speedup |
| --- | ---: | ---: | ---: |
| IPv4 lookup + fields | 110 | 359 | **3.3×** |
| IPv6 lookup + fields | 93 | 401 | **4.3×** |
| IPv4 lookup + fields + JSON | 383 | 699 | **1.8×** |
| IPv6 lookup + fields + JSON | 464 | 788 | **1.7×** |

Measured on Ryzen 9 9950X3D2, Windows 11, Rust 1.95.0, using `maxminddb 0.32.0`
and netindex's public codec. Times are warm mean ns/query. Some lookup workloads
favor MMDB; those converted fixtures were about 2.2× larger and validated opening
was slower.
See [benchmarks](https://github.com/0xARYA/netindex/blob/main/BENCHMARKS.md) for all
workloads, methodology, and reproduction instructions.

The latest [performance audit](https://github.com/0xARYA/netindex/blob/main/PERFORMANCE.md)
covers external-build I/O, every packed width, and application PGO versus ThinLTO.

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

```toml
[dependencies]
netindex = "3"
```

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

[`Reader::open`] validates the complete index before returning a reader. Backing
bytes must stay immutable. Your application decodes payloads.

[`Reader::lookup_ip`] and [`Reader::lookup_asn`] collect borrowed matches into a
vector; misses return an empty vector. ASN lookup matches an exact key and does
not resolve an IP's ASN. Record IDs follow insertion order; query results need
not follow that order.

Use [`Reader::visit_ip`], [`Reader::visit_asn`], or [`Reader::visit_all`] to avoid
allocating a result vector. A visitor error stops delivery:

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

`Reader::visit_batch` accepts borrowed `Query` values and calls the visitor with
its input query position and each borrowed match. Delivery follows input order,
then normal per-query ordering; duplicate queries run independently. There is no
output buffer, reordering, thread pool or automatic cross-query SIMD policy.
`BatchError` identifies the failed query and retains the original error. Earlier
callbacks have already run; later queries do not run. Callers own output storage
and any rollback policy.

`contains_ip` and `contains_asn` stop at the first match. The `try_visit_*` methods
return `ControlFlow::Break(value)` for successful early termination and preserve
visitor errors separately. `visit_intersecting` takes an address, canonical CIDR
or inclusive range and returns whole original intersecting targets. It searches
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

[`ReaderSet`] shares up to 64 readers and uses IP coverage to skip files that
cannot match. It returns every matching assertion with its source position and
source-local record ID. ASN queries search every reader. Schema decoding and
provider precedence remain application decisions.

The default router uses 64 KiB; each reader retains a 1 KiB coverage summary.
`ReaderSet::with_fine_routing` uses a 1 MiB table and can skip more files; measure
its extra cache footprint against your workload. Both preserve all matches.

`replaced(position, reader)` creates a new set sharing unchanged readers. Coarse
routing reuses coverage cached during validation; fine routing scans only the
replacement. Old sets and borrowed matches remain usable; the application publishes
the new set. Use [`CoverageRouter`] to manage readers separately.

## Memory-mapped files

Enable `mmap` for `MappedReader::map_file`. It checks file-size limits before
mapping, validates the index, and owns the mapping independently of the file handle.

```toml
[dependencies]
netindex = { version = "3", features = ["mmap"] }
```

```rust,ignore,no_run
use std::fs::File;

use netindex::{Limits, MappedReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("membership.nidx")?;
    // SAFETY: No process modifies or truncates this file while the reader is alive.
    let reader = unsafe { MappedReader::map_file(&file, Limits::default()) }?;

    let matches = reader.lookup_ip("203.0.113.42".parse()?)?;
    println!("{} matching entries", matches.len());

    Ok(())
}
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

[`Builder::into_bytes`] returns owned bytes; [`Builder::write_to`] writes to a
caller-owned output. Metadata is stored once as opaque bytes.

Version 3 chooses compact shared-payload references when they save space. The
new format-4 encoding keeps targets, insertion IDs and borrowed opaque payloads
unchanged; readers also accept existing format-3 files. **2.x readers cannot open
format-4 files.** Upgrade readers before rebuilding artifacts, or use
`Builder::into_legacy_bytes`, `Builder::write_legacy_to`, or
`ExternalBuilder::new_legacy` to continue producing format 3. Uniform-length and
unshared values retain their smaller existing representation. See [FORMAT.md](FORMAT.md).
The in-memory writer temporarily sorts one span per distinct value when selecting
the new encoding. The external builder keeps its directory and IDs on disk; it
reuses its existing slot spill for value IDs, charging sixteen temporary bytes
per distinct value and 32-byte disk-table cells instead of 24-byte cells to the
temporary-byte budget. Individual payloads at least `u32::MAX` bytes need an
additional eight temporary bytes per assertion for a wide-length escape.
These are construction costs; readers borrow the directory without allocating a
second copy.

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
See [partitioned-build measurements](benches/partitioned_dedup/README.md).

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
[FORMAT.md](FORMAT.md) and [bitpacking measurements](benches/bitpacking/RESULTS.md).
Stage output and check success before publishing; output errors can leave partial
bytes.

### Shared IPv4 aliases

For an index whose complete IPv4 network section is repeated under explicit
IPv6 prefixes, `Reader::write_shared_aliases` can wrap the base in a format-5
artifact. The reader projects targets, shares borrowed payloads and preserves
logical insertion IDs without copying the IPv4 rows. Windows must be disjoint
from each other and from native IPv6 targets; overlaps and duplicates among
source IPv4 networks still produce separate matches. The source IPv4 IDs must
start at zero in sorted order. See [the format rules](FORMAT.md#shared-ipv4-aliases-format-5).

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

The `codec` feature provides flat typed records with shared strings and bytes.
Declare fields once, then encode records with that schema:

```toml
[dependencies]
netindex = { version = "3", features = ["codec"] }
```

```rust,ignore
use netindex::{
    Builder, Error, Limits, Reader, Target,
    codec::{Decoder, Encoder, Field, Kind, Schema, Value},
};

fn main() -> Result<(), Error> {
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

    Ok(())
}
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

[`Reader::open`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.open
[`Reader::lookup_ip`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.lookup_ip
[`Reader::lookup_asn`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.lookup_asn
[`Reader::visit_ip`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.visit_ip
[`Reader::visit_asn`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.visit_asn
[`Reader::visit_all`]: https://docs.rs/netindex/latest/netindex/struct.Reader.html#method.visit_all
[`ReaderSet`]: https://docs.rs/netindex/latest/netindex/struct.ReaderSet.html
[`CoverageRouter`]: https://docs.rs/netindex/latest/netindex/struct.CoverageRouter.html
[`Builder::into_bytes`]: https://docs.rs/netindex/latest/netindex/struct.Builder.html#method.into_bytes
[`Builder::write_to`]: https://docs.rs/netindex/latest/netindex/struct.Builder.html#method.write_to
