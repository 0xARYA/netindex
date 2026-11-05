# Changelog

## 3.0.0 - Unreleased

Choose indexed shared-payload references by default when they reduce artifact
size. Both builders and the reader support format 4, while readers continue to
accept format 3. Payload bytes, metadata, assertion IDs, original targets and
all-match ordering are preserved. Indexed validation borrows the span directory
and needs no distinct-value allocation.

This changes writer compatibility: 2.x readers reject format-4 output. Upgrade
readers before rebuilding, or use `Builder::into_legacy_bytes`,
`Builder::write_legacy_to`, or `ExternalBuilder::new_legacy` for format 3.
External builds account additional temporary reference files and larger disk
cells against their existing disk limit. See FORMAT.md for exact selection and
validation rules. This release has not been published to the registry.

Explicit IPv6 alias windows can share the IPv4 network table through
`Reader::write_shared_aliases`. Format 5 preserves logical targets, insertion
IDs and all matches. Upgrade readers before using it. Ordinary builder output
remains format 3 or 4.

## 2.1.0 - 2026-11-04

Deduplicate every external-builder payload, including values outside the memory
cache, using a disk-backed hash table and complete byte comparisons. Preserve
first-seen spans, insertion IDs, targets, and all assertions without changing the
public API or `.nidx` format. Uncached values require random temporary-file I/O;
table growth counts both tables against the temporary-disk budget.

Reduce external-builder file operations with positioned I/O and comparisons
against buffered payload bytes. Keep the reader handle separate from the append
handle, including on Windows where positioned I/O changes the file cursor.

Specialize AVX2 predecessor search for 1-, 2-, 4- and 8-byte columns. Add the
`simd-utf8` feature for accelerated strings with scalar search; existing `simd`
continues to enable both. Add all-width lookup benchmarks and correctness checks.

## 2.0.2 - 2026-10-08

Validate shared string pools in batches, with optional SIMD UTF-8 validation.
Preserve per-value errors and reject boundaries that split Unicode characters.
Avoid constructing unused targets during index validation.

Add reproducible string-pool and wide IPv6 benchmarks with dated measurements.
The public API and `.nidx` layout are unchanged.

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
