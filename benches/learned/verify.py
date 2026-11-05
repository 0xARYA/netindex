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
prepared = repo / 'target/learned'
for name, expected in snapshot['prepared_sources'].items():
    assert digest(prepared / name) == expected, name
# Independently rerun preparation in a disposable shadow checkout, then compare
# the entire declared generated source tree. No recorded measurement is rerun.
import shutil
import subprocess
import tempfile
with tempfile.TemporaryDirectory(prefix='learned-repro-', dir=repo/'target') as temporary:
    shadow = pathlib.Path(temporary)
    shutil.copytree(repo/'src', shadow/'src')
    for name in ['README.md', 'Cargo.toml']:
        shutil.copyfile(repo/name, shadow/name)
    (shadow/'benches').mkdir()
    for name in ['interleaved', 'cross_query', 'learned']:
        shutil.copytree(repo/'benches'/name, shadow/'benches'/name)
    subprocess.run(['python3', str(shadow/'benches/learned/prepare.py')], check=True, capture_output=True)
    recreated = json.loads((shadow/'target/learned/snapshot.json').read_text())
    assert recreated['original_sources'] == snapshot['original_sources']
    assert recreated['prepared_sources'] == snapshot['prepared_sources']
assert result['epsilon_values']==[4,32,128]
def coordinate(run):
    return run['scenario'],run['records'],run['profile'],run['epsilon']
original={coordinate(run):run for run in result['runs']}
repeated={coordinate(run):run for run in supplemental['runs']}
assert len(original)==144 and len(repeated)==66
assert set(repeated)=={key for key in original if key[1]==20_000_000 or key[0].startswith(('curved-','clustered-'))}
for key,run in repeated.items():
    assert run['arguments']==original[key]['arguments']
    assert run['input_sha256']==original[key]['input_sha256']
for scenario,records,profile,_ in original:
    assert {key[3] for key in original if key[:3]==(scenario,records,profile)}==set(result['epsilon_values'])
