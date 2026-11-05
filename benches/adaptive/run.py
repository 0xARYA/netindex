"""Linux serial budgeted adaptive-search study: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT."""
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
snapshot = json.loads((repo / 'target/adaptive/snapshot.json').read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
for name, expected in snapshot['prepared_sources'].items():
    assert digest(repo / 'target/adaptive' / name) == expected, name
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
    for budget in ([0,1<<20,128<<20] if ordinal%2==0 else [128<<20,1<<20,0]):
        for profile in (('default', 'simd') if ordinal % 2 == 0 else ('simd', 'default')):
            name = f'{scenario}-{records}-{profile}-b{budget}'
            args = [str(executables[profile]), str(private), scenario, str(records)]
            started = time.monotonic_ns()
            process = subprocess.run(args, capture_output=True, text=True, timeout=300, env={**os.environ,'NETINDEX_STUDY_BUDGET':str(budget)})
            log = work / f'{name}.jsonl'
            log.write_text(process.stdout)
            (work / f'{name}.stderr').write_text(process.stderr)
            if process.returncode:
                raise RuntimeError(f'{name}: {process.stderr}')
            rows = [json.loads(line) for line in process.stdout.splitlines()]
            assert all(row['cross_query_avx2'] for row in rows if row['phase'] == 'checked')
            measurements = [row for row in rows if row['phase'] == 'measure']
            assert len(measurements) == (96 if scenario in ('v4', 'v6', 'asn') else 24)
            runs.append(dict(scenario=scenario, records=records, profile=profile, budget_bytes=budget, arguments=args,
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
                  [here/'reader.inc.rs',here/'packed.inc.rs',here/'study_adaptive.rs',here/'prepare.py',here/'run.py',here/'Cargo.toml',here/'Cargo.lock',
                   here.parent/'flat_btree/study_flat_btree.rs',here.parent/'traversal/btree.inc.rs',
                   here.parent/'interleaved/src/main.rs',here.parent/'interleaved/reader.inc.rs',here.parent/'interleaved/packed.inc.rs',
                   here.parent/'cross_query/reader.inc.rs',here.parent/'cross_query/packed.inc.rs',here.parent/'cross_query/study_simd.rs']},
              executable_sha256={name: digest(path) for name, path in executables.items()},
              budgets_bytes=[0,1<<20,128<<20],
              variants={'0':'ordinary','108':'prior cross-query AVX2','600':'admitted global caches/ordered ASN','601':'directory-aware IPv4/cache count ASN','602':'prefix-sampled adaptive policy','603':'adaptive requests of32 queries'},
              scope='Candidate-only optional additional caches per budgeted process: native u32 sorted IPv4 fences, u128 sorted IPv6 fences and ASN flat B+ leaves/nodes/ranges. Admission priority IPv4/IPv6/ASN, skip zero or unaffordable candidates, aggregate checked before construction and actual capacities checked before publishing. Existing reader directory/validation/mapping/allocator/object memory excluded from additional-vector budget. Policy uses only batch length, first32 query numeric span/family and admitted cache presence; no timing/scenario/traffic label. Late-wide/late-hot workloads expose prefix sampling failures. Original borrowed query-major targets/IDs/payloads/errors remain unchanged.',
              timing_scope='CPU20, portable release/codegen-units1, default/SIMD builds, no PGO/LTO/custom flags. Four forward/reverse trials with one warmup corpus each. Uniform/hot/late-wide/late-hot exact corpora 65536 plus boundaries times16; shapes256 plus boundaries times256. Includes policy prefix scan/dispatch, no timed cache construction. Cache startup per budget after normal reader validation. No other study mirrors resident.',
              limitations='One WSL host, warm synthetic fixed corpora, throughput costs not latency percentiles/cold/>RAM/native-provider/significance/universal capacity. Budget controls extra reported vector-element capacity, not RSS or total reader/allocator/validation peak. Family admission order fixed, not optimal. Prefix32/4096 span heuristic is explicit and vulnerable to unrepresentative prefixes; adversarial controls are separate from uniform/hot. No production default/API/format change or deployed adaptive policy.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
