# Shared-reservation and streaming comparison — 2026-09-06

The initial local targets passed: at least 60% less peak RSS for noisy 32 MiB
compressed images at four workers, with no more than 15% median validation-time
regression. Zlib peak RSS fell by 97.1% and Zstandard by 94.0%; median validation
times also decreased. These are local synthetic measurements, not release defaults.

## Provenance and method

- Parent revision `b513716`; this increment was uncommitted during measurement.
  Measured-source SHA-256: `32f815e851efbcc12608bf2646cc8b1f4feafa3ce381fab7b8e952cd7b39e1b8` (verified against the final source tree).
- Apple M4 Max, 36 GiB RAM, 14 logical/physical CPUs, macOS 26.6.2 (25G83), aarch64;
  rustc 1.94.0, release build, default compiler flags.
- Scratch `/private/tmp` on `/System/Volumes/Data` (`/dev/disk3s5`), 626 GiB available
  before the run. Ordinary desktop session; competing load uncontrolled. No build
  or test jobs ran during the matrix.
- Same versioned recipes, seed, dimensions, encodings, patterns and exact file
  fingerprints as the [committed baseline](../2026-09-06-m4-max/README.md).
  Every report/fixture manifest was checked for equality with its baseline.
- 24 configurations × 1/2/4 workers × three repetitions = 216 successful samples
  and 1,440 completed file operations. Fresh sample processes exclude generation
  from peak RSS. Pre-sample verification touches every file: OS cache remains
  uncontrolled / likely warm; these runs do not measure device throughput.
- Per-call working limit 256 MiB; workers now share a 512 MiB validation allowance.
  `peak_reserved_bytes` records peak aggregate admitted bytes separately from RSS.
  Each successful sample checks the pool returns to zero. Original benchmark
  preflight limits were retained to compare the same admitted workloads.
- XISF fixtures contain SHA-256 checksums; FITS fixtures do not. Codec/history and
  metadata allowances are reservations, not native allocator interception or a
  hard process-memory ceiling. Codecs/lockfile were unchanged.

## Four-worker full-validation comparison

Wall times are medians for the entire sample; RSS is the maximum process-lifetime
peak across three repetitions. The eight-file sets contain 8 MiB decoded images;
four-file sets contain 32 MiB images. Negative time changes indicate improvement.

| Workload | Before → after wall time (ms) | Time change | Before → after peak RSS (MiB) | New peak reservations (MiB) |
|---|---:|---:|---:|---:|
| [fits / noise / 8 MiB](fits-noise-8mib-full.json) | 1.39 → 1.35 | -3.2% | 6.83 → 6.84 | 0.25 |
| [fits / gradient / 8 MiB](fits-gradient-8mib-full.json) | 1.34 → 1.32 | -1.6% | 6.80 → 6.83 | 0.25 |
| [xisf / noise / 8 MiB](xisf-noise-8mib-full.json) | 31.95 → 31.77 | -0.6% | 7.05 → 7.14 | 0.33 |
| [xisf / gradient / 8 MiB](xisf-gradient-8mib-full.json) | 32.47 → 31.93 | -1.7% | 7.08 → 7.25 | 0.33 |
| [zlib / noise / 8 MiB](zlib-noise-8mib-full.json) | 73.84 → 72.67 | -1.6% | 71.12 → 7.77 | 4.58 |
| [zlib / gradient / 8 MiB](zlib-gradient-8mib-full.json) | 8.55 → 8.02 | -6.2% | 39.88 → 7.41 | 4.54 |
| [zstd / noise / 8 MiB](zstd-noise-8mib-full.json) | 34.74 → 32.74 | -5.8% | 80.77 → 16.86 | 12.58 |
| [zstd / gradient / 8 MiB](zstd-gradient-8mib-full.json) | 2.16 → 0.92 | -57.3% | 48.53 → 16.55 | 12.35 |
| [fits / noise / 32 MiB](fits-noise-32mib-full.json) | 2.47 → 2.48 | +0.6% | 6.78 → 6.81 | 0.25 |
| [fits / gradient / 32 MiB](fits-gradient-32mib-full.json) | 2.45 → 2.41 | -1.8% | 6.78 → 6.80 | 0.25 |
| [xisf / noise / 32 MiB](xisf-noise-32mib-full.json) | 63.86 → 64.01 | +0.2% | 6.98 → 7.00 | 0.33 |
| [xisf / gradient / 32 MiB](xisf-gradient-32mib-full.json) | 64.55 → 64.11 | -0.7% | 7.06 → 7.02 | 0.33 |
| [zlib / noise / 32 MiB](zlib-noise-32mib-full.json) | 151.73 → 145.23 | -4.3% | 261.59 → 7.64 | 4.58 |
| [zlib / gradient / 32 MiB](zlib-gradient-32mib-full.json) | 17.56 → 15.52 | -11.6% | 136.06 → 7.45 | 4.58 |
| [zstd / noise / 32 MiB](zstd-noise-32mib-full.json) | 72.42 → 66.20 | -8.6% | 272.50 → 16.31 | 12.58 |
| [zstd / gradient / 32 MiB](zstd-gradient-32mib-full.json) | 4.71 → 1.74 | -63.1% | 144.25 → 16.39 | 12.38 |

