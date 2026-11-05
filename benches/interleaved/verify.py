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
assert digest(here / 'inspect.py') == asm['inspection_source_sha256']
for name, expected in result['source_sha256'].items():
    assert digest(here / name) == expected, name
snapshot = result['snapshot']
for name, expected in snapshot['original_sources'].items():
    assert digest(repo / name) == expected, name
prepared = repo / 'target/interleaved/library'
for name, expected in snapshot['prepared_sources'].items():
    assert digest(prepared / name) == expected, name
for name in ['reader', 'packed']:
    original = (repo / f'src/{name}.rs').read_text()
    fragment = (here / f'{name}.inc.rs').read_text()
    marker = original.find('#[cfg(test)]')
    expected = original + '\n' + fragment if marker < 0 else original[:marker] + fragment + '\n' + original[marker:]
    assert (prepared / f'src/{name}.rs').read_text() == expected
count = 0
for collection, log_root in [(result, work), (supplemental, repeat_work)]:
    for run in collection['runs']:
        assert run['exit_code'] == 0
        executable, source = map(pathlib.Path, run['arguments'][:2])
        profile = run.get('profile', 'default')
        assert digest(executable) == result['executable_sha256'][profile]
        assert digest(source) == run['input_sha256']
        name = f"{run['scenario']}-{run['records']}-{profile}" if 'profile' in run else run['scenario']
        raw = log_root / f'{name}.jsonl'
        assert digest(raw) == run['raw_log_sha256']
        assert [json.loads(line) for line in raw.read_text().splitlines()] == run['rows']
        checks = {row['traffic']: row for row in run['rows'] if row['phase'] == 'checked'}
        exact = run['scenario'] in ('v4', 'v6', 'asn')
        assert set(checks) == ({'uniform', 'hot'} if exact else {'uniform'})
        for check in checks.values():
            assert check['full_assertions_checked'] == int(run['arguments'][3])
            assert check['borrowed_pointer_parity'] and check['partial_and_empty_batches']
            assert check['variants'] == [0, 1, 4, 8, 16]
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
        assert coordinates == {(t, trial, lanes) for t in checks for trial in range(1, 5) for lanes in [0, 1, 4, 8, 16]}
assert len(result['runs']) == 32 and len(supplemental['runs']) == 2 and count == 960
for row in asm['rows']:
    assert row['executable_sha256'] == result['executable_sha256'][row['profile']]
    assert digest(work / f"disassembly-{row['profile']}.txt") == row['disassembly_sha256']
normalized = {name: hashlib.sha256((here / name).read_bytes().replace(b'\r\n', b'\n')).hexdigest()
              for name in result['source_sha256']}
output.write_text(json.dumps(dict(date='2026-11-04', verified_processes=34, verified_measurements=count,
    receipt_sha256={p.name: digest(p) for p in [primary, repeat, assembly]},
    verifier_sha256=digest(pathlib.Path(__file__)), source_sha256_lf=normalized,
    checks='Original/prepared source hashes and exact fragment reconstruction; all benchmark/runner/dependency-lock/executable/input/raw-log hashes; exact measurement coordinates and checked matches/checksums scaled by repetitions; derived ns/query; repeat and assembly linkage. Fresh prepared Cargo.lock generated for unit tests is build auxiliary output outside the recorded source snapshot. LF-normalized benchmark source hashes support Git checkout comparisons; raw measurement hashes remain in primary receipt.',
    limits='Verifies retained local evidence integrity and internal consistency; does not rerun executables, independently establish timing accuracy, validate all possible queries or generalize to other machines/workloads.'), indent=2)+'\n')
print(output)
