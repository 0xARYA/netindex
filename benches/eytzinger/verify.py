"""Verify retained local receipts: PRIMARY REPEAT ASM PRIMARY_WORK REPEAT_WORK OUTPUT."""
import hashlib
import json
import pathlib
import sys

primary, repeat, assembly, work, repeat_work, output = map(pathlib.Path, sys.argv[1:])
here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


result, supplemental, asm = [json.loads(p.read_text()) for p in [primary, repeat, assembly]]
assert digest(primary) == supplemental['primary_sha256'] == asm['primary_sha256']
assert digest(here / 'repeat.py') == supplemental['repeat_source_sha256']
assert digest(here / 'inspect_asm.py') == asm['inspection_source_sha256']
for name, expected in result['source_sha256'].items():
    assert digest(repo / name) == expected, name
snapshot = result['snapshot']
for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
prepared = repo / 'target/eytzinger'
for name, expected in snapshot['prepared_sources'].items():
    assert digest(prepared / name) == expected, name
# Independently rerun preparation in a disposable shadow checkout, then compare
# the entire declared generated source tree. No recorded measurement is rerun.
import shutil
import subprocess
import tempfile
with tempfile.TemporaryDirectory(prefix='cross-query-repro-', dir=repo/'target') as temporary:
    shadow = pathlib.Path(temporary)
    shutil.copytree(repo/'src', shadow/'src')
    for name in ['README.md', 'Cargo.toml']:
        shutil.copyfile(repo/name, shadow/name)
    (shadow/'benches').mkdir()
    for name in ['interleaved', 'cross_query', 'eytzinger']:
        shutil.copytree(repo/'benches'/name, shadow/'benches'/name)
    subprocess.run(['python3', str(shadow/'benches/eytzinger/prepare.py')], check=True, capture_output=True)
    recreated = json.loads((shadow/'target/eytzinger/snapshot.json').read_text())
    assert recreated['original_sources'] == snapshot['original_sources']
    assert recreated['prepared_sources'] == snapshot['prepared_sources']
count = 0
for collection, log_root in [(result, work), (supplemental, repeat_work)]:
    for run in collection['runs']:
        assert run['exit_code'] == 0
        executable, source = map(pathlib.Path, run['arguments'][:2])
        profile = run.get('profile', 'default')
        assert digest(executable) == result['executable_sha256'][profile]
        assert digest(source) == run['input_sha256']
        name = f"{run['scenario']}-{run['records']}-{profile}" if collection is result else f"{run['scenario']}-{profile}"
        raw = log_root / f'{name}.jsonl'
        assert digest(raw) == run['raw_log_sha256']
        assert [json.loads(line) for line in raw.read_text().splitlines()] == run['rows']
        caches = [row for row in run['rows'] if row['phase']=='cache']
        assert len(caches)==1
        cache=caches[0]
        with source.open('rb') as stream:
            header=stream.read(80)
        flags=int.from_bytes(header[12:16],'little')
        counts=[int.from_bytes(header[40:48],'little'),
                (int.from_bytes(header[24:32],'little')+255)//256 if flags&1 else 0,
                (int.from_bytes(header[32:40],'little')+255)//256 if flags&2 else 0]
        expected=[]
        for entries,width in zip(counts,[4,16,16]):
            expected += [entries*width,(entries+1)*width,(entries+1)*4]
        assert cache['allocation_bytes']==expected
        assert sum(expected)<=cache['budget_bytes']==512*1024*1024
        assert cache['build_ns']>0
        checks = {row['traffic']: row for row in run['rows'] if row['phase'] == 'checked'}
        exact = run['scenario'] in ('v4', 'v6', 'asn')
        assert set(checks) == ({'uniform', 'hot'} if exact else {'uniform'})
        for check in checks.values():
            assert check['full_assertions_checked'] == int(run['arguments'][3])
            assert check['borrowed_pointer_parity'] and check['partial_and_empty_batches'] and check['forced_scalar_fallback'] and check['cross_query_avx2']
            assert check['variants'] == [0, 8, 16, 108, 112, 300, 301, 302]
        coordinates = set()
        for row in run['rows']:
            if row['phase'] != 'measure':
                continue
            coordinate = (row['traffic'], row['trial'], row['lanes'])
            assert coordinate not in coordinates
            coordinates.add(coordinate)
            check = checks[row['traffic']]
            passes = 16 if exact else 256
            for metric in ['matches', 'checksum']:
                assert row[metric] == check[metric] * passes
            assert row['queries'] == check['corpus_queries'] * passes
            assert abs(row['ns_per_query'] - row['elapsed_ns'] / row['queries']) < 1e-9
            count += 1
        assert coordinates == {(t, trial, lanes) for t in checks for trial in range(1, 5) for lanes in [0, 8, 16, 108, 112, 300, 301, 302]}
assert len(result['runs']) == 32 and len(supplemental['runs']) == 6 and count == 1792
for row in asm['rows']:
    assert row['executable_sha256'] == result['executable_sha256'][row['profile']]
    assert digest(work / f"disassembly-{row['profile']}.txt") == row['disassembly_sha256']
normalized = {name: hashlib.sha256((repo / name).read_bytes().replace(b'\r\n', b'\n')).hexdigest()
              for name in result['source_sha256']}
output.write_text(json.dumps(dict(date='2026-11-04', verified_processes=38, verified_measurements=count,
    receipt_sha256={p.name: digest(p) for p in [primary, repeat, assembly]},
    verifier_sha256=digest(pathlib.Path(__file__)), source_sha256_lf=normalized,
    checks='Original/prepared source hashes and full fresh shadow-checkout reproduction; all benchmark/runner/dependency-lock/executable/input/raw-log hashes; header-derived cache capacity accounting/budget; exact measurement coordinates and checked matches/checksums scaled by repetitions; derived ns/query; repeat and assembly linkage. Fresh prepared Cargo.lock generated for unit tests is build auxiliary output outside the recorded source snapshot. LF-normalized benchmark source hashes support Git checkout comparisons; raw measurement hashes remain in primary receipt.',
    limits='Verifies retained local evidence integrity and internal consistency; does not rerun benchmark executables, independently establish timing accuracy, validate all possible queries or generalize to other machines/workloads.'), indent=2)+'\n')
print(output)
