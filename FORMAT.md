# File format

Use `.nidx` for index files. The extension is a naming convention; validation
uses the header and encoded contents.

## File layout

All integers are little-endian. Offsets and lengths are unsigned; target addresses
are numeric IP values, independent of host byte order. No native Rust structs or
pointers are serialized. A file contains an 80-byte header followed immediately
by metadata, IPv4 section, IPv6 section, ASN rows, payload slots, and payload bytes.
Empty sections consume no bytes; trailing data is rejected.

| Header offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII `IPINDEX` followed by NUL |
| 8 | 4 | Format identifier: 2 |
| 12 | 4 | Flags: bit 0 packed IPv4, bit 1 packed IPv6, bit 2 compact payload slots, bit 3 fixed-length payload references |
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

## Interval sections

IPv4 rows are 20 bytes: start, inclusive end, subtree maximum end, and record ID
as four `u32` values; prefix length and target kind as two `u8` values; then two
reserved zero bytes. IPv6 rows are 56 bytes: the three endpoints are `u128`, with
the same `u32` ID and final four bytes. Target kind is 0 for address, 1 for network,
or 2 for inclusive range. Addresses carry a full-length prefix and equal bounds;
networks are canonical and their endpoint matches the prefix; ranges carry prefix 0.

IP rows sort strictly by `(start, end, record ID)`. Their implicit balanced interval
tree uses `mid = low + (high - low) / 2` for each half-open position range. Each
midpoint stores the maximum endpoint of that subtree. Readers verify the exact
augmentation; an incorrect maximum could otherwise silently suppress matches.

## ASN section

ASN rows are eight bytes: nonzero ASN `u32` and record ID `u32`, sorted strictly
by `(ASN, ID)`. They do not map IPs to ASNs.

## Payloads

Each assertion has a payload slot in insertion order: offset relative to the
payload section and length. Compact slots are two `u32` fields (8 bytes) and
require a payload pool no larger than `u32::MAX`; otherwise slots are two `u64`
fields (16 bytes). Bits 2 and 3 cannot both be set. Identical payloads reuse
the same span.

Each first-seen span starts at the current payload end; repeated spans must match
an earlier offset and length exactly. There are no gaps, partial overlaps, or
unused suffix. Zero-length payloads are valid. This shares complete payloads, not
strings inside different payloads.

When all payloads have the same byte length and the pool fits in `u32::MAX`,
the writer selects fixed-length references if they are smaller. This section
begins with the common length as `u32`, a reference byte width as `u8`, and three
reserved zero bytes. Each insertion-order assertion then stores a little-endian
pool ordinal using zero through four bytes. Width is the minimum whole-byte
width for `pool_length / common_length - 1` when the common length is nonzero;
one shared value requires no reference bytes. All-empty payloads use length
zero and width zero.
A reference selects the span `ordinal * common_length .. (ordinal + 1) * common_length`.
The first distinct ordinal must be zero; subsequent first-seen ordinals increase
by one, and every pool value must be referenced. Repeated ordinals share borrowed
bytes. Validation checks these rules without an auxiliary span directory.
Variable-length payloads retain offset/length slots. This encoding assumes no
payload schema and introduces no reader allocation.

## Validation

The reader validates the format identifier, reserved fields, exact file length,
checked section arithmetic, limits, target invariants, ordering, unique IDs,
interval maxima, and payload spans before returning a reader. Unknown flags
are rejected. There is no embedded checksum or authenticity mechanism. Payload
schemas, integrity, and provenance belong to the consumer; changing opaque
payload bytes need not cause a format error.

## Packed nonoverlapping sections

Each address family selects its encoding independently. Packing is allowed only
for disjoint addresses and canonical CIDRs, and the writer selects it only when
the section is smaller than its interval representation. Ranges, duplicates, and
overlaps retain the interval representation; no evidence is discarded.

A packed section begins with `ceil(count / 256)` 32-byte block descriptors.
Each descriptor holds a `u128` base address, `u64` data offset relative to the
section, `u8` delta width and flags, `u8` shift, `u16` row count, and a `u32` ID base.
The low six width bits select the delta width. Bit 7 marks consecutive IDs:
row IDs are the ID base plus the row position, without overflow. Otherwise the
ID base is zero and the block retains its explicit ID column.
The shift byte's high bit marks a uniform target kind: width bit 6 selects
network (1) or address (0), and the block omits its kind column. Width bit 6 must
be zero when the block has an explicit kind column. The low seven shift bits
select shifts zero through 127. Delta widths are 1, 2, 4, 8, or 16. Blocks have
256 rows except the last; their data follows the descriptor table contiguously.

Each block stores fixed-width unsigned deltas, prefix bytes, target kind bytes
when not uniform, and explicit `u32` IDs when not consecutive. The first delta
is zero. Addresses are `base + (delta << shift)`, with no overflow; inclusive ends follow the prefix.
Kinds remain 0 for address and 1 for network. IDs and payload slots preserve
original targets independently of their sorted positions.

Readers validate descriptor bounds, reserved fields, contiguous data, exact
section length, prefix alignment, address-family bounds, unique IDs, and strict
disjoint ordering. Lookup searches block bases then the selected delta column
for a predecessor and checks its inclusive endpoint. AVX2 and scalar readers use
identical bytes; SIMD is runtime dispatched and needs no global CPU build flag.

## Optional shared values

The `shared-values` feature encodes a separate pool; it does not change index
bytes or the opaque payload contract. Pools may be embedded in metadata or kept
in separate files. Records encode IDs using the consumer's schema.

A pool begins with eight ASCII bytes `NIVALUES`, followed by a `u32` value count,
then count plus one `u32` offsets relative to the value bytes. Offsets are
nondecreasing, begin at zero, and end at the exact data length. Empty values are
valid. IDs are zero-based positions in the directory. Identical values share IDs
when built through `ValuePoolBuilder`; parsers do not require deduplication.

The complete pool and value count are bounded by `Limits.bytes` and
`Limits.records`. The writer supports at most `u32::MAX` values and data bytes.
`ValuePool::open` validates the directory without allocating. `StringPool::open`
also validates every value as UTF-8; its private validated state permits borrowed
string access without repeated scans. Backing bytes must remain immutable.
