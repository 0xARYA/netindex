"""Verify retained P1-10 files independently and collect raw measurement receipts.

Run on Linux: python3 verify.py BASE INPUT_DIRECTORY OUTPUT_JSON
BASE contains source/, baseline/, outputs/, logs/ from the documented commands.
"""

import hashlib
import json
import mmap
import os
import pathlib
import platform
import re
import statistics
import struct
import subprocess
import sys


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return {"bytes": path.stat().st_size, "sha256": digest.hexdigest()}


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def resources(path):
    text = path.read_text()
    return {"maximum_rss_kib": int(re.search(r"Maximum resident set size \(kbytes\): (\d+)", text)[1]),
            "user_seconds": float(re.search(r"User time \(seconds\): ([\d.]+)", text)[1]),
            "system_seconds": float(re.search(r"System time \(seconds\): ([\d.]+)", text)[1])}


def columns(data):
    assert data[:8] == b"NETINDEX"
    version, flags = struct.unpack_from("<II", data, 8)
    count, v4, v6, asn, metadata, pool_bytes, p4, p6 = struct.unpack_from("<8Q", data, 16)
    assert count == v4 + v6 + asn
    slots = 80 + metadata + (p4 if flags & 1 else v4 * 20) + (p6 if flags & 2 else v6 * 56) + asn * 8
    if version == 3:
        assert flags & 4 and not flags & ~7
        values, width, ids = None, 8, slots
        pool = slots + count * 8
    else:
        assert version == 4 and flags & 16 and not flags & ~19
        values, width = struct.unpack_from("<IB", data, slots)
        assert 0 < values <= count and width == ((values - 1).bit_length() + 7) // 8
        assert data[slots + 5:slots + 8] == bytes(3)
        ids = slots + 8 + values * 8
        pool = ids + count * width
    assert pool + pool_bytes == len(data)
    return {"count": count, "slots": slots, "pool": pool, "values": values, "width": width, "ids": ids}


def verify_pair(original, compact):
    with original.open("rb") as old_file, compact.open("rb") as new_file, \
            mmap.mmap(old_file.fileno(), 0, access=mmap.ACCESS_READ) as old, \
            mmap.mmap(new_file.fileno(), 0, access=mmap.ACCESS_READ) as new:
        a, b = columns(old), columns(new)
        assert a["count"] == b["count"]
        assert old[16:80] == new[16:80]
        assert old[80:a["slots"]] == new[80:b["slots"]], "metadata, targets, IDs, augmentation or ordering changed"
        assert old[a["pool"]:] == new[b["pool"]:], "opaque pool changed"
        end = 0
        for ordinal in range(b["values"]):
            start, length = struct.unpack_from("<II", new, b["slots"] + 8 + ordinal * 8)
            assert start == end
            end += length
        assert end == len(new) - b["pool"]
        next_ordinal = 0
        for record in range(a["count"]):
            at = b["ids"] + record * b["width"]
            ordinal = int.from_bytes(new[at:at + b["width"]], "little")
            assert ordinal < b["values"] and ordinal <= next_ordinal
            if ordinal == next_ordinal:
                next_ordinal += 1
            expected = struct.unpack_from("<II", old, a["slots"] + record * 8)
            actual = struct.unpack_from("<II", new, b["slots"] + 8 + ordinal * 8)
            assert expected == actual, (record, expected, actual)
        assert next_ordinal == b["values"]
        return {"assertions": a["count"], "directory_values": b["values"], "reference_width": b["width"],
                "old_reference_bytes": a["pool"] - a["slots"], "new_reference_bytes": b["pool"] - b["slots"],
                "saved_bytes": len(old) - len(new), "metadata_targets_ids_order_and_pool_byte_identical": True,
                "every_insertion_id_selects_identical_span": True, "first_seen_ordinals_and_directory_verified": True}


