#!/usr/bin/env python3
"""Measure one P4 test binary's peak RSS and owned temporary bytes on macOS.

Usage: python p4_measure_test.py PATH_TO_TEST_BINARY EXACT_TEST_NAME
The child receives an isolated TMPDIR; the script samples scratch files every
10 ms and reports high-water marks. Files created and removed between samples
can make the observed scratch peak lower than the true peak.
"""

import os
import resource
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
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    binary = str(Path(sys.argv[1]).resolve())
    name = sys.argv[2]
    with tempfile.TemporaryDirectory(prefix="astromuninn-p4-measure-") as temporary:
        environment = os.environ.copy()
        environment["TMPDIR"] = temporary
        start = time.monotonic()
        child = subprocess.Popen([binary, "--exact", name, "--nocapture"], env=environment)
        peak_scratch = 0
        peak_total = 0
        while child.poll() is None:
            peak_scratch = max(peak_scratch, scratch_bytes(Path(temporary)))
            peak_total = max(peak_total, total_bytes(Path(temporary)))
            time.sleep(0.01)
        peak_scratch = max(peak_scratch, scratch_bytes(Path(temporary)))
        peak_total = max(peak_total, total_bytes(Path(temporary)))
        elapsed = time.monotonic() - start
        peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
        print(f"test={name} exit={child.returncode} elapsed_s={elapsed:.3f} "
              f"peak_rss_bytes={peak_rss} sampled_peak_scratch_bytes={peak_scratch} "
              f"sampled_peak_total_bytes={peak_total}")
        return child.returncode


if __name__ == "__main__":
    raise SystemExit(main())
