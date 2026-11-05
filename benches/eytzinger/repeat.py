"""Repeat the six large family/build cases in fresh processes using frozen inputs."""
import hashlib
import json
import pathlib
import subprocess
import sys
import time

primary, work, destination = map(pathlib.Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=False)
here = pathlib.Path(__file__).resolve().parent
result = json.loads(primary.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


for name, expected in result['source_sha256'].items():
    assert digest(here.parents[1] / name) == expected
assert subprocess.check_output(['rustc', '+1.95.0', '-Vv'], text=True) == result['toolchain']
runs = []
for run in result['runs']:
    if run['records'] != 20_000_000:
        continue
    executable, source = map(pathlib.Path, run['arguments'][:2])
    assert digest(executable) == result['executable_sha256'][run['profile']]
    assert digest(source) == run['input_sha256']
    started = time.monotonic_ns()
    process = subprocess.run(run['arguments'], capture_output=True, text=True, timeout=300)
    log = work / f"{run['scenario']}-{run['profile']}.jsonl"
    log.write_text(process.stdout)
    (work / f"{run['scenario']}-{run['profile']}.stderr").write_text(process.stderr)
    if process.returncode:
        raise RuntimeError(process.stderr)
    rows = [json.loads(line) for line in process.stdout.splitlines()]
    assert len([r for r in rows if r['phase'] == 'measure']) == 64
    assert digest(source) == run['input_sha256']
    runs.append(dict(scenario=run['scenario'], profile=run['profile'], records=run['records'], arguments=run['arguments'],
                     input_sha256=run['input_sha256'], raw_log_sha256=digest(log),
                     wall_ns=time.monotonic_ns()-started, exit_code=process.returncode, rows=rows))
assert len(runs) == 6
destination.write_text(json.dumps(dict(date='2026-11-04', primary_path=str(primary),
    primary_sha256=digest(primary), repeat_source_sha256=digest(pathlib.Path(__file__)),
    scope='Supplemental fresh-process repeat of all six 20M family/build cases. Same frozen executable, immutable inputs, complete original-input and callback checks, corpora, trial order, warmup and measurement protocol. Four in-process trials per variant and uniform/hot corpus. Main-run drift and hot regressions remain relevant; no library adoption or significant-speedup claim.',
    runs=runs), indent=2)+'\n')
print(destination)
