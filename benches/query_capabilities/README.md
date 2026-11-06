# P2 query and application qualification

The standalone application exercises the production reader. It adds no library
dependencies, schema, runtime, release flags or encoded layout. See
[results and decisions](RESULTS.md).

With Rust 1.95.0, Python 3.12+ and `llvm-tools-preview` installed:

```sh
rustup component add llvm-tools-preview --toolchain 1.95.0
python benches/query_capabilities/qualify.py --out target/p2-study-NEW --cpu 20
python benches/query_capabilities/analyze.py target/p2-study-NEW/logs target/p2-study-NEW/logs/summary.json
```

The output directory must be new. Optional repeated `--input PATH.nidx` arguments
add private retained artifacts. Linux uses CPU affinity; Windows sets the child
process affinity immediately after creation, before its measured query loops.
Choose an available logical CPU. The runner compares baseline `8d4f5b1` with the
current working source, uses one codegen unit and explicit host targets, and
records hashes, binary sizes, training summaries, measurements and startup.
It preserves all artifacts in its output directory. Nothing is published.

Normal, ThinLTO, application PGO, and ThinLTO+PGO builds run serially in three
process orders. Each process reverses the method order in its middle round.
The analyzer checks input identities and equivalent normalized checksums across
all measured methods/builds, then reports median/minimum/maximum and sample count.
Profiles use separate fresh directories, the matching compiler's `llvm-profdata`,
and matching generation/use settings. The instrumented application trains with
seed 101, 25% hot selection and one in seven attempted misses; held-out seed 707
uses 10% hot selection and one in three attempted misses. Actual miss rates depend
on target coverage. Training excludes correctness oracles and scan/caller controls.
It retains whole-domain queries and inspection, with fewer repetitions.

The CLI also accepts `verify`, `startup`, `train` and `measure`, followed by shapes
or filenames. The default eight shapes are packed/bitpacked IPv4/IPv6, overlapping
ranges, duplicate ASNs, mixed targets and shared aliases. `verify` compares full
results with a linear oracle; focused tests additionally use original assertions
and independent format fixtures. Timing startup excludes that oracle.

On Linux, counter modes require `perf stat --control` FIFOs. They acknowledge
enable/disable around the kernel after setup and oracle checks:

```sh
mkdir -p target/p2-counter
mkfifo target/p2-counter/command target/p2-counter/ack
NETINDEX_PERF_CTL="$PWD/target/p2-counter/command" \
NETINDEX_PERF_ACK="$PWD/target/p2-counter/ack" \
taskset -c 20 perf stat -D -1 \
  --control "fifo:$PWD/target/p2-counter/command,$PWD/target/p2-counter/ack" \
  -x , -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
  PATH_TO_BINARY counter-point packed-v4
```

Use fresh FIFOs for each run. Other modes are `counter-batch-32`, `counter-member`,
`counter-member-caller`, `counter-range` and `counter-inspect`. Point/batch/member
modes execute 1,048,576 queries, ranges execute 16,384 queries, and inspection
executes 100,000 calls. Hardware/permission support must be checked locally;
generic cache events are not a portable cache-level or bandwidth measurement.

Windows startup JSON records kernel RVAs relative to the loaded image base.
They locate the actual linked functions despite ASLR. PE exception-directory
entries bound each archived disassembly. Linux ELF symbols provide names.
Large binaries, profiles, complete disassemblies and provider files stay private.
The committed `raw` directory contains text evidence only; trailing whitespace
and line endings were normalized. Linux's compiled harness snapshot preserves
its earlier query mix, distinct from the revised Windows PGO protocol.

Source manifests hash compiled working-file bytes before Git newline normalization.
Counter process streams and startup JSONL were consolidated by original filename.

The Windows compiled harness is retained separately. Later import-order cleanup
was rebuilt under all four settings: linked code, writable data, unwind and
relocation sections are byte-identical to the measured binaries; read-only
constant/metadata bytes differ. See `raw/windows/import-equivalence.json`.
