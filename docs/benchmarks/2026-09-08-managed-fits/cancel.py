"""Measure cooperative SIGINT shutdown of whole-image HCOMPRESS validation.

Run from the repository root after building capture_probe. Uses the generator in
run.py and independently installed fpack. Creates and removes only owned scratch.
Shutdown includes process teardown; it is not an isolated codec timing guarantee.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

from run import image, write_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = Path("target/release/examples/capture_probe").resolve(strict=True)
    fpack = shutil.which("fpack")
    if not fpack:
        raise SystemExit("fpack is required")
    args.output.mkdir(parents=True, exist_ok=False)
    write_json(args.output / "conditions.json", dict(
        target_shutdown_seconds=0.25, signal_delay_seconds=1,
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()))
    results = []
    with tempfile.TemporaryDirectory(prefix="astro-hcompress-cancel-") as scratch:
        source, encoded = Path(scratch) / "input.fits", Path(scratch) / "packed.fits"
        image(source, "noise", 42)
        subprocess.run([fpack, "-h", "-w", "-O", str(encoded), str(source)],
                       check=True, capture_output=True, timeout=120)
        before = hashlib.sha256(encoded.read_bytes()).hexdigest()
        for workers in [1, 4]:
            for repeat in range(3):
                process = subprocess.Popen([str(binary), "full", str(workers), "256", str(encoded)],
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                try:
                    time.sleep(1)
                    if process.poll() is not None:
                        raise RuntimeError("probe exited before cancellation")
                    start = time.perf_counter()
                    process.send_signal(signal.SIGINT)
                    stdout, stderr = process.communicate(timeout=10)
                    elapsed = time.perf_counter() - start
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.communicate()
                result = dict(workers=workers, repeat=repeat, shutdown_seconds=elapsed,
                              returncode=process.returncode, stdout=stdout, stderr=stderr)
                write_json(args.output / f"{workers}-{repeat}.json", result)
                assert process.returncode != 0 and "cancelled" in stderr and not stdout
                assert elapsed < 0.25
                assert hashlib.sha256(encoded.read_bytes()).hexdigest() == before
                results.append(result)
    write_json(args.output / "summary.json", dict(samples=results, sources_unchanged=True,
                                                 source_sha256=before))
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
