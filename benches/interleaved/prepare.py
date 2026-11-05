"""Create an isolated core snapshot; no tracked library source is edited."""
import hashlib
import json
import pathlib
import shutil
import subprocess

here = pathlib.Path(__file__).resolve().parent
repo = here.parents[1]
destination = repo / 'target/interleaved/library'
destination.mkdir(parents=True, exist_ok=False)
shutil.copytree(repo / 'src', destination / 'src')
shutil.copyfile(repo / 'README.md', destination / 'README.md')
manifest = (repo / 'Cargo.toml').read_text().split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "netindex"', 'name = "netindex-interleave-core"\npublish = false', 1)
(destination / 'Cargo.toml').write_text(manifest + '\n[workspace]\n', newline='\n')
sources = {}
for path in sorted((repo / 'src').rglob('*.rs')):
    relative = str(path.relative_to(repo)).replace('\\', '/')
    sources[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
for name in ['reader', 'packed']:
    path = destination / f'src/{name}.rs'
    original = path.read_text()
    patch = (here / f'{name}.inc.rs').read_text()
    marker = original.find('#[cfg(test)]')
    if marker < 0:
        path.write_text(original + '\n' + patch, newline='\n')
    else:
        path.write_text(original[:marker] + patch + '\n' + original[marker:], newline='\n')
receipt = dict(base_revision=subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
               original_sources=sources,
               prepared_sources={str(path.relative_to(destination)).replace('\\', '/'): hashlib.sha256(path.read_bytes()).hexdigest()
                                 for path in sorted(destination.rglob('*')) if path.is_file()})
(destination.parent / 'snapshot.json').write_text(json.dumps(receipt, indent=2) + '\n', newline='\n')
print(destination)
