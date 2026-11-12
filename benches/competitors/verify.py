"""Verify a local comparison receipt, then add Git-compatible text hashes."""
import ast
import hashlib
import itertools
import json
import pathlib
import sys

receipt = pathlib.Path(sys.argv[1])
result = json.loads(receipt.read_text())
here = pathlib.Path(__file__).parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


normalized = {}
for name, expected in result['source_sha256'].items():
    source = here / name
    raw = source.read_bytes()
    normalized[name] = hashlib.sha256(raw.replace(b'\r\n', b'\n')).hexdigest()
    if digest(source) != expected:
        assert normalized[name] == result.get('source_lf_sha256', {}).get(name), name
ast.parse((here / 'run.py').read_text())
assert len(result['runs']) == 14
observed = set()
total = 0
for run in result['runs']:
    coordinate = (run['family'], run['shape'], run['records'], run['profile'])
    assert run['records'] == 100_000
    assert coordinate not in observed
    observed.add(coordinate)
    assert run['exit_code'] == 0
    executable, directory = map(pathlib.Path, run['arguments'][:2])
    assert digest(executable) == result['executable_sha256'][run['profile']]
    for filename, original in run['inputs'].items():
        path = directory / filename
        assert path.stat().st_size == original['bytes']
        assert digest(path) == original['sha256'], path
    log = directory.with_suffix('.jsonl')
    assert digest(log) == run['raw_log_sha256']
    assert [json.loads(line) for line in log.read_text().splitlines()] == run['rows']
    measured = [row for row in run['rows'] if row['phase'] == 'measure']
    engines = ['netindex', 'roaring-default', 'roaring-optimized'] if run['family'] == 'asn' else ['netindex', 'netindex-direct', 'ip_network_table']
    trials = range(1, 7 if run['family'] == 'asn' else 5)
    expected = set(itertools.product(['uniform', 'hot'], engines, trials))
    actual = [(row['traffic'], row['engine'], row['trial']) for row in measured]
    assert len(set(actual)) == len(actual) and set(actual) == expected
    for traffic in ['uniform', 'hot']:
        assert len({row['checksum'] for row in measured if row['traffic'] == traffic}) == 1
    for row in measured:
        assert row['queries'] == 1_048_576
        samples = sorted(value for sample, count in row['histogram_ns'].items()
                         for value in [int(sample)] * count)
        assert len(samples) == row['sample_count'] == (1_048_576 + 1020) // 1021
        for percentile in [50, 95, 99]:
            assert row[f'p{percentile}_ns'] == samples[len(samples) * percentile // 100]
    total += len(measured)
assert total == 408
result['source_lf_sha256'] = normalized
result['text_hash_scope'] = 'source_sha256 hashes measured working-copy bytes; source_lf_sha256 normalizes CRLF to LF for Git checkout comparisons. Binary inputs and executables always use raw byte hashes.'
result['verifier_sha256'] = digest(pathlib.Path(__file__))
result['verification'] = dict(processes=len(observed), measurements=total, sources=True,
                              executables=True, inputs=True, raw_logs=True,
                              trial_coordinates=True, histograms=True)
receipt.write_text(json.dumps(result, indent=2) + '\n')
print(f'Verified {len(observed)} processes and {total} measurements, including inputs and logs')
