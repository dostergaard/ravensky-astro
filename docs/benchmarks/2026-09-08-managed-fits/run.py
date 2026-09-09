"""Bounded managed-FITS measurements using independent CFITSIO fpack fixtures.

Run from the repository root after building the release capture_probe example:
python3 docs/benchmarks/2026-09-08-managed-fits/run.py --output NEW_DIRECTORY
Requires Python 3.12+, fpack on PATH, and <=1 GiB free temporary scratch.
Only exclusively owned scratch is removed. No real captures are modified.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import struct
import subprocess
import tempfile
import time


def write_json(path, data):
    with path.open("x") as stream:
        json.dump(data, stream, indent=2)
        stream.write("\n")


def image(path, pattern, seed, floating=False):
    width, height = 2048, (1024 if floating else 2048)
    keys = [("SIMPLE", "T"), ("BITPIX", "-32" if floating else "16"),
            ("NAXIS", "2"), ("NAXIS1", str(width)), ("NAXIS2", str(height))]
    header = "".join(f"{key:<8}= {value:>20}".ljust(80) for key, value in keys)
    header += "END".ljust(80)
    header = header.ljust((len(header) + 2879) // 2880 * 2880)
    state = seed
    with path.open("xb") as stream:
        stream.write(header.encode("ascii"))
        chunk = bytearray()
        for index in range(width * height):
            state ^= (state << 13) & 0xffffffff
            state ^= state >> 17
            state ^= (state << 5) & 0xffffffff
            if pattern == "noise":
                value = (state & 65535) - 32768
            elif pattern == "gradient":
                value = index % width
            else:
                value = (index // 64 + seed) % 8  # realistic PLIO mask runs
            chunk.extend(struct.pack(">f", value * 0.13) if floating else struct.pack(">h", value))
            if len(chunk) >= 65536:
                stream.write(chunk)
                chunk.clear()
        stream.write(chunk)
        stream.write(bytes((-stream.tell()) % 2880))
        stream.flush()
        os.fsync(stream.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--scratch", type=Path, default=Path(tempfile.gettempdir()))
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/capture_probe"))
    parser.add_argument("--note", default="")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    fpack = shutil.which("fpack")
    if not fpack:
        raise SystemExit("fpack is required for independent fixture generation")
    if shutil.disk_usage(args.scratch).free < 1024**3:
        raise SystemExit("at least 1 GiB free scratch space is required")
    args.output.mkdir(parents=True, exist_ok=False)
    version = subprocess.run([fpack, "-V"], check=True, capture_output=True, text=True)
    write_json(args.output / "environment.json", dict(
        platform=platform.platform(), python=platform.python_version(), note=args.note,
        fpack_version=version.stdout + version.stderr,
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        cache_state="uncontrolled / likely warm; hashes precede each sample",
        target_peak_rss_bytes=192 * 1024**2, target_reserved_bytes=512 * 1024**2,
        started_unix_seconds=time.time()))
    cases = [
        ("rice-noise", "noise", False, ["-r", "-t", "2048,32"]),
        ("rice-gradient", "gradient", False, ["-r", "-t", "2048,32"]),
        ("plio-mask", "mask", False, ["-p", "-t", "2048,32"]),
        ("hcompress-noise-32rows", "noise", False, ["-h", "-t", "2048,32"]),
        ("hcompress-noise-whole", "noise", False, ["-h", "-w"]),
        ("hcompress-float-quantized", "noise", True, ["-h", "-w", "-q1", "4"]),
    ]
    summaries = []
    hashes = set()
    with tempfile.TemporaryDirectory(prefix="astro-managed-fits-", dir=args.scratch) as scratch:
        scratch = Path(scratch)
        for name, pattern, floating, flags in cases:
            print("Preparing", name, flush=True)
            files = []
            for index in range(4):
                source = scratch / f"source-{index}.fits"
                encoded = scratch / f"encoded-{index}.fits"
                image(source, pattern, 42 + index, floating)
                command = [fpack, *flags, "-O", str(encoded), str(source)]
                result = subprocess.run(command, capture_output=True, text=True, timeout=120, check=True)
                (args.output / f"{name}-generation-{index}.log").write_text(result.stdout + result.stderr)
                files.append(encoded)
            expected_inputs = None
            for workers in [1, 2, 4]:
                reports = []
                for repeat in range(3):
                    print(name, workers, repeat, flush=True)
                    result = subprocess.run([str(binary), "full", str(workers), "2", *map(str, files)],
                                            capture_output=True, text=True, timeout=120)
                    (args.output / f"{name}-{workers}-{repeat}.stderr").write_text(result.stderr)
                    report = json.loads(result.stdout)
                    write_json(args.output / f"{name}-{workers}-{repeat}.json", report)
                    if result.returncode or not report["complete"]:
                        raise RuntimeError("measurement failed; retain diagnostic evidence")
                    assert report["sources_unchanged"] and report["reserved_bytes_after"] == 0
                    assert len(report["operations"]) == 8
                    assert all(op["success"] for op in report["operations"])
                    assert report["peak_reserved_bytes"] <= 512 * 1024**2
                    if report["peak_rss_bytes"] is not None:
                        assert report["peak_rss_bytes"] < 192 * 1024**2
                    if expected_inputs is None:
                        expected_inputs = report["inputs"]
                    assert expected_inputs == report["inputs"]
                    hashes.add(report["source_sha256"])
                    reports.append(report)
                rss = [r["peak_rss_bytes"] for r in reports if r["peak_rss_bytes"] is not None]
                summaries.append(dict(case=name, workers=workers,
                    median_wall_seconds=statistics.median(r["wall_seconds"] for r in reports),
                    min_wall_seconds=min(r["wall_seconds"] for r in reports),
                    max_wall_seconds=max(r["wall_seconds"] for r in reports),
                    peak_rss_bytes=max(rss) if rss else None,
                    peak_reserved_bytes=max(r["peak_reserved_bytes"] for r in reports)))
            for path in scratch.iterdir():
                path.unlink()  # only this exclusively created directory's files
    assert len(hashes) == 1
    write_json(args.output / "summary.json", dict(samples=54, operations=432,
        measured_source_sha256=hashes.pop(), summaries=summaries))


if __name__ == "__main__":
    main()
