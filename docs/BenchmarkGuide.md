# Benchmarking RavenSky Astro on your system

This guide is for developers using the RavenSky Astro crates independently of
any RavenSky application. You can run `astro-bench` directly or call its Rust
library from your own tool. No AstroMuninn installation or configuration is needed.

The implemented workloads measure raw file reads and FITS/XISF structural or full
validation. Metadata extraction, scientific metrics, adaptive scheduling and
automatic recommendations for application settings are future extensions.

## 1. Get and build the tooling

Use a checkout of `ravensky-astro` that includes `astro-bench`. At this development
checkpoint the work is on `feature/file-validation`, pending review and merge;
obtain that checkout from the maintainer if it is not available on the remote.
After merge, a revision containing these changes is sufficient.

Keep the complete repository: `astro-bench` is currently unpublished
(`publish = false`), inherits workspace dependencies, and fingerprints adjacent
validator sources at build time. Installing the published `ravensky-astro` crate
alone does not install this tool. A benchmark measures the validator in this
checkout, which may differ from the version your application currently uses.

Prerequisites:

- Rust and Cargo through rustup. The checkout selects stable Rust; its current
  manifest requires Rust 1.94 or newer. Use the checked-in `Cargo.lock`.
- A native C build toolchain for the bundled CFITSIO and Zstandard dependencies.
  Have a C compiler and build tools (`make`/CMake as required by the selected
  dependency build path) available. On macOS, install the Xcode Command Line Tools.
- An existing writable scratch directory and free space for generated fixtures.
- Python 3.12 or newer only for report-reading examples and investigation scripts.
  Those scripts use the standard library; PyYAML and a Python package install are
  not required. A virtual environment is optional.

Run from the **repository root**, not the `astro-bench` subdirectory:

```sh
cargo build --locked --release -p astro-bench --bin astro-bench
target/release/astro-bench --help
mkdir -p target/bench-results target/bench-scratch
```

The examples below use a POSIX shell, such as bash or zsh, and the default Cargo
target directory. If you set `CARGO_TARGET_DIR`, adjust binary and result paths;
the Python scripts accept `--binary` for this purpose. On Windows the executable
has an `.exe` suffix and shell syntax needs adapting. These workflows have been
verified on macOS; Windows/Linux qualification remains outstanding. Unix CPU
telemetry is optional, peak RSS is available on macOS/Linux, and unavailable
measurements are JSON `null`, not zero.

Build before measuring. Do not run compilation, another benchmark suite, or a
dependency update during a comparison. Record the revision with
`git rev-parse HEAD` and describe CPU, RAM, OS, storage, power mode and competing
work in `--note`.

## 2. Run a small first benchmark

This generates four 512 × 512 UInt16 FITS images (0.5 MiB of pixels each), then
performs three full-validation samples with one worker and three with two:

```sh
target/release/astro-bench run \
  --encoding fits --pattern noise --workload full \
  --width 512 --height 512 --frames 4 --seed 42 \
  --workers 1,2 --repeats 3 --disk-mib 64 \
  --scratch target/bench-scratch \
  --output target/bench-results/first-fits.json \
  --note 'First run; replace with CPU, RAM, OS, storage and competing load'
```

Success produces a JSON report with **six samples and two worker summaries**.
Fixture generation, syncing, validation and hashing happen before measurement.
Each sample starts a fresh child process. The report survives; the owned fixture
directory is removed after the run. This small example checks your setup and
emphasizes overhead; use larger workloads for performance decisions.

Choose a new output filename for every run. Existing reports are never replaced,
and the output parent directory must exist. Scratch and results are separate:
`--scratch` selects the volume containing the generated images; `--output` selects
where the report is saved. Omit `--scratch` to use the OS temporary directory.

## 3. Compare representative workloads and worker counts

Keep the recipe, workload, build and environment consistent when comparing worker
counts. Include one worker as a baseline. Eight frames allow up to eight workers
to have useful work; more workers than frames cannot create additional file work.

This example uses eight 8 MiB images with Zstandard-compressed XISF payloads:

```sh
target/release/astro-bench run \
  --encoding zstd --pattern noise --workload full \
  --width 2048 --height 2048 --frames 8 --seed 42 \
  --workers 1,2,4,8 --repeats 5 --disk-mib 512 \
  --scratch target/bench-scratch \
  --output target/bench-results/zstd-noise-full.json \
  --note 'Replace with hardware, scratch volume, power mode and competing load'
```

