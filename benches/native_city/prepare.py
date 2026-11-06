"""Snapshot Byway City adapters into a standalone benchmark."""

import argparse
import hashlib
import json
import shutil
from pathlib import Path


VERSIONS = ("2.0.2", "2.1.0", "local")
SOURCE_FILES = (
    "sources/geo/schema.rs",
    "sources/geo/model.rs",
    "sources/native_component/payload.rs",
    "mmdb/database.rs",
)
COMMON_DEPENDENCIES = """\
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
ipnet = { version = "2", features = ["serde"] }
memmap2 = "0.9.11"
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("byway", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("version", choices=VERSIONS)
    args = parser.parse_args()

    snapshot_sources(args.byway, args.output)
    write_manifests(args.output, args.version)

    hashes = {}
    for file in sorted(args.output.rglob("*")):
        if file.is_file():
            with file.open("rb") as stream:
                hashes[str(file.relative_to(args.output))] = hashlib.file_digest(
                    stream, "sha256"
                ).hexdigest()
    (args.output / "snapshot-hashes.json").write_text(
        json.dumps(hashes, indent=2) + "\n", encoding="utf-8"
    )
    print(args.output)


def snapshot_sources(byway, output):
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(byway / "crates/ip-intel-format/src", output / "format/src")
    shutil.copytree(byway / "vendor/maxminddb", output / "maxminddb")

    source = byway / "services/ip-intel-aggregator/src"
    for relative in SOURCE_FILES:
        target = output / "src" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / relative, target)
    shutil.copyfile(Path(__file__).with_name("adapter.rs"), output / "src/main.rs")


def write_manifests(output, version):
    local = version == "local"
    checkout = Path(__file__).resolve().parents[2]
    selector = (
        f"path = {json.dumps(checkout.as_posix())}"
        if local
        else f'version = "={version}"'
    )
    dependencies = (
        f"netindex = {{ {selector}, features = "
        '["external-sort", "mmdb", "mmap", "shared-values", "simd"] }\n'
        + COMMON_DEPENDENCIES
    )

    features = 'default = ["component-layout"]\n' if local else ""
    (output / "Cargo.toml").write_text(
        '[package]\nname = "native-city-audit"\nversion = "0.0.0"\nedition = "2024"\n'
        "[workspace]\n[profile.release]\ncodegen-units = 1\n"
        f"[features]\n{features}component-layout = []\n"
        "[dependencies]\n"
        + dependencies
        + 'ip-intel-format = { path = "format" }\nmaxminddb = "0.32.0"\nsha2 = "0.10"\ntempfile = "3"\n'
        '[patch.crates-io]\nmaxminddb = { path = "maxminddb" }\n',
        encoding="utf-8",
    )
    (output / "format/Cargo.toml").write_text(
        '[package]\nname = "ip-intel-format"\nversion = "0.0.0"\nedition = "2024"\n'
        "[dependencies]\n" + dependencies,
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
