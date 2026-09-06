# Benchmark and Calibration Design

Status: first foundation implemented and locally verified on `feature/file-validation`.
The [initial baseline](benchmarks/2026-09-06-m4-max/README.md) contains 216 successful
release samples with raw reports and measured limitations.
This increment establishes a validator baseline; it does not change validation,
implement application calibration, or select release resource defaults.

## Objective
Provide reproducible synthetic workloads and structured measurements reusable by
RavenSky crates and applications. Separate measurements from application policy:
apps decide which settings to recommend, preview, apply and reset.

## Scope and architecture
`astro-bench` is an opt-in workspace crate with a library and CLI; production
crates do not depend on it. The initial library generates fixtures and runs one
bounded serial/fixed-concurrency sample with cooperative cancellation and progress.
The CLI generates once, then starts a fresh child process for each repeated sample
so process-lifetime peak RSS excludes fixture generation and previous samples.
CLI children execute the same library path an application can call. In-process
applications must label RSS as process-lifetime, not per-workload allocation.

Initial workloads: sequential raw I/O, structural validation and full validation
of ordinary FITS, raw XISF, zlib XISF and Zstandard XISF. Fixtures are UInt16
monochrome images with seeded noise or a smooth gradient. Later increments add
LZ4/tiled FITS, multiple images/auxiliary blocks, metadata and scientific metrics,
then application workflow/calibration integration. Do not claim initial synthetic
coverage establishes real-producer compatibility or all native decoder behavior.
No generic plugin framework or dependency from the umbrella crate is needed yet.

## Fixture contract
Version the recipe and report schema independently. Record seed, dimensions,
pattern, codec, counts, stored/decoded sizes and SHA-256 fingerprints. Generate
in bounded chunks into an exclusively created scratch subdirectory; never use
sparse/preallocated zero files as a storage-throughput workload. Stream zlib/zstd
encoding; bound output writes with a configured disk quota. Finish and sync files,
then validate them and hash their bytes before timing. Fixture preparation time
is separate. Identical recipes reproduce pixel bytes; compressed encodings may
change with backend versions, so retain both recipe and content fingerprints.

Use only generated fixtures for this increment. A generated set owns only its
unique directory and removes it on normal completion/error/cancellation. Explicit
cleanup reports failures; abrupt process/OS termination can leave a clearly named
scratch directory. Child processes read the owned manifest and image files, never
clean up parent ownership. Reject traversal, symlinks and malformed manifests.

## Execution and resource model
Bound frames, workers, repeats, per-image decoded bytes, scratch disk and elapsed
sample time. Defaults are intentionally small; reject excessive configuration
before generation. Parallelism is explicitly requested; the runner must not
silently tune it or oversubscribe beyond its configured worker count. This is a
measurement harness, not the future adaptive scheduler. The first fixtures avoid
CFITSIO paths, making native FITS reentrancy irrelevant to these samples.
Before adding native workloads, enforce their verified capability restrictions.

Use fixed worker threads and an atomic work index, with no unbounded task queue.
Retain bounded per-file timings/results. Join workers on errors and cancellation;
failed or partial samples must not be counted as successful throughput. Library
callbacks execute on the coordinating caller thread. CLI interrupt/timeout stops
and reaps the current read-only child before removing owned fixtures. Normal
library cancellation remains cooperative and cannot interrupt a blocked OS read.

Measure wall time (including thread start/join), per-file duration, successful
bytes/files, validator-managed read counts and Unix CPU-time deltas. Report peak
process RSS on macOS/Linux with explicit units/provenance; use null elsewhere.
RSS includes runtime/native allocations, not filesystem cache, and is not a hard
memory budget or an allocation ledger. Retain raw repetitions plus median/min/max
summaries; never merge structural/full or different workloads into one score.

## Provenance and cache interpretation
Report OS/architecture, available parallelism, package version, build profile,
compiler, user-supplied environment notes, recipe/fingerprints and validator limits.
Record source revision and a source-content fingerprint separately for dirty builds.
Preparation and verification touch the files; label cache state uncontrolled /
likely warm. Fresh processes reset RSS, not the OS page cache. No privileged
cache flushing. Small runs measure warm data and overhead; larger data/storage
matrices are required before drawing disk or responsiveness conclusions.

## Calibration extension
Expose measurement reports and progress/cancellation to applications without
writing their config. Apps select scratch volumes and workload groups, display
budgets and recommended settings, and apply only user-approved performance caps.
Keep format/readiness/file-safety settings outside calibration. Match profiles to
build/workload/hardware/storage context, retain uncertainty and reject noisy or
incomplete calibration. Later automatic runtime scheduling must still back off
under pressure; calibration is not a permanent entitlement to machine resources.
Evaluate acceleration already present in dependencies before adding SIMD/GPU paths.

## Execution and verification
1. Add failing tests for deterministic fixture content, bounded generation,
   format validity, manifest rejection, cancellation, measured-file accounting,
   worker limits, cleanup and CLI report behavior.
2. Implement generation, the reusable runner and versioned reports, then CLI
   isolation and interrupts. Reuse existing codecs/hash/serde dependencies;
   `ctrlc` supplies portable CLI signal handling and `libc` supplies narrow Unix
   resource measurement. No signal handler is installed by the library.
3. Run fmt, Clippy, workspace tests/docs and release build. Capture repeated
   serial/fixed-concurrency results before editing validator behavior.
4. Record baseline limits and findings; use the same workload recipes to compare
   reservations/streaming changes. No numerical release defaults are justified
   by the initial local run alone.

Remaining evidence: native decoder allocation peaks, Windows/Linux execution,
real captures, remote/rotating storage, large/mixed workloads and foreground
responsiveness under contention. Keep noisy performance comparisons outside unit
tests. The workspace Performance and Resource Design policy governs rollout.

Implemented bounds and CLI/library usage are in the [crate README](../astro-bench/README.md).
Current limits are 256 frames, 64 MiB decoded/image, 16 workers, 20 repetitions,
8 GiB maximum scratch quota and a conservative 512 MiB aggregate admission
estimate. These are initial harness bounds, not release application defaults or
hard process-memory guarantees. Shared reservations and streaming are now
implemented; the [comparison](benchmarks/2026-09-06-m4-max-streaming/README.md)
records measured memory reductions and throughput. Application calibration,
native controls and broader workload/platform evidence remain separate work.
