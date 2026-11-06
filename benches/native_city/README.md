# Native City qualification adapter

This standalone benchmark reuses externally owned application modules. It does
not add a provider codec to netindex or change the application workspace.
`prepare.py` requires the matching Byway checkout; it snapshots its format crate,
City schema/normalizer, payload encoder, native database validator and vendored
maxminddb into an ignored output directory. Review the source hashes against the
retained result receipt before comparing another source revision.

```powershell
python benches/native_city/prepare.py C:/code/0xARYA/byway target/native-city/2.0.2 2.0.2
python benches/native_city/prepare.py C:/code/0xARYA/byway target/native-city/2.1.0 2.1.0
cargo +1.95.0 build --release --manifest-path target/native-city/2.0.2/Cargo.toml
cargo +1.95.0 build --release --manifest-path target/native-city/2.1.0/Cargo.toml
```

Match transitive dependencies between both locks before timing. The recorded
experiment uses identical dependencies except netindex and the corresponding
application-format dependency edge. Use a fresh output filename for each run;
existing output files are never overwritten. Run processes serially, pinned to
the same available CPU. `run.ps1` records the executable hash, arguments, exit
status, elapsed wall time and process peak working set; `-TextOutput` also allows
running Criterion executables with the same process controls.

```powershell
./benches/native_city/run.ps1 `
  -Executable target/native-city/2.1.0/target/release/native-city-audit.exe `
  -Output target/native-city/convert-new-1 `
  -BenchmarkArguments @('convert','target/city-study/city.mmdb',
    'target/native-city/new-1.nidx','target/native-city')
```

`convert` reports native structural/record validation, aliased traversal and
encoding/external construction, normalized output validation, total elapsed time
and temporary disk. It uses a 4 MiB payload cache. It excludes download, gzip,
application orchestration, atomic publication and explicit durable-file syncing.
No live service is involved. The baseline must reproduce the retained original
component hash before its conversion path is considered qualified.

Other argument lists:

- `@('native', NATIVE)` opens and fully validates native City, then measures owned
  normalized location lookups using the application decoder.
- `@('converted', NATIVE, COMPONENT)` fully validates an immutable mapped
  component, then measures owned normalized lookups. `NATIVE` is an unused
  positional placeholder in this mode; it is not read.
- `@('parity', NATIVE, COMPONENT)` compares serialized owned location answers for
  100,005 fixed boundary/random queries plus sampled canonical network endpoints
  and their IPv4-compatible, mapped and 6to4 representations. It fails on any
  mismatch. This process retains both readers and is excluded from serving RSS.

Lookup percentiles include per-query clock overhead, record allocation and drop,
but exclude JSON, HTTP and networking. Queries run in a single thread after a
256-query warm-up. Repeated processes are descriptive samples rather than a
capacity or tail-latency guarantee. Full validation touches mapped bytes before
lookups, so these are warm-file observations; cold mappings and >RAM workloads
remain separate tasks.

For P3, prepare with `local` in place of a registry version. Record the Byway
revision, copied source hashes, generated lockfile and executable identity. Run
`convert` against the retained City snapshot, then use the
[shared alias replay](../shared_aliases/README.md) with its snapshot-specific
windows, followed by `rebuild SHARED BITPACKED TEMP_DIRECTORY bitpacked`.
The alias parameters apply only to the retained fixture.

```powershell
./benches/native_city/qualify.ps1 -Executable PATH_TO_ADAPTER_EXE `
  -Native CITY.mmdb -Shared CITY_FORMAT5.nidx -Bitpacked CITY_COMPACT.nidx `
  -OutputDirectory target/city-qualification-new
```

The runner checks normalized parity before timing and rotates the three cases
across three fresh processes per case. It primes the file cache by reading each
input before launching. Reload opens a distinct, hash-verified copy while
retaining the old reader. Its process peak working set includes both readers and
validation; it does not model request concurrency or a changed next snapshot.
Optional `-Trace PATH` supplies one IP per line for both lookup and parity
(parity also checks the boundary corpus). With no trace, the existing fixed
corpus is used. Keep private traces and source artifacts outside Git.
