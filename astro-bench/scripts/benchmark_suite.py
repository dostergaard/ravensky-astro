"""Bounded experiments around the existing release CLI; Python standard library only.

Run from the Rust workspace root. Each invocation owns a new output directory.
Raw Rust reports retain their original schema/provenance. Supplementary experiment
records identify this script separately. No application settings are changed.
"""

import argparse
import collections
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time

MIB = 1024**2


def require(condition, message):
    if not condition:
        raise ValueError(message)


def percentile(values, percent):
    require(bool(values), "no observations")
    return sorted(values)[max(0, math.ceil(len(values) * percent / 100) - 1)]


def audit_report(data, workers, repeats):
    require(data["schema_version"] == 1, "unknown report schema")
    require(data["provenance"]["build_profile"] == "release", "not a release build")
    require(len(data["provenance"]["source_sha256"]) == 64, "missing source fingerprint")
    files = data["manifest"]["files"]
    require(len(files) == data["manifest"]["recipe"]["frames"], "manifest count mismatch")
    require(collections.Counter(s["workers"] for s in data["samples"]) ==
            {w: repeats for w in workers}, "missing or duplicate samples")
    require(len({s["workload"] for s in data["samples"]}) == 1, "mixed workloads")
    for sample in data["samples"]:
        require(sample["completed_files"] == len(files), "incomplete sample")
        require(sorted(f["index"] for f in sample["files"]) == list(range(len(files))),
                "missing or duplicate file results")
        for field in ["stored_bytes", "decoded_bytes"]:
            require(sample[field] == sum(f[field] for f in files), f"wrong {field}")
        require(math.isfinite(sample["wall_seconds"]) and sample["wall_seconds"] > 0,
                "invalid elapsed time")
        require(0 <= sample["peak_reserved_bytes"] <= 512 * MIB, "reservation overflow")
        require(sample["peak_rss_bytes"] is None or sample["peak_rss_bytes"] > 0,
                "invalid RSS")


def require_comparable(left, right):
    require(left["provenance"]["source_sha256"] == right["provenance"]["source_sha256"],
            "source fingerprints differ")
    require(left["manifest"] == right["manifest"], "fixture recipes/bytes differ")


def scaling_cases():
    cases = []
    for order, workers in [("forward", "1,2,4,8"), ("reverse", "8,4,2,1")]:
        for encoding in ["fits-gzip", "fits-gzip2"]:
            for size, width, height in [(8, 2048, 2048), (64, 4096, 8192)]:
                for tile, rows in [("row", 1), ("32rows", 32), ("image", height)]:
                    for pattern in ["noise", "gradient"]:
                        cases.append(dict(name=f"{encoding}-{tile}-{pattern}-{size}mib-{order}",
                                          encoding=encoding, width=width, height=height,
                                          tile_rows=rows, pattern=pattern, frames=8,
                                          workers=workers, repeats=5, disk_mib=1024))
    return cases


def command(binary, output, scratch, note, case):
    args = [str(binary), "run", "--output", str(output), "--scratch", str(scratch),
            "--note", note, "--workload", case.get("workload", "full")]
    for key in ["encoding", "width", "height", "tile_rows", "pattern", "frames",
                "workers", "repeats", "disk_mib"]:
        if key in case:
            args.extend(["--" + key.replace("_", "-"), str(case[key])])
    return args


def write_json(path, data):
    with path.open("x") as stream:
        json.dump(data, stream, indent=2, allow_nan=False)
        stream.write("\n")


def stop_owned(process):
    # Every process passed here was launched by this script in a fresh session.
    # Stop descendants too on POSIX, including a sample child after parent failure.
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.wait(timeout=5)


def launch(args, log):
    return subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            start_new_session=True)


def probe_tick(start, deadline, phase):
    time.sleep(max(0, deadline - time.perf_counter()))
    woke = time.perf_counter()
    hashlib.sha256(PROBE_BYTES).digest()
    finished = time.perf_counter()
    return dict(at_seconds=woke - start, phase=phase,
                wake_delay_seconds=max(0, woke - deadline),
                task_seconds=finished - woke), finished + 0.02


