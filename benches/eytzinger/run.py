"""Linux serial Eytzinger RAM-layout study: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT."""
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import time

default, simd, work, destination = map(pathlib.Path, sys.argv[1:])
executables = {'default': default.resolve(strict=True), 'simd': simd.resolve(strict=True)}
work.mkdir(parents=True, exist_ok=False)
if work.stat().st_dev == pathlib.Path('/mnt/c').stat().st_dev:
    raise SystemExit('use a private Linux filesystem directory')
if 20 not in os.sched_getaffinity(0) or os.getenv('RUSTFLAGS') or os.getenv('CARGO_ENCODED_RUSTFLAGS'):
    raise SystemExit('guest CPU 20 and an unmodified build profile are required')
here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
snapshot = json.loads((repo / 'target/eytzinger/snapshot.json').read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
for name, expected in snapshot['prepared_sources'].items():
    assert digest(repo / 'target/eytzinger' / name) == expected, name
cases = [(family, records, repo / f'target/traffic-fixtures/{family}-{records}.nidx')
         for records in (1_000_000, 20_000_000) for family in ('v4', 'v6', 'asn')]
cases += [(f'{shape}-{payload}', 65_536, repo / f'target/shape-fixtures/{shape}-{payload}.nidx')
          for shape in ('dense-asn', 'sparse-asn', 'sparse-v6', 'deep-v6', 'duplicates-v4')
          for payload in ('shared', 'unique')]
runs = []
for ordinal, (scenario, records, source) in enumerate(cases):
    private = work / source.name
    shutil.copyfile(source, private)
    original_hash = digest(source)
    assert digest(private) == original_hash
    for profile in (('default', 'simd') if ordinal % 2 == 0 else ('simd', 'default')):
        name = f'{scenario}-{records}-{profile}'
        args = [str(executables[profile]), str(private), scenario, str(records)]
        started = time.monotonic_ns()
        process = subprocess.run(args, capture_output=True, text=True, timeout=300)
        log = work / f'{name}.jsonl'
        log.write_text(process.stdout)
        (work / f'{name}.stderr').write_text(process.stderr)
        if process.returncode:
            raise RuntimeError(f'{name}: {process.stderr}')
        rows = [json.loads(line) for line in process.stdout.splitlines()]
        assert all(row['cross_query_avx2'] for row in rows if row['phase'] == 'checked')
        measurements = [row for row in rows if row['phase'] == 'measure']
        assert len(measurements) == (64 if scenario in ('v4', 'v6', 'asn') else 32)
        runs.append(dict(scenario=scenario, records=records, profile=profile, arguments=args,
                         input_bytes=private.stat().st_size, input_sha256=original_hash,
                         raw_log_sha256=digest(log), exit_code=process.returncode,
                         wall_ns=time.monotonic_ns()-started, rows=rows))
        print(name, round(runs[-1]['wall_ns']/1e9, 3), flush=True)
    assert digest(private) == original_hash

result = dict(date='2026-11-04', snapshot=snapshot,
              toolchain=subprocess.check_output(['rustc', '+1.95.0', '-Vv'], text=True),
              kernel=platform.release(), architecture=platform.machine(),
              cpuinfo=pathlib.Path('/proc/cpuinfo').read_text().split('\n\n')[0],
              topology=subprocess.check_output(['lscpu', '-e=CPU,CORE,SOCKET,NODE'], text=True),
              affinity_cpu=20, profile='release codegen-units=1; default/mmap and simd/mmap; portable compiler target; no PGO/LTO/custom Rust flags',
              source_sha256={str(p.relative_to(repo)): digest(p) for p in
                  [here/'reader.inc.rs',here/'packed.inc.rs',here/'study_eytzinger.rs',here/'prepare.py',here/'run.py',here/'Cargo.toml',here/'Cargo.lock',
                   here.parent/'interleaved/src/main.rs',here.parent/'interleaved/reader.inc.rs',here.parent/'interleaved/packed.inc.rs',
                   here.parent/'cross_query/reader.inc.rs',here.parent/'cross_query/packed.inc.rs',here.parent/'cross_query/study_simd.rs']},
              executable_sha256={name: digest(path) for name, path in executables.items()},
              variants={'0':'ordinary scalar visitor', '8':'P1-01 interleaved 8','16':'P1-01 interleaved 16','108':'P1-02 AVX2 8','112':'P1-02 native u32 ASN / same IP8', '300':'sorted key-only RAM mirror', '301':'Eytzinger candidate-tracking', '302':'Eytzinger arithmetic child/path recovery'},
              scope='Generated isolated core with ordinary sorted on-disk bytes unchanged. Cache constructs sorted key-only, Eytzinger key and u32 original-rank arrays for ASNs and packed IP fences. Sorted mirror and both Eytzinger methods use identical key width and global fence scope; original IPv4 scalar/interleaved/SIMD paths retain their directory, while new mirrors search all fences. ASN lower bounds return original sorted ranks and emit every duplicate from original rows. IP upper bounds select original packed blocks, then use unchanged tail search/decoding/payload emission. Interval and optional broad-route readers use existing visits. All original assertions and complete query match/payload/pointer/error semantics checked as in P1-02. Cache build elapsed and allocated capacities are reported before lookup timing; all comparator methods share the additional resident caches.',
              timing_scope='Single guest CPU 20; warm validated immutable private Linux mappings; 65536 queries plus boundary/wrong-family queries, sixteen passes for exact 1M/20M cases; 256 queries plus boundaries and 256 passes for 65K shapes. Four forward/reverse-order trials; one warmup pass before each measurement. Includes setup, ISA checks, bounds checks, query-major visitors and black_box target/payload; no timed allocation, decoding to application schema, JSON or networking.',
              limitations='Research RAM mirrors only, no on-disk reordering or format migration. Layout-only baseline is key-only sorted mirror; comparison with ordinary encoded reader also changes key density, RAM copies and IPv4 routing scope. Heap keys have natural Vec alignment, no guaranteed cache-line alignment, no explicit prefetch or added SIMD. Sorted plus Eytzinger keys/ranks coexist for fair paired runs; cache capacities are not RSS/allocator/validation peak. Requested logical cache allocations bounded at 512MiB; startup includes all mirrors, not independent candidate-only builds. Synthetic warm fixed corpora on WSL CPU20, four trials and host uncertainty. No universal advantage, cold/>RAM/native-provider capacity or public API adoption claim.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