## Serial and concurrent behavior

Noisy 32 MiB compressed images; median wall time / maximum peak RSS. Streaming
preserves useful concurrency while removing whole-input/output working arrays.

| Workload | 1 worker (ms / MiB) | 2 workers (ms / MiB) | 4 workers (ms / MiB) |
|---|---:|---:|---:|
| [zlib](zlib-noise-32mib-full.json) | 523.44 / 6.77 | 267.84 / 7.05 | 145.23 / 7.64 |
| [zstd](zstd-noise-32mib-full.json) | 232.80 / 8.83 | 119.77 / 11.31 | 66.20 / 16.31 |

All full-validation groups stayed within the 15% slowdown threshold (none exceeded
it, including cases outside the primary target). The largest median structural
increase was 0.045 ms per eight-file sample, within the stated 0.5 ms
absolute allowance. Three repetitions are preliminary: the largest min/max spread
was 44.0% of the median in `zlib-noise-8mib-structural`, 4 workers.
Retain raw timings and add representative contention/storage measurements before
setting release thresholds or recommended application defaults.

## Verification and limits

- Formatting, Clippy with warnings denied, 100 all-target workspace tests, four
  doctests, documentation with warnings denied and release build passed. One
  preexisting SEP star-detection test remains ignored.
- New deterministic tests cover atomic admission, busy versus impossible requests,
  concurrency, reservation release on failure/cancellation/unwinding, small-budget
  large-output validation, truncated/trailing data, output mismatch, excessive
  Zstandard history, concatenated/skippable frames and the native FITS restriction.
- Zlib now requires explicit `StreamEnd`, including its trailer; previously a
  truncated trailer could be accepted by the buffered decoder's EOF behavior.
- XML/native allocations retain conservative allowances and some infallible
  allocation paths. This is not a guarantee of graceful recovery from every OOM.
- Shared-budget full CFITSIO tile decoding deliberately returns `Unsupported` until
  native allocation and cross-loader exclusivity controls are implemented. Structural
  tiled checks and standalone full decoding remain available. LZ4 is still bounded
  whole-block decoding; it is covered by correctness tests, not this timing matrix.
- Windows/Linux execution, native decoder peaks, real captures, remote/rotating
  storage, adaptive scheduling and foreground responsiveness remain rollout work.
  AstroMuninn runtime and config behavior are not changed in this increment.

## Reproduce

```sh
cargo build --workspace --all-features --release
sh astro-bench/scripts/validator-baseline.sh \
  docs/benchmarks/NEW-COMPARISON /private/tmp \
  'CPU; RAM; OS; scratch volume; competing load; shared validator allowance 512 MiB'
```

See [implementation and resource audit](../../ValidationResourceImplementation.md)
for the API, memory lifetimes and remaining integration requirements.
