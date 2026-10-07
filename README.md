# netindex

Read and write immutable IP-range and exact-ASN indexes. Each assertion retains
its original target and an opaque payload. Queries return every match, including
overlaps and duplicates.

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

## Quick start

Add `netindex` to your dependencies:

```toml
[dependencies]
netindex = "0.1"
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

for matched in matches {
    assert_eq!(matched.payload, b"evidence");
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

## API overview

| Type | Purpose |
| --- | --- |
| [`Builder`] | Buffer unsorted assertions and encode to bytes or a writer. |
| [`Reader`] | Validate backing bytes and query the index. |
| [`Target`] | An IP address, canonical CIDR, inclusive range, or nonzero ASN. |
| [`Match`] | Original target, insertion-order ID, and borrowed payload. |
| [`Limits`] | Record and complete-file byte ceilings. |
| [`CoverageRouter`] | Select possible readers when querying multiple files. |
| `ExternalBuilder` | Build with bounded buffers and temporary files; requires `external-sort`. |

## Reading

[`Reader::open`] accepts immutable `AsRef<[u8]>` backing: owned bytes, a borrowed
slice, or a caller-owned mapping. Opening validates the complete index. Payload
decoding belongs to the application.

[`Reader::lookup_ip`] and [`Reader::lookup_asn`] collect borrowed matches into a
vector. Misses return an empty vector. Exact-ASN lookup does not resolve an IP's
ASN. Payloads remain valid while their reader is retained.

Use [`Reader::visit_ip`], [`Reader::visit_asn`], or [`Reader::visit_all`] to process
matches without allocating a result vector. A visitor error stops delivery and
retains the application's error type:

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

For up to 64 files, derive each reader's coverage and pass it to
[`CoverageRouter::new`] in reader order. Query every returned candidate. Coverage
can admit misses but does not discard matches. Retain readers and their router
together; rebuild the router when the reader set changes. ASN queries still visit
each reader's exact-ASN index.

## Building

[`Builder`] accepts unsorted assertions. [`Builder::into_bytes`] returns owned
encoded bytes; [`Builder::write_to`] writes the same format into a caller-owned
writer. Identical payloads share bytes without merging assertions. Metadata is
stored once and remains opaque.

Enable `external-sort` for `ExternalBuilder` when buffering all assertions would
exceed the desired working set. It sorts bounded runs on disk and preserves
insertion-order IDs. `ExternalOptions` controls run size, payload-cache bytes, and
temporary-disk usage. An uncached repeated payload may occupy additional output
bytes. Failed pushes invalidate this builder; discard it.

External finalization requires an empty seekable output at position zero. Both
writers can leave partial output on failure. Stage output and publish it only
after successful finalization and any application payload checks. Synchronization
and release activation belong to the caller.

## MMDB conversion

Enable `mmdb` for `mmdb::visit_networks`. It validates a native MMDB reader, then
visits one network at a time. The callback encodes application fields and pushes
into `Builder` or `ExternalBuilder`; it can skip records without adding assertions.
Input may be owned bytes or a mapping. It cannot be parsed directly from a
sequential download stream.

Iteration options control aliases, empty records, and networks without data.
Include aliased networks when preserving native IPv6 alias lookups. Conversion
errors stop delivery but do not undo earlier pushes; discard the partial build.
Payload schemas, metadata selection, and output publication remain caller-owned.

## Memory and files

Default limits are one million assertions and 128 MiB per file. These bound logical
records and bytes, not process RSS. Validation state, allocator overhead, caller
buffers, and the OS file cache consume additional memory.

The crate does not open or map files. A mapping owner must keep its file unchanged
for the reader's lifetime. Mapped-page faults can block a lookup even though index
search performs no explicit I/O. Choose owned bytes or a mapping for the workload.

## Threads and runtimes

`Reader<B>` is `Send + Sync` when its backing is. Share it through `Arc` or borrow
it across scoped threads; sharing does not copy database bytes. Each caller owns
its visitor and result storage. The crate creates no threads and uses no runtime.

Parallelize independent queries or bounded batches. Move opening, building, and
substantial lookup work away from async runtime workers. Bound admitted jobs and
keep permits inside blocking jobs until they finish, including after cancellation.

The Tokio and compio examples share readers with bounded blocking workers and
await admitted work before returning a batch failure:

```sh
cargo run --example tokio
cargo run --example compio
```

## Features

| Feature | Behavior |
| --- | --- |
| `external-sort` | Disk-backed building with configurable buffer and temporary-file limits. |
| `mmdb` | Validated network iteration for caller-defined MMDB conversion. |
| `simd` | Runtime AVX2 dispatch on x86-64, with scalar fallback. File bytes are unchanged. |

All features are disabled by default. Global `target-cpu=native` is not required.
The supported toolchain is Rust 1.95. The public error enum is non-exhaustive;
error messages are diagnostics, not a stable wire format.

The [file specification](FORMAT.md) defines the encoded layout and validation rules.

See [contributor documentation](CONTRIBUTING.md) for checks and benchmarks.
