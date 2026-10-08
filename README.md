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
favor MMDB; converted files are about 2.2× larger and validated opening is slower.
See [benchmarks](https://github.com/0xARYA/netindex/blob/main/BENCHMARKS.md) for all
workloads, methodology, and reproduction instructions.

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
netindex = "2"
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

## Separate source files

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
netindex = { version = "2", features = ["mmap"] }
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

Enable `external-sort` for `ExternalBuilder` when buffering all assertions would
exceed your budget. `ExternalOptions` bounds sorted runs, the payload cache, and
temporary-disk usage. Insertion IDs are preserved. Repeated payloads outside the
cache may occupy additional output bytes. Discard the builder after a failed push.

The external writer requires empty, seekable output at position zero. Either
writer can leave partial output on failure. Stage output, check success, and
validate your payloads before publishing. File synchronization and release
activation belong to your application.

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
netindex = { version = "2", features = ["codec"] }
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
| `simd` | Runtime AVX2 search on x86-64 and SIMD string validation, with scalar fallbacks. |

All features are disabled by default. Global `target-cpu=native` is not required.
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
