# Validator benchmark completion — 2026-09-07

## Findings and scope

The available-machine benchmark investigation is complete. Production validator
code and application defaults are unchanged. The recorded results establish
bounded memory and useful concurrency on this Mac, while exposing small-tile
scaling limits and invalid HDU checksums in three older local captures.

- **960 scaling samples**: peak RSS at most **9.54 MiB** through eight workers and
  64 MiB synthetic images, passing the predeclared 64 MiB investigation target.
- **40 contention samples**: the scheduling/short-task probe passed its targets
  both alone and with two bounded CPU/memory competitors.
- **10 cancellation trials**: CLI exit, child reaping and scratch cleanup all
  completed within **38.85 ms**, below the two-second local target.
- **270 real-capture samples** on internal SSD, mechanical HDD and external SSD:
  **240 successful / 30 rejected**. All rejections involve the three local FITS
  files' invalid HDU checksums; independent CFITSIO verification agrees.
- **15 additional successful samples** on three **714.29 MiB FITS files**:
  peak RSS at most **6.33 MiB**. No capture was copied, repaired or modified.
- Two separate instrumented profile runs explain candidate costs. Their timings
  are excluded from throughput comparisons.

This gives 1,255 successful primary unprofiled samples plus 30 explicitly rejected
samples. The first aborted local-capture attempt is retained separately: 45
structural successes and one failed full sample are not pooled into these counts.
The [audit](audit.json) records counts, fingerprints, limits and fixture identity.

## Environment and provenance

Apple M4 Max, 36 GiB RAM, 14 logical/physical cores; macOS 26.6.2 (25G83), arm64,
rustc 1.94.0. Release workspace/all-feature builds; AC power and low-power mode off.
Desktop load, power/core placement and OS cache were uncontrolled. No agent builds
or tests ran concurrently with timing experiments. The contention test's two
explicit competitors each continuously hashed a touched 32 MiB payload in a
separate process. This is bounded CPU/memory traffic, not induced OS RAM pressure.

| Storage | Dataset used | Stored selection size |
|---|---|---:|
| Internal APFS SSD `/dev/disk3s5` | Three older FITS + three XISF captures in local `tests/data` | 96.24 MiB |
| User-described mechanical HDD `/dev/disk10s1` | Three FITS + three XISF from supplied `RavenSkyTestFiles/QA` | 535.80 MiB |
| User-described external SSD `/dev/disk7s1` | Three FITS + three XISF selected recursively from supplied `0Working` | 672.82 MiB |
| Same external SSD, large-file extension | First three sorted FITS files in supplied `NGC7000/app` | 2,142.86 MiB |

Stored free space at inspection: internal 625 GiB, HDD 825 GiB, external SSD
approximately 1 TiB. Swap usage was zero at the beginning and end; observed
`vm_stat` swap-in/out counters remained zero. The machine already used compressed
memory. See [final VM counters](vm-after.txt); these snapshots are not continuous
pressure monitoring or proof of responsiveness under low memory.

All Rust reports share measured-source SHA-256
`1b0a1b651e6c3a84b4a204268dfe9aa3eebf5556e0f754d5f268c16c3e7e4c1d`, independently recomputed from the build script's declared source
scope after the experiments. Recorded revisions are `13ab8c6` (scaling) and
`da8927c` (later binaries); library and synthetic-runner contents are identical.
The capture example has its additional source fingerprint
`944d8de7f775b9e3c9ccb3b279508112e888f2e299872ff6979552d61de5ec60`. Each experiment records its Python-script hash. Revisions are
supplementary to content fingerprints because documentation/tooling commits were
made between phases without changing measured Rust code.

## Scaling and tile costs

Each case uses eight UInt16 files, five fresh-process repetitions at each of
1/2/4/8 workers, then another round in reverse worker order. 8 MiB images are
2048 × 2048; 64 MiB images are 4096 × 8192. Tiles span the full width and either
one row, 32 rows or the whole image. Generation/hashing occur outside timing and
warm the cache. Whole-image/32-row results cannot isolate codec cost because tile
layout also changes compression and stored size.

The table pools ten observations per worker count across the two orders. Times
are median milliseconds **for eight files**, not per file. “Best” is the lowest
observed median, not a recommendation; close differences can be noise. Order drift
is the largest absolute `(forward median / reverse median − 1)` across workers.
Raw reports retain min/max, CPU, byte counts, per-file timing and RSS.

