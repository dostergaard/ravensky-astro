# Bounded FITS GZIP baseline — 2026-09-06

## Result

All 24 configurations / 216 isolated samples completed, totaling 1,440 file
operations. Every sample reported the expected files and stored bytes; validation
samples also reported the expected decoded bytes. Repeated recipes had identical
fixture fingerprints across read, structural and full reports. The runner checked
that all shared reservations were released before accepting each sample.

Observed process peak RSS across all samples was **6.23–7.83 MiB**. The declared
four-worker full-validation target of **less than 64 MiB** passed in every case.
Maximum concurrent validator reservations were **5.57 MiB**, within the shared
512 MiB allowance. These reservations are accounting, not an OS memory quota;
process RSS includes runtime overhead and excludes filesystem cache and parent
fixture generation.

Whole-image tiles achieved **2.73–3.45×** median speedup from one to four workers.
Row tiles achieved **1.03–2.21×**. Six of eight row-tile full workloads took longer
with four workers than two (roughly 13–36% longer); the two GZIP_2 gradient cases
continued to benefit. Row-tile structural checks also slowed down at four workers.
This is a reason to profile small-tile overhead and measure worker counts before
selecting defaults. The measurements do not isolate the cause of the saturation.

## Method and provenance

- Apple M4 Max, 36 GiB RAM, 14 logical/physical cores; macOS 26.6.2 (25G83), arm64.
- rustc 1.94.0 (4a4ef493e, 2026-03-02), release workspace build.
- Local APFS Data volume `/dev/disk3s5`, scratch `/private/tmp`, 626 GiB available
  at start. Desktop load uncontrolled; no concurrent agent builds/tests.
- Source revision `28979fc3f92bbfbf2ba0f459337bc75ac1823908` plus the benchmark
  extension in this change. Measured-source SHA-256:
  `1b0a1b651e6c3a84b4a204268dfe9aa3eebf5556e0f754d5f268c16c3e7e4c1d`.
  All 24 reports agree; independently recomputing the build-script hash from the
  validator/harness sources, manifests and lockfile matched after the run.
- Generator version 2: UInt16, deterministic seed 42, noise or horizontal gradient,
  GZIP_1/GZIP_2 with Q descriptors, full-width row or whole-image tiles. Fixtures
  have no FITS CHECKSUM/DATASUM; full validation checks GZIP CRC32 and ISIZE.
  CFITSIO pixel-parity tests independently verified both encodings and partial
  edge tiles. Generation and measured validation use bounded Rust streams.
- 8 MiB decoded/image uses 2048 × 2048 pixels and eight files (64 MiB total).
  32 MiB uses 4096 × 4096 and four files (128 MiB total). Each workload runs three
  repetitions at each of 1, 2 and 4 workers in fresh processes. Each child's
  timeout is 120 seconds; scratch quota is 512 MiB.
- Preparation and pre-sample hash verification touch every file. Cache state is
  uncontrolled / likely warm; new processes do not reset the OS cache. Timings
  exclude generation and fingerprint verification, while process-lifetime RSS
  includes child setup. Read/structural workloads cover only 8 MiB noise fixtures.

Reproduce after `cargo build --workspace --all-features --release`:

```sh
sh astro-bench/scripts/fits-gzip-baseline.sh REPORT_DIRECTORY SCRATCH_DIRECTORY \
  'CPU, RAM, OS, storage, compiler and competing-workload notes'
```

Use a new output directory. The [script](../../../astro-bench/scripts/fits-gzip-baseline.sh)
retains the exact matrix; each report linked below includes every raw repetition,
file fingerprint, timings, CPU time, byte counts, memory and environment metadata.

## Full validation

Times are median **milliseconds for the complete set**, not per image. Speedup is
one-worker median divided by four-worker median. RSS and reserved memory are the
maximum of the three four-worker samples. Spread is the largest `(max − min) /
median` among the three worker groups, expressed as a percentage; it is not a
confidence interval. The raw JSON also reports stored-byte throughput, which
should not be compared across differently compressible recipes as decoded speed.

