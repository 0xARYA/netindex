# netindex

Read and write immutable IP-range and exact-ASN indexes. Queries return every
matching assertion, including overlaps and duplicates, with its original target
and borrowed payload bytes.

Licensed under [MIT](https://github.com/0xARYA/netindex/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/0xARYA/netindex/blob/main/LICENSE-APACHE), at your option.

## Why netindex?

Use netindex for datasets built periodically and queried repeatedly: membership
feeds, network policy, and IP intelligence. Disjoint networks use compact blocks;
overlaps use an interval index. Identical payloads share storage without merging
assertions. Your application chooses the payload schema and encoding.

Readers work with owned bytes, borrowed slices, or memory-mapped files. Writers
accept unsorted input, with disk-backed construction available for larger datasets.
For an existing MMDB or a frequently changing routing table, a native reader or
mutable trie may fit better. See [benchmarks](https://github.com/0xARYA/netindex/blob/main/BENCHMARKS.md)
for workloads and measurement instructions.

## Quick start

```toml
[dependencies]
netindex = "1"
```

```rust
use netindex::{Builder, Limits, Reader, Target};

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
# Ok::<(), Box<dyn std::error::Error>>(())
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
# use netindex::{Builder, Limits, Reader, Target};
# let mut builder = Builder::new(Limits::default());
# builder.push(Target::Address("203.0.113.42".parse()?), b"evidence")?;
# let reader = Reader::open(builder.into_bytes(b"")?, Limits::default())?;
reader.visit_ip("203.0.113.42".parse()?, |matched| {
    let text = std::str::from_utf8(matched.payload)?;
    assert_eq!(text, "evidence");

    Ok::<_, Box<dyn std::error::Error>>(())
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Separate source files

[`ReaderSet`] shares up to 64 readers and uses IP coverage to skip files that
cannot match. It returns every matching assertion with its source position and
source-local record ID. ASN queries search every reader. Schema decoding and
provider precedence remain application decisions.

```rust
use std::sync::Arc;

use netindex::{Builder, Error, Limits, Reader, ReaderSet, Target};

# let mut builder = Builder::new(Limits::default());
# builder.push(Target::Address("203.0.113.42".parse()?), b"listed")?;
# let bytes = builder.into_bytes(b"source")?;
let reader = Arc::new(Reader::open(bytes, Limits::default())?);
let set = ReaderSet::new(vec![reader])?;

set.visit_ip("203.0.113.42".parse()?, |source, matched| {
    assert_eq!(source, 0);
    assert_eq!(matched.payload, b"listed");

    Ok::<_, Error>(())
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The default router uses 64 KiB plus 1 KiB of cached coverage per source.
`ReaderSet::with_fine_routing` uses a 1 MiB table and can skip more files; measure
its extra cache footprint against your workload. Both preserve all matches.

`replaced(position, reader)` creates a new set sharing unchanged readers. It scans
only the replacement and rebuilds routing from retained coverage. Old sets and
borrowed matches remain usable; your application decides when to publish the new
set. Use [`CoverageRouter`] directly if you need to manage readers separately.

## Memory-mapped files

Enable `mmap` for `MappedReader::map_file`. It checks file-size limits before
mapping, validates the index, and owns the mapping independently of the file handle.

```rust,no_run
# #[cfg(feature = "mmap")]
# {
use std::fs::File;

use netindex::{Limits, MappedReader};

let file = File::open("membership.nidx")?;
// SAFETY: No process modifies or truncates this file while the reader is alive.
let reader = unsafe { MappedReader::map_file(&file, Limits::default()) }?;
let matches = reader.lookup_ip("203.0.113.42".parse()?)?;
# drop(matches);
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

Mapping is unsafe because changes to the file by any process can invalidate
borrowed memory. Publish replacements as separate files and retain old readers
until their requests finish. `mapping()` exposes the read-only mapping for
platform access hints; no preloading or advice is applied by default.

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

```rust
# #[cfg(feature = "shared-values")]
# {
use netindex::{values::{StringPool, ValuePoolBuilder}, Limits};

let mut builder = ValuePoolBuilder::new(Limits::default());
let city = builder.intern("Montréal".as_bytes())?;
assert_eq!(city, builder.intern("Montréal".as_bytes())?);

let bytes = builder.into_bytes()?;
let strings = StringPool::open(&bytes, Limits::default())?;
assert_eq!(strings.get(city)?, "Montréal");
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

`ValuePool` borrows arbitrary bytes. `StringPool` validates UTF-8 once and borrows
strings without rescanning. Both readers allocate no heap memory. IDs are local
to a pool and must not be reused with another release.

## MMDB conversion

Enable `mmdb` for `mmdb::visit_networks`. It validates the MMDB and visits one
network at a time. Your callback selects and encodes fields for either builder,
or skips a record. Input needs random-access bytes or a mapping; a sequential
download stream is insufficient.

Iteration options control aliases, empty records, and networks without data.
Include aliases to preserve native IPv6 alias lookups. Discard a partial build
after conversion errors; earlier pushes are not rolled back. The function's API
documentation includes a conversion example.

## Memory and concurrency

Default limits are one million assertions and 128 MiB per file. These bound
logical records and file bytes, not RSS. Validation state, allocator overhead,
caller buffers, and the OS file cache use additional memory. Large packed IPv4
sections add a lookup directory of up to 32 KiB per reader.

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

## Features

| Feature | Behavior |
| --- | --- |
| `mmap` | Validated file-backed opening through `MappedReader`. |
| `external-sort` | Disk-backed building with buffer and temporary-file limits. |
| `mmdb` | Validated network iteration for caller-defined MMDB conversion. |
| `shared-values` | Shared byte/string pools with caller-defined record schemas. |
| `simd` | Runtime AVX2 dispatch on x86-64, with scalar fallback. |

All features are disabled by default. Global `target-cpu=native` is not required.
Rust 1.95 is supported. The public error enum is non-exhaustive; messages are
diagnostics, not a stable wire format.

See the [file specification](https://github.com/0xARYA/netindex/blob/main/FORMAT.md) for encoding and validation, and
[contributor guide](https://github.com/0xARYA/netindex/blob/main/CONTRIBUTING.md) for checks and measurement practices.
