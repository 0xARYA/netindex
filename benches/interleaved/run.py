"""Linux serial interleaving study: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT."""
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
snapshot = json.loads((repo / 'target/interleaved/snapshot.json').read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
for name, expected in snapshot['prepared_sources'].items():
    assert digest(repo / 'target/interleaved/library' / name) == expected, name
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
        measurements = [row for row in rows if row['phase'] == 'measure']
        assert len(measurements) == (40 if scenario in ('v4', 'v6', 'asn') else 20)
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
              source_sha256={str(p.relative_to(here)): digest(p) for p in
                             [here/'src/main.rs', here/'reader.inc.rs', here/'packed.inc.rs',
                              here/'Cargo.toml', here/'Cargo.lock', here/'prepare.py', here/'run.py']},
              executable_sha256={name: digest(path) for name, path in executables.items()},
              scope='Generated core snapshot with unchanged ordinary entry points and appended experimental methods. Ordinary scalar visitor versus 1/4/8/16-lane round-robin lower-bound ASN and packed-IP fence searches, followed by existing packed tail search and decoding. IPv4 retains its validated directory. Interval/range/overlap and optional broad-route readers use the existing scalar path. Results emit in original query-major order, each preserving sorted target order, IDs, payloads and multiplicity. All source assertions checked against their original generators. Every timed corpus checked against independent original-input oracles and scalar full match sequences; borrowed pointers match. Empty/partial batches and sentinel callback failures checked on interval, packed, broad-route, empty and duplicate-ASN fixtures.',
              timing_scope='One thread on guest CPU 20, warm fully validated private mappings. Four forward/reverse-order trials for scalar and 1/4/8/16 lanes. Exact cases use 65536 rotating queries plus boundary/wrong-family queries with sixteen timed passes; shape cases use 256 plus boundary queries with 256 passes. One warmup pass per measurement. Includes batch setup, visitors, hits/ID/query-index accumulation and black_box targets/payloads. No timed allocation, output buffers, JSON, networking or per-query clock sampling. Aggregate ns/query is throughput cost, not per-query or batch tail latency.',
              limitations='Synchronous fixed groups, not asynchronous AMAC scheduling; no explicit prefetch or cross-query SIMD. Packed tails/payload access and interval traversal remain serial, so this only interleaves part of IP lookup. One-lane control also changes search kernel relative to std partition_point and measures prototype setup overhead. Synthetic 1M/20M exact-key and 65K shape fixtures, fixed warm corpora, four in-process trials and a live WSL host. Not native-provider serving, cold/>RAM, HTTP or production capacity. Experimental API exists only in ignored generated sources, not the library; no adoption decision follows from a single best case.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