| Case (forward / reverse reports) | 1 worker ms | 2 workers ms | 4 workers ms | 8 workers ms | Best | Order drift |
|---|---:|---:|---:|---:|---:|---:|
| [fits-gzip-32rows-gradient-64mib](scaling/fits-gzip-32rows-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip-32rows-gradient-64mib-reverse.json) | 156.56 | 82.27 | 45.66 | 27.62 | 8 | 4.0% |
| [fits-gzip-32rows-gradient-8mib](scaling/fits-gzip-32rows-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip-32rows-gradient-8mib-reverse.json) | 21.55 | 11.70 | 6.78 | 4.79 | 8 | 13.6% |
| [fits-gzip-32rows-noise-64mib](scaling/fits-gzip-32rows-noise-64mib-forward.json) / [reverse](scaling/fits-gzip-32rows-noise-64mib-reverse.json) | 121.60 | 64.35 | 38.08 | 30.18 | 8 | 5.9% |
| [fits-gzip-32rows-noise-8mib](scaling/fits-gzip-32rows-noise-8mib-forward.json) / [reverse](scaling/fits-gzip-32rows-noise-8mib-reverse.json) | 15.82 | 8.44 | 5.20 | 5.15 | 8 | 4.6% |
| [fits-gzip-image-gradient-64mib](scaling/fits-gzip-image-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip-image-gradient-64mib-reverse.json) | 119.51 | 62.63 | 33.99 | 19.26 | 8 | 1.8% |
| [fits-gzip-image-gradient-8mib](scaling/fits-gzip-image-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip-image-gradient-8mib-reverse.json) | 15.86 | 9.63 | 5.58 | 3.50 | 8 | 29.5% |
| [fits-gzip-image-noise-64mib](scaling/fits-gzip-image-noise-64mib-forward.json) / [reverse](scaling/fits-gzip-image-noise-64mib-reverse.json) | 113.98 | 60.10 | 33.87 | 24.82 | 8 | 1.6% |
| [fits-gzip-image-noise-8mib](scaling/fits-gzip-image-noise-8mib-forward.json) / [reverse](scaling/fits-gzip-image-noise-8mib-reverse.json) | 13.45 | 7.23 | 4.29 | 3.39 | 8 | 3.5% |
| [fits-gzip-row-gradient-64mib](scaling/fits-gzip-row-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip-row-gradient-64mib-reverse.json) | 268.51 | 184.01 | 194.31 | 308.32 | 2 | 21.6% |
| [fits-gzip-row-gradient-8mib](scaling/fits-gzip-row-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip-row-gradient-8mib-reverse.json) | 53.55 | 39.26 | 43.49 | 77.53 | 2 | 16.5% |
| [fits-gzip-row-noise-64mib](scaling/fits-gzip-row-noise-64mib-forward.json) / [reverse](scaling/fits-gzip-row-noise-64mib-reverse.json) | 267.71 | 182.28 | 211.18 | 314.58 | 2 | 21.0% |
| [fits-gzip-row-noise-8mib](scaling/fits-gzip-row-noise-8mib-forward.json) / [reverse](scaling/fits-gzip-row-noise-8mib-reverse.json) | 54.52 | 42.61 | 51.65 | 75.47 | 2 | 19.5% |
| [fits-gzip2-32rows-gradient-64mib](scaling/fits-gzip2-32rows-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip2-32rows-gradient-64mib-reverse.json) | 128.48 | 65.88 | 37.40 | 24.32 | 8 | 1.6% |
| [fits-gzip2-32rows-gradient-8mib](scaling/fits-gzip2-32rows-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip2-32rows-gradient-8mib-reverse.json) | 18.23 | 10.52 | 6.60 | 4.73 | 8 | 8.4% |
| [fits-gzip2-32rows-noise-64mib](scaling/fits-gzip2-32rows-noise-64mib-forward.json) / [reverse](scaling/fits-gzip2-32rows-noise-64mib-reverse.json) | 122.78 | 64.66 | 38.57 | 30.25 | 8 | 4.1% |
| [fits-gzip2-32rows-noise-8mib](scaling/fits-gzip2-32rows-noise-8mib-forward.json) / [reverse](scaling/fits-gzip2-32rows-noise-8mib-reverse.json) | 15.76 | 8.50 | 5.30 | 5.24 | 8 | 4.7% |
| [fits-gzip2-image-gradient-64mib](scaling/fits-gzip2-image-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip2-image-gradient-64mib-reverse.json) | 115.55 | 59.47 | 32.59 | 18.63 | 8 | 3.1% |
| [fits-gzip2-image-gradient-8mib](scaling/fits-gzip2-image-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip2-image-gradient-8mib-reverse.json) | 15.31 | 9.14 | 5.50 | 3.39 | 8 | 10.0% |
| [fits-gzip2-image-noise-64mib](scaling/fits-gzip2-image-noise-64mib-forward.json) / [reverse](scaling/fits-gzip2-image-noise-64mib-reverse.json) | 113.96 | 60.21 | 34.25 | 24.36 | 8 | 5.6% |
| [fits-gzip2-image-noise-8mib](scaling/fits-gzip2-image-noise-8mib-forward.json) / [reverse](scaling/fits-gzip2-image-noise-8mib-reverse.json) | 13.63 | 7.26 | 4.31 | 3.50 | 8 | 3.0% |
| [fits-gzip2-row-gradient-64mib](scaling/fits-gzip2-row-gradient-64mib-forward.json) / [reverse](scaling/fits-gzip2-row-gradient-64mib-reverse.json) | 534.20 | 303.81 | 253.92 | 303.11 | 4 | 16.4% |
| [fits-gzip2-row-gradient-8mib](scaling/fits-gzip2-row-gradient-8mib-forward.json) / [reverse](scaling/fits-gzip2-row-gradient-8mib-reverse.json) | 125.47 | 70.57 | 65.75 | 76.87 | 4 | 4.5% |
| [fits-gzip2-row-noise-64mib](scaling/fits-gzip2-row-noise-64mib-forward.json) / [reverse](scaling/fits-gzip2-row-noise-64mib-reverse.json) | 269.42 | 183.87 | 222.63 | 311.12 | 2 | 5.5% |
| [fits-gzip2-row-noise-8mib](scaling/fits-gzip2-row-noise-8mib-forward.json) / [reverse](scaling/fits-gzip2-row-noise-8mib-reverse.json) | 54.96 | 40.42 | 50.85 | 76.90 | 2 | 3.3% |

