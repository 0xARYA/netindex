"""Replay a private City trace and measure startup/reload on Linux."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from profile import residency


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "native", "shared", "bitpacked", "trace", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--cpu", type=int, required=True)
    args = parser.parse_args()
    os.sched_setaffinity(0, {args.cpu})
    os.nice(10)
    exe = args.executable.resolve()
    native = args.native.resolve()
    trace = args.trace.resolve()
    cases = {case: getattr(args, case).resolve() for case in ("native", "shared", "bitpacked")}

    def run(*command):
        with tempfile.TemporaryDirectory(dir=args.output.parent) as tmp:
            timing = Path(tmp) / "time"
            result = subprocess.run(
                ["/usr/bin/time", "-o", timing, "-f", "%M %F %R", exe, *command],
                capture_output=True, text=True, timeout=120,
            )
            if result.returncode:
                # A parity failure can contain a private address and location.
                raise RuntimeError(f"{command[0]} failed (exit {result.returncode})")
            rss, major, minor = map(int, timing.read_text().split())
            return dict(rows=list(map(json.loads, result.stdout.splitlines())),
                        peak_rss_kib=rss, major_faults=major, minor_faults=minor)

    def digest(path):
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()

    with args.output.open("x") as output, tempfile.TemporaryDirectory(dir=args.output.parent) as tmp:
        hashes = {case: digest(path) for case, path in cases.items()}
        parity = {case: run("parity", native, cases[case], trace)["rows"][-1]
                  for case in ("shared", "bitpacked")}
        replacements = {}
        for case, path in cases.items():
            replacement = Path(tmp) / case
            shutil.copyfile(path, replacement)
            with replacement.open("rb") as f:
                os.fsync(f.fileno())
            if replacement.samefile(path) or digest(replacement) != hashes[case]:
                raise ValueError("reload needs a distinct, byte-identical file")
            replacements[case] = replacement
        receipt = dict(cpu=args.cpu, executable_sha256=digest(exe), input_sha256=hashes,
                       parity=parity, runs=[])
        for trial in range(3):
            order = list(cases)[trial:] + list(cases)[:trial]
            modes = ["cold", "warm", "reload"]
            if trial % 2:
                modes.reverse()
            for mode in modes:
                for case in order:
                    path = cases[case]
                    cache = residency(path, "cold" if mode == "cold" else "warm")
                    command = ["native", path, "-", trace] if case == "native" else [
                        "converted", native, path, trace,
                    ]
                    replacement_cache = None
                    if mode == "reload":
                        replacement = replacements[case]
                        replacement_cache = residency(replacement, "cold")
                        command = (["reload-native", path, "-", replacement] if case == "native" else
                                   ["reload-converted", native, path, replacement])
                    result = run(*command)
                    receipt["runs"].append(dict(trial=trial, case=case, mode=mode,
                                                cache=cache, replacement_cache=replacement_cache, **result))
                    print(f"{trial} {mode} {case}", flush=True)
        json.dump(receipt, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