PROBE_BYTES = bytes(64 * 1024)


def probe_summary(ticks):
    return {phase: {
        "observations": len(group),
        "p95_wake_seconds": percentile([t["wake_delay_seconds"] for t in group], 95),
        "max_wake_seconds": max(t["wake_delay_seconds"] for t in group),
        "p95_task_seconds": percentile([t["task_seconds"] for t in group], 95),
    } for phase in sorted({t["phase"] for t in ticks})
        if (group := [t for t in ticks if t["phase"] == phase])}


def execute(args, log_path, probe=False):
    ticks = []
    start = time.perf_counter()
    deadline = start + 0.02
    with log_path.open("x") as log:
        process = launch(args, log)
        try:
            while process.poll() is None:
                require(time.perf_counter() - start < 600, "experiment exceeded ten minutes")
                if probe:
                    # CLI announces samples before spawning; this phase includes
                    # each child's fingerprint verification and inter-sample gaps.
                    phase = "sample_phase" if "worker(s)" in log_path.read_text() else "preparation"
                    tick, deadline = probe_tick(start, deadline, phase)
                    ticks.append(tick)
                else:
                    time.sleep(0.05)
            require(process.returncode == 0, f"benchmark failed; see {log_path}")
        finally:
            stop_owned(process)
    return dict(ticks=ticks, summaries=probe_summary(ticks), elapsed_seconds=time.perf_counter()-start)


def read_report(path, case):
    require(path.stat().st_size <= 4 * MIB, "oversized report")
    data = json.loads(path.read_text())
    audit_report(data, [int(w) for w in case["workers"].split(",")], case["repeats"])
    return data


def scaling(options):
    prior = {}
    for case in scaling_cases():
        print(case["name"], flush=True)
        output = options.output / (case["name"] + ".json")
        execute(command(options.binary, output, options.scratch, options.note, case),
                output.with_suffix(".log"))
        report = read_report(output, case)
        key = case["name"].rsplit("-", 1)[0]
        if key in prior:
            require_comparable(prior[key], report)
        else:
            prior[key] = report
    require(len({d["provenance"]["source_sha256"] for d in prior.values()}) == 1,
            "measured sources changed during matrix")


def idle_probe(seconds=5):
    ticks = []
    start = time.perf_counter()
    deadline = start + 0.02
    while time.perf_counter() - start < seconds:
        tick, deadline = probe_tick(start, deadline, "idle")
        ticks.append(tick)
    return dict(ticks=ticks, summaries=probe_summary(ticks))


def contention(options):
    write_json(options.output / "idle-before.json", idle_probe())
    # Two separately scheduled processes, each retaining exactly 32 MiB payload.
    # Hashing continuously creates bounded CPU/memory traffic, not OS RAM pressure.
    for load in ["alone", "two-contenders"]:
        with (options.output / f"{load}.log").open("x") as log:
            contenders = []
            try:
                if load == "two-contenders":
                    for _ in range(2):
                        contenders.append(launch([sys.executable, __file__, "contender"], log))
                    time.sleep(0.25)
                for workers in [1, 2, 4, 8]:
                    case = dict(encoding="fits-gzip2", width=2048, height=2048,
                                tile_rows=1, pattern="gradient", frames=128,
                                workers=str(workers), repeats=5, disk_mib=2048)
                    name = f"{load}-{workers}workers"
                    output = options.output / f"{name}.json"
                    print(name, flush=True)
                    result = execute(command(options.binary, output, options.scratch,
                                             options.note + f"; contention={load}", case),
                                     output.with_suffix(".log"), probe=True)
                    read_report(output, case)
                    require(all(c.poll() is None for c in contenders), "contender exited early")
                    write_json(options.output / f"{name}-probe.json", result)
            finally:
                for process in contenders:
                    stop_owned(process)
    write_json(options.output / "idle-after.json", idle_probe())


def children(pid):
    result = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True, check=False)
    require(result.returncode in [0, 1], "could not inspect owned process children")
    return [int(s) for s in result.stdout.split()]


