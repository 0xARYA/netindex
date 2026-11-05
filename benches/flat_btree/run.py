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
snapshot = json.loads((repo / 'target/flat_btree/snapshot.json').read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
for name, expected in snapshot['prepared_sources'].items():
    assert digest(repo / 'target/flat_btree' / name) == expected, name
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
        assert len(measurements) == (80 if scenario in ('v4', 'v6', 'asn') else 40)
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
                  [here/'reader.inc.rs',here/'packed.inc.rs',here/'study_flat_btree.rs',here/'prepare.py',here/'run.py',here/'Cargo.toml',here/'Cargo.lock',
                   here.parent/'interleaved/src/main.rs',here.parent/'interleaved/reader.inc.rs',here.parent/'interleaved/packed.inc.rs',
                   here.parent/'eytzinger/study_eytzinger.rs',here.parent/'cross_query/reader.inc.rs',here.parent/'cross_query/packed.inc.rs',here.parent/'cross_query/study_simd.rs']},
              executable_sha256={name: digest(path) for name, path in executables.items()},
              variants={'0':'ordinary scalar visitor', '108':'P1-02 AVX2 8','112':'P1-02 native u32 ASN / same IP8', '300':'sorted key-only RAM mirror', '301':'Eytzinger candidate-tracking', '302':'Eytzinger arithmetic child/path recovery','400':'flat B+ binary node','401':'flat B+ count node','402':'flat B+ AVX2 node','404':'flat B+ ordered AVX2 mask rank'},
              scope='Research-only packed-fence and ASN rank search. Existing ordinary, cross-query AVX2, sorted-mirror and Eytzinger controls coexist with flat bottom-up B+ node arrays. 64-byte logical nodes: 16 u32 or 4 u128 keys, no cache-line alignment guarantee. Compare binary, count, detected AVX2 count and ordered-mask node search; forced scalar fallback is checked. No rank map in B+ layout: sorted leaves retain original ranks. Original packed tails/decoding/emission and interval/broad-route fallbacks remain unchanged. All arrays coexist; construction excludes lookup timing.',
              timing_scope='Single guest CPU20, private immutable Linux mappings, same frozen original-input oracle harness and traffic/shape fixtures as P1-03. Four forward/reverse trials, one corpus warmup before each timing; 65536 plus boundaries times16 for exact cases, 256 plus boundaries times256 for shapes. Complete query-major borrowed matches and typed errors verified before timing. No PGO/LTO/custom Rust flags, timed allocation, application schema or networking.',
              limitations='Synthetic warm fixed corpora under WSL host uncertainty; throughput cost not latency percentiles or cold/>RAM behavior. Global fence search differs from original IPv4 directory. Natural Vec alignment only, no prefetch, new batching or AVX-512; not a PlanB reproduction or LPM transformation. All old/new arrays resident in all comparisons. Capacity excludes allocator/metadata-object overhead and is not RSS; build includes every layout, not candidate-only startup. No default/API/format adoption implied.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
