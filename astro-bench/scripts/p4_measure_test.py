#!/usr/bin/env python3
"""Measure one P4 test binary's peak RSS and owned temporary bytes on macOS.

Usage: python p4_measure_test.py PATH_TO_TEST_BINARY EXACT_TEST_NAME [--ignored]
The child receives an isolated TMPDIR; the script samples scratch files every
10 ms and reports high-water marks. Files created and removed between samples
can make the observed scratch peak lower than the true peak.
"""

import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def scratch_bytes(root: Path) -> int:
    total = 0
    for directory, _, files in os.walk(root):
        for name in files:
            path = Path(directory) / name
            if ".p1-scratch-" in directory or name.startswith(".astro-io-decode-"):
                try:
                    total += path.stat().st_size
                except FileNotFoundError:
                    pass
    return total


def total_bytes(root: Path) -> int:
    total = 0
    for directory, _, files in os.walk(root):
        for name in files:
            try:
                total += (Path(directory) / name).stat().st_size
            except FileNotFoundError:
                pass
    return total


def main() -> int:
    if len(sys.argv) not in (3, 4) or (len(sys.argv) == 4 and sys.argv[3] != "--ignored"):
        print(__doc__, file=sys.stderr)
        return 2
    binary = str(Path(sys.argv[1]).resolve())
    name = sys.argv[2]
    disk_budget = int(os.environ.get("ASTROMUNINN_P4_BENCH_DISK_MIB", "256")) * 1024 * 1024
    if disk_budget > 2 * 1024**3:
        raise ValueError("benchmark disk budget exceeds 2 GiB")
    available = shutil.disk_usage(tempfile.gettempdir()).free
    if available < disk_budget + 1024**3:
        raise RuntimeError("benchmark refused: scratch budget plus 1 GiB host headroom unavailable")
    timeout_seconds = int(os.environ.get("ASTROMUNINN_P4_BENCH_TIMEOUT_SECONDS", "120"))
    if not 1 <= timeout_seconds <= 3600:
        raise ValueError("benchmark timeout must be 1..=3600 seconds")
    with tempfile.TemporaryDirectory(prefix="astromuninn-p4-measure-") as temporary:
        environment = os.environ.copy()
        environment["TMPDIR"] = temporary
        start = time.monotonic()
        arguments = [binary, "--exact", name, "--nocapture"]
        if len(sys.argv) == 4:
            arguments.append("--ignored")
        child = subprocess.Popen(arguments, env=environment)
        peak_scratch = 0
        peak_total = 0
        try:
            while child.poll() is None:
                if time.monotonic() - start >= timeout_seconds:
                    raise TimeoutError(f"owned benchmark child exceeded {timeout_seconds} seconds")
                peak_scratch = max(peak_scratch, scratch_bytes(Path(temporary)))
                peak_total = max(peak_total, total_bytes(Path(temporary)))
                time.sleep(0.01)
        finally:
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        peak_scratch = max(peak_scratch, scratch_bytes(Path(temporary)))
        peak_total = max(peak_total, total_bytes(Path(temporary)))
        elapsed = time.monotonic() - start
        peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
        print(f"test={name} exit={child.returncode} elapsed_s={elapsed:.3f} "
              f"peak_rss_bytes={peak_rss} sampled_peak_scratch_bytes={peak_scratch} "
              f"sampled_peak_total_bytes={peak_total} sampling_interval_ms=10")
        return child.returncode


if __name__ == "__main__":
    raise SystemExit(main())