Six row-tile cases favor two workers; the two GZIP_2 gradient cases favor four.
All 32-row and whole-image cases have their lowest pooled median at eight, but
some 32-row small/noise differences between four and eight are only about 1%.
Increasing to eight workers is therefore not a universal improvement. Preserve
outliers: the largest within-group pooled spread is 66.2%, and maximum worker-order
drift is 29.5%, both in small whole-image cases. These are not confidence intervals.

[One-worker profile](profile-permitted/profile-1workers.txt) and
[four-worker profile](profile-permitted/profile-4workers.txt) sampled GZIP_2 row
validation of 256 gradient frames. Stacks show Deflate tree setup/decompression,
`read`/`lseek`, buffer clearing and allocation. The main thread waits on its workers;
the signal thread waits for interrupts. SHA-256 frames in child setup are fixture
verification, outside the validator timing. Sample counts across different numbers
of threads/windows are not comparable CPU percentages.

Code locations supporting candidate follow-ups:

- [Descriptor checks](../../../astro-io/src/validation/fits.rs) perform a small
  read for each table row; full GZIP validation rereads each descriptor.
- [GZIP streaming](../../../astro-io/src/validation/fits/gzip.rs) constructs
  decoder/input/output state for every tile.
- [Context reads](../../../astro-io/src/validation/mod.rs) seek then read;
  [shared input](../../../astro-io/src/validation/input.rs) buffers codec reads.

Together with the tile-height experiment, these observations justify testing
batched descriptor reads and reusable decoder/scratch state in a later optimization
increment. They do not establish a single causal bottleneck or justify relaxing
validation semantics. No such optimization was mixed into this benchmark change.
The [first profiling attempt](profile/FAILURE.md) was blocked by sandbox process
inspection; the separately authorized rerun succeeded.

## Responsiveness proxy and cancellation

The independent Python probe sleeps for 20 ms, records excess wake delay, then
hashes 64 KiB and records task duration. It keeps preparation separate from the
sample phase. That phase includes child hash verification and inter-sample gaps,
so it is not exclusively time inside validation. Idle-before/after raw records
are retained alongside every loaded observation.

Predeclared thresholds: p95 wake delay <20 ms; p95 short-task duration below twice
pooled idle p95 plus 2 ms (2.219 ms here). Idle p95 wake delay was about 5.03–5.06 ms.
All loaded p95 wake delays were about 5.01 ms; maximum observed loaded sample-phase
wake delay was 5.20 ms. All short-task thresholds passed. Scheduling/core placement
can make tiny tasks faster under load; these results do not show a GUI speedup.

