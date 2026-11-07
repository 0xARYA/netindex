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
    parser.add_argument(
        "--baseline", type=Path, help="alternate a baseline with the candidate"
    )
    args = parser.parse_args()
    os.sched_setaffinity(0, {args.cpu})
    os.nice(10)
    exe = args.executable.resolve()
    baseline = args.baseline.resolve() if args.baseline else None
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
        receipt = {
            "cpu": args.cpu,
            "executable_sha256": sha256_file(exe),
            "input_sha256": hashes,
            "parity": parity,
            "runs": [],
        }
        if baseline:
            receipt["baseline_sha256"] = sha256_file(baseline)
            receipt["baseline_parity"] = {
                case: run_trial(
                    baseline, ("parity", native, cases[case], trace), args.output.parent
                )["rows"][-1]
                for case in ("shared", "bitpacked")
            }
        for trial in range(3):
            order = list(cases)[trial:] + list(cases)[:trial]
            modes = ["cold", "warm", "reload"]
            if trial % 2:
                modes.reverse()
            for mode in modes:
                for case in order:
                    path = cases[case]
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
                        command = (
                            ["reload-native", path, "-", replacement]
                            if case == "native"
                            else ["reload-converted", native, path, replacement]
                        )
                    builds = [("candidate", exe)]
                    if baseline and case != "native":
                        builds.insert(0, ("baseline", baseline))
                        if trial % 2:
                            builds.reverse()
                    for build, executable in builds:
                        cache = verify_residency(
                            path, "cold" if mode == "cold" else "warm"
                        )
                        if mode == "reload":
                            replacement_cache = verify_residency(replacement, "cold")
                        result = run_trial(executable, command, args.output.parent)
                        row = dict(
                            trial=trial,
                            case=case,
                            mode=mode,
                            cache=cache,
                            replacement_cache=replacement_cache,
                            **result,
                        )
                        if baseline:
                            row["build"] = build
                        receipt["runs"].append(row)
                        print(f"{trial} {mode} {case} {build}", flush=True)
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
