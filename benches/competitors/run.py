"""Linux serial comparison: DEFAULT_EXE SIMD_EXE FRESH_DIRECTORY OUTPUT_JSON."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('default', type=pathlib.Path)
parser.add_argument('simd', type=pathlib.Path)
parser.add_argument('work', type=pathlib.Path)
parser.add_argument('destination', type=pathlib.Path)
parser.add_argument('--cpu', type=int, required=True)
options = parser.parse_args()
default, simd, work, destination = options.default, options.simd, options.work, options.destination
executables = {'default': default.resolve(strict=True), 'simd': simd.resolve(strict=True)}
if sys.platform != 'linux':
    raise SystemExit('the matrix runner requires Linux')
if options.cpu not in os.sched_getaffinity(0):
    raise SystemExit(f'CPU {options.cpu} is unavailable')
if os.getenv('RUSTFLAGS') or os.getenv('CARGO_ENCODED_RUSTFLAGS'):
    raise SystemExit('recorded profile requires no custom Rust flags')
if destination.exists():
    raise SystemExit('choose a new receipt path')
work.mkdir(parents=True, exist_ok=False)
windows_mount = pathlib.Path('/mnt/c')
if windows_mount.exists() and work.stat().st_dev == windows_mount.stat().st_dev:
    raise SystemExit('use a private Linux filesystem directory')
os.sched_setaffinity(0, {options.cpu})
here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


cases = [('v4', 'disjoint'), ('v4', 'overlap'), ('v6', 'disjoint'), ('v6', 'overlap'),
         ('asn', 'dense'), ('asn', 'sparse'), ('asn', 'clustered')]
runs = []
records = 100_000
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

result = dict(date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(),
              checkout_revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
              checkout_status=subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True).splitlines(),
              toolchain=subprocess.check_output(['rustc', '+1.95.0', '-Vv'], text=True),
              kernel=platform.release(), architecture=platform.machine(),
              cpuinfo=pathlib.Path('/proc/cpuinfo').read_text().split('\n\n')[0],
              topology=subprocess.check_output(['lscpu', '-e=CPU,CORE,SOCKET,NODE'], text=True),
              affinity_cpu=options.cpu, profile='release, codegen-units=1; portable target; no PGO, LTO or custom Rust flags; default and optional netindex SIMD binaries',
              executable_sha256={name: digest(path) for name, path in executables.items()},
              source_sha256={str(p.relative_to(here)): digest(p)
                             for p in (here/'src/main.rs', here/'Cargo.toml', here/'Cargo.lock', here/'run.py')},
              scope='Identical original inputs; in-memory encoded netindex, heap trie and bitmaps. Independent HashMap/HashSet oracles check queries and input boundaries before timing. Checksums must agree after timing. Unique CIDRs use the same longest-prefix answer (address, prefix, ID); both direct lookup and all-match reduction are measured. Unique ASN inputs use Boolean membership with empty payloads; default and optimized Roaring are separate. Each sample repeats a fixed 65536-query corpus sixteen times after one warmup pass. Uniform traffic spreads across generated IDs, hot traffic uses the first 1024 IDs. Disjoint and ASN workloads have half hits; overlap parents cover all gaps.',
              latency_scope='Single-thread warm lookup on the selected logical CPU. Sample every 1021 queries, including clock overhead. Aggregate timing includes output fingerprint or Boolean accumulation. Construction and validation are outside lookup timing. Serialized bytes do not measure heap or RSS.',
              limitations='Synthetic 100K inputs, one trie and one bitmap implementation. Four prefix or six membership trials per corpus. Fixed corpora do not establish cold I/O, memory pressure, concurrency, or performance on arbitrary routing data. Specialized projections do not preserve the all-match record contract.',
              runs=runs)
with destination.open('x', encoding='utf-8') as stream:
    json.dump(result, stream, indent=2)
    stream.write('\n')
print(destination)
