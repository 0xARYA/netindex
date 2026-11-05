# File format

Use `.nidx` for index files. The extension is a naming convention; validation
uses the header and encoded contents.

## File layout

All integers are little-endian. Offsets and lengths are unsigned; target addresses
are numeric IP values, independent of host byte order. No native Rust structs or
pointers are serialized. A format-3 or format-4 file contains an 80-byte header
followed immediately by metadata, IPv4 section, IPv6 section, ASN rows, payload
slots, and payload bytes.
Empty sections consume no bytes; trailing data is rejected.

| Header offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII `NETINDEX` |
| 8 | 4 | Format identifier: 3 or 4 |
| 12 | 4 | Flags: bit 0 packed IPv4, bit 1 packed IPv6, bit 2 compact payload slots, bit 3 fixed-length payload references, bit 4 indexed payload references |
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

Span slots appear in insertion order and contain an offset relative to the
payload section and a length. Compact slots are two `u32` fields (8 bytes) and
require a payload pool no larger than `u32::MAX`; otherwise slots are two `u64`
fields (16 bytes). At most one of bits 2, 3 and 4 may be set. Slots may reuse a span for
identical payloads; duplicate bytes in distinct spans are also valid.
Both current builders reuse the first-seen span for identical payloads. The
external builder uses a bounded memory cache with a disk-backed fallback; older
external builds may contain repeated bytes after their cache filled. Both forms
use this same format and remain readable.

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

### Indexed shared-payload references (format 4)

When smaller than the format-3 slot encoding, both builders select a shared span
directory with insertion-order payload IDs. The payload-slot section contains:

| Relative offset | Width | Field |
| ---: | ---: | --- |
| 0 | 4 | Number of directory entries, `U`, as `u32` |
| 4 | 1 | Reference width, `w` |
| 5 | 3 | Reserved zero bytes |
| 8 | `8 * U` | Directory entries: payload-relative offset and length as two `u32` fields |
| `8 + 8 * U` | `w * N` | One little-endian directory ordinal per insertion-order assertion |

The pool must fit in `u32::MAX`. `1 <= U <= N`, and `w` is the minimum whole-byte
width for `U - 1` (zero through four). The first directory offset is zero; each
next offset equals the previous offset plus length, and the final span ends
exactly at the pool length. Empty spans are valid. Every reference is less than
`U`; first-seen ordinals occur as 0, 1, ..., `U - 1`, with no unused entries.
Duplicate bytes in distinct directory entries remain valid.

Validation needs no heap directory or visited-value set: the borrowed span table
is validated once, then references are checked against the next unseen ordinal.
Lookup reads the assertion's ordinal and then its borrowed span. Payload bytes,
metadata, targets, assertion IDs and all-match ordering are unchanged. No schema
is required. Directory bytes count against the artifact-byte limit.

Reference bytes total `8 + 8*U + w*N`; writers select this encoding only when
strictly smaller than the best format-3 representation. Fixed-length references
remain preferable for uniform lengths, and unshared variable-length values retain
span slots. Values beyond the compact pool limit retain format-3 wide spans.

Format 4 requires bit 4, with bits 2 and 3 clear. Format 3 forbids bit 4. Other
header, section and validation rules are identical. Readers accept both formats.
Netindex 2.x rejects format 4. Netindex 3.x builders select the smaller encoding
by default; `Builder::into_legacy_bytes`, `Builder::write_legacy_to`, and
`ExternalBuilder::new_legacy` preserve format-3 output for older consumers.

## Shared IPv4 aliases (format 5)

Format 5 is an opt-in envelope around one complete, unmodified format-3 or
format-4 file. It starts with ASCII `NETINDEX`, version `5` as `u32`, then the
descriptor count as `u32` (1 through 64). The descriptors follow immediately;
each is 24 bytes:

| Relative offset | Width | Field |
| ---: | ---: | --- |
| 0 | 16 | Canonical IPv6 window address as `u128` |
| 16 | 4 | Logical ID of the first projected assertion |
| 20 | 1 | Window prefix length, 0 through 96 |
| 21 | 1 | ID stride, 1 or 2 |
| 22 | 2 | Reserved zero bytes |

