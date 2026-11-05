"""Check measured sources and dependency locks, permitting only CRLF/LF changes."""
import hashlib
import json
import pathlib
import sys

repo=pathlib.Path(__file__).resolve().parents[2]
receipt=json.loads((pathlib.Path(sys.argv[1])/'receipt.json').read_text())
for name,expected in receipt['source_sha256'].items():
    raw=(repo/name).read_bytes()
    lf=raw.replace(b'\r\n',b'\n')
    assert expected in {hashlib.sha256(value).hexdigest() for value in [raw,lf,lf.replace(b'\n',b'\r\n')]},name
print(json.dumps({'measured_source_files_verified':len(receipt['source_sha256']),
                  'line_endings_normalized':True,'measurement_revision':receipt['revision']}))
