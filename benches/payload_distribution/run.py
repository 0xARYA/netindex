"""Serial pinned, fresh-process measurements over immutable private input copies."""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import time

repo=pathlib.Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser()
p.add_argument('--output',type=pathlib.Path,required=True)
p.add_argument('--asn-source',type=pathlib.Path,required=True)
p.add_argument('--cpu',type=int,default=20)
p.add_argument('--city-original',type=pathlib.Path,default=repo/'target/city-study/original.ipidx')
p.add_argument('--city-current',type=pathlib.Path,default=repo/'target/city-study/rebuilt-2.1.0-1.nidx')
args=p.parse_args()
assert args.cpu in os.sched_getaffinity(0)
out=args.output.resolve()
out.mkdir(parents=True,exist_ok=False)
started=time.time()
env=os.environ.copy()
env['CARGO_TARGET_DIR']=str(out/'build')
assert not env.get('RUSTFLAGS')
def sha(path):
    digest=hashlib.sha256()
    with path.open('rb') as source:
        while block:=source.read(1024*1024): digest.update(block)
    return digest.hexdigest()
def run(command,name,timed=False):
    if timed: command=['/usr/bin/time','-v','-o',str(out/(name+'.time')),'taskset','-c',str(args.cpu),*command]
    with (out/(name+'.log')).open('w') as output, (out/(name+'.stderr')).open('w') as errors:
        subprocess.run(command,cwd=repo,env=env,stdout=output,stderr=errors,check=True)

manifest=str(repo/'benches/payload_distribution/Cargo.toml')
run(['cargo','+1.95.0','build','--locked','--release','--manifest-path',manifest],'build')
run(['cargo','+1.95.0','test','--locked','--manifest-path',manifest],'tests')
run(['cargo','+1.95.0','clippy','--locked','--all-targets','--manifest-path',manifest,'--','-D','warnings'],'clippy')
binary=out/'study'
shutil.copyfile(out/'build/release/netindex-payload-distribution',binary)
binary.chmod(0o755)
inputs=out/'inputs'
inputs.mkdir()
for source,name in [(args.city_original,'city-original.nidx'),(args.city_current,'city-current.nidx'),
                    (args.asn_source/'asn.rows','asn.rows')]:
    shutil.copyfile(source,inputs/name)
shutil.copyfile(args.asn_source/'source.json',out/'asn-source.json')
assert sha(inputs/'city-original.nidx')=='83d775f419696a224b5f57a114ec42ea5f1289b5ea86d4ae171d00218b3d2f18'
asn=json.loads((out/'asn-source.json').read_text())
assert sha(inputs/'asn.rows')==asn['files']['asn.rows']
run([str(binary),'ingest',str(inputs/'asn.rows'),str(inputs/'asn.nidx'),str(out)],'ingest',True)
before={path.name:sha(path) for path in sorted(inputs.iterdir())}
cases=['city-original','city-current','asn']
for repeat in range(2):
    for case in cases[::1 if repeat==0 else -1]:
        run([str(binary),'analyze',str(inputs/(case+'.nidx'))],case+f'-r{repeat}',True)
assert before=={path.name:sha(path) for path in sorted(inputs.iterdir())}
run(['rustc','+1.95.0','-vV'],'rustc')
run(['lscpu'],'lscpu')
source_files=[repo/'Cargo.toml',repo/'Cargo.lock',*sorted((repo/'src').rglob('*.rs')),
    *sorted((repo/'benches/payload_distribution').glob('*.py')),
    *sorted((repo/'benches/payload_distribution/src').glob('*.rs')),
    repo/'benches/payload_distribution/Cargo.toml',repo/'benches/payload_distribution/Cargo.lock']
receipt={'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
    'platform':platform.platform(),'python':platform.python_version(),'cpu':args.cpu,
    'allowed_cpus':sorted(os.sched_getaffinity(0)),'started_at':started,'completed_at':time.time(),
    'source_sha256':{path.relative_to(repo).as_posix():sha(path) for path in source_files},
    'files':{path.relative_to(out).as_posix():sha(path) for path in sorted(out.rglob('*'))
        if path.is_file() and 'build' not in path.relative_to(out).parts}}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(out)