Repeat into new reports with `--pattern gradient` to exercise highly compressible
data, and with `--workload read` or `--workload structural` to measure those
operations separately. Reverse the worker order in a second run (`8,4,2,1`) to
help expose order, cache and changing-load effects. Do not combine different
operations into one performance score.

| Option | Meaning |
| --- | --- |
| `--encoding fits` | Ordinary FITS |
| `--encoding xisf` | Uncompressed XISF |
| `--encoding zlib` / `zstd` | XISF compressed with the selected codec |
| `--encoding fits-gzip` / `fits-gzip2` | Integer tiled FITS GZIP_1 / byte-shuffled GZIP_2 |
| `--workload read` | Read stored bytes; no format validation |
| `--workload structural` | Check declared container layout and extents |
| `--workload full` | Read all bytes, decode supported payloads and verify present checksums |

For tiled FITS, compare tile heights because decoder setup and I/O costs vary:

```sh
target/release/astro-bench run \
  --encoding fits-gzip2 --tile-rows 32 --pattern gradient --workload full \
  --width 2048 --height 2048 --frames 8 \
  --workers 1,2,4,8 --repeats 5 --disk-mib 512 \
  --scratch target/bench-scratch \
  --output target/bench-results/gzip2-32rows-full.json \
  --note 'Replace with hardware, storage and competing load'
```

Use `--tile-rows 1` for row tiles, or omit the option for one whole-image tile.
It is valid only with the two FITS GZIP encodings. Synthetic FITS has no HDU
CHECKSUM/DATASUM; synthetic XISF has an attachment SHA-256 checksum and GZIP tiles
have CRC32 checks. Comparing their timings is not an isolated codec comparison.

To try another drive, create a scratch parent on that drive and substitute its
quoted path for `--scratch`. Keep other options unchanged and identify the volume
in the note. Files are generated and hashed there before timing, so these results
are **not guaranteed cold-disk throughput**, even with fresh child processes.

## 4. Read and interpret the report

Print the first report's worker summaries using Python:

```sh
python3 - <<'PY'
import json
from pathlib import Path

report = json.loads(Path('target/bench-results/first-fits.json').read_text())
assert report['schema_version'] == 1
print('Build:', report['provenance'])
for summary in report['summaries']:
    rss = summary['maximum_peak_rss_bytes']
    rss_text = 'unavailable' if rss is None else f'{rss / 1048576:.2f} MiB'
    print(f"{summary['workers']} workers: "
          f"median {summary['median_wall_seconds'] * 1000:.3f} ms, "
          f"{summary['median_stored_mib_per_second']:.1f} stored MiB/s, "
          f"peak RSS {rss_text}")
PY
```

| Report field | Interpretation |
| --- | --- |
| `provenance` | Build/compiler/revision, measured-source hash, platform, notes and cache/telemetry qualifications |
| `manifest` | Recipe, generator version, stored/decoded sizes and file fingerprints |
| `preparation_seconds` | Fixture preparation, excluded from sample wall time |
| `samples` | Every successful repetition, including worker count and per-file results |
| `summaries` | Min/median/max wall time, median throughput, pooled per-file p50/p95 and maximum RSS for each worker count |
| `samples[].read_bytes` | Managed bytes read, including rereads; not physical device I/O counters |
| `samples[].cpu_seconds` | Process user + system CPU-time delta; can exceed wall time with parallel work |
| `samples[].peak_reserved_bytes` | Peak shared validator reservations, including explicit allowances; not measured RSS |
| `samples[].peak_rss_bytes` | Process-lifetime resident-memory high-water mark, including child setup/native overhead |

Throughput uses **stored input bytes divided by elapsed time**. Structural mode
does not read most payload bytes; its reported rate is not disk bandwidth. Full
mode may read bytes more than once. Generation and fingerprint checks touch the
files, so OS caches are uncontrolled and likely warm. RSS does not measure the
filesystem cache or the effect on another application's responsiveness.

Inspect variation and individual samples before drawing conclusions. Compare
identical recipe/content fingerprints for a code change; retain source hashes and
raw reports. Different compressed sizes or checksum work change the workload.
Prefer a worker count that produces repeatable useful gains within your memory
and responsiveness requirements; a tiny improvement in one warm run does not
justify a universal default. Repeat under representative everyday background load.
The tooling does not currently choose settings or write your application's config.

## 5. Measure your own FITS/XISF files

Build the read-only diagnostic example and run it on explicit files:

```sh
cargo build --locked --release -p astro-bench --example capture_probe
target/release/examples/capture_probe full 2 3 \
  '/path/to/captures/frame1.fits' '/path/to/captures/frame2.xisf'
```