| Encoding / tile / pattern / MiB per image | 1 worker ms | 2 workers ms | 4 workers ms | Speedup | RSS MiB | Reserved MiB | Spread |
|---|---:|---:|---:|---:|---:|---:|---:|
| [GZIP_1 / image / gradient / 32](fits-gzip-image-gradient-32mib-full.json) | 33.38 | 17.66 | 9.95 | 3.35× | 7.44 | 5.57 | 13.3% |
| [GZIP_1 / image / gradient / 8](fits-gzip-image-gradient-8mib-full.json) | 15.48 | 8.83 | 5.30 | 2.92× | 7.47 | 5.52 | 9.8% |
| [GZIP_1 / image / noise / 32](fits-gzip-image-noise-32mib-full.json) | 27.72 | 15.00 | 8.73 | 3.18× | 7.38 | 5.57 | 1.9% |
| [GZIP_1 / image / noise / 8](fits-gzip-image-noise-8mib-full.json) | 13.59 | 7.44 | 4.40 | 3.09× | 7.42 | 5.57 | 9.1% |
| [GZIP_1 / row / gradient / 32](fits-gzip-row-gradient-32mib-full.json) | 67.05 | 46.04 | 51.81 | 1.29× | 7.56 | 5.35 | 18.9% |
| [GZIP_1 / row / gradient / 8](fits-gzip-row-gradient-8mib-full.json) | 53.92 | 38.61 | 52.33 | 1.03× | 7.64 | 5.34 | 2.4% |
| [GZIP_1 / row / noise / 32](fits-gzip-row-noise-32mib-full.json) | 66.82 | 45.96 | 55.82 | 1.20× | 7.83 | 5.35 | 24.0% |
| [GZIP_1 / row / noise / 8](fits-gzip-row-noise-8mib-full.json) | 54.12 | 44.24 | 49.77 | 1.09× | 7.55 | 5.34 | 16.2% |
| [GZIP_2 / image / gradient / 32](fits-gzip2-image-gradient-32mib-full.json) | 31.89 | 17.37 | 9.25 | 3.45× | 7.45 | 5.57 | 96.9% |
| [GZIP_2 / image / gradient / 8](fits-gzip2-image-gradient-8mib-full.json) | 14.55 | 8.83 | 5.34 | 2.73× | 7.41 | 5.49 | 14.3% |
| [GZIP_2 / image / noise / 32](fits-gzip2-image-noise-32mib-full.json) | 28.01 | 14.98 | 8.83 | 3.17× | 7.42 | 5.57 | 1.3% |
| [GZIP_2 / image / noise / 8](fits-gzip2-image-noise-8mib-full.json) | 13.54 | 7.33 | 4.34 | 3.12× | 7.42 | 5.57 | 9.8% |
| [GZIP_2 / row / gradient / 32](fits-gzip2-row-gradient-32mib-full.json) | 138.26 | 78.39 | 62.55 | 2.21× | 7.62 | 5.32 | 17.2% |
| [GZIP_2 / row / gradient / 8](fits-gzip2-row-gradient-8mib-full.json) | 126.02 | 72.71 | 60.30 | 2.09× | 7.58 | 5.32 | 4.5% |
| [GZIP_2 / row / noise / 32](fits-gzip2-row-noise-32mib-full.json) | 67.22 | 46.19 | 52.24 | 1.29× | 7.47 | 5.35 | 3.2% |
| [GZIP_2 / row / noise / 8](fits-gzip2-row-noise-8mib-full.json) | 53.57 | 38.91 | 50.41 | 1.06× | 7.53 | 5.34 | 10.2% |

## Read and structural controls

Same definitions as above. All use eight 8 MiB noise images. Structural validation
examines declared layouts and extents without inflating payloads; raw read scans
stored bytes. Submillisecond structural results particularly emphasize overhead.

| Workload / encoding / tile / pattern / MiB per image | 1 worker ms | 2 workers ms | 4 workers ms | Speedup | RSS MiB | Reserved MiB | Spread |
|---|---:|---:|---:|---:|---:|---:|---:|
| [read / GZIP_1 / image / noise / 8](fits-gzip-image-noise-8mib-read.json) | 3.19 | 1.93 | 1.13 | 2.83× | 6.48 | 0.00 | 6.5% |
| [structural / GZIP_1 / image / noise / 8](fits-gzip-image-noise-8mib-structural.json) | 0.40 | 0.33 | 0.29 | 1.38× | 6.64 | 0.07 | 18.0% |
| [read / GZIP_1 / row / noise / 8](fits-gzip-row-noise-8mib-read.json) | 3.41 | 1.94 | 1.18 | 2.90× | 6.48 | 0.00 | 7.8% |
| [structural / GZIP_1 / row / noise / 8](fits-gzip-row-noise-8mib-structural.json) | 7.23 | 5.78 | 11.03 | 0.66× | 6.62 | 0.07 | 16.8% |
| [read / GZIP_2 / image / noise / 8](fits-gzip2-image-noise-8mib-read.json) | 3.31 | 1.85 | 1.11 | 2.99× | 6.48 | 0.00 | 5.4% |
| [structural / GZIP_2 / image / noise / 8](fits-gzip2-image-noise-8mib-structural.json) | 0.37 | 0.32 | 0.28 | 1.33× | 6.64 | 0.07 | 8.6% |
| [read / GZIP_2 / row / noise / 8](fits-gzip2-row-noise-8mib-read.json) | 3.36 | 1.97 | 1.13 | 2.99× | 6.48 | 0.00 | 9.2% |
| [structural / GZIP_2 / row / noise / 8](fits-gzip2-row-noise-8mib-structural.json) | 7.26 | 5.76 | 11.45 | 0.63× | 6.64 | 0.07 | 29.3% |

## Interpretation and remaining evidence

The largest spread (96.9%) is GZIP_2 / image / gradient / 32 MiB at four workers:
8.72, 9.25 and 17.68 ms. Retain that outlier; three repetitions with uncontrolled
desktop load do not establish stable tail latency. No historical native-CFITSIO
baseline was run, so these results establish current behavior rather than a
speedup over that backend.

The two tile extremes demonstrate bounded memory on 8/32 MiB decoded images and
show that more workers do not always improve throughput. They do not establish
maximum-file behavior, cold/remote storage speed, performance on real captures,
foreground application responsiveness, or a universal worker count. UInt16
synthetic fixtures are a benchmark subset, not coverage of every supported integer
width or FITS layout.

Next: profile row-tile scaling before choosing calibration recommendations;
complete bounded/native resource controls for remaining compressed-FITS layouts,
then collect real-file, Windows/Linux and memory-pressure/foreground-contention
evidence before monitor rollout and automatic application defaults. No production
validator API, application setting or scheduler changed in this increment.
