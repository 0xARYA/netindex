"""Verify retained evidence and emit a compact, reproducible measurements table."""
import argparse
import collections
import hashlib
import json
import pathlib
import statistics

p = argparse.ArgumentParser()
p.add_argument('directory', type=pathlib.Path)
p.add_argument('--output', type=pathlib.Path, required=True)
args = p.parse_args()
root = args.directory
receipt = json.loads((root/'receipt.json').read_text())
for name, expected in receipt['files'].items():
    path = root/name
    assert path.is_file(), f'missing evidence: {name}'
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, name
checks, kernel_checks = [], []
groups = collections.defaultdict(lambda: collections.defaultdict(list))
kernel = collections.defaultdict(lambda: collections.defaultdict(list))
coordinates = set()
for path in sorted(root.glob('*.jsonl')):
    repeat = int(path.stem.rsplit('-r',1)[1])
    prechecks = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        phase = row['phase']
        if phase == 'checked':
            assert row['full_assertions_checked'] == row['records']
            assert row['borrowed_pointer_parity'] and row['partial_and_empty_batches']
            checks.append(row)
            prechecks[row['traffic']] = row
        elif phase == 'kernel_checked':
            assert row['alignments'] == 64 and row['forced_portable_fallback']
            assert row['avx512'] if receipt['machine']=='x86_64' else row['neon']
            kernel_checks.append(row)
        elif phase == 'measure':
            checked = prechecks[row['traffic']]
            passes = 256 if checked['records']==65536 else 16
            assert row['queries']==checked['corpus_queries']*passes
            assert row['matches']==checked['matches']*passes
            assert row['checksum']==(checked['checksum']*passes) % (1<<64)
            key = (row['scenario'], int(path.stem.rsplit('-r',1)[0].rsplit('-',1)[1]), row['traffic'])
            coordinate = (phase, repeat, key, row['trial'],row['lanes'])
            assert coordinate not in coordinates
            coordinates.add(coordinate)
            assert abs(row['ns_per_query']-row['elapsed_ns']/row['queries']) < 1e-9
            groups[key][row['lanes']].append(row['ns_per_query'])
        elif phase == 'kernel_measure':
            key = (row['width'],row['count'],row['traffic'])
            coordinate = (phase,repeat,key,row['trial'],row['mode'])
            assert coordinate not in coordinates
            coordinates.add(coordinate)
            assert abs(row['ns_per_query']-row['elapsed_ns']/row['queries']) < 1e-9
            kernel[key][row['mode']].append(row['ns_per_query'])
        else:
            raise AssertionError(phase)
assert len(groups)==22 and len(kernel)==50
assert len(checks)==22*receipt['repeats']
assert len(kernel_checks)==receipt['repeats']
expected_variants = {0,8,800,801} if receipt['machine']=='x86_64' else {0,8,800,802,803}
for variants in groups.values():
    assert set(variants)==expected_variants
    assert all(len(values)==4*receipt['repeats'] for values in variants.values())
expected_modes = {0,1,2} if receipt['machine']=='x86_64' else {0,1,3,4}
for variants in kernel.values():
    assert set(variants)==expected_modes
    assert all(len(values)==4*receipt['repeats'] for values in variants.values())
def table(groups):
    return [{'case':list(key),'variants':{str(v):{'median_ns':statistics.median(ns), 'min_ns':min(ns),'max_ns':max(ns),
        'process_medians_ns':[statistics.median(ns[i:i+4]) for i in range(0,len(ns),4)]}
        for v,ns in sorted(variants.items())}} for key,variants in sorted(groups.items())]
output = {'machine':receipt['machine'],'cpu':receipt['cpu'],'revision':receipt['revision'],
    'receipt_sha256':hashlib.sha256((root/'receipt.json').read_bytes()).hexdigest(),
    'files_verified':len(receipt['files']),'processes':len(list(root.glob('*.jsonl'))),
    'timings':len(coordinates),'full_contract_prechecks':len(checks),
    'kernel_assertions':sum(row['checks'] for row in kernel_checks),
    'end_to_end':table(groups),'kernels':table(kernel)}
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({k:v for k,v in output.items() if k not in ['end_to_end','kernels']}))
