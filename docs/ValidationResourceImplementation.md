# Shared validator memory and streaming implementation

Status: implemented and locally verified after checkpoint `b513716`.
The [216-sample comparison](benchmarks/2026-09-06-m4-max-streaming/README.md)
passes the initial memory/throughput targets. Consumer/native integration remains
separate; no release defaults are selected by this result.

Follow-up: [CFITSIO controls](CfitsioControlImplementation.md) now coordinate
native concurrency across loaders and metadata helpers. The historical scope below
describes the streaming checkpoint; native allocation limits remain unresolved.

## Objective and scope
Add caller-owned nonblocking memory admission to the existing validator, and
stream XISF zlib/Zstandard payloads without retaining decoded images. Preserve
standalone entry points, validation levels, checksum-before-decode, exact extents,
source observations and cancellation. No monitor scheduler, config editor,
automatic tuning, release, or unrelated loader refactor is part of this increment.

## Design and affected components
- `validation/resources.rs`: cloneable shared byte budget, atomic reservations,
  live/peak counters and RAII release. Capacity contention is `ResourceBusy`;
  requirements exceeding a free budget or a per-call limit are `ResourceLimit`.
  All acquisition is nonblocking. Failure unwinds partial stages and releases
  resources before the caller retries, avoiding hold-and-wait deadlocks.
- `validation/mod.rs`: additive `validate_file_with_budget`, per-call accounting,
  reserved fallible I/O buffers and report telemetry. Standalone calls use a
  private budget. Reservations account buffers and explicit parser/backend
  allowances; they are not OS reservations or hard process-RSS ceilings.
- XISF: reserve XML/parser growth before allocation, retain reservations for
  metadata/inline lifetimes, borrow inline subblocks, use bounded attached readers
  and discarded output chunks. Zstandard frames are admitted individually using
  declared history requirements, including concatenated/skippable frames. LZ4
  retains input/output blocks under explicit reservations. Keep whole-file final
  reads initially so padding/gap coverage cannot regress.
- FITS: reserve header/table bookkeeping before growth and bound native stages.
  The shared-budget entry point rejects full native CFITSIO decoding because its
  allocation quota/exclusivity is not enforced across existing loaders. Standalone
  compatibility retains the existing estimated native path and restrictions.
- `astro-bench`: exercise concurrent calls through one shared budget and report
  reserved-byte telemetry, while retaining the existing recipes for comparison.

Parser and codec internals can allocate infallibly; conservative allowances and
input/history limits bound admitted work without promising recovery from arbitrary
OS/native allocation failures. No hidden thread pool or waiting queue is added.
The caller owns fairness/retry policy. Each validation uses one outer worker;
streaming codecs do not start codec worker threads. Accelerator changes are deferred
until memory/streaming effects have been measured independently.

## Sequence and verification
1. Failing tests for admission, busy versus impossible, concurrent callers, release
   on success/error/cancellation/unwinding, and bounded streaming of large output.
2. Implement reservations and per-stage integration, then streaming/exactness tests
   for trailing bytes, truncated streams, output mismatch and frame history limits.
3. Run workspace formatting, Clippy, tests/doctests, docs and release builds.
4. Repeat the 24-configuration baseline (216 samples), preserve raw measurements
   and compare identical fixture hashes. Initial local acceptance targets: at least
   60% less peak RSS for noisy 32 MiB compressed images at four workers; no more
   than 15% median full-validation slowdown for those workloads. Investigate
   larger regressions; sub-millisecond structural differences need absolute-time
   context (target less than 0.5 ms extra per eight-file set), not timing unit tests.

Windows/Linux, real captures, remote I/O and foreground responsiveness remain
release gates. These local targets do not select application defaults. Native
CFITSIO accounting/exclusivity needs coordinated loader work before shared full
native decoding can be enabled.

## Resource audit and implemented contract

`MemoryBudget` is a cloneable atomic byte ledger, not allocated backing memory.
`try_reserve` returns an owned guard; a per-call `Account` checks both local and
shared capacity. Stage growth never waits. A busy/error path drops buffers and
guards before returning, so the caller can requeue without retaining partial work.
No public prepared-file cache is introduced yet; layout discovery and stage
admission happen within each call. An application may reserve its own buffers
against the same budget. It must account returned reports and pending-job state.

Managed byte buffers use fallible `try_reserve_exact`, retain their reservation
alongside the allocation, and drop the allocation first. XML parser scratch has
a retained 32× header-length allowance; nodes/attributes and FITS structural
keywords add 1 KiB allowances before insertion. These cover collection/string
growth and overlap conservatively rather than intercepting every allocator call.
Text/node/child/descriptor-vector growth uses fallible reservations where exposed;
quick-xml, BTreeMap, small formatting/Arc and codec internals can still allocate
infallibly. Large metadata may hit the working limit earlier than the old estimates.

Zlib uses `flate2::Decompress` and requires `Status::StreamEnd`, exact output and
consumed input. Its 64 KiB input/output buffers and 1 MiB state allowance stay live
through the codec call. Audited flate2 1.1.9's Rust backend boxes miniz inflater
state; it does not create decoder threads. Zstandard reads frame headers before
native creation, reserves rounded frame history plus 1 MiB context/block overhead,
and sets the native window limit. The vendored 1.5.7 decoder's
`ZSTD_estimateDStreamSize` comprises context, one input block and a history ring
with two extra blocks; blocks are at most 128 KiB. The allowance covers this
audited shape, but is not an allocator hook. Re-audit when changing backends.

Format reference: [Zstandard 1.5.7 frame format](https://github.com/facebook/zstd/blob/v1.5.7/doc/zstd_compression_format.md).
Backend reference: [Zstandard decoder sizing](https://github.com/facebook/zstd/blob/v1.5.7/lib/decompress/zstd_decompress.c).
Each concatenated/skippable frame is admitted separately and counted against the
structure limit. LZ4 retains complete blocks; CFITSIO shared full decoding is
explicitly unavailable until allocation/exclusivity controls cover existing loaders.
No new native thread or acceleration behavior was enabled.

## Verification result

100 workspace all-target tests, four doctests, formatting, Clippy with warnings
denied, docs with warnings denied and release build passed on macOS. One preexisting
SEP test remains ignored. Deterministic tests include live reservation cleanup,
atomic contention, malformed/truncated/trailing streams, decoded-size mismatch,
small-budget 8 MiB output, concatenated/skippable frames, history rejection and
the native FITS gate. Whole-file reads, checksum-before-decode and source-change
checks remain in place.
Release CLI SIGINT checks during preparation and measurement also passed, with
cancellation diagnostics and no report or scratch remnants. Blocking remote I/O
cancellation latency remains unmeasured.

Compared with identical baseline fixture hashes, four-worker noisy 32 MiB zlib
RSS fell from 261.6 to 7.6 MiB and Zstandard from 272.5 to 16.3 MiB; median sample
times fell 4.3% and 8.6%. Full raw/FITS results stayed close to baseline. See the
comparison for all worker counts, variance, reservation telemetry and limits.
This establishes the initial shared-buffer/streaming increment, not release
readiness or foreground responsiveness under contention.
