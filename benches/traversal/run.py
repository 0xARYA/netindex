"""Linux serial flat B+ tree RAM-layout study: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT."""
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
snapshot = json.loads((repo / 'target/traversal/snapshot.json').read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
for name, expected in snapshot['prepared_sources'].items():
    assert digest(repo / 'target/traversal' / name) == expected, name
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
        assert len(measurements) == (136 if scenario in ('v4', 'v6', 'asn') else 68)
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
                  [here/'reader.inc.rs',here/'packed.inc.rs',here/'eytzinger.inc.rs',here/'btree.inc.rs',here.parent/'flat_btree/study_flat_btree.rs',here/'prepare.py',here/'run.py',here/'Cargo.toml',here/'Cargo.lock',
                   here.parent/'interleaved/src/main.rs',here.parent/'interleaved/reader.inc.rs',here.parent/'interleaved/packed.inc.rs',
                   here.parent/'eytzinger/study_eytzinger.rs',here.parent/'cross_query/reader.inc.rs',here.parent/'cross_query/packed.inc.rs',here.parent/'cross_query/study_simd.rs']},
              executable_sha256={name: digest(path) for name, path in executables.items()},
              variants={str(v):n for v,n in [(0,'ordinary'),(108,'prior AVX2'),(300,'sorted mirror'),(301,'legacy candidate'),(302,'legacy arithmetic'),(401,'legacy B+ count'),(404,'legacy B+ ordered'),(520,'specialized B+ count'),(521,'specialized B+ AVX2 ordered')] + [(500+i,f'Eyt arithmetic={bool(i&1)} prefetch={bool(i&2)} aligned={bool(i&4)}') for i in range(8)]},
              scope='Research-only 2x2x2 arithmetic-child/prefetch/64-byte-alignment traversal factors on matched 64-byte key groups. Both aligned and natural group copies coexist. Eytzinger original ranks, sorted controls and prior B+ arrays retained. Additional specialized scalar-count and direct AVX2 B+ traversals remove per-node method dispatch and hoist ISA detection outside node loops. Original packed tails, query-major borrowed emissions and interval/broad-route fallback semantics unchanged.',
              timing_scope='CPU20 private Linux mappings; portable release/codegen-units1 without PGO/LTO/custom flags. Four forward/reverse trials with one corpus warmup each. Same complete original-input oracles and fixtures as P1-04. Exact cases:65536 plus boundaries times16; shapes:256 plus boundaries times256. No timed construction/allocation/application schema/networking.',
              limitations='Synthetic warm throughput costs on one WSL host; no latency-percentile/cold/>RAM/significance/universal claims. Global fences differ from original IPv4 directory. New Eyt mirrors use identical grouped accessors; only aligned group type guarantees 64-byte alignment. Prefetch targets one 64-byte descendant group (four ASN/two IP levels ahead), clipped to valid allocation. All arrays coexist within512MiB logical budget; capacities are not RSS, build spans all layouts. B+ specialization combines dispatch/inlining/ISA-hoisting changes; causal attribution requires further ablations. No production API/format/default change.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
