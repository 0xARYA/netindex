"""Summarize JSONL samples and reject mismatched inputs or result checksums."""
import argparse
from collections import defaultdict
from fractions import Fraction
import json
from pathlib import Path
import statistics


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("logs", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    inputs = {}
    checksums = {}
    timings = defaultdict(list)
    for file in sorted(args.logs.glob("measure-*.jsonl")):
        variant = file.name.removeprefix("measure-").rsplit("-r", 1)[0]
        for line in file.read_text().splitlines():
            row = json.loads(line)
            case = Path(row["case"]).name
            if row["phase"] == "input":
                identity = {k: row[k] for k in ["sha256", "bytes", "records", "seed"]}
                if case in inputs and inputs[case] != identity:
                    raise ValueError(f"input mismatch: {case}")
                inputs[case] = identity
            if row["phase"] != "timing":
                continue
            method = row["method"]
            family = "point" if method.startswith("batch-") else method
            family = "member" if family.startswith("member") else family
            family = family.removesuffix("-scan")
            key = (case, family)
            checksum = Fraction(row["checksum"], row["queries"])
            if key in checksums and checksums[key] != checksum:
                raise ValueError(f"checksum mismatch: {key}")
            checksums[key] = checksum
            timings[(case, variant, method)].append(row["ns_per_query"])
    rows = []
    for (case, variant, method), values in sorted(timings.items()):
        rows.append({"case": case, "variant": variant, "method": method,
                     "samples": len(values), "ns_per_query": {
                         "median": statistics.median(values), "min": min(values), "max": max(values)}})
    result = {"inputs": inputs, "checksum_groups_verified": len(checksums), "timings": rows}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"{len(rows)} timing cells; {len(checksums)} equivalent-result groups verified")


if __name__ == "__main__":
    main()