Arguments are `structural|full`, worker count, pass count, then file paths. This
example attempts six validations: two inputs × three passes. JSON is written to
stdout; save it to a new file using your shell's no-overwrite facility if needed.
Inputs are read-only and hashed before and after timing to check byte preservation.
Keep them unchanged for the entire run. Hashing warms caches. Passes may overlap,
including concurrent reads of the same input; this is not an arrival-stream test.

Capture reports use a separate diagnostic schema (`kind: "capture_probe"`), not
the synthetic report's `manifest`/`summaries` structure. A validation rejection
returns exit status 2 with diagnostic JSON and no successful throughput score.
`Unsupported` means a feature cannot be validated by this path; it does not mean
the file is corrupt. Shared-budget full validation still excludes native
compressed-FITS layouts beyond the supported integer GZIP subset. See the
[validator's coverage and limits](../astro-io/README.md#file-validation-unreleased).

For repeated FITS-only, XISF-only and mixed comparisons:

```sh
python3 astro-bench/scripts/capture_matrix.py '/path/to/captures' \
  --recursive --per-format 3 \
  --output target/bench-results/captures \
  --note 'Replace with hardware, source drive and competing load'
```

The output directory must be new, and the input selection must contain both FITS
and XISF files. Use `capture_probe` directly for a single-format collection.
The script selects up to three files of each
format in sorted traversal order, records first-observed/repeated reads plus
SHA-256, and runs structural/full validation with 1/2/4 workers, 16 passes and five
fresh-process repetitions, producing 90 samples.
First-observed does not mean cold. To retain deliberately invalid/unsupported
inputs as diagnostics, add `--allow-rejected`; inspect `outcomes.json` and exclude
incomplete samples from throughput comparisons. Source changes and process
failures still abort the matrix. See the [capture reference](../astro-bench/README.md#measure-existing-captures)
for input, pass, operation and memory bounds.

The small committed `astro-bench/tests/test_data` corpus intentionally includes
invalid and unsupported cases. It is useful for rejection diagnostics, not as a
representative performance workload or an all-valid capture directory.

## 6. Call the library from a separate Rust project

Keep your tool next to a complete checkout:

```text
development/
  ravensky-astro/
  my-benchmark/
    Cargo.toml
    src/main.rs
```

Use this `my-benchmark/Cargo.toml`:

```toml
[package]
name = "my-benchmark"
version = "0.1.0"
edition = "2021"

[dependencies]
astro-bench = { path = "../ravensky-astro/astro-bench" }
anyhow = "1.0"
serde_json = "1.0"
```

Use this complete `src/main.rs`:

```rust
use astro_bench::{run_sample, Encoding, FixtureSet, Recipe, Workload};
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn main() -> anyhow::Result<()> {
    let scratch = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let cancel = AtomicBool::new(false);
    let recipe = Recipe {
        width: 512,
        height: 512,
        frames: 4,
        encoding: Encoding::Zstd,
        ..Recipe::default()
    };
    Workload::Full.check_budget(&recipe, 2)?;
    let fixtures = FixtureSet::generate(&scratch, recipe, &cancel)?;
    let measurement = run_sample(&fixtures, Workload::Full, 2, &cancel, |_| {});
    // Attempt explicit cleanup even when measurement fails.
    let cleanup = fixtures.cleanup();
    if let Err(error) = &cleanup {
        eprintln!("Fixture cleanup failed: {error}");
    }
    let sample = measurement?;
    cleanup?;
    println!("{}", serde_json::to_string_pretty(&sample)?);
    Ok(())
}
```

From `my-benchmark`, run `cargo run --release`, optionally followed by
`-- '/existing/scratch/directory'`. The output is one serialized `Sample`, not the
CLI's full provenance/manifest/repetition report. Keep the recipe and manifest
alongside samples if you build your own reporting tool.

The separate project resolves dependencies using its own `Cargo.lock`; it does
not inherit the checkout's lockfile. Preserve that lockfile and record the tool's
build configuration for reproducibility. The benchmark build fingerprint covers
the source checkout's lockfile, so it alone does not identify your external tool's
resolved dependencies. Use the locked workspace CLI for comparisons against the
repository's recorded measurements.

`FixtureSet::generate` owns its new scratch subdirectory; `cleanup` reports removal
errors, while Drop cleanup is best effort. `FixtureSet::open` verifies an existing
manifest and files but does not own their cleanup. `run_sample` measures prepared
synthetic fixtures; use the capture diagnostic for arbitrary files.

The caller controls cancellation through the supplied `AtomicBool` (set it to
`true` from another thread or your existing cancellation handler). The library
installs no signal handler, joins its workers before returning and reports errors
instead of partial success. Progress callbacks run on the calling thread and are
included in wall time; keep them cheap. Each call has its own bounded worker group
and shared validation allowance: launching several samples simultaneously does
not create one process-wide budget. Coordinate them in your tool.

Library RSS includes fixture generation and earlier work in your process. Use
the CLI's fresh child processes when you need isolated sample high-water marks.
Generate the complete API reference from the repository root with:

```sh
cargo doc --locked -p astro-bench -p astro-io --no-deps --open
```

## 7. Run the deeper investigation suites

After the release CLI build, run these from the repository root, choosing a new
output directory for each invocation:

```sh
python3 astro-bench/scripts/benchmark_suite.py scaling \
  --output target/bench-results/scaling --scratch target/bench-scratch \
  --note 'Replace with hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py contention \
  --output target/bench-results/contention --scratch target/bench-scratch \
  --note 'Replace with hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py cancellation \
  --output target/bench-results/cancellation --scratch target/bench-scratch \
  --note 'Replace with hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py profile \
  --output target/bench-results/profile --scratch target/bench-scratch \
  --note 'Replace with hardware, storage, power and competing load'
```

| Suite | Scope and requirements |
| --- | --- |
| `scaling` | 960 samples across GZIP variants, sizes, tile heights and forward/reverse worker orders; substantially more work than the quick start |
| `contention` | Bounded CPU competitors and a scheduling/short-task probe; not OS low-memory pressure or a GUI switching test |
| `cancellation` | POSIX and `pgrep`; checks CLI SIGINT exit, child reaping and scratch cleanup |
| `profile` | POSIX, `pgrep` and macOS `/usr/bin/sample`; instrumented runs are separate from throughput comparisons |

The suites enforce command deadlines and scratch quotas; profiling preflight
requires up to 4 GiB. Run one suite at a time. Retain logs and partial results on
failure and rerun into a new output directory. See the
[suite reference](../astro-bench/README.md#run-a-benchmark) and
[recorded investigation](benchmarks/2026-09-07-m4-max-completion/README.md)
for exact conditions and interpretation.

## Limits, stopping and troubleshooting

| Symptom or question | Action / explanation |
| --- | --- |
| Existing report or output directory | Choose a new name; the tooling preserves previous evidence |
| Scratch quota rejected | Reduce dimensions/frame count, or increase `--disk-mib` within free space and the 8192 MiB maximum; preflight includes expansion and metadata |
| Image too large | Synthetic fixtures are capped at 64 MiB decoded/image (`width × height × 2`); use the bounded capture diagnostic for larger existing images |
| Admission estimate rejected | Reduce workers or dimensions; the synthetic runner enforces a conservative 512 MiB estimate as well as shared validator reservations |
| More workers do not help | Check frame count, I/O contention, compression/tile layout, CPU load and variability; the CLI does not adapt concurrency |
| Timeout | Inspect logs and storage availability; synthetic `--timeout-seconds` defaults to 120 per child including fingerprint verification, and allows 1–3600 |
| Missing native build tools | Install the native compiler/build prerequisites shown by the dependency build error, then repeat the locked release build |
| Missing CPU/RSS values | Treat `null` as unavailable telemetry; do not substitute zero or claim a bound |

Synthetic bounds are 1–256 frames, 1–16 workers, 1–20 repetitions and at most
16,384 tiles per image. The disk quota is not a RAM allowance, and reservation
accounting is not an OS-enforced RSS ceiling. See the
[resource reference](../astro-bench/README.md#resource-bounds-and-measurement-limits).

Ctrl-C cancels preparation or stops and reaps the CLI's active measurement child
before owned fixture cleanup. Preparation and direct library/capture calls use
cooperative cancellation; a blocked OS read can delay their return. Abrupt process
termination can leave an `astro-bench-*` scratch subdirectory. Confirm that its
owning process has exited before removing that specific leftover directory.

When sharing results, include the exact command, raw JSON, revision/source hashes,
hardware/storage/power notes and any errors or missing telemetry. Review notes and
logs for private paths or information before sharing. Keep failed or unsupported
samples visibly separate, and state which platforms and workloads you actually
tested. Automatic calibration remains described in the
[design document](BenchmarkAndCalibrationDesign.md), not implemented by these commands.