| Load / workers | Median validation seconds (128 files) | Probe p95 wake ms | Probe p95 task ms | Max RSS MiB |
|---|---:|---:|---:|---:|
| [alone-1workers](contention/alone-1workers-probe.json) | 2.021 | 5.006 | 0.033 | 7.08 |
| [alone-2workers](contention/alone-2workers-probe.json) | 1.121 | 5.006 | 0.032 | 7.53 |
| [alone-4workers](contention/alone-4workers-probe.json) | 0.827 | 5.008 | 0.035 | 8.22 |
| [alone-8workers](contention/alone-8workers-probe.json) | 1.052 | 5.006 | 0.030 | 8.78 |
| [two-contenders-1workers](contention/two-contenders-1workers-probe.json) | 2.251 | 5.005 | 0.028 | 7.09 |
| [two-contenders-2workers](contention/two-contenders-2workers-probe.json) | 1.232 | 5.006 | 0.030 | 7.28 |
| [two-contenders-4workers](contention/two-contenders-4workers-probe.json) | 0.991 | 5.007 | 0.032 | 8.45 |
| [two-contenders-8workers](contention/two-contenders-8workers-probe.json) | 1.115 | 5.006 | 0.099 | 9.38 |

[Cancellation records](cancellation/cancellation.json) contain five preparation
trials (about 1.3 ms to exit) and five sample-child trials (8.8–38.9 ms). Every
trial returned failure, produced no successful report, reaped any observed child
and removed its owned scratch directory. This measures the CLI's parent-driven
SIGINT/child-stop path, including fingerprint-verification time. It does not
measure the library's cooperative cancellation latency inside a blocked remote read.
Normal application switching and pressure/backoff behavior still need product
verification; a scheduling/hash probe is only a proxy.

## Real captures and storage/cache effects

Capture measurements use 16 passes per sample, 1/2/4 workers and five independent
processes. Passes share one atomic operation sequence and may overlap, including
reads of the same file. Hash preparation and final source verification are outside
timed validation. Input hashes match throughout each matrix; no capture bytes
changed. This is sustained cached read-only work, not a stream of newly acquired
unique files. The maximum capture reservation was 18.15 MiB; maximum capture RSS
was 8.36 MiB (reservations include conservative allowances and are not RSS).

First-observed and immediate-repeat sequential reads include SHA-256 computation:

| Selection | First-observed MiB/s | Immediate-repeat MiB/s |
|---|---:|---:|
| [captures-local-diagnostic](captures-local-diagnostic/fingerprint-reads.json) | 2727.9 | 2766.1 |
| [captures-hdd](captures-hdd/fingerprint-reads.json) | 44.9 | 2366.3 |
| [captures-external-ssd](captures-external-ssd/fingerprint-reads.json) | 823.9 | 2822.9 |
| [large-fits](large-fits/fingerprint-reads.json) | 583.7 | 2805.9 |

These are read-plus-hash rates with uncontrolled caches, not guaranteed cold-device
bandwidth. The first local attempt already touched local data; the diagnostic rerun
is especially warm. The device selections differ in sizes/content, so do not rank
drives from the subsequent warm validation rates or copy those rates into automatic
storage defaults. No cache eviction or writes to external volumes were performed.

Times below are median milliseconds **for 16 passes through the selected group**.
Each format group has three files; mixed groups have six. Rejected sets have no
successful throughput entry. Source/operation details are in the linked directories.

| Volume / group / level | 1 worker ms | 2 workers ms | 4 workers ms | Max RSS MiB |
|---|---:|---:|---:|---:|
| [local-diagnostic / fits / structural](captures-local-diagnostic/) | 2.24 | 1.75 | 2.17 | 6.27 |
| [local-diagnostic / xisf / structural](captures-local-diagnostic/) | 2.34 | 2.14 | 1.94 | 6.97 |
| [local-diagnostic / mixed / structural](captures-local-diagnostic/) | 4.43 | 3.42 | 2.71 | 6.89 |
| [local-diagnostic / fits / full](captures-local-diagnostic/) | rejected | rejected | rejected | 6.94 |
| [local-diagnostic / xisf / full](captures-local-diagnostic/) | 63.84 | 36.57 | 25.47 | 7.36 |
| [local-diagnostic / mixed / full](captures-local-diagnostic/) | rejected | rejected | rejected | 7.34 |
| [hdd / fits / structural](captures-hdd/) | 2.16 | 1.63 | 2.23 | 6.30 |
| [hdd / xisf / structural](captures-hdd/) | 2.95 | 2.29 | 2.36 | 7.14 |
| [hdd / mixed / structural](captures-hdd/) | 5.00 | 2.85 | 2.28 | 6.92 |
| [hdd / fits / full](captures-hdd/) | 241.12 | 127.48 | 78.09 | 6.75 |
| [hdd / xisf / full](captures-hdd/) | 455.06 | 252.76 | 153.94 | 7.55 |
| [hdd / mixed / full](captures-hdd/) | 678.51 | 368.10 | 232.71 | 7.41 |
| [external-ssd / fits / structural](captures-external-ssd/) | 3.20 | 2.51 | 3.54 | 6.28 |
| [external-ssd / xisf / structural](captures-external-ssd/) | 6.54 | 4.28 | 2.89 | 8.27 |
| [external-ssd / mixed / structural](captures-external-ssd/) | 9.73 | 5.51 | 3.79 | 7.98 |
| [external-ssd / fits / full](captures-external-ssd/) | 537.01 | 289.74 | 177.64 | 6.75 |
| [external-ssd / xisf / full](captures-external-ssd/) | 70.36 | 39.45 | 24.30 | 8.36 |
| [external-ssd / mixed / full](captures-external-ssd/) | 617.57 | 329.29 | 210.80 | 8.36 |

