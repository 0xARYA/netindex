"""Collect frozen native-adapter runs and fail on artifact or dependency drift."""
import hashlib
import json
import pathlib
import sys
import tomllib

root = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
versions = ("2.0.2", "2.1.0")


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load(name):
    row = json.loads((root / f'{name}.json').read_text(encoding='utf-8-sig'))
    if isinstance(row['rows'], dict):
        row['rows'] = [row['rows']]
    if row['exit_code'] != 0:
        raise ValueError(f'failed run: {name}')
    row['raw_log_sha256'] = digest(root / f'{name}.jsonl')
    return row


inputs = root.parent / 'city-study'
native_hash = digest(inputs / 'city.mmdb')
original_hash = digest(inputs / 'original.ipidx')
if native_hash != 'd6ff9368ab19a13b096fd6392f752aa6c0890f2c92752d09253fab9e50501e46':
    raise ValueError('native input changed')
if original_hash != '83d775f419696a224b5f57a114ec42ea5f1289b5ea86d4ae171d00218b3d2f18':
    raise ValueError('retained component changed')
with (inputs / 'original.ipidx').open('rb') as stream:
    stream.seek(80)
    if stream.read(8) != b'BWNVPOOL':
        raise ValueError('metadata envelope changed')
    receipt = json.loads(stream.read(int.from_bytes(stream.read(4), 'little')))
conversions = []
dependencies = {}
source_hashes = {}
for version in versions:
    directory = root / version
    lock = directory / 'Cargo.lock'
    dependencies[version] = tomllib.loads(lock.read_text())['package']
    snapshot = json.loads((directory / 'snapshot-hashes.json').read_text())
    source_hashes[version] = {name: digest(directory / name) for name in snapshot}
    for trial in (1, 2):
        run = load(f'convert-{version}-rust195-{trial}')
        artifact = root / f'native-{version}-rust195-{trial}.nidx'
        run.update(version=version, trial=trial, artifact_sha256=digest(artifact))
        expected = original_hash if version == '2.0.2' else '1be3a0505fd8eb3dd7ce2b7f807d1744541a86e553cb7c1be78bc95ad65c72a7'
        if run['artifact_sha256'] != expected:
            raise ValueError(f'{version} artifact drift')
        conversions.append(run)
if [p for p in dependencies['2.0.2'] if p['name'] != 'netindex'] != [p for p in dependencies['2.1.0'] if p['name'] != 'netindex']:
    raise ValueError('transitive dependencies differ')
for name, value in source_hashes['2.0.2'].items():
    if not name.endswith('Cargo.toml') and source_hashes['2.1.0'].get(name) != value:
        raise ValueError(f'application source drift: {name}')
parity = [load(f'parity-{version}') for version in versions]
serving = [load(f'serve-{kind}-{trial}') for trial in (1, 2) for kind in ('native', *versions)]
result = {
    'date': '2026-11-04', 'revision': '6efec832 plus standalone benchmark adapter',
    'toolchain': 'rustc 1.95.0 (59807616e 2026-04-14)',
    'platform': 'Windows x86_64 MSVC', 'cpu': 'AMD Ryzen 9 9950X3D2',
    'affinity_mask': 1048576,
    'profile': 'release, codegen-units=1; external-sort/mmdb/mmap/shared-values/simd; no custom RUSTFLAGS, PGO or LTO',
    'native_sha256': native_hash, 'original_component_sha256': original_hash,
    'native_bytes': (inputs / 'city.mmdb').stat().st_size,
    'application_metadata': receipt,
    'scope': 'Full native structural and canonical-record validation, expanded alias traversal, original application normalization and payload encoder, external construction, and full normalized application payload validation. Both baseline artifacts reproduce the original byte for byte; both candidates reproduce the independently verified opaque replay. Conversion excludes download, gzip, application orchestration, explicit fsync and atomic publication.',
    'serving_scope': 'Separate processes. Native owned bytes plus application decoder versus immutable mapped converted files, fully validated before timing. Identical 100005-query mixed IPv4/IPv6 corpus, 256-query warmup, 42802 hits. Owned normalized records allocated and dropped; no JSON/network work. Reported process peak working set includes opening and serving. Not cold I/O, >RAM, concurrent load or deployed-service capacity.',
    'latency_limitation': 'Per-query Instant timing is quantized in 100ns ticks on this host and includes clock overhead. Two sequential descriptive trials do not establish significant latency gains or production percentiles.',
    'parity_scope': '108123 queries: 100005 fixed edge/random queries plus every 4096th canonical network endpoints and IPv4-compatible, mapped and 6to4 endpoints. Serialized owned geo records compare exactly. This complements complete assertion/payload replay verification; it does not exhaust the address space.',
    'dependencies': dependencies,
    'source_sha256': source_hashes,
    'conversion_runs': conversions, 'parity_runs': parity, 'serving_runs': serving,
}
destination.write_text(json.dumps(result, indent=2) + '\n')
print(destination)