def main():
    base, inputs, output = map(pathlib.Path, sys.argv[1:])
    document = {"schema": "netindex-compact-references-v1", "date": "2026-11-05", "baseline_revision": "552ab08",
                "environment": {"platform": platform.platform(), "cpu": next(line.split(":", 1)[1].strip() for line in pathlib.Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")),
                                "pinned_cpu": 20, "rust": subprocess.check_output(["rustc", "+1.95.0", "-Vv"], text=True),
                                "features": "all", "profile": "release; codegen-units=1; no PGO/LTO/custom CPU flags", "page_cache": "warmed before each lookup process; no cache dropping"},
                "scope": {"build": "opaque insertion-order replay; excludes preparation, native decoding and fsync; process peak RSS includes retained replay assertions and verification", "lookup": "scalar query loop with runtime SIMD search enabled; callback consumes ID, length, first and last payload byte; sampled target starts and uniform mixed IPv4/IPv6; no application decoding", "open": "full mmap validation; process RSS immediately after open; filesystem cache excluded from RSS; warm file cache"},
                "sources": {}, "binaries": {}, "inputs": {}, "outputs": {}, "builds": [], "lookups": [], "independent": {}, "summary": {}}
    for tree in ["source", "baseline"]:
        for path in [base / tree / "Cargo.toml", base / tree / "Cargo.lock", *sorted((base / tree / "src").rglob("*.rs")), base / tree / "examples/reference_lookup.rs"]:
            document["sources"][str(path.relative_to(base))] = sha(path)
        for name in (["rebuild", "reference_lookup"] if tree == "source" else ["reference_lookup"]):
            path = base / tree / "target/release/examples" / name
            document["binaries"][str(path.relative_to(base))] = sha(path)
    document["sources"]["source/examples/rebuild.rs"] = sha(base / "source/examples/rebuild.rs")
    for dataset, filename in [("city", "city-current.nidx"), ("asn", "asn.nidx")]:
        original = inputs / filename
        document["inputs"][dataset] = sha(original)
        for mode in ["external-legacy", "external", "memory-legacy", "memory"]:
            for round_ in [4, 5]:
                key = f"{dataset}-{mode}-r{round_}"
                path = base / "outputs" / f"{key}.nidx"
                document["outputs"][key] = sha(path)
                log = rows(base / "logs" / f"{key}.jsonl")
                assert [row["phase"] for row in log] == ["prepared", "built", "verified"]
                assert log[-1]["assertions"] == log[1]["records"] and log[-1]["queries"] == 100_005
                document["builds"].append({"key": key, "dataset": dataset, "mode": mode, "round": round_, "phases": log, **resources(base / "logs" / f"{key}.time")})
                expected = document["inputs"][dataset] if "legacy" in mode else document["outputs"][f"{dataset}-external-r4"]
                assert document["outputs"][key] == expected, "writers or rounds disagree"
        document["independent"][dataset] = verify_pair(original, base / "outputs" / f"{dataset}-external-r4.nidx")
        expected_patterns = None
        for variant in ["old-v3", "new-v4", "new-v3"]:
            for round_ in [6, 7, 8]:
                key = f"lookup-{dataset}-{variant}-r{round_}"
                log = rows(base / "logs" / f"{key}.jsonl")
                identities = [{field: row[field] for field in ["pattern", "queries_per_sample", "matches_per_sample", "digest"]} for row in log[1:]]
                if expected_patterns is None:
                    expected_patterns = identities
                assert identities == expected_patterns, "query results differ across readers/encodings"
                document["lookups"].append({"key": key, "dataset": dataset, "variant": variant, "round": round_, "phases": log, **resources(base / "logs" / f"{key}.time")})
        summary = {}
        for variant in ["old-v3", "new-v4", "new-v3"]:
            entries = [row for row in document["lookups"] if row["dataset"] == dataset and row["variant"] == variant]
            summary[variant] = {"open_ms": [row["phases"][0]["open_ns"] / 1e6 for row in entries], "open_rss_kib": [row["phases"][0]["open_rss_kib"] for row in entries], "lookup_ns": {pattern: [statistics.median(next(phase for phase in row["phases"][1:] if phase["pattern"] == pattern)["ns_per_query"]) for row in entries] for pattern in ["sampled-target-hits", "uniform-mixed-ip"]}}
        document["summary"][dataset] = summary
    rejection = (base / "logs/old-reader-reject.log").read_text()
    assert "Version(4)" in rejection
    document["old_reader_rejects_format4"] = rejection.strip()
    document["disassembly"] = sha(base / "logs/reference_lookup.asm")
    document["environment"]["build_environment_overrides"] = {key: os.environ.get(key) for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS"]}
    root = output.resolve().parents[2]
    current = {}
    for name, entry in document["sources"].items():
        if name.startswith("source/"):
            path = root / name.removeprefix("source/")
            assert sha(path) == entry, f"measured source differs from workspace: {path}"
            current[str(path.relative_to(root))] = entry
    document["production_snapshot_matches_workspace"] = True
    document["collector"] = sha(pathlib.Path(__file__))
    document["checks"] = json.loads((root / "target/compact-checks/results.json").read_text())
    assert all(check["exit_code"] == 0 for check in document["checks"])
    document["fuzz_smoke"] = {}
    for target, filename in [("reader", "fuzz-read.log"), ("final_writer", "fuzz-encode-final.log")]:
        path = base / "logs" / filename
        match = re.search(r"Done (\d+) runs in (\d+) second", path.read_text())
        assert match is not None
        document["fuzz_smoke"][target] = {"runs": int(match[1]), "seconds": int(match[2]), "sanitizer": "address", "log": sha(path)}
    output.write_text(json.dumps(document, indent=2) + "\n")
    print(json.dumps({"verified": document["independent"], "result": str(output)}, indent=2))


if __name__ == "__main__":
    main()