Both external-volume matrices passed all 90 samples. The local matrix passed 60
and rejected 30 FITS-only/mixed full samples. For all three local FITS inputs,
[independent CFITSIO 4.7.0 checks](cfitsio-checksum-check.json) returned valid data
checksums (`+1`) and invalid HDU checksums (`−1`). Valid/invalid supplied checksum
fixtures served as controls. This confirms the rejection, not what originally
caused the header/checksum discrepancy. Files were not repaired. The initial
aborted [local attempt](captures-local/) remains separate diagnostic evidence.

The [supplied small fixtures](../../../astro-bench/tests/test_data/README.md) also
exercise checksum failures, missing XISF thumbnail attachments and currently
unsupported RICE/random-groups layouts. All 14 committed fixtures match the copies
on the mechanical drive byte-for-byte; their identities are in the audit.

The [large FITS extension](large-fits/) uses three 714.29 MiB files, one pass and
five repetitions per worker count. Median times are 116.66 / 80.65 / 47.55 ms at
1/2/4 configured workers; with three files, at most three validations are active.
Peak RSS is 6.33 MiB. This is strong evidence of streamed I/O for this layout,
not a hard memory ceiling for arbitrary formats or validation of 64 GiB files.

## Reproduction, verification and review gates

The [suite](../../../astro-bench/scripts/benchmark_suite.py) has `scaling`,
`contention`, `cancellation` and macOS `profile` modes. The
[capture matrix](../../../astro-bench/scripts/capture_matrix.py) accepts an explicit
capture directory, `--recursive`, `--per-format 3` and `--allow-rejected` for
intentional diagnostic runs. Always use a new output directory. See the
[crate commands/resource contract](../../../astro-bench/README.md) and
[execution plan](../../BenchmarkCompletion.md).

For the large-file extension, use the capture probe on the first three sorted
FITS files in the supplied `NGC7000/app` directory, `full`, one pass, workers 1/2/4,
five fresh processes each. Run `fingerprint_reads` before those samples, as in the
capture matrix. CFITSIO checks used read-only `ffopen`, `ffthdu`, `ffmahd`, `ffvcks`
and `ffclos` from `/opt/homebrew/lib/libcfitsio.4.7.0.dylib`; reported statuses are
`−1` invalid, `0` absent, `+1` valid. The independent checks were outside timing.

Verified: 123 workspace all-target tests, four doctests, nine Python tests,
formatting, Clippy with warnings denied, Rust documentation with warnings denied,
and release workspace/example builds. One existing SEP test remains ignored.
Final auditing checked report/file/operation counts, full-success checksum/codec
coverage, source/fixture fingerprints, memory reservations and cancellation/proxy
targets. Source hashing is independent of Git revision labels. Profiling,
failed attempts and rejected samples are explicitly separated from accepted timing.

Remaining release evidence: Windows/Linux execution, network/rotating-storage
scenarios beyond the supplied HDD, controlled cold-cache work, low-memory/swap
pressure, actual foreground application switching, other producers/codecs and
maximum-size files. The user has no available Windows/Linux or network environment.
Automatic-mode comparisons require the future scheduler; native allocation
controls for excluded compressed-FITS layouts remain implementation work.

These gaps do not invalidate the recorded local measurements. They prevent treating
this result as universal calibration or monitoring-release approval. The benchmark
changes are ready for user review; no merge, release or defaults change is included.
