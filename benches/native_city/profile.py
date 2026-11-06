"""Measure Linux City file residency, startup, and lookup hardware counters."""

import argparse
import json
import os
import select
import subprocess
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path

from city_files import sha256_file, verify_residency
from city_process import trial_process


EVENTS = ("cycles:u", "instructions:u", "cache-references:u", "cache-misses:u")
FAULTS = ("minor-faults", "major-faults")
GATE_TIMEOUT = 120
ACK_TIMEOUT = 10


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "native", "shared", "bitpacked", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--perf", default="perf")
    parser.add_argument("--cpu", required=True, type=int)
    args = parser.parse_args()

    os.sched_setaffinity(0, {args.cpu})
    executable = args.executable.resolve()
    cases = {"native": args.native, "shared": args.shared, "bitpacked": args.bitpacked}

    receipt = {"cpu": args.cpu, "runs": []}
    with args.output.open("x") as output:
        for trial in range(3):
            order = list(cases)[trial:] + list(cases)[:trial]
            modes = ["cold", "warm", "lookup"]
            if trial % 2:
                modes.reverse()
            for mode in modes:
                for case in order:
                    path = cases[case]
                    cache = verify_residency(path, "warm" if mode == "lookup" else mode)
                    command = (
                        [executable, "native", path]
                        if case == "native"
                        else [executable, "converted", args.native, path]
                    )
                    result = measure(args.perf, command, mode == "lookup")
                    receipt["runs"].append(
                        dict(trial=trial, case=case, mode=mode, cache=cache, **result)
                    )
                    print(f"{trial} {mode} {case}", flush=True)

        receipt["sha256"] = {
            path.name: sha256_file(path) for path in (args.executable, *cases.values())
        }
        receipt["layout"] = {
            case: json.loads(
                subprocess.check_output(
                    [
                        executable,
                        "layout-native" if case == "native" else "layout-converted",
                        path,
                    ],
                    text=True,
                    timeout=120,
                )
            )
            for case, path in cases.items()
        }
        json.dump(receipt, output, indent=2)
        output.write("\n")


def measure(perf, command, lookup_only):
    events = EVENTS if lookup_only else EVENTS + FAULTS
    env = os.environ.copy()
    env.pop("NETINDEX_PROFILE_LOOKUPS", None)
    env["LC_ALL"] = "C"

    with tempfile.TemporaryDirectory() as tmp:
        log = Path(tmp) / "stat.csv"
        prefix = [perf, "stat", "-x", ",", "-o", str(log), "-e", ",".join(events)]
        if lookup_only:
            env["NETINDEX_PROFILE_LOOKUPS"] = "1"
            stdout = measure_lookup(prefix, command, env)
        else:
            with trial_process(
                [*prefix, "--", *command],
                text=True,
                stdout=subprocess.PIPE,
                env=env,
            ) as process:
                stdout, _ = process.communicate(timeout=120)
                if process.returncode:
                    raise subprocess.CalledProcessError(process.returncode, command)
        counters = parse_counters(log, events)

    return {
        "rows": [json.loads(line) for line in stdout.splitlines()],
        "counters": counters,
    }


def measure_lookup(prefix, command, env):
    with ExitStack() as pipes:
        ctl_r, ctl_w = os.pipe()
        child_control = pipes.enter_context(os.fdopen(ctl_r, "rb"))
        control = pipes.enter_context(os.fdopen(ctl_w, "wb", buffering=0))
        ack_r, ack_w = os.pipe()
        ack = pipes.enter_context(os.fdopen(ack_r, "rb", buffering=0))
        child_ack = pipes.enter_context(os.fdopen(ack_w, "wb"))

        invocation = [
            *prefix,
            "--delay=-1",
            "--control",
            f"fd:{ctl_r},{ack_w}",
            "--",
            *command,
        ]
        with trial_process(
            invocation,
            env=env,
            pass_fds=(ctl_r, ack_w),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        ) as process:
            child_control.close()
            child_ack.close()

            phases = PhaseReader(process.stdout.fileno())
            opened = phases.expect("opened")
            advance_gate(process, phases, control, ack, "lookup-ready", b"enable")
            advance_gate(process, phases, control, ack, "lookup-finished", b"disable")

            stdout, _ = process.communicate(timeout=30)
            if process.returncode:
                raise subprocess.CalledProcessError(process.returncode, command)

    return b"\n".join((opened, phases.pending + stdout)).decode()


def advance_gate(process, phases, control, ack, expected, action):
    phases.expect(expected)
    control.write(action + b"\n")
    ready = select.select([ack], [], [], ACK_TIMEOUT)[0]
    response = os.read(ack.fileno(), 64) if ready else b""
    if response.strip(b"\x00\r\n") != b"ack":
        raise RuntimeError(f"perf did not acknowledge {action!r}: {response!r}")

    process.stdin.write(b"continue\n")
    process.stdin.flush()


class PhaseReader:
    """Read newline-delimited phases without an unbounded buffered readline."""

    def __init__(self, fd):
        self.fd = fd
        self.pending = b""

    def expect(self, phase):
        deadline = time.monotonic() + GATE_TIMEOUT
        while b"\n" not in self.pending:
            remaining = max(0, deadline - time.monotonic())
            if not remaining or not select.select([self.fd], [], [], remaining)[0]:
                raise TimeoutError(f"timed out waiting for {phase}")
            block = os.read(self.fd, 4096)
            if not block:
                raise RuntimeError(f"adapter exited before {phase}")
            self.pending += block
            if len(self.pending) > 65536:
                raise RuntimeError(f"oversized adapter output before {phase}")

        line, _, self.pending = self.pending.partition(b"\n")
        if json.loads(line)["phase"] != phase:
            raise ValueError(f"unexpected adapter phase: {line!r}")

        return line


def parse_counters(log, events):
    counters = {}
    contents = log.read_text()
    for line in contents.splitlines():
        fields = line.split(",")
        if len(fields) >= 5 and fields[2] in events:
            counters[fields[2]] = {
                "count": int(fields[0]),
                "running_pct": float(fields[4]),
            }
    if set(counters) != set(events):
        raise ValueError(f"missing perf counters: {contents}")

    return counters


if __name__ == "__main__":
    main()
