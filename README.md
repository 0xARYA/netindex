# netindex

Netindex builds immutable indexes for IPv4 and IPv6 addresses, CIDRs, inclusive
ranges, and exact ASN keys. Use it for periodically rebuilt datasets where you
need every match: a CIDR rule, an overlapping exception range, and duplicate
records can all match the same IP. Each record keeps its original target, ID,
and borrowed payload. Your application decides what the payload means.

## Quick start

Add `netindex = "3"` to your Cargo dependencies. Netindex supports Rust 1.95.

```rust
use netindex::{Builder, Limits, Reader, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ip = "203.0.113.42".parse()?;
    let limits = Limits::default();
    let mut builder = Builder::new(limits);
    builder.push(
        Target::Network {
            address: "203.0.113.0".parse()?,
            prefix: 24,
        },
        b"policy",
    )?;
    builder.push(Target::Address(ip), b"exception")?;

    let reader = Reader::open(builder.into_bytes(b"")?, limits)?;
    let matches = reader.lookup_ip(ip)?;
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().any(|record| record.payload == b"policy"));
    assert!(matches.iter().any(|record| record.payload == b"exception"));

    Ok(())
}
```

Use `visit_ip` to process all matches without a result vector, or
`contains_ip` for a yes/no answer. Enable `mmap` to open immutable `.nidx`
files or `external-sort` to build with bounded buffers and temporary files.

## When to use it

Netindex is a fit when several records can apply to one address and you need
the original records back. If you only need a winning prefix or yes/no
membership, a routing trie or bitmap can be faster.

On retained City data, compact references saved 38% on file size and roughly
halved validation time versus the previous netindex encoding, with slightly
slower warm lookups. The [benchmark guide](https://github.com/0xARYA/netindex/blob/main/BENCHMARKS.md)
also compares decoded City lookups with native MMDB and shows where
specialized indexes win.

## Documentation

- [API reference](https://docs.rs/netindex/3.0.1/netindex/)
- [Usage guide](https://github.com/0xARYA/netindex/blob/main/GUIDE.md): queries,
  mapped files, multiple sources, builders, codecs, and feature flags
- [Upgrading from 2.x](https://github.com/0xARYA/netindex/blob/main/GUIDE.md#upgrading-from-2x):
  upgrade readers before writing new file formats
- [File format](https://github.com/0xARYA/netindex/blob/main/FORMAT.md) ·
  [Examples](https://github.com/0xARYA/netindex/blob/main/examples/README.md) ·
  [Contributing](https://github.com/0xARYA/netindex/blob/main/CONTRIBUTING.md)

Licensed under [MIT](https://github.com/0xARYA/netindex/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/0xARYA/netindex/blob/main/LICENSE-APACHE), at your option.
