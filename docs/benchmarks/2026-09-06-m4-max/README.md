# Initial validator baseline — 2026-09-06

This is the baseline of the existing validator before managed reservations and
streaming changes. The benchmark foundation is implemented; these local synthetic
measurements are not application calibration or release acceptance evidence.

## Environment and method

- Apple M4 Max, 36 GiB RAM, 14 logical/physical CPUs; macOS 26.6.2 (25G83), aarch64.
- Scratch: `/private/tmp`, on `/System/Volumes/Data` (`/dev/disk3s5`), with 627 GiB
  available before the run. Ordinary desktop session; competing load uncontrolled.
- Release profile; rustc 1.94.0 (4a4ef493e 2026-03-02). No build/test jobs ran during
  the matrix. Portable default build flags; no added target-CPU/acceleration flags.
- Parent revision: `dbf4bba4ee1377558854bda7bc5d49a9a71c61e4`. Validator and harness
  changes were uncommitted. Measured-source SHA-256 (also verified against the tree):
  `2535e294edc33332f71f0f5b58a4719830a21701cf13e15891701531698c1c73`.
- 24 workload configurations, each with 1/2/4 workers and three repetitions:
  **216 successful samples / 1,440 completed file operations**. All raw samples,
  per-file timings, fixture fingerprints, CPU deltas and limits are retained here.
- Small sets: eight 2048×2048 UInt16 images (8 MiB/image; 64 MiB decoded/set).
  Large sets: four 4096×4096 images (32 MiB/image; 128 MiB decoded/set).
  Seed 42; independent noise and highly compressible gradient patterns.
- Four encodings: ordinary FITS, raw XISF, zlib XISF and Zstandard XISF. XISF has
  SHA-256 attachment checksums; FITS has none. Codec versions/options are captured
  by the source/lockfile fingerprint. This is not a like-for-like codec contest.
- Every sample uses a fresh process. Fixture generation is excluded from RSS and
  timing; child fingerprint verification is excluded from timing but included in
  child lifetime RSS. File generation and verification touch all bytes, so the
  cache is **uncontrolled / likely warm**. No privileged cache flushing.
- Default validator limits remain 64 MiB headers / 256 MiB working per call /
  100,000 structures / 64 GiB decoded. Harness scratch quota is 512 MiB and the
  conservative aggregate admission estimate is at most 512 MiB. Neither estimate
  nor a successful run establishes a hard process-memory ceiling.

## Full validation results

Each cell is **median sample wall time in milliseconds / maximum peak process RSS
in MiB across three repetitions**. Wall time includes all files, worker startup,
result coordination and joins. RSS is `getrusage(RUSAGE_SELF)` on macOS, in bytes
converted to MiB here. CPU, per-file timings and min/max wall times are in JSON.

### Eight 8 MiB images

| Encoding / pixels | 1 worker (ms / MiB) | 2 workers (ms / MiB) | 4 workers (ms / MiB) |
|---|---:|---:|---:|
| [fits / noise](fits-noise-8mib-full.json) | 3.80 / 6.3 | 2.18 / 6.5 | 1.39 / 6.8 |
| [fits / gradient](fits-gradient-8mib-full.json) | 3.91 / 6.4 | 2.15 / 6.5 | 1.34 / 6.8 |
| [xisf / noise](xisf-noise-8mib-full.json) | 112.78 / 6.5 | 58.16 / 6.6 | 31.95 / 7.0 |
| [xisf / gradient](xisf-gradient-8mib-full.json) | 112.98 / 6.5 | 58.02 / 6.6 | 32.47 / 7.1 |
| [zlib / noise](zlib-noise-8mib-full.json) | 263.64 / 22.5 | 135.76 / 38.7 | 73.84 / 71.1 |
| [zlib / gradient](zlib-gradient-8mib-full.json) | 28.90 / 14.7 | 15.44 / 23.0 | 8.55 / 39.9 |
| [zstd / noise](zstd-noise-8mib-full.json) | 117.34 / 25.0 | 61.32 / 43.5 | 34.74 / 80.8 |
| [zstd / gradient](zstd-gradient-8mib-full.json) | 4.49 / 17.0 | 3.05 / 27.5 | 2.16 / 48.5 |

### Four 32 MiB images

