"""Snapshot application-owned City adapters into an ignored standalone benchmark.

Usage: python benches/native_city/prepare.py BYWAY_ROOT OUTPUT_DIRECTORY VERSION
VERSION is 2.0.2 or 2.1.0. No application workspace or deployment is changed.
Build the result with cargo +1.95.0 build --release --manifest-path OUTPUT_DIRECTORY/Cargo.toml.
"""
import hashlib
import json
import pathlib
import shutil
import sys

byway, output = map(pathlib.Path, sys.argv[1:3])
version = sys.argv[3]
if version not in ("2.0.2", "2.1.0"):
    raise SystemExit("unsupported comparison version")
output.mkdir(parents=True, exist_ok=False)
sources = byway / "services/ip-intel-aggregator/src"
shutil.copytree(byway / "crates/ip-intel-format/src", output / "format/src")
shutil.copytree(byway / "vendor/maxminddb", output / "maxminddb")
for directory in ("src/sources/geo", "src/sources/native_component", "src/mmdb"):
    (output / directory).mkdir(parents=True, exist_ok=True)
copies = {
    "sources/geo/schema.rs": "src/sources/geo/schema.rs",
    "sources/geo/model.rs": "src/sources/geo/model.rs",
    "sources/native_component/payload.rs": "src/sources/native_component/payload.rs",
    "mmdb/database.rs": "src/mmdb/database.rs",
}
for source, target in copies.items():
    shutil.copyfile(sources / source, output / target)
shutil.copyfile(pathlib.Path(__file__).with_name("adapter.rs"), output / "src/main.rs")
dependency = f'netindex = {{ version = "={version}", features = ["external-sort", "mmdb", "mmap", "shared-values", "simd"] }}\n'
common = 'serde = { version = "1", features = ["derive"] }\nserde_json = "1"\nthiserror = "2"\nipnet = { version = "2", features = ["serde"] }\nmemmap2 = "0.9.11"\n'
(output / "Cargo.toml").write_text(
    '[package]\nname = "native-city-audit"\nversion = "0.0.0"\nedition = "2024"\n'
    '[workspace]\n[profile.release]\ncodegen-units = 1\n[dependencies]\n' + dependency + common +
    'ip-intel-format = { path = "format" }\nmaxminddb = "0.32.0"\nsha2 = "0.10"\ntempfile = "3"\n'
    '[patch.crates-io]\nmaxminddb = { path = "maxminddb" }\n')
(output / "format/Cargo.toml").write_text(
    '[package]\nname = "ip-intel-format"\nversion = "0.0.0"\nedition = "2024"\n[dependencies]\n' + dependency + common)
hashes = {str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in output.rglob('*') if p.is_file()}
(output / "snapshot-hashes.json").write_text(json.dumps(hashes, indent=2) + '\n')
print(output)