count = 0
for collection, log_root in [(result, work), (supplemental, repeat_work)]:
    for run in collection['runs']:
        assert run['exit_code'] == 0
        executable, source = map(pathlib.Path, run['arguments'][:2])
        profile = run.get('profile', 'default')
        assert digest(executable) == result['executable_sha256'][profile]
        assert digest(source) == run['input_sha256']
        name = f"{run['scenario']}-{run['records']}-{profile}" if collection is result else f"{run['scenario']}-{profile}"
        raw = log_root / f"{name}-e{run['epsilon']}.jsonl"
        assert digest(raw) == run['raw_log_sha256']
        assert [json.loads(line) for line in raw.read_text().splitlines()] == run['rows']
        caches = [row for row in run['rows'] if row['phase']=='cache']
        assert len(caches)==1
        cache=caches[0]
        with source.open('rb') as stream:
            header=stream.read(80)
            field=lambda offset:int.from_bytes(header[offset:offset+8],'little')
            flags=int.from_bytes(header[12:16],'little')
            nv4,nv6,nasn=field(24),field(32),field(40)
            off4=80+field(48)
            off6=off4+(field(64) if flags&1 else nv4*20)
            offasn=off6+(field(72) if flags&2 else nv6*56)
            def endpoints(offset,count,stride,width):
                if not count:return 0,0
                stream.seek(offset);first=int.from_bytes(stream.read(width),'little')
                stream.seek(offset+(count-1)*stride);last=int.from_bytes(stream.read(width),'little')
                return first,last
            counts=[nasn,(nv4+255)//256 if flags&1 else 0,(nv6+255)//256 if flags&2 else 0]
            edges=[endpoints(offasn,nasn,8,4),endpoints(off4,counts[1],32,16),endpoints(off6,counts[2],32,16)]
        if edges[2][1]-edges[2][0]>2**64-1:counts[2]=0
        remaining=128<<20
        assert cache['budget_bytes']==remaining and cache['epsilon']==run['epsilon']
        assert len(cache['allocation_bytes'])==9 and len(cache['dimensions'])==3
        for index in [1,2,0]:
            width=8 if index==2 else 4
            expected_count=counts[index] if counts[index]*width<=remaining else 0
            nk,ns,nr=cache['dimensions'][index]
            key_bytes,segment_bytes,radix_bytes=cache['allocation_bytes'][index*3:index*3+3]
            assert nk==expected_count and key_bytes==nk*width
            if not nk:
                assert ns==nr==segment_bytes==radix_bytes==0
                continue
            assert 0<ns<=nk
            capacity=0
            maximum=(remaining-key_bytes)//40
            assert maximum>=ns
            while capacity<ns:capacity=min(max(1,capacity)*2,maximum)
            assert segment_bytes==capacity*40
            span=edges[index][1]-edges[index][0]
            bits=min((ns-1).bit_length(),12)
            expected_radix=0 if ns==1 else (span>>max(0,span.bit_length()-bits))+2
            assert nr==expected_radix and radix_bytes==nr*4
            remaining-=key_bytes+segment_bytes+radix_bytes
            assert remaining>=0
        assert cache['build_ns']>0
        checks = {row['traffic']: row for row in run['rows'] if row['phase'] == 'checked'}
        exact = run['scenario'] in ('v4', 'v6', 'asn')
        assert set(checks) == ({'uniform', 'hot'} if exact else {'uniform'})
        for check in checks.values():
            assert check['full_assertions_checked'] == int(run['arguments'][3])
            assert check['borrowed_pointer_parity'] and check['partial_and_empty_batches'] and check['forced_scalar_fallback'] and check['cross_query_avx2']
            assert check['variants'] == [0,108,700,701,702]
        for traffic in checks:
            probes=[row for row in run['rows'] if row['phase']=='probe' and row['traffic']==traffic]
            assert len(probes)==2 and {p['method'] for p in probes}=={701,702}
            assert probes[0]['stats']==probes[1]['stats']
            eligible,fallback,worksum,maxwindow=probes[0]['stats']
            assert 0<=fallback<=eligible<=checks[traffic]['corpus_queries']
            assert 0<=maxwindow<=2*run['epsilon']+5
            assert 0<=worksum<=eligible*maxwindow
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
        assert coordinates == {(t, trial, lanes) for t in checks for trial in range(1, 5) for lanes in [0,108,700,701,702]}
assert len(result['runs']) == 144 and len(supplemental['runs']) == 66 and count == 5280
for row in asm['rows']:
    assert row['executable_sha256'] == result['executable_sha256'][row['profile']]
    assert digest(work / f"disassembly-{row['profile']}.txt") == row['disassembly_sha256']
# Rebuild the additional input files with the retained, hash-linked generator.
generator=pathlib.Path(result['runs'][0]['arguments'][0]).with_name('netindex-learned-fixtures')
with tempfile.TemporaryDirectory(prefix='learned-fixtures-repro-',dir=repo/'target') as temporary:
    regenerated=pathlib.Path(temporary)/'files'
    subprocess.run([str(generator),str(regenerated)],check=True)
    nonlinear={pathlib.Path(r['arguments'][1]).name:r['input_sha256'] for r in result['runs'] if r['scenario'].startswith(('curved-','clustered-'))}
    assert len(nonlinear)==8
    assert {p.name:digest(p) for p in regenerated.iterdir()}==nonlinear
fixture_provenance=dict(executable=str(generator),executable_sha256=digest(generator),
    source_sha256=result['source_sha256']['benches/learned/fixtures.rs'],input_sha256=nonlinear,
    check='All eight curved/clustered input files reproduced byte-for-byte using retained generator.')
normalized = {name: hashlib.sha256((repo / name).read_bytes().replace(b'\r\n', b'\n')).hexdigest()
              for name in result['source_sha256']}
output.write_text(json.dumps(dict(date='2026-11-04', verified_processes=210, verified_measurements=count,
    receipt_sha256={p.name: digest(p) for p in [primary, repeat, assembly]},
    verifier_sha256=digest(pathlib.Path(__file__)), source_sha256_lf=normalized, fixture_provenance=fixture_provenance,
    checks='Original/prepared source hashes and full fresh shadow-checkout reproduction; all benchmark/runner/dependency-lock/executable/input/raw-log hashes; header-derived native widths/domain eligibility, geometric segment capacities, radix dimensions, aggregate budget and binary/radix probe parity; exact measurement coordinates and checked matches/checksums scaled by repetitions; derived ns/query; repeat and assembly linkage. Fresh prepared Cargo.lock generated for unit tests is build auxiliary output outside the recorded source snapshot. LF-normalized benchmark source hashes support Git checkout comparisons; raw measurement hashes remain in primary receipt.',
    limits='Verifies retained local evidence integrity and internal consistency; does not rerun benchmark executables, independently establish timing accuracy, validate all possible queries or generalize to other machines/workloads.'), indent=2)+'\n')
print(output)
