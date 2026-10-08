# Fixed format fixtures

These synthetic artifacts were assembled directly from the format specification,
using little-endian integer fields, independently of the Rust builder.
All `.nidx` fixtures begin with the eight ASCII bytes `NETINDEX`.

- `interval.nidx`: two IPv4 address assertions, `203.0.113.42` and
  `203.0.113.43`, IDs 0 and 1, and their four-byte little-endian ID payloads.
  It uses interval rows and sixteen-byte payload slots, with no metadata.
- `packed-ipv4.nidx`: forty IPv4 addresses with numeric values 0, 2, …, 78,
  IDs 0 through 39, and the shared payload `P`. One packed descriptor uses
  base 0, shift 1, one-byte deltas, and compact payload slots. Metadata is empty.
- `packed-ipv6.nidx`: the same forty IDs and shared payload, with IPv6 addresses
  `id << 80`, shift 80, and full-length address prefixes.
- `consecutive-ids.nidx`: IPv4 addresses `10.0.0.1` and `10.0.0.2`,
  implicit IDs 1 and 2, plus exact ASN 7 with ID 0. All share payload `x`.
  The packed descriptor uses a one-byte delta and ID base 1.
- `strided-ids.nidx`: IPv4 `/24` networks `0.0.0.0` and `0.0.1.0`,
  implicit IDs 0 and 2 with a step of two, plus IPv6 address `::1` with ID 1.
  Payloads are empty; fixed-length references use no per-record bytes.
- `mixed.nidx`: IPv4 and IPv6 `/0` networks, address assertions at IPv4
  numeric value 42 and IPv6 maximum, and exact ASN 42. IDs are 0 through 4;
  all share payload `P`. Interval maxima include the covering networks.

- `fixed-payloads.nidx`: six ASN assertions with two-byte payloads `aa`, `bb`,
  and `cc`, using independent one-byte references `[0, 1, 1, 0, 2, 1]`.
- `uniform-kind.nidx`: two IPv4 `/24` networks with consecutive IDs, a uniform
  network kind in the descriptor, and one shared one-byte payload.

The tests specify lookup expectations separately. No fixture is regenerated
by the implementation under test.

`anonymous.mmdb` is MaxMind's `GeoIP2-Anonymous-IP-Test.mmdb` from
[commit b5f0b07f4dbea979821d55c61eef2700cfac4923](https://github.com/maxmind/MaxMind-DB/tree/b5f0b07f4dbea979821d55c61eef2700cfac4923),
redistributed under `maxmind-LICENSE-MIT.txt`. SHA-256:
`d080856c8dfb306780a2c20cf1b32fd873bd6ac262fbe5cb5ea37e497348244d`.
It contains IPv4 and IPv6 records, including IPv4-mapped and 6to4 aliases.

`values.bin` seeds shared-value fuzzing with two nonempty values.
[values.rs](../values.rs) also checks a separately specified fixed-byte pool.

`codec.bin` independently encodes a required boolean and optional string `x`,
with a one-byte metadata-length prefix for the codec fuzz target. Its record
is `[1, 1, 0, 0, 0, 0]`: presence bit, true boolean, and string ID zero.
