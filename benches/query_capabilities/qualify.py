"""Build and qualify the benchmark application, never the library release policy.

Run with Python 3.12+, Rust 1.95.0 and its llvm-tools-preview component.
Outputs stay in an ignored directory; datasets are never copied into the repo.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time

BASELINE = "8d4f5b1fb33994ced4ea39e7478fc4e3697c8081"
REPO = Path(__file__).resolve().parents[2]
STUDY = Path(__file__).resolve().parent
VARIANTS = ["default", "thin", "pgo", "thin-pgo"]
SHAPES = ["packed-v4", "bitpacked-v4", "packed-v6", "bitpacked-v6",
          "overlaps", "asn", "mixed", "aliases"]


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def run(args, log, env=None, cpu=None):
    with log.open("w", encoding="utf-8") as output:
        before = time.perf_counter_ns()
        kwargs = {}
        if cpu is not None and os.name != "nt":
            kwargs["preexec_fn"] = lambda: os.sched_setaffinity(0, {cpu})
        process = subprocess.Popen([str(a) for a in args], cwd=REPO,
                                   env=env, stdout=output, stderr=subprocess.STDOUT,
                                   **kwargs)
        if cpu is not None and os.name == "nt":
            set_affinity = ctypes.windll.kernel32.SetProcessAffinityMask
            set_affinity.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
            if not set_affinity(int(process._handle), 1 << cpu):
                process.terminate()
                process.wait()
                raise ctypes.WinError()
        result = process.wait()
        elapsed = (time.perf_counter_ns() - before) / 1e6
    if result:
        raise RuntimeError(f"exit {result}: {args}; see {log}")
    return elapsed


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cpu", type=int, default=20)
    parser.add_argument("--input", action="append", default=[])
    parser.add_argument("--target", default="x86_64-pc-windows-msvc" if os.name == "nt"
                        else "x86_64-unknown-linux-gnu")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    logs, binaries = out / "logs", out / "bin"
    logs.mkdir()
    binaries.mkdir()
    inputs = SHAPES + [str(Path(p).resolve()) for p in args.input]
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(out / "build")
    env.pop("RUSTFLAGS", None)
    env.pop("CARGO_PROFILE_RELEASE_LTO", None)
    suffix = ".exe" if "windows" in args.target else ""
    executable = out / "build" / args.target / "release" / ("netindex-query-study" + suffix)
    rust_sysroot = Path(subprocess.check_output(
        ["rustc", "+1.95.0", "--print", "sysroot"], text=True).strip())
    host = next(line.split(": ", 1)[1] for line in subprocess.check_output(
        ["rustc", "+1.95.0", "-vV"], text=True).splitlines() if line.startswith("host:"))
    profdata = rust_sysroot / "lib" / "rustlib" / host / "bin" / ("llvm-profdata" + suffix)
    if not profdata.exists():
        raise RuntimeError("Install: rustup component add llvm-tools-preview --toolchain 1.95.0")
    archive = out / "baseline.tar"
    with archive.open("wb") as file:
        subprocess.run(["git", "archive", BASELINE], cwd=REPO, stdout=file, check=True)
    baseline = out / "baseline"
    baseline.mkdir()
    with tarfile.open(archive) as tar:
        tar.extractall(baseline, filter="data")
    helper = baseline / "benches" / "query_capabilities"
    helper.mkdir(parents=True)
    for name in ["Cargo.toml", "Cargo.lock"]:
        shutil.copy2(STUDY / name, helper / name)
    shutil.copytree(STUDY / "src", helper / "src")
    manifest = STUDY / "Cargo.toml"

    def build(label, current_env, baseline_build=False):
        command = ["cargo", "+1.95.0", "build", "--locked", "--release", "--target", args.target,
                   "--manifest-path", helper / "Cargo.toml" if baseline_build else manifest]
        if baseline_build:
            command += ["--no-default-features", "--features", "simd"]
        run(command, logs / f"build-{label}.log", current_env)
        shutil.copy2(executable, binaries / (label + suffix))

    build("baseline", env, True)
    build("default", env)
    thin = env | {"CARGO_PROFILE_RELEASE_LTO": "thin"}
    build("thin", thin)
    for variant, base in [("pgo", env), ("thin-pgo", thin)]:
        profile = out / (variant + "-profiles")
        profile.mkdir()
        build(variant + "-instrumented", base | {"RUSTFLAGS": f"-Cprofile-generate={profile.as_posix()}"})
        run([binaries / (variant + "-instrumented" + suffix), "train", *inputs],
            logs / f"train-{variant}.jsonl",
            base | {"LLVM_PROFILE_FILE": (profile / "%p-%m.profraw").as_posix()}, args.cpu)
        run([profdata, "merge", "-o", profile / "merged.profdata", *profile.glob("*.profraw")],
            logs / f"merge-{variant}.log")
        run([profdata, "show", profile / "merged.profdata"], logs / f"profile-{variant}.txt")
        build(variant, base | {"RUSTFLAGS": f"-Cprofile-use={(profile / 'merged.profdata').as_posix()} -Cllvm-args=-pgo-warn-missing-function"})
    for round_number, order in enumerate([VARIANTS, VARIANTS[::-1], ["pgo", "default", "thin-pgo", "thin"]]):
        for variant in order:
            run([binaries / (variant + suffix), "measure", *inputs],
                logs / f"measure-{variant}-r{round_number}.jsonl", cpu=args.cpu)
        print(f"measurement round {round_number} completed", flush=True)
    startup = []
    for trial in range(10):
        order = ["baseline", *VARIANTS]
        if trial % 2:
            order.reverse()
        for variant in order:
            file = logs / f"startup-{variant}-{trial}.jsonl"
            wall = run([binaries / (variant + suffix), "startup", "packed-v4"], file, cpu=args.cpu)
            row = next(json.loads(s) for s in file.read_text().splitlines()
                       if s.startswith('{') and json.loads(s)["phase"] == "input")
            startup.append({"variant": variant, "trial": trial, "wall_ms": wall,
                            "main_ready_ms": row["process_ready_ms"], "setup_ms": row["setup_ms"]})
    (logs / "startup-wall.json").write_text(json.dumps(startup, indent=2) + "\n")
    identity = {"baseline": BASELINE, "target": args.target, "cpu": args.cpu,
                "input_paths": inputs, "source_sha256": {
                    str(p.relative_to(REPO)): digest(p) for p in [
                        *sorted((REPO / "src").rglob("*.rs")), STUDY / "src/main.rs",
                        STUDY / "Cargo.toml", STUDY / "Cargo.lock"]},
                "binary_bytes": {v: (binaries / (v + suffix)).stat().st_size for v in ["baseline", *VARIANTS]},
                "binary_sha256": {v: digest(binaries / (v + suffix)) for v in ["baseline", *VARIANTS]},
                "profile_sha256": {v: digest(out / (v + "-profiles") / "merged.profdata") for v in ["pgo", "thin-pgo"]}}
    (logs / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    run(["rustc", "+1.95.0", "-vV"], logs / "rust.txt")
    print(f"complete: {out}", flush=True)


if __name__ == "__main__":
    main()
