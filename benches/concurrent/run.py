"""Linux shared-reader scaling: EXE FRESH_EXT4_DIRECTORY OUTPUT_JSON."""
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import time

executable, work, destination = map(pathlib.Path, sys.argv[1:])
executable = executable.resolve(strict=True)
work.mkdir(parents=True, exist_ok=False)
if work.stat().st_dev == pathlib.Path('/mnt/c').stat().st_dev:
    raise SystemExit('private input copies must be on Linux storage')
repo = pathlib.Path(__file__).resolve().parents[2]
cpus = list(range(0, 32, 2))
if not set(cpus + [31]) <= os.sched_getaffinity(0):
    raise SystemExit('the recorded worker/coordinator CPU set is unavailable')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


rows = []
for records in (1_000_000, 20_000_000):
    for family in ('v4', 'v6', 'asn'):
        source = repo / f'target/traffic-fixtures/{family}-{records}.nidx'
        private = work / source.name
        shutil.copyfile(source, private)
        original_hash = digest(source)
        if digest(private) != original_hash:
            raise ValueError('input copy changed')
        args = [str(executable), str(private), family, str(records), '2000000', ','.join(map(str, cpus))]
        load_before = pathlib.Path('/proc/loadavg').read_text().strip()
        started = time.monotonic_ns()
        process = subprocess.run(args, capture_output=True, text=True, timeout=300)
        row = dict(family=family, records=records, arguments=args,
                   artifact_bytes=private.stat().st_size, artifact_sha256=original_hash,
                   wall_ns=time.monotonic_ns()-started, exit_code=process.returncode,
                   guest_load_before=load_before, guest_load_after=pathlib.Path('/proc/loadavg').read_text().strip(),
                   rows=[json.loads(line) for line in process.stdout.splitlines()])
        (work / f'{family}-{records}.json').write_text(json.dumps(row, indent=2)+'\n')
        (work / f'{family}-{records}.stderr').write_text(process.stderr)
        private.unlink()
        if process.returncode != 0:
            raise RuntimeError(process.stderr)
        if len(row['rows']) != 11:
            raise ValueError('incomplete matrix')
        rows.append(row)
        print(family, records, round(row['wall_ns']/1e9, 3), flush=True)

result = dict(date='2026-11-04', revision='570c616 plus concurrent-reader benchmark',
              toolchain=subprocess.check_output(['rustc', '+1.95.0', '-V'], text=True).strip(),
              kernel=platform.release(), architecture=platform.machine(),
              topology=subprocess.check_output(['lscpu', '-e=CPU,CORE,SOCKET,NODE'], text=True),
              profile='release, codegen-units=1; mmap only; no SIMD, PGO, LTO or custom RUSTFLAGS',
              executable_sha256=digest(executable),
              source_sha256=digest(pathlib.Path(__file__).with_name('src') / 'main.rs'),
              manifest_sha256=digest(pathlib.Path(__file__).with_name('Cargo.toml')),
              lock_sha256=digest(pathlib.Path(__file__).with_name('Cargo.lock')),
              scope='One fully validated, page-warmed immutable mapped Reader shared by scoped threads. 1/2/4/8/16 workers on guest CPUs 0/2/.../30; coordinator 31. Counts ascending then descending. Each worker independently checks complete IDs/targets/payloads and hit counts for every timed query before synchronization, warms up 100000 lookups, then executes 2000000 queries with a worker-specific seed, half hits. Timed visitors allocate nothing, accumulate hits/checksum and black_box targets/payloads. Entire timed output counts/checksums are verified. Throughput uses gate release to last worker loop completion; setup/validation/join/histogram work excluded. Latency sampled every 1021 queries (odd stride alternates hit/miss), including Instant overhead; no per-query JSON/network work.',
              limitations='Closed-loop visitor service times, not request queueing, HTTP or open-loop capacity. Warm guest filesystem with 1M/20M exact-key fixtures, not cold/pressure or overlap-heavy traffic. Only two trials, virtual CPU topology/host scheduling and desktop load limit scaling conclusions. Latency percentiles pool all workers equally by sample count; sample timing overhead is not subtracted. Coordinator shares an SMT sibling with the final worker at 16 readers but waits during the measured loops.',
              processes=rows)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
