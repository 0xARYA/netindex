"""Summarize SIMD code from the actual retained executable's disassembly."""
import argparse
import collections
import json
import pathlib
import re
import subprocess

p = argparse.ArgumentParser()
p.add_argument('directory', type=pathlib.Path)
p.add_argument('--output', type=pathlib.Path, required=True)
args = p.parse_args()
root = args.directory
symbols = subprocess.check_output(['nm','-S','-C',str(root/'study')],text=True)
sizes = {int(m[1],16):int(m[2],16) for line in symbols.splitlines()
    if (m:=re.match(r'^([0-9a-f]+) ([0-9a-f]+) [tT] ',line))}
functions = []
current = None
for line in (root/'study.disasm').read_text().splitlines():
    if m:=re.match(r'^([0-9a-f]+) <(.+)>:$',line):
        current = None
        if 'study_arch::avx512' in m[2] or 'study_arch::neon' in m[2]:
            current = {'name':m[2],'address':m[1],'bytes':sizes[int(m[1],16)],'instructions':collections.Counter()}
            functions.append(current)
    elif current is not None and ':' in line and '\t' in line:
        address = re.match(r'^\s*([0-9a-f]+):',line)
        if address is None or int(address[1],16) >= int(current['address'],16)+current['bytes']:
            continue
        instruction = re.match(r'^\s*[0-9a-f]+:\s+(?:[0-9a-f]{8}\s+|[0-9a-f]{2}\s+)+([a-z][a-z0-9.]*)',line)
        if instruction:
            current['instructions'][instruction[1]] += 1
assert len(functions) == (5 if json.loads((root/'receipt.json').read_text())['machine']=='x86_64' else 8)
args.output.write_text(json.dumps(functions,indent=2)+'\n')
print(json.dumps(functions))