The embedded file starts at byte `16 + 24 * descriptor_count` and runs to EOF.
Its offsets and validation rules are unchanged. Envelopes cannot nest. The
complete envelope counts toward the byte limit. Format-3/4 readers reject it;
ordinary builders continue to emit formats 3/4.

A descriptor projects the entire nonempty IPv4 section into its window. That
section must contain only networks, with physical IDs 0 through `N-1` in sorted
row order. The 32 bits immediately after the window prefix select the source
IPv4 query, and any trailing bits are ignored. A source network of prefix length
`p` becomes an IPv6 network of length `window_prefix + p`. Payloads are borrowed
from the source row. IPv4 overlaps and duplicates project independently. The
windows are sorted by address, mutually disjoint and disjoint from every
physical IPv6 assertion, including any ranges that cross a window boundary.

Normally the source IPv4 IDs are unchanged and each descriptor reserves a
contiguous block of `N` logical IDs. One descriptor may instead have stride 2
and first ID 1: source IPv4 rows then receive IDs `2*i`, and that window receives
`2*i+1`. Contiguous blocks must start at or above the end of the source (or
interleaved pair), may not overlap, and must fit within the logical count.
Other physical IDs fill the unreserved logical IDs in increasing order.
The record limit applies to `physical_count + N * descriptor_count`.

IP visits return only assertions in the queried family, in the same target/ID
order as a materialized file. Whole-index visits merge windows into IPv6 target
order. ASN keys and metadata are unchanged. Readers do not infer alias windows
or apply provider-specific address translation.

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
The low five width bits select the delta width. Bit 7 marks implicit IDs;
bit 5 selects a step of two instead of one and requires bit 7. Row IDs are the
ID base plus the row position times the step, without overflow. Otherwise the
ID base is zero and the block retains its explicit ID column.

The shift byte's high bit marks a uniform target kind: width bit 6 selects
network (1) or address (0), and the block omits its kind column. Width bit 6 must
be zero when the block has an explicit kind column. The low seven shift bits
select shifts zero through 127. Delta widths are 1, 2, 4, 8, or 16. Blocks have
256 rows except the last; their data follows the descriptor table contiguously.

Each block stores fixed-width unsigned deltas, prefix bytes, target kind bytes
when not uniform, and explicit `u32` IDs when not implicit. The first delta
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

## Optional record codec

`codec` leaves the index format unchanged. Its separate metadata starts with
eight ASCII bytes `NETCODEC`, a `u32` field count, and two bytes per field:
kind and presence mode. Kinds 0 through 8 are bool, u8, u16, u32, u64, i64,
f64, string, and bytes. Modes 0, 1, and 2 mean required, optional, and globally
absent. Unknown declarations are rejected. Field positions identify the schema;
field names are not encoded.

A `u64` string-pool byte length follows the declarations. The string pool and
then the byte pool each use the `NIVALUES` encoding above. Both pools consume
their exact declared region; there are no trailing bytes outside the byte pool.
String-pool values are validated as UTF-8 when opened.

Records begin with `ceil(optional fields / 8)` presence bytes, least-significant
bit first, followed by field slots in declaration order without alignment
padding. Required fields have no presence bit; absent fields have neither bit
nor slot. Unused high presence bits must be zero. Scalars are little-endian;
booleans are zero or one. Strings and bytes contain `u32` IDs in their respective
pools. Floating-point bits are preserved, including signed zero and NaN payloads.

Missing optional slots are written as zero, but readers ignore their contents.
Row creation validates length and bitmap padding; accessing fields validates
booleans and reference bounds. `Record::validate()` visits every field. Payloads
from another schema or pool are not self-identifying: consumers must check their
schema and keep metadata and records together.

Related designs: fixed-position fields in [Simple Binary Encoding](https://github.com/FIXTradingCommunity/fix-simple-binary-encoding/blob/master/v2-0-RC2/doc/02FieldEncoding.md),
dictionary references and presence bits in [Arrow](https://arrow.apache.org/docs/format/Columnar.html),
and direct access to serialized data in [FlatBuffers](https://flatbuffers.dev/internals/).
