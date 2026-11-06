"""Linux serial comparison: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT_JSON."""
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys
import time

default, simd, work, destination = map(pathlib.Path, sys.argv[1:])
executables = {'default': default.resolve(strict=True), 'simd': simd.resolve(strict=True)}
work.mkdir(parents=True, exist_ok=False)
if work.stat().st_dev == pathlib.Path('/mnt/c').stat().st_dev:
    raise SystemExit('use a private Linux filesystem directory')
if 20 not in os.sched_getaffinity(0):
    raise SystemExit('guest CPU 20 is unavailable')
if os.getenv('RUSTFLAGS') or os.getenv('CARGO_ENCODED_RUSTFLAGS'):
    raise SystemExit('recorded profile requires no custom Rust flags')
here = pathlib.Path(__file__).parent
repo = here.parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


cases = [('v4', 'disjoint'), ('v4', 'overlap'), ('v6', 'disjoint'), ('v6', 'overlap'),
         ('asn', 'dense'), ('asn', 'sparse'), ('asn', 'clustered')]
runs = []
for records in (1_000, 100_000, 1_000_000):
    for ordinal, (family, shape) in enumerate(cases):
        reference = None
        for profile in (('default', 'simd') if ordinal % 2 == 0 else ('simd', 'default')):
            name = f'{family}-{shape}-{records}-{profile}'
            directory = work / name
            args = [str(executables[profile]), str(directory), family, shape, str(records)]
            started = time.monotonic_ns()
            process = subprocess.run(args, capture_output=True, text=True, timeout=300)
            log = work / f'{name}.jsonl'
            log.write_text(process.stdout)
            (work / f'{name}.stderr').write_text(process.stderr)
            if process.returncode:
                raise RuntimeError(f'{name}: {process.stderr}')
            rows = [json.loads(line) for line in process.stdout.splitlines()]
            measured = [r for r in rows if r['phase'] == 'measure']
            expected_rows = 36 if family == 'asn' else 24
            if len(measured) != expected_rows or any(r['queries'] != 1_048_576 for r in measured):
                raise ValueError(f'incomplete matrix: {name}')
            inputs = {p.name: dict(bytes=p.stat().st_size, sha256=digest(p))
                      for p in sorted(directory.iterdir())}
            if reference is not None and inputs != reference:
                raise ValueError(f'default/SIMD fixture or corpus drift: {name}')
            reference = inputs
            runs.append(dict(family=family, shape=shape, records=records, profile=profile,
                             arguments=args, wall_ns=time.monotonic_ns()-started,
                             exit_code=process.returncode, raw_log_sha256=digest(log),
                             inputs=inputs, rows=rows))
            print(name, round(runs[-1]['wall_ns']/1e9, 3), flush=True)

result = dict(date='2026-11-04', base_revision='0d0f56c9cf5d957919b1448feabb37b5b9698f4e',
              toolchain=subprocess.check_output(['rustc', '+1.95.0', '-Vv'], text=True),
              kernel=platform.release(), architecture=platform.machine(),
              cpuinfo=pathlib.Path('/proc/cpuinfo').read_text().split('\n\n')[0],
              topology=subprocess.check_output(['lscpu', '-e=CPU,CORE,SOCKET,NODE'], text=True),
              affinity_cpu=20, profile='release, codegen-units=1; portable target; no PGO, LTO or custom Rust flags; default and optional netindex SIMD binaries',
              executable_sha256={name: digest(path) for name, path in executables.items()},
              source_sha256={str(p.relative_to(here)): digest(p)
                             for p in (here/'src/main.rs', here/'Cargo.toml', here/'Cargo.lock', here/'run.py')},
              geography_evidence=dict(path='benches/results/2026-11-04-native-city.json',
                                      sha256=digest(repo/'benches/results/2026-11-04-native-city.json'),
                                      scope='Previously measured Windows native City owned normalized records and 108123-query parity. Not pooled with these Linux synthetic timings.'),
              scope='All structures resident in one process, built from identical original keys/prefixes; netindex owns Vec bytes, trie and bitmaps own heap structures. Original std HashMap masked-prefix equality or HashSet membership independently checks every timed query and all input boundaries/neighbours before timing. Every measured checksum checked. No duplicate prefixes; longest-prefix selects Option<(network address,prefix,insertion ID)> by reducing netindex all-match visitors. Membership projects unique ASN keys to Boolean with empty payloads. Default and optimize() Roaring measured separately, serialized roundtrips checked. Corpus is 65536 fixed rotating queries, 16 passes per measurement and one warmup pass. Four trials alternate/rotate engine order; default/SIMD process order reverses by case. Uniform means spread across generated leaf IDs, not global IP space; hot uses first 1024 IDs. Disjoint/membership have half hits, overlapping prefixes cover gaps and have all hits.',
              latency_scope='Closed-loop single-thread service times on guest CPU 20, no JSON/HTTP. Sample every 1021 queries; Instant overhead included, clock controls retained. Full output fingerprint/Boolean accumulation included. Serialized sizes are not heap/RSS measurements. Construction and validation excluded from lookup timing; whole-process wall time includes prechecks.',
              limitations='Synthetic 1K/100K/1M assertion/key families, only one routing implementation and one Rust Roaring implementation, four prefix/six membership in-process trials per corpus; no cold I/O, >RAM or concurrency. Live WSL host scheduling and fixed corpus/cache effects limit extrapolation. Original-target/ID/payload preservation beyond the selected projection is not claimed for trie/bitmap formats. SIMD availability comes from runtime dispatch; generic compiler target for all libraries. Geography evidence uses different hardware-facing OS interfaces and application schema; no cross-suite aggregate ranking.',
              runs=runs)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
