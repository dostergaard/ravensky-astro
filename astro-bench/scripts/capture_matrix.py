"""Measure a local, explicitly selected capture directory without copying its files.

Only reports are written. Filenames/paths are not included in report JSON. A
directory containing both FITS and XISF is required; no recursion or symlink input.
Use a controlled directory and do not modify files during measurement.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from benchmark_suite import require, stop_owned, write_json


def select_inputs(root, recursive, per_format):
    require(1 <= per_format <= 8, "select 1..8 files per format")
    selected = {"fits": [], "xisf": []}
    for directory, dirs, names in os.walk(root):
        dirs[:] = sorted(d for d in dirs if recursive and not d.startswith("."))
        for name in sorted(names):
            path = Path(directory) / name
            suffix = path.suffix.lower()
            if suffix not in [".fits", ".fit", ".fts", ".xisf"]:
                continue
            key = "xisf" if suffix == ".xisf" else "fits"
            if len(selected[key]) < per_format:
                require(path.is_file() and not path.is_symlink(), "non-regular capture input")
                selected[key].append(path)
        if all(len(group) == per_format for group in selected.values()):
            break
    require(all(selected.values()), "both FITS and XISF inputs are required")
    paths = sorted(selected["fits"] + selected["xisf"])
    require(sum(p.stat().st_size for p in paths) <= 8 * 1024**3, "selection exceeds 8 GiB")
    return paths


def fingerprint_reads(paths):
    records = []
    for repeat in range(2):
        for index, path in enumerate(paths):
            before = path.stat()
            start = time.perf_counter()
            cpu = time.process_time()
            with path.open("rb") as stream:
                hash_state = hashlib.sha256()
                size = 0
                while chunk := stream.read(256 * 1024):
                    size += len(chunk)
                    require(size <= before.st_size, "capture grew during read")
                    hash_state.update(chunk)
                require(size == before.st_size, "capture shrank during read")
                digest = hash_state.hexdigest()
            after = path.stat()
            require((before.st_size, before.st_mtime_ns, before.st_ino) ==
                    (after.st_size, after.st_mtime_ns, after.st_ino), "capture changed during read")
            records.append(dict(repeat=repeat, index=index, stored_bytes=before.st_size,
                                sha256=digest, wall_seconds=time.perf_counter() - start,
                                cpu_seconds=time.process_time() - cpu))
    for index in range(len(paths)):
        require(records[index]["sha256"] == records[index + len(paths)]["sha256"],
                "capture bytes changed between reads")
    return dict(kind="sequential_read_plus_sha256", samples=records,
                cache_state="first observed read and immediate repeat; OS cache uncontrolled, no eviction")


def run(binary, paths, level, workers, passes, output):
    with output.open("x") as stream, output.with_suffix(".log").open("x") as log:
        child = subprocess.Popen([str(binary), level, str(workers), str(passes),
                                  *map(str, paths)], stdout=stream, stderr=log,
                                 stdin=subprocess.DEVNULL, start_new_session=True)
        try:
            code = child.wait(timeout=120)
            require(code == 0, f"capture sample failed; retain diagnostic in {output}")
        finally:
            stop_owned(child)
    require(output.stat().st_size <= 4 * 1024**2, "oversized capture report")
    data = json.loads(output.read_text())
    require(data["complete"] and data["sources_unchanged"], "incomplete or changed capture sample")
    require(data["reserved_bytes_after"] == 0, "capture reservations leaked")
    require(0 <= data["peak_reserved_bytes"] <= data["shared_memory_bytes"],
            "capture exceeded shared reservations")
    require(len(data["operations"]) == len(paths) * passes, "capture count mismatch")
    require(sorted((o["pass"], o["index"]) for o in data["operations"]) ==
            [(p, i) for p in range(passes) for i in range(len(paths))],
            "missing/duplicate capture operations")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/capture_probe"))
    parser.add_argument("--note", required=True)
    parser.add_argument("--recursive", action="store_true")
    parser.add_argument("--per-format", type=int, default=3)
    options = parser.parse_args()
    binary = options.binary.resolve(strict=True)
    paths = select_inputs(options.inputs, options.recursive, options.per_format)
    groups = {"fits": [p for p in paths if p.suffix.lower() != ".xisf"],
              "xisf": [p for p in paths if p.suffix.lower() == ".xisf"], "mixed": paths}
    require(all(groups.values()), "both FITS and XISF inputs are required")
    require(len(paths) * 16 <= 4096, "too many operations")
    options.output.mkdir(parents=True, exist_ok=False)
    write_json(options.output / "experiment.json", dict(
        schema_version=1, note=options.note, started_unix_seconds=time.time(),
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        cache_state="uncontrolled / likely warm; hashes before and after each sample",
        groups={name: len(group) for name, group in groups.items()}))
    read_evidence = fingerprint_reads(paths)
    write_json(options.output / "fingerprint-reads.json", read_evidence)
    hashes = {path: read_evidence["samples"][i]["sha256"] for i, path in enumerate(paths)}
    fingerprints = {}
    sources = set()
    for level in ["structural", "full"]:
        for name, group in groups.items():
            for workers in [1, 2, 4]:
                for repeat in range(5):
                    output = options.output / f"{name}-{level}-{workers}workers-{repeat}.json"
                    print(output.name, flush=True)
                    data = run(binary, group, level, workers, 16, output)
                    require([f["sha256"] for f in data["inputs"]] == [hashes[p] for p in group],
                            "capture changed since initial fingerprint reads")
                    require(fingerprints.setdefault(name, data["inputs"]) == data["inputs"],
                            "capture fingerprints changed across samples")
                    sources.add((data["source_sha256"], data["probe_source_sha256"]))
    require(len(sources) == 1, "measured sources changed during capture matrix")


if __name__ == "__main__":
    main()
