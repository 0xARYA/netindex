"""Require identical native inputs, also matching the earlier P1 fixture bytes."""
import hashlib
import json
import pathlib
import sys

repo = pathlib.Path(__file__).resolve().parents[2]
inputs = []
for directory in sys.argv[1:]:
    receipt = json.loads((pathlib.Path(directory)/'receipt.json').read_text())
    inputs.append({k:v for k,v in receipt['files'].items() if k.startswith('inputs/')})
assert inputs and len(inputs[0])==16
assert all(value==inputs[0] for value in inputs)
for name,sha in inputs[0].items():
    scenario,count = pathlib.Path(name).stem.rsplit('-',1)
    old = repo/'target/traffic-fixtures'/pathlib.Path(name).name if scenario in ['v4','v6','asn'] else repo/'target/shape-fixtures'/(scenario+'.nidx')
    assert hashlib.sha256(old.read_bytes()).hexdigest()==sha, str(old)
print(json.dumps({'identical_inputs_between_native_runs':len(inputs[0]),
                  'identical_to_frozen_P1_fixtures':len(inputs[0])}))
