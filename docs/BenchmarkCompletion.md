# Validator benchmark completion

## Objective

Finish the remaining measurement work for the current bounded validator: explain
row-tile scaling, extend size/tile/worker coverage, and measure cancellation and
contention. Preserve raw evidence and independent rollback commits. Merge remains
subject to the user's review.

## Constraints and non-goals

Keep production validation and application defaults unchanged while measuring.
Other native codec implementations, automatic scheduling and the calibration UI
are separate implementation steps. Existing fixtures and report schema stay
unchanged. No system-wide cache flushing, deliberate exhaustion of RAM, or writes
to real capture files. Missing platforms/storage/captures remain explicit gates.

## Current state

`79da7f8` contains the 216-sample GZIP baseline. Whole-image tiles scale well, but
many row-tile cases regress from two to four workers. FITS descriptor inspection
performs small seek/read operations for each row, and GZIP initializes decoding
per tile. These are hypotheses to investigate with CPU/wall ratios, tile-height
comparisons and sampled stacks; code inspection alone does not prove causation.

## Proposed design and resource model

Add standard-library Python orchestration around the existing release CLI rather
than a second validation runner. Keep native Rust provenance, fresh sample
processes, quota ownership and shared 512 MiB validation accounting. New tools
validate reports before summarizing; timing assertions remain outside unit tests.

Scaling covers GZIP_1/GZIP_2, row/intermediate/whole-image tiles, noise/gradient,
8/64 MiB images, 1/2/4/8 workers and five repetitions. Reverse worker order in a
second round to expose order/load effects. Generation stays single-threaded;
validation uses at most eight outer workers and no internal codec threads. Run
one CLI at a time except in the explicitly labeled cross-process contention test.

Use a separate lightweight timed SHA-256/sleep probe to record scheduling delay
and short-task latency during idle and benchmark runs. This is an automated
responsiveness proxy, not a GUI/application-switching test. Record its raw bounded
samples and identify generation versus measurement phases. Run bounded competing
CPU/memory traffic separately; do not equate it with OS low-memory pressure.
Exercise SIGINT during preparation and active child execution, recording elapsed
exit time, failure/no-report status, child reaping and owned scratch cleanup.

The orchestration retains only bounded probe/report data, owns only its launched
children and scratch directories, and has finite deadlines. Record script hash as
well as the Rust source fingerprint. Profiling runs are separate from timing runs
because sampling perturbs execution. macOS sampling is optional on other systems.

## Affected areas and execution

1. Add tested orchestration/report auditing and document commands; commit tooling.
2. Build release, capture scaling and independent profiles without parallel builds.
3. Capture idle/contended responsiveness and cancellation; audit all results.
4. Record conclusions, available external evidence and remaining gates; commit
   evidence separately. Do not merge or change release defaults.

## Verification and acceptance

Test rejection of incomplete/mixed-source reports, fixture mismatch and invalid
matrix settings. Check script syntax, local links and existing CLI regressions.
Run Rust checks if Rust changes become necessary. Verify exact file/byte counts,
successful exits and reservation bounds for every timing sample.

Predeclared local investigation targets: full-validation peak RSS below 64 MiB
at up to eight workers; SIGINT exit/cleanup within two seconds on local storage;
responsiveness-proxy p95 wake delay below 20 ms and p95 short-task latency below
twice the idle value plus 2 ms. Failures remain evidence, not reasons to discard
samples. No universal throughput or release-default threshold is inferred.

## Risks and open evidence

Warm caches, short samples, CPU scheduling, power mode and desktop load can affect
results. Preserve forward/reverse rounds and all outliers. Larger fixtures remain
bounded by the harness's 64 MiB/image limit and do not prove 64 GiB-file behavior.
Real captures, remote/rotating storage and Windows/Linux environments have been
requested from the user; perform those checks if provided. Otherwise document the
exact missing evidence and reproducible commands without claiming completion of
release qualification. Automatic-mode comparisons require the future scheduler.

## Capture measurement extension

Six existing local captures were found in ignored `tests/data`. Add a read-only
`capture_probe` example to measure arbitrary explicitly selected inputs without
inventing synthetic recipes or copying/modifying captures. This diagnostic runner
keeps its results separate from synthetic schema-one reports and identifies its
own source hash in addition to the existing measured-source fingerprint.

Use up to eight workers and one shared 512 MiB budget, with per-call limits of
`min(256 MiB, 512 MiB / workers)`. The native paths excluded by shared admission
remain excluded. Bound inputs to 256 files / 8 GiB stored total, passes to 256,
and retained operation records to 4,096. Hash inputs in 64 KiB chunks before and
after timing; reject final-component symlinks and duplicate canonical paths.
Do not include file names or capture metadata in committed reports. Hash equality
establishes byte preservation, not an immutable snapshot or protection against
hostile directory mutation. File I/O remains read-only.

A bounded atomic work index distributes mixed formats/sizes without serializing
all validation. Worker results are bounded by the operation cap, joined on every
exit, and include error kinds. Failed/unsupported inputs must produce a failed
sample, never apparently successful throughput. Reuse existing Unix telemetry;
record unavailable platform telemetry as null. No production API changes.

First test mixed-format repeated accounting/source preservation, invalid-input
diagnostics, resource/work limits and symlink/duplicate rejection. Build/test only
between timing suites. Run the six real captures alone and mixed in fresh
processes; retain unsupported-layout results separately from successful timings.
Additional untracked test fixtures will be kept local unless the user authorizes
committing them.

The user subsequently approved committing the 14 local format fixtures and
provided `/Volumes/WD_ElementsHD/RavenSkyTestFiles` (mechanical HDD) and
`/Volumes/ap_projects/0Working` (external SSD). They have no available network
storage or Windows/Linux test environment. Capture matrices will select bounded
FITS/XISF groups from these volumes, including images beyond the synthetic 64 MiB
limit. Record sequential first-observed/repeated read-plus-SHA-256 times before
the validation matrices; cache state remains uncontrolled, not guaranteed cold.
These larger capture observations are exploratory: the 64 MiB RSS target above
applies to the synthetic GZIP matrix, not an invented universal bound for every
XISF codec/window or capture layout.
