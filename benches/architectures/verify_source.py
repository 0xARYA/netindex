"""Recreate prepared code independently and compare it with native-run receipts."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

repo = pathlib.Path(__file__).resolve().parents[2]
shadow = pathlib.Path(tempfile.mkdtemp(prefix='architectures-verify-',dir=repo/'target'))
subprocess.run([sys.executable,str(repo/'benches/architectures/prepare.py'),'--output',str(shadow)],check=True)
actual = json.loads((shadow/'prepared.json').read_text())
def source_hashes(path):
    source = path.read_bytes().replace(b'\r\n',b'\n')
    return {hashlib.sha256(source).hexdigest(),hashlib.sha256(source.replace(b'\n',b'\r\n')).hexdigest()}
for directory in sys.argv[1:]:
    expected = json.loads((pathlib.Path(directory)/'prepared.json').read_text())
    assert set(actual['prepared'])==set(expected['prepared'])
    for name,sha in expected['prepared'].items():
        if name.endswith('.rs') or name=='library/README.md':
            assert sha in source_hashes(shadow/name), name
        else:
            assert sha==actual['prepared'][name], name
    for name,sha in expected['original'].items():
        assert sha in source_hashes(repo/name), name
    print(json.dumps({'directory':directory,'prepared_files_verified':len(expected['prepared']),
                      'original_sources_verified':len(expected['original']),
                      'source_line_endings_normalized':True}))
