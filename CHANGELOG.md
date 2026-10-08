# Changelog

## 2.0.1 — 2026-10-07

Use Rust 2024 for the library and fuzz targets, with resolver 3 in both workspaces.
The public API, `.nidx` layout, and Rust 1.95 requirement are unchanged.

## 2.0.0 — 2026-10-07

Breaking change: `.nidx` files now use the `NETINDEX` signature and layout version 3.
Rebuild files produced by earlier releases; the reader does not accept older layouts.

Immutable `.nidx` indexes for IPv4, IPv6, and exact ASN keys, retaining overlaps,
duplicates, original targets, and borrowed payloads. Includes memory-mapped
readers, coverage routing, bounded external sorting, MMDB network iteration,
shared value pools, an optional record codec, and optional AVX2.

Requires Rust 1.95; licensed under MIT OR Apache-2.0.
