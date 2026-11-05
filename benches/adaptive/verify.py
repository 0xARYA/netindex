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
prepared = repo / 'target/adaptive'
for name, expected in snapshot['prepared_sources'].items():
    assert digest(prepared / name) == expected, name
# Independently rerun preparation in a disposable shadow checkout, then compare
# the entire declared generated source tree. No recorded measurement is rerun.
import shutil
import subprocess
import tempfile
with tempfile.TemporaryDirectory(prefix='adaptive-repro-', dir=repo/'target') as temporary:
    shadow = pathlib.Path(temporary)
    shutil.copytree(repo/'src', shadow/'src')
    for name in ['README.md', 'Cargo.toml']:
        shutil.copyfile(repo/name, shadow/name)
    (shadow/'benches').mkdir()
    for name in ['interleaved', 'cross_query', 'eytzinger', 'flat_btree', 'traversal', 'adaptive']:
        shutil.copytree(repo/'benches'/name, shadow/'benches'/name)
    subprocess.run(['python3', str(shadow/'benches/adaptive/prepare.py')], check=True, capture_output=True)
    recreated = json.loads((shadow/'target/adaptive/snapshot.json').read_text())
    assert recreated['original_sources'] == snapshot['original_sources']
    assert recreated['prepared_sources'] == snapshot['prepared_sources']
assert result['budgets_bytes']==[0,1<<20,128<<20]
def coordinate(run):
    return run['scenario'],run['records'],run['profile'],run['budget_bytes']
original={coordinate(run):run for run in result['runs']}
repeated={coordinate(run):run for run in supplemental['runs']}
assert len(original)==96 and len(repeated)==18
assert set(repeated)=={key for key in original if key[1]==20_000_000}
for key,run in repeated.items():
    assert run['arguments']==original[key]['arguments']
    assert run['input_sha256']==original[key]['input_sha256']
for scenario,records,profile,_ in original:
    assert {key[3] for key in original if key[:3]==(scenario,records,profile)}==set(result['budgets_bytes'])
count = 0
for collection, log_root in [(result, work), (supplemental, repeat_work)]:
    for run in collection['runs']:
        assert run['exit_code'] == 0
        executable, source = map(pathlib.Path, run['arguments'][:2])
        profile = run.get('profile', 'default')
        assert digest(executable) == result['executable_sha256'][profile]
        assert digest(source) == run['input_sha256']
        name = f"{run['scenario']}-{run['records']}-{profile}" if collection is result else f"{run['scenario']}-{profile}"
        raw = log_root / f"{name}-b{run['budget_bytes']}.jsonl"
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
        n=counts[0]
        nodes=depth=0
        while n>16:
            n=(n+15)//16
            nodes+=n
            depth+=1
        asn=[counts[0]*4,nodes*4,depth*16]
        expected=[0]*5
        remaining=run['budget_bytes']
        for positions,cost in [([3],counts[1]*4),([4],counts[2]*16),([0,1,2],sum(asn))]:
            if 0<cost<=remaining:
                remaining-=cost
                for index in positions:
                    expected[index]=asn[index] if index<3 else cost
        assert cache['allocation_bytes']==expected
        assert sum(expected)<=cache['budget_bytes']==run['budget_bytes']
        assert cache['build_ns']>0
        checks = {row['traffic']: row for row in run['rows'] if row['phase'] == 'checked'}
        exact = run['scenario'] in ('v4', 'v6', 'asn')
        assert set(checks) == ({'uniform', 'hot', 'late-wide', 'late-hot'} if exact else {'uniform'})
        for check in checks.values():
            assert check['full_assertions_checked'] == int(run['arguments'][3])
            assert check['borrowed_pointer_parity'] and check['partial_and_empty_batches'] and check['forced_scalar_fallback'] and check['cross_query_avx2']
            assert check['variants'] == [0, 108, 600, 601, 602, 603]
        plans={row['traffic']:row['selected'] for row in run['rows'] if row['phase']=='plan'}
        assert set(plans)==set(checks)
        for traffic,selected in plans.items():
            values=[]
            scenario=run['scenario']
            records=run['records']
            for index in range(32):
                local=traffic in ['hot','late-wide']
                limit=min(records,1024) if local else records
                ident=((index*2654435761)&0xffffffff)%limit
                miss=index%2
                if scenario in ['v4','v6','asn']:
                    value=ident*2+2+miss
                elif scenario.startswith('dense-asn'):
                    value=ident+1+miss*records
                elif scenario.startswith('sparse-asn'):
                    value=ident*65536+1+miss
                elif scenario.startswith('sparse-v6'):
                    value=(ident<<112)|ident|(miss<<32)
                elif scenario.startswith('deep-v6'):
                    value=((ident//65)*2+miss)<<64
                else:
                    value=(ident//64)*2+2+miss
                values.append(value)
            family=0 if 'asn' in scenario else (2 if 'v6' in scenario else 1)
            present=expected[[0,3,4][family]]>0
            wanted=0 if max(values)-min(values)<=4096 else (601 if family==1 else 600) if present else 108
            assert selected==wanted,(scenario,traffic,selected,wanted)
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
        assert coordinates == {(t, trial, lanes) for t in checks for trial in range(1, 5) for lanes in [0, 108, 600, 601, 602, 603]}
assert len(result['runs']) == 96 and len(supplemental['runs']) == 18 and count == 6624
for row in asm['rows']:
    assert row['executable_sha256'] == result['executable_sha256'][row['profile']]
    assert digest(work / f"disassembly-{row['profile']}.txt") == row['disassembly_sha256']
normalized = {name: hashlib.sha256((repo / name).read_bytes().replace(b'\r\n', b'\n')).hexdigest()
              for name in result['source_sha256']}
output.write_text(json.dumps(dict(date='2026-11-04', verified_processes=114, verified_measurements=count,
    receipt_sha256={p.name: digest(p) for p in [primary, repeat, assembly]},
    verifier_sha256=digest(pathlib.Path(__file__)), source_sha256_lf=normalized,
    checks='Original/prepared source hashes and full fresh shadow-checkout reproduction; all benchmark/runner/dependency-lock/executable/input/raw-log hashes; independent aggregate admission/capacity budget model and prefix-sample policy traces; exact measurement coordinates and checked matches/checksums scaled by repetitions; derived ns/query; repeat and assembly linkage. Fresh prepared Cargo.lock generated for unit tests is build auxiliary output outside the recorded source snapshot. LF-normalized benchmark source hashes support Git checkout comparisons; raw measurement hashes remain in primary receipt.',
    limits='Verifies retained local evidence integrity and internal consistency; does not rerun benchmark executables, independently establish timing accuracy, validate all possible queries or generalize to other machines/workloads.'), indent=2)+'\n')
print(output)