| Encoding / pixels | 1 worker (ms / MiB) | 2 workers (ms / MiB) | 4 workers (ms / MiB) |
|---|---:|---:|---:|
| [fits / noise](fits-noise-32mib-full.json) | 7.60 / 6.4 | 4.05 / 6.5 | 2.47 / 6.8 |
| [fits / gradient](fits-gradient-32mib-full.json) | 7.45 / 6.3 | 4.09 / 6.5 | 2.45 / 6.8 |
| [xisf / noise](xisf-noise-32mib-full.json) | 228.38 / 6.4 | 118.03 / 6.6 | 63.86 / 7.0 |
| [xisf / gradient](xisf-gradient-32mib-full.json) | 228.12 / 6.4 | 117.62 / 6.6 | 64.55 / 7.1 |
| [zlib / noise](zlib-noise-32mib-full.json) | 536.33 / 70.2 | 274.60 / 134.0 | 151.73 / 261.6 |
| [zlib / gradient](zlib-gradient-32mib-full.json) | 56.85 / 39.1 | 31.44 / 71.2 | 17.56 / 136.1 |
| [zstd / noise](zstd-noise-32mib-full.json) | 239.21 / 73.0 | 126.06 / 139.5 | 72.42 / 272.5 |
| [zstd / gradient](zstd-gradient-32mib-full.json) | 8.04 / 40.9 | 6.28 / 75.5 | 4.71 / 144.2 |

## Raw-read and structural controls

Noise, eight 8 MiB images; median sample wall time in milliseconds. Structural
throughput based on container length would be misleading because it skips most
payload bytes. Raw-read results are predominantly cache/memory/OS-path measurements.

| Encoding | Read, 1 / 2 / 4 workers (ms) | Structural, 1 / 2 / 4 workers (ms) |
|---|---:|---:|
| fits | [3.484 / 2.020 / 1.179](fits-noise-8mib-read.json) | [0.249 / 0.205 / 0.187](fits-noise-8mib-structural.json) |
| xisf | [3.417 / 1.872 / 1.186](xisf-noise-8mib-read.json) | [0.263 / 0.219 / 0.208](xisf-noise-8mib-structural.json) |
| zlib | [3.340 / 1.933 / 1.124](zlib-noise-8mib-read.json) | [0.292 / 0.248 / 0.216](zlib-noise-8mib-structural.json) |
| zstd | [3.332 / 1.862 / 1.123](zstd-noise-8mib-read.json) | [0.249 / 0.223 / 0.224](zstd-noise-8mib-structural.json) |

## Interpretation and next work

1. Concurrency earns its place: four workers reduce median full-validation time
   for noisy 32 MiB zlib images from 536.33 ms to 151.73 ms (3.53×), and noisy
   Zstandard images from 239.21 ms to 72.42 ms (3.30×). This supports testing
   bounded concurrency rather than choosing universal serialization.
2. Memory grows materially with active compressed files: noisy 32 MiB zlib rises
   from 70.2 to 261.6 MiB peak RSS; Zstandard rises from 73.0 to 272.5 MiB.
   These observations are consistent with the current complete-input/output
   allocation paths. Streaming and shared reservations should be compared against
   these same recipes while preserving validation correctness and useful throughput.
3. Workload matters: Zstandard gradient images gain only 1.71× at the larger size,
   while peak RSS rises from 40.9 to 144.2 MiB. A future scheduler/calibration policy
   should consider marginal throughput, memory and responsiveness, not CPU count alone.
4. Ordinary FITS/raw XISF stay near 6–7 MiB RSS in these cases. Their full-validation
   times differ substantially because the XISF workload includes SHA-256 checksums
   and different read paths. Do not infer that container choice alone causes the gap.
5. Three repetitions are an initial diagnostic, not a statistical acceptance rule.
   The largest within-group min/max spread is about 19% of its median; very short
   structural/cache samples are especially sensitive to overhead. Keep raw variation
   and expand repetitions/representative workloads before numerical release targets.

Next implementation: specify the additive caller-owned resource API; test aggregate
reservation/release on success, failure and cancellation; stream zlib/Zstandard
validation with bounded buffers; audit parser/native allocation and backend-thread
capabilities. Repeat this matrix to compare memory and throughput, then add LZ4,
tiled FITS, many subblocks, metadata/metrics, real captures and cold/remote storage.

Not measured here: Windows/Linux execution, native CFITSIO decode peaks, foreground
application/task-switch responsiveness, contention/backoff behavior, hardware-specific
acceleration alternatives or automatic scheduling. Manual SIGINT checks passed during
preparation and the measurement phase with no output/scratch remnants; deterministic
cancellation/timeout tests passed. These checks do not establish cancellation latency
for blocked remote I/O. Application calibration and config changes remain separate.

## Reproduce

From the repository root, choose a fresh output directory and describe the actual
machine/load/storage in the environment note:

```sh
cargo build --workspace --release
sh astro-bench/scripts/validator-baseline.sh \
  docs/benchmarks/NEW-RUN /private/tmp \
  'CPU; RAM; OS; scratch volume; competing workload and cache conditions'
```

Validation for this increment: formatting, Clippy with warnings denied, 93 workspace
all-target tests (one preexisting ignored test), three doctests, workspace docs and
release build passed. All 24 reports were checked for source identity, complete
file accounting and positive measurements. Existing validator behavior and
AstroMuninn runtime/configuration were not changed by the benchmark increment.