def cancellation(options):
    require(os.name == "posix", "SIGINT/child inspection experiment requires POSIX")
    results = []
    for phase in ["preparation", "sample_child"]:
        for repeat in range(5):
            with tempfile.TemporaryDirectory(prefix="astro-suite-", dir=options.scratch) as scratch:
                case = dict(encoding="fits-gzip2", width=2048, height=2048,
                            tile_rows=1, pattern="gradient", frames=128,
                            workers="4", repeats=5, disk_mib=2048)
                name = f"cancel-{phase}-{repeat}"
                output = options.output / f"{name}.json"
                log_path = options.output / f"{name}.log"
                with log_path.open("x") as log:
                    process = launch(command(options.binary, output, scratch, options.note, case), log)
                    try:
                        start = time.perf_counter()
                        child_ids = []
                        while True:
                            require(process.poll() is None, "process exited before cancellation")
                            require(time.perf_counter() - start < 120, "cancellation phase not reached")
                            child_ids = children(process.pid)
                            ready = bool(child_ids) if phase == "sample_child" else bool(list(Path(scratch).iterdir()))
                            if ready:
                                break
                            time.sleep(0.01)
                        sent = time.perf_counter()
                        process.send_signal(signal.SIGINT)
                        code = process.wait(timeout=10)
                        elapsed = time.perf_counter() - sent
                        require(code != 0 and not output.exists(), "cancel produced a success report")
                        require(not list(Path(scratch).iterdir()), "scratch remained after cancellation")
                        for pid in child_ids:
                            try:
                                os.kill(pid, 0)
                            except ProcessLookupError:
                                continue
                            raise ValueError("sample child survived cancellation")
                        results.append(dict(phase=phase, repeat=repeat, exit_seconds=elapsed,
                                            exit_code=code, scratch_clean=True, children_reaped=True,
                                            within_two_seconds=elapsed < 2))
                    finally:
                        stop_owned(process)
    write_json(options.output / "cancellation.json", results)


def profile(options):
    require(sys.platform == "darwin", "sample profiling requires macOS")
    for workers in [1, 4]:
        case = dict(encoding="fits-gzip2", width=2048, height=2048, tile_rows=1,
                    pattern="gradient", frames=256, workers=str(workers), repeats=1, disk_mib=4096)
        output = options.output / f"profile-{workers}workers.json"
        with output.with_suffix(".log").open("x") as log:
            process = launch(command(options.binary, output, options.scratch, options.note, case), log)
            try:
                start = time.perf_counter()
                while not (ids := children(process.pid)):
                    require(process.poll() is None and time.perf_counter() - start < 120,
                            "profile child not reached")
                    time.sleep(0.02)
                capture = subprocess.run(["/usr/bin/sample", str(ids[0]), "3", "1", "-file",
                                          str(options.output / f"profile-{workers}workers.txt")],
                                         capture_output=True, text=True, timeout=20, check=False)
                require(capture.returncode == 0, "sample failed: " + capture.stderr)
                require(process.wait(timeout=120) == 0, "profile workload failed")
                read_report(output, case)
            finally:
                stop_owned(process)


def main():
    if sys.argv[1:] == ["contender"]:
        payload = b"\xa5" * (32 * MIB)
        end = time.monotonic() + 600
        while time.monotonic() < end:
            hashlib.sha256(payload).digest()
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["scaling", "contention", "cancellation", "profile"])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, default=Path(tempfile.gettempdir()))
    parser.add_argument("--binary", type=Path, default=Path("target/release/astro-bench"))
    parser.add_argument("--note", required=True)
    options = parser.parse_args()
    options.binary = options.binary.resolve(strict=True)
    require(options.scratch.is_dir(), "scratch must be an existing directory")
    options.output.mkdir(parents=True, exist_ok=False)
    with options.binary.open("rb") as binary:
        binary_hash = hashlib.file_digest(binary, "sha256").hexdigest()
    write_json(options.output / "experiment.json", dict(
        schema_version=1, mode=options.mode, note=options.note, platform=platform.platform(),
        python=sys.version, script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        binary_sha256=binary_hash,
        started_unix_seconds=time.time(), cache_state="uncontrolled / likely warm"))
    globals()[options.mode](options)


if __name__ == "__main__":
    main()
