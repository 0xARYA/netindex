# Fixed format fixtures

These synthetic artifacts were assembled directly from the format specification,
using little-endian integer fields, independently of the Rust builder.

- `interval.ipidx`: two IPv4 address assertions, `203.0.113.42` and
  `203.0.113.43`, IDs 0 and 1, and their four-byte little-endian ID payloads.
  It uses interval rows and sixteen-byte payload slots, with no metadata.
- `packed-ipv4.ipidx`: forty IPv4 addresses with numeric values 0, 2, …, 78,
  IDs 0 through 39, and the shared payload `P`. One packed descriptor uses
  base 0, shift 1, one-byte deltas, and compact payload slots. Metadata is empty.
- `packed-ipv6.ipidx`: the same forty IDs and shared payload, with IPv6 addresses
  `id << 80`, shift 80, and full-length address prefixes.
- `mixed.ipidx`: IPv4 and IPv6 `/0` networks, address assertions at IPv4
  numeric value 42 and IPv6 maximum, and exact ASN 42. IDs are 0 through 4;
  all share payload `P`. Interval maxima include the covering networks.

The tests specify lookup expectations separately. No fixture is regenerated
by the implementation under test.

`anonymous.mmdb` is MaxMind's `GeoIP2-Anonymous-IP-Test.mmdb` from
[commit b5f0b07f4dbea979821d55c61eef2700cfac4923](https://github.com/maxmind/MaxMind-DB/tree/b5f0b07f4dbea979821d55c61eef2700cfac4923),
redistributed under `maxmind-LICENSE-MIT.txt`. SHA-256:
`d080856c8dfb306780a2c20cf1b32fd873bd6ac262fbe5cb5ea37e497348244d`.
It contains IPv4 and IPv6 records, including IPv4-mapped and 6to4 aliases.
