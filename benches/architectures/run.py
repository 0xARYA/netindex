"""Run native, pinned qualification and retain raw, hashed evidence."""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument('--output', required=True)
p.add_argument('--fixtures', required=True)
p.add_argument('--cpu', type=int)
p.add_argument('--repeats', type=int, default=2)
args = p.parse_args()
started_at = time.time()
repo = pathlib.Path(__file__).resolve().parents[2]
out = pathlib.Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=False)
allowed = sorted(os.sched_getaffinity(0))
cpu = args.cpu if args.cpu is not None else allowed[-1]
assert cpu in allowed
env = os.environ.copy()
env['NETINDEX_CPU'] = str(cpu)
env['CARGO_TARGET_DIR'] = str(out / 'build')
assert not env.get('RUSTFLAGS'), 'portable release build required'
assert args.repeats >= 2
def run(command, log):
    with (out / log).open('w') as output:
        subprocess.run(command, cwd=repo, env=env, stdout=output, stderr=subprocess.STDOUT, check=True)

manifest = str(repo / 'benches/architectures/Cargo.toml')
run(['cargo','+1.95.0','build','--locked','--release','--features','simd','--manifest-path',manifest], 'build.log')
binary = out / 'build/release/netindex-architectures-study'
shutil.copyfile(binary, out / 'study')
binary = out / 'study'
binary.chmod(0o755)
run(['cargo','+1.95.0','generate-lockfile','--manifest-path',str(repo/'target/architectures/library/Cargo.toml')], 'core-lock.log')
shutil.copyfile(repo/'target/architectures/library/Cargo.lock', out/'core-Cargo.lock')
run(['cargo','+1.95.0','test','--locked','--release','--lib','--features','simd','--manifest-path',str(repo/'target/architectures/library/Cargo.toml'),'--','--test-threads=1'], 'core-tests.log')
run(['cargo','+1.95.0','clippy','--locked','--all-targets','--features','simd','--manifest-path',manifest,'--','-D','warnings'], 'clippy.log')
fixtures = pathlib.Path(args.fixtures).resolve()
if not fixtures.exists(): run([str(binary),'fixtures',str(fixtures)], 'fixtures.log')
owned = out / 'inputs'
owned.mkdir()
for path in sorted(fixtures.glob('*.nidx')): shutil.copyfile(path, owned / path.name)
assert len(list(owned.glob('*.nidx'))) == 16
for repeat in range(args.repeats):
    run([str(binary),'kernels'], f'kernels-r{repeat}.jsonl')
    cases = sorted(owned.glob('*.nidx'), reverse=repeat%2==1)
    for path in cases:
        scenario, records = path.stem.rsplit('-',1)
        run([str(binary),str(path),scenario,records], f'{path.stem}-r{repeat}.jsonl')
run(['objdump','-d','-C',str(binary)], 'study.disasm')
run(['rustc','+1.95.0','-vV'], 'rustc.txt')
run(['lscpu'], 'lscpu.txt')
run(['uname','-a'], 'uname.txt')
receipt = {'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
    'platform':platform.platform(),'machine':platform.machine(),'allowed_cpus':allowed,'cpu':cpu,
    'repeats':args.repeats,'rustflags':env.get('RUSTFLAGS',''),'started_at':started_at,'completed_at':time.time(),
    'files':{path.relative_to(out).as_posix():hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(out.rglob('*')) if path.is_file() and 'build' not in path.relative_to(out).parts}}
shutil.copyfile(repo/'target/architectures/prepared.json',out/'prepared.json')
receipt['files']['prepared.json'] = hashlib.sha256((out/'prepared.json').read_bytes()).hexdigest()
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(out)
