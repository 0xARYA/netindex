# Bounded-memory Linux benchmark

This separate package adds no runtime dependency to netindex. Use Rust 1.95.0
on Linux with cgroup-v2 memory already enabled, root access, and a private ext4
work directory. Do not use a host-mounted WSL filesystem for the work copies.
Generate the 20M `traffic` fixtures first (see `BENCHMARKS.md`).

```sh
CARGO_TARGET_DIR=/root/.cache/netindex-perf-20261104/target \
  cargo +1.95.0 build --locked --release --manifest-path benches/memory/Cargo.toml
python3 benches/memory/run.py \
  /root/.cache/netindex-perf-20261104/target/release/netindex-memory-study \
  /root/.cache/netindex-perf-20261104/FRESH-RUN-DIRECTORY \
  benches/results/NEW-memory.json
```

`run.py` creates one private, fsynced file copy and one cgroup per process. It
evicts that file before launch so its cache is charged to the child. CPU 20 is
fixed, swap is disabled only for that cgroup, and memory is 1 GiB for warm/cold
controls or 32 MiB for pressure. It never drops global caches, changes sibling
limits or enables root controllers. Successful and failed processes retain
receipts; private input copies and empty cgroups are removed after exit. Each
child has a three-minute timeout. Use a fresh work-directory name when rerunning.

The reader is fully validated before cache treatment. Warm runs touch every
page and require complete residency. Cold runs discard their mapping's PTEs and
private-file cache, requiring `mincore` to report zero resident pages. Pressure
runs keep post-validation state under the 32 MiB limit and must register memory
limit events. All modes run four disjoint 8,192-query passes. Each query checks
hit count, ID and payload; these assertions remain in the timed loop. Reports
retain per-pass minor/major faults, elapsed time, residency and cgroup counters.

This is controlled guest memory/cache evidence, not physical-media cold latency.
Host VHD/device caches remain uncontrolled. Default mmap readahead is unchanged.
Do not compare these checked Linux loops directly with Windows Criterion timing.
The [recorded results](../results/2026-11-04-memory.json) include two processes
per family/mode, source/manifest/lock/executable hashes and kernel version.
