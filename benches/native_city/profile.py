"""Measure Linux City file residency, startup, and lookup hardware counters."""

import argparse
import ctypes
import hashlib
import json
import mmap
import os
from pathlib import Path
import select
import subprocess
import tempfile


def residency(path, cache):
    with path.open("rb", buffering=0) as stream:
        if cache == "cold":
            os.posix_fadvise(stream.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
        else:
            while stream.read(1024 * 1024):
                pass
        size = path.stat().st_size
        pages = (size + mmap.PAGESIZE - 1) // mmap.PAGESIZE
        vector = (ctypes.c_ubyte * pages)()
        with mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_COPY) as view:
            address = ctypes.addressof(ctypes.c_char.from_buffer(view))
            libc = ctypes.CDLL(None, use_errno=True)
            if libc.mincore(ctypes.c_void_p(address), ctypes.c_size_t(size), vector) != 0:
                raise OSError(ctypes.get_errno(), "mincore")
    resident = sum(value & 1 for value in vector)
    if resident != (0 if cache == "cold" else pages):
        raise ValueError(f"{path.name}: {resident}/{pages} pages resident ({cache})")
    return {"resident": resident, "total": pages}


EVENTS = ("cycles:u", "instructions:u", "cache-references:u", "cache-misses:u")
FAULTS = ("minor-faults", "major-faults")


def run(perf, command, lookup_only):
    events = EVENTS if lookup_only else EVENTS + FAULTS
    env = os.environ.copy()
    env.pop("NETINDEX_PROFILE_LOOKUPS", None)
    env["LC_ALL"] = "C"
    with tempfile.TemporaryDirectory() as tmp:
        log = Path(tmp) / "stat.csv"
        prefix = [perf, "stat", "-x", ",", "-o", str(log), "-e", ",".join(events)]
        if lookup_only:
            env["NETINDEX_PROFILE_LOOKUPS"] = "1"
            ctl_r, ctl_w = os.pipe()
            ack_r, ack_w = os.pipe()
            with os.fdopen(ctl_r, "rb"), os.fdopen(ack_w, "wb"), \
                 os.fdopen(ctl_w, "wb", buffering=0) as control, \
                 os.fdopen(ack_r, "rb", buffering=0) as ack:
                with subprocess.Popen(
                    [*prefix, "--delay=-1", "--control", f"fd:{ctl_r},{ack_w}", "--", *command],
                    env=env, pass_fds=(ctl_r, ack_w), text=True,
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                ) as proc:
                    # Only the lookup loop runs while the counters are enabled.
                    def gate(expected, action):
                        line = proc.stdout.readline()
                        if json.loads(line)["phase"] != expected:
                            raise ValueError(f"unexpected profile gate: {line}")
                        control.write(action + b"\n")
                        ready = select.select([ack], [], [], 10)[0]
                        response = os.read(ack.fileno(), 64) if ready else b""
                        if response.strip(b"\x00\r\n") != b"ack":
                            raise RuntimeError(f"perf did not acknowledge {action!r}: {response!r}")
                        proc.stdin.write("continue\n")
                        proc.stdin.flush()
                    try:
                        opened = proc.stdout.readline()
                        gate("lookup-ready", b"enable")
                        gate("lookup-finished", b"disable")
                        stdout, _ = proc.communicate(timeout=30)
                        if proc.returncode:
                            raise subprocess.CalledProcessError(proc.returncode, command)
                        stdout = opened + stdout
                    except BaseException:
                        proc.kill()
                        raise
        else:
            stdout = subprocess.check_output(
                [*prefix, "--", *command], text=True, env=env, timeout=120
            )
        counters = {}
        for line in log.read_text().splitlines():
            fields = line.split(",")
            if len(fields) >= 5 and fields[2] in events:
                counters[fields[2]] = {
                    "count": int(fields[0]),
                    "running_pct": float(fields[4]),
                }
        if set(counters) != set(events):
            raise ValueError(f"missing perf counters: {log.read_text()}")
    return {"rows": [json.loads(line) for line in stdout.splitlines()], "counters": counters}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "native", "shared", "bitpacked", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--perf", default="perf")
    parser.add_argument("--cpu", required=True, type=int)
    args = parser.parse_args()
    os.sched_setaffinity(0, {args.cpu})
    exe = str(args.executable.resolve())
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
                    cache = residency(path, "warm" if mode == "lookup" else mode)
                    command = [exe, "native", str(path)] if case == "native" else [
                        exe, "converted", str(args.native), str(path),
                    ]
                    result = run(args.perf, command, mode == "lookup")
                    receipt["runs"].append(dict(trial=trial, case=case, mode=mode, cache=cache, **result))
                    print(f"{trial} {mode} {case}", flush=True)
        for path in (args.executable, *cases.values()):
            with path.open("rb") as stream:
                receipt.setdefault("sha256", {})[path.name] = hashlib.file_digest(stream, "sha256").hexdigest()
        receipt["layout"] = {
            case: json.loads(subprocess.check_output(
                [exe, "layout-native" if case == "native" else "layout-converted", path], text=True
            )) for case, path in cases.items()
        }
        json.dump(receipt, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
