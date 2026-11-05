"""Run replacement under active readers: EXE FRESH_EXT4_DIRECTORY OUTPUT_JSON."""
import hashlib
import json
import pathlib
import platform
import subprocess
import sys
import time

executable, work, destination = map(pathlib.Path, sys.argv[1:])
executable = executable.resolve(strict=True)
work.mkdir(parents=True, exist_ok=False)
if work.stat().st_dev == pathlib.Path('/mnt/c').stat().st_dev:
    raise SystemExit('use a private Linux filesystem directory')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


started = time.monotonic_ns()
process = subprocess.run([str(executable), str(work)], capture_output=True, text=True, timeout=180)
(work / 'replacement.jsonl').write_text(process.stdout)
(work / 'replacement.stderr').write_text(process.stderr)
if process.returncode != 0:
    raise RuntimeError(process.stderr)
rows = [json.loads(line) for line in process.stdout.splitlines()]
if len(rows) != 12 or any(row['publications'] != 50 or row['rejected_candidates'] != 10 or row['retired_mappings_released'] != 49 or not row['all_mappings_released_after_snapshots'] for row in rows):
    raise ValueError('incomplete replacement matrix')
here = pathlib.Path(__file__).parent
result = dict(date='2026-11-04', revision='570c616 plus replacement benchmark',
              toolchain=subprocess.check_output(['rustc', '+1.95.0', '-V'], text=True).strip(),
              kernel=platform.release(), architecture=platform.machine(),
              profile='release codegen-units=1, mmap only; no optional SIMD, LTO, PGO or custom RUSTFLAGS',
              executable_sha256=digest(executable), source_sha256=digest(here / 'src/bin/replacement.rs'),
              lock_sha256=digest(here / 'Cargo.lock'), raw_log_sha256=digest(work / 'replacement.jsonl'),
              inputs={p.name: dict(bytes=p.stat().st_size, sha256=digest(p)) for p in sorted(work.glob('*.nidx'))},
              wall_ns=time.monotonic_ns()-started, exit_code=process.returncode,
              scope='Linux/WSL, 1/4/16 workers on guest CPUs 0/2/.../30, writer on 31 (SMT sibling of final worker at 16). Both coarse/fine ReaderSet routing, two trials reversing worker-count order. Each immutable replacement has 196672 assertions: IPv4 /24s with gaps and duplicates, IPv6 /72s, ASNs; generation changes targets and payloads. One unchanged three-assertion reader. Fifty alternating replacements per case, at least 10ms apart; each has a separately mapped/prevalidated Reader moved out of the staging queue, with no staging owner retained after publication. Weak references prove 49 retired mappings released after worker completion while the original and final snapshots remain, then all 51 released after their owners drop. Candidate ReaderSet::replaced construction happens outside application RwLock<Arc<Snapshot>> publication. Workers acquire once per 32-query batch; every sixteenth batch retains the original snapshot. All results are checked against independent original-target scans, including source positions, IDs, targets, payloads and exact multiplicity. Every worker must observe both generations. Old borrowed payloads retained across all swaps. Ten failed candidates per case (invalid mapped bytes/invalid replacement position) must leave the published Arc unchanged.',
              latency_scope='Sampled 32-query batch times include snapshot acquisition and correctness checks. Publication lock metrics include waiting/swap but exclude destruction. Candidate construction excludes file mapping and validation (preloaded immutable inputs). Throughput includes checked mixed queries and writer work; no HTTP/network. This benchmark uses caller-owned standard RwLock policy, not a library activation API or an ArcSwap/RCU comparison.',
              limitations='Small warm synthetic replacement files, prevalidated generations, two trials on a live virtualized host. Files remain immutable; no pathname replacement/deployment or physical-disk failure simulation. Full file-open costs are separate P0-01/P0-06 results. Batch percentiles are not individual-query latencies or production SLOs.',
              rows=rows)
destination.write_text(json.dumps(result, indent=2)+'\n')
print(destination)
