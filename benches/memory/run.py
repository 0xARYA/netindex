"""Run Linux-only warm/cold/>memory cases in private cgroups and file copies.

python3 benches/memory/run.py EXE WORK_DIRECTORY RESULT_JSON
Requires root and an already enabled cgroup-v2 memory controller. Never drops
global caches, changes root controller policy or adjusts any sibling cgroup.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

executable, work, destination = map(pathlib.Path, sys.argv[1:])
executable = executable.resolve(strict=True)
work.mkdir(parents=True, exist_ok=False)
if work.stat().st_dev == pathlib.Path('/mnt/c').stat().st_dev:
    raise SystemExit('use a Linux filesystem, not the host-mounted drive')
if 'memory' not in pathlib.Path('/sys/fs/cgroup/cgroup.subtree_control').read_text().split():
    raise SystemExit('root memory controller is not already enabled')
repo = pathlib.Path(__file__).resolve().parents[2]
rows = []


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


for trial in (1, 2):
    modes = ('warm', 'cold', 'pressure') if trial == 1 else ('pressure', 'cold', 'warm')
    for family in ('v4', 'v6', 'asn'):
        source = repo / f'target/traffic-fixtures/{family}-20000000.nidx'
        original_hash = digest(source)
        for mode in modes:
            name = f'{family}-{mode}-{trial}'
            private = work / f'{name}.nidx'
            shutil.copyfile(source, private)
            if digest(private) != original_hash:
                raise ValueError('copy differs')
            with private.open('rb') as stream:
                os.fsync(stream.fileno())
                os.posix_fadvise(stream.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
            group = pathlib.Path(f'/sys/fs/cgroup/netindex-perf-{os.getpid()}-{name}')
            group.mkdir()
            budget = (32 if mode == 'pressure' else 1024) * 1024 * 1024
            (group / 'memory.max').write_text(str(budget))
            (group / 'memory.swap.max').write_text('0')

            def enter():
                os.sched_setaffinity(0, {20})
                (group / 'cgroup.procs').write_text('0')

            try:
                started = time.monotonic_ns()
                process = subprocess.run([str(executable), mode, str(private), family, '20000000'],
                                         capture_output=True, text=True, preexec_fn=enter, timeout=180)
                row = dict(name=name, family=family, mode=mode, trial=trial,
                           artifact_bytes=private.stat().st_size, artifact_sha256=original_hash,
                           memory_max_bytes=budget, swap_max_bytes=0, affinity_cpus=[20],
                           wall_ns=time.monotonic_ns()-started, exit_code=process.returncode,
                           cgroup_memory_peak_bytes=int((group / 'memory.peak').read_text()),
                           memory_events=(group / 'memory.events').read_text(),
                           memory_stat=(group / 'memory.stat').read_text(),
                           rows=[json.loads(line) for line in process.stdout.splitlines()])
                (work / f'{name}.json').write_text(json.dumps(row, indent=2)+'\n')
                (work / f'{name}.stderr').write_text(process.stderr)
            finally:
                # subprocess.run waits for termination, including on its timeout.
                if not (group / 'cgroup.procs').read_text().strip():
                    group.rmdir()
                    private.unlink()
            if process.returncode != 0:
                raise RuntimeError(f'{name}: {process.stderr}')
            opened = row['rows'][0]
            if mode == 'cold' and opened['resident_pages_before_queries'] != 0:
                raise RuntimeError(f'{name}: private-file eviction was incomplete')
            if mode == 'pressure' and row['artifact_bytes'] <= budget:
                raise RuntimeError('input must exceed the memory budget')
            events = {key: int(value) for key, value in (line.split() for line in row['memory_events'].splitlines())}
            if events['oom_kill'] or (mode == 'pressure' and events['max'] == 0):
                raise RuntimeError(f'{name}: pressure control did not qualify')
            rows.append(row)
            print(name, round(row['wall_ns']/1e9, 3), flush=True)

result = dict(date='2026-11-04', toolchain=subprocess.check_output(['rustc', '+1.95.0', '-V'], text=True).strip(),
              kernel=subprocess.check_output(['uname', '-a'], text=True).strip(),
              executable_sha256=digest(executable),
              source_sha256=digest(pathlib.Path(__file__).with_name('src') / 'main.rs'),
              manifest_sha256=digest(pathlib.Path(__file__).with_name('Cargo.toml')),
              lock_sha256=digest(pathlib.Path(__file__).with_name('Cargo.lock')),
              scope='WSL2 Linux ext4 guest filesystem, isolated cgroup-v2 memory limits, no swap. Private files fsynced and evicted before child starts so page-cache charge belongs to the child. Warm: touch every mapped page after validation. Cold: discard private mapping PTEs and private-file cache after validation; mincore must report zero resident pages before timing. Pressure: 32MiB cgroup with 82.5/200.0MB inputs; no cache reset after validation. Four disjoint 8192-query passes, half hits/gaps, assert IDs/payloads and hit counts. Per-lookup correctness assertions remain in timed code. No JSON/allocation in visitor; phase metrics/output excluded from measured loops.',
              limitations='Guest cache state is controlled; host VHD/device caches are not. This is not physical-media cold latency. Linux/WSL results are not directly comparable to native Windows Criterion results. Memory pressure includes file cache and child memory, not whole host RAM. Default mmap advice and no readahead override.',
              rows=rows)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
