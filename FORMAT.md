# File format

All integers are little-endian. Offsets and lengths are unsigned; target addresses
are numeric IP values, independent of host byte order. No native Rust structs or
pointers are serialized. A file contains an 80-byte header followed immediately
by metadata, IPv4 section, IPv6 section, ASN rows, payload slots, and payload bytes.
Empty sections consume no bytes; trailing data is rejected.

| Header offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII `IPINDEX` followed by NUL |
| 8 | 4 | Format identifier: 2 |
| 12 | 4 | Flags: bit 0 packed IPv4, bit 1 packed IPv6, bit 2 compact payload slots |
| 16 | 8 | Total assertion count |
| 24 | 8 | IPv4 row count |
| 32 | 8 | IPv6 row count |
| 40 | 8 | ASN row count |
| 48 | 8 | Metadata byte length |
| 56 | 8 | Payload byte length |
| 64 | 8 | Packed IPv4 section bytes, zero for interval encoding |
| 72 | 8 | Packed IPv6 section bytes, zero for interval encoding |

The three section counts sum to the assertion count. Each row references exactly
one unique insertion-order ID in `0..count`; IDs may appear out of order after
sorting. Metadata and payloads are uninterpreted bytes.

Interval IPv4 rows are 20 bytes: start, inclusive end, subtree maximum end, and record ID
as four `u32` values; prefix length and target kind as two `u8` values; then two
reserved zero bytes. IPv6 rows are 56 bytes: the three endpoints are `u128`, with
the same `u32` ID and final four bytes. Target kind is 0 for address, 1 for network,
or 2 for inclusive range. Addresses carry a full-length prefix and equal bounds;
networks are canonical and their endpoint matches the prefix; ranges carry prefix 0.

IP rows sort strictly by `(start, end, record ID)`. Their implicit balanced interval
tree uses `mid = low + (high - low) / 2` for each half-open position range. Each
midpoint stores the maximum endpoint of that subtree. Readers verify the exact
augmentation; an incorrect maximum could otherwise silently suppress matches.

ASN rows are eight bytes: nonzero ASN `u32` and record ID `u32`, sorted strictly
by `(ASN, ID)`. They do not map IPs to ASNs.

Each assertion has a payload slot in insertion order: offset relative to the
payload section and length. Compact slots are two `u32` fields (8 bytes) and
require a payload pool no larger than `u32::MAX`; otherwise slots are two `u64`
fields (16 bytes). Identical payloads reuse the same span.
Each first-seen span starts at the current payload end; repeated spans must match
an earlier offset and length exactly. There are no gaps, partial overlaps, or
unused suffix. Zero-length payloads are valid. This shares complete payloads, not
strings inside different payloads.

The reader validates version, reserved fields, exact file length, checked section
arithmetic, limits, target invariants, ordering, unique IDs, interval maxima, and
payload spans before returning a reader. There is no embedded checksum or
authenticity mechanism; transport integrity/provenance belongs to the consumer.
Opaque payload changes can remain structurally valid and are not format errors.

## Packed nonoverlapping sections

Each address family selects its encoding independently. Packing is allowed only
for disjoint addresses and canonical CIDRs, and the writer selects it only when
the section is smaller than its interval representation. Ranges, duplicates, and
overlaps retain the interval representation; no evidence is discarded.

A packed section begins with `ceil(count / 256)` 32-byte block descriptors.
Each descriptor holds a `u128` base address, `u64` data offset relative to the
section, `u8` delta width, `u8` shift, `u16` row count, and four reserved zero bytes.
Delta widths are 1, 2, 4, 8, or 16; shifts are 0 through 127. Blocks have 256 rows,
except the last; their data follows the complete descriptor table contiguously.

Each block stores four columns: fixed-width unsigned deltas, prefix bytes, target
kind bytes, and `u32` original assertion IDs. The first delta is zero. Addresses
are `base + (delta << shift)`, with no overflow; inclusive ends follow the prefix.
Kinds remain 0 for address and 1 for network. IDs and payload slots preserve
original targets independently of their sorted positions.

Readers validate descriptor bounds, reserved fields, contiguous data, exact
section length, prefix alignment, address-family bounds, unique IDs, and strict
disjoint ordering. Lookup searches block bases then the selected delta column
for a predecessor and checks its inclusive endpoint. AVX2 and scalar readers use
identical bytes; SIMD is runtime dispatched and needs no global CPU build flag.
The writer sizes blocks before output, then encodes them directly without
temporary section files or materializing a second encoded index.

Payload-schema compatibility belongs to the consumer.
