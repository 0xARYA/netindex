"""Collect paired traffic or shape Criterion baselines with input provenance.

Usage: python benches/collect_matrix.py traffic|shapes RUN_DIRECTORY OUTPUT_JSON
Run from the repository root, after both serial passes have finished.
"""
import hashlib
import json
import pathlib
import sys

kind, directory, destination = sys.argv[1:]
if kind not in ('traffic', 'shapes'):
    raise SystemExit('unknown matrix')
directory, destination = pathlib.Path(directory), pathlib.Path(destination)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


processes = []
for trial in (1, 2):
    run = json.loads((directory / f'{kind}-{trial}.json').read_text(encoding='utf-8-sig'))
    if run['exit_code'] != 0:
        raise ValueError('failed process')
    run['raw_log_sha256'] = digest(directory / f'{kind}-{trial}.log')
    processes.append(run)
if processes[0]['executable_sha256'] != processes[1]['executable_sha256']:
    raise ValueError('benchmark executable changed')
group = pathlib.Path(f'target/criterion/lookup-{kind}')
rows = []
for path in sorted(group.glob(f'*/*/{kind}-20261104-1/benchmark.json')):
    benchmark = json.loads(path.read_text())
    name, value = benchmark['function_id'], int(benchmark['value_str'])
    queries = benchmark['throughput']['Elements']
    estimates, samples = [], []
    for trial in (1, 2):
        baseline = path.parent.parent / f'{kind}-20261104-{trial}'
        other = json.loads((baseline / 'benchmark.json').read_text())
        if other != benchmark:
            raise ValueError('benchmark identity changed')
        estimates.append(json.loads((baseline / 'estimates.json').read_text())['mean'])
        samples.append(json.loads((baseline / 'sample.json').read_text()))
    row = {'scenario': name, 'queries_per_batch': queries,
           'mean_ns_per_batch': estimates,
           'mean_ns_per_query': [e['point_estimate'] / queries for e in estimates],
           'samples': samples}
    if kind == 'traffic':
        family, pattern = name.split('-', 1)
        fixture = pathlib.Path(f'target/traffic-fixtures/{family}-{value}.nidx')
        row.update(records=value, pattern=pattern, family=family,
                   expected_hits_per_batch={'mostly-hits': 16221, 'mostly-misses': 163}.get(pattern, 8192))
    else:
        fixture = pathlib.Path(f'target/shape-fixtures/{name}.nidx')
        row.update(records=65536, batch=value)
    row.update(artifact_bytes=fixture.stat().st_size, artifact_sha256=digest(fixture))
    rows.append(row)
expected = 30 if kind == 'traffic' else 40
if len(rows) != expected:
    raise ValueError(f'expected {expected} scenarios, found {len(rows)}')
result = {
    'date': '2026-11-04', 'revision': '6efec832 plus new benchmark harnesses',
    'toolchain': 'rustc 1.95.0 (59807616e 2026-04-14)',
    'platform': 'Windows x86_64 MSVC', 'cpu': 'AMD Ryzen 9 9950X3D2',
    'profile': 'release, codegen-units=1; no optional SIMD, custom RUSTFLAGS, PGO or LTO',
    'affinity_mask': 1048576,
    'source_sha256': digest(pathlib.Path(f'benches/{kind}.rs')),
    'cargo_lock_sha256': digest(pathlib.Path('Cargo.lock')),
    'scope': ('16384 repeated queries. Uniform 50% hit/gap mix, mostly hits (16221/16384), mostly misses (163/16384), approximately 90% queries on 16 hot IDs, and 32-query adjacent-ID locality runs; last two are 50% hits. Exact-key IPv4/IPv6/ASN, payload LEu32(id%4096). Every query checks hit count, ID, original target and payload before timing. No JSON/allocation/network work in visitor loop. Warm repeated corpus, not full uniform address-space traffic or cold/>RAM I/O.'
              if kind == 'traffic' else
              '65536 original assertions; dense and sparse ASN, 16-byte-delta sparse IPv6, up to 65 nested IPv6 prefixes per cluster, and 64 duplicate IPv4 assertions per key. Shared and 100% unique eight-byte payloads. Each of 256 queries is compared with a brute-force scan of original targets and complete ID/target/payload lists. Caller batches 1/8/32/256 rotate through the same query corpus and use the existing scalar-query visitor API; no new batch API, interleaved execution or cross-query SIMD. No JSON/allocation/network work in timed visitor loop.'),
    'limitations': 'Two serial descriptive passes on a live desktop. Fixed warm query corpus and default build establish workload baselines, not optimization gains, deployed capacity or broad tail-latency guarantees.',
    'processes': processes, 'rows': rows,
}
destination.write_text(json.dumps(result, indent=2) + '\n')
print(destination)
