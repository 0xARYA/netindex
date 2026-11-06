"""Replay a private City trace and measure startup/reload on Linux."""

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from city_files import sha256_file, verify_residency
from city_process import trial_process


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
    cases = {
        case: getattr(args, case).resolve()
        for case in ("native", "shared", "bitpacked")
    }

    with (
        args.output.open("x") as output,
        tempfile.TemporaryDirectory(dir=args.output.parent) as tmp,
    ):
        hashes = {case: sha256_file(path) for case, path in cases.items()}
        parity = {
            case: run_trial(
                exe, ("parity", native, cases[case], trace), args.output.parent
            )["rows"][-1]
            for case in ("shared", "bitpacked")
        }
        replacements = prepare_replacements(cases, hashes, Path(tmp))
        receipt = dict(
            cpu=args.cpu,
            executable_sha256=sha256_file(exe),
            input_sha256=hashes,
            parity=parity,
            runs=[],
        )
        for trial in range(3):
            order = list(cases)[trial:] + list(cases)[:trial]
            modes = ["cold", "warm", "reload"]
            if trial % 2:
                modes.reverse()
            for mode in modes:
                for case in order:
                    path = cases[case]
                    cache = verify_residency(path, "cold" if mode == "cold" else "warm")
                    command = (
                        ["native", path, "-", trace]
                        if case == "native"
                        else [
                            "converted",
                            native,
                            path,
                            trace,
                        ]
                    )
                    replacement_cache = None
                    if mode == "reload":
                        replacement = replacements[case]
                        replacement_cache = verify_residency(replacement, "cold")
                        command = (
                            ["reload-native", path, "-", replacement]
                            if case == "native"
                            else ["reload-converted", native, path, replacement]
                        )
                    result = run_trial(exe, command, args.output.parent)
                    receipt["runs"].append(
                        dict(
                            trial=trial,
                            case=case,
                            mode=mode,
                            cache=cache,
                            replacement_cache=replacement_cache,
                            **result,
                        )
                    )
                    print(f"{trial} {mode} {case}", flush=True)
        json.dump(receipt, output, indent=2)
        output.write("\n")


def run_trial(executable, command, directory):
    with tempfile.TemporaryDirectory(dir=directory) as tmp:
        timing = Path(tmp) / "time"
        with trial_process(
            ["/usr/bin/time", "-o", timing, "-f", "%M %F %R", executable, *command],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        ) as process:
            stdout, _ = process.communicate(timeout=120)
            if process.returncode:
                # A parity failure can contain a private address and location.
                raise RuntimeError(f"{command[0]} failed (exit {process.returncode})")
        rss, major, minor = map(int, timing.read_text().split())

    return {
        "rows": list(map(json.loads, stdout.splitlines())),
        "peak_rss_kib": rss,
        "major_faults": major,
        "minor_faults": minor,
    }


def prepare_replacements(cases, hashes, directory):
    replacements = {}
    for case, path in cases.items():
        replacement = directory / case
        shutil.copyfile(path, replacement)
        with replacement.open("rb") as stream:
            os.fsync(stream.fileno())
        if replacement.samefile(path) or sha256_file(replacement) != hashes[case]:
            raise ValueError("reload needs a distinct, byte-identical file")
        replacements[case] = replacement

    return replacements


if __name__ == "__main__":
    main()
