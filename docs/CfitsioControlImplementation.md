# CFITSIO controls

Current follow-up: [managed compressed-FITS validation](CompressedFitsResourceImplementation.md)
replaces the validator's remaining native fallback. The limitations below record
this earlier checkpoint; existing native image loaders remain outside the managed
validator's allocation contract.

Status: concurrency implementation complete and locally verified on macOS after
checkpoint `eb06530`. Native allocation limits remain a separate open step.

Follow-up: [bounded FITS GZIP validation](BoundedFitsGzipImplementation.md) now
avoids native decoding for a precisely defined integer GZIP profile. Other native
codecs/layouts and native image loaders still have the allocation limitations below.

## Objective

Close the native concurrency gap across FITS loading, metadata extraction and
standalone validation. Audit native allocation behavior before enabling shared
full validation of compressed FITS.

## Constraints and current state

The shared validator rejects native compressed-image decoding. Standalone full
validation uses estimated reservations. Existing loaders and metadata helpers
call CFITSIO independently, including public helpers accepting caller-owned
`FitsFile` handles. Preserve these interfaces and useful parallelism. No monitor
scheduler, native library fork or automatic calibration is included here.

## Design and resource model

Add `astro_io::fits::backend` with a single backend access gate per linked copy of
astro-io. Query the linked CFITSIO's `fits_is_reentrant` once. Reentrant builds
enter directly; other builds admit one thread, allowing nested calls by that same
thread. A closure API keeps the permit alive through error handling and handle
destruction. Loaders use blocking admission; validation uses nonblocking admission
and returns `ResourceBusy` so scheduler workers can requeue. No worker pool or job
queue is introduced. Native calls themselves remain non-interruptible.

Route all production native entry points in astro-io and astro-metadata through
this gate. Expose the same closure API for callers opening/closing their own raw
handles. Such callers must enclose the entire handle lifetime; arbitrary direct
fitsio calls or independently linked copies cannot be protected by this gate.
Reentrant CFITSIO still requires separate file handles for concurrent readers.

Memory reservations are unchanged in this increment. Native GZIP uses unbounded
`realloc` during inflation and tile caches can retain multiple column bins.
Therefore neither the standalone estimate nor serialization establishes a native
allocation ceiling. Keep shared full native decoding explicitly unsupported.

## Affected areas and execution

1. Add deterministic gate tests: nonblocking contention, nested calls, permit
   release on unwind, waiting callers, and simultaneous reentrant callers.
2. Implement the gate and wrap FITS path/handle helpers, metadata helpers and
   the standalone native validation stage, including open/close and errors.
3. Add a concurrent cross-crate fixture test, document the caller protocol and
   native allocation findings, and verify the workspace.

## Verification

Run targeted tests first, then formatting, Clippy with warnings denied, workspace
tests/doctests, docs and release build. Forced gate modes test the serial fallback
on any host; actual Windows/Linux backend behavior remains a platform gate.
Reentrant overlap is tested with barriers rather than timing thresholds. The
existing streaming benchmark does not exercise compressed FITS and is not rerun
as evidence for native controls. Native throughput/RSS, concurrent loader overlap,
remote cancellation and foreground responsiveness need representative compressed
FITS workloads before release targets/defaults can be selected.

## Risks and remaining allocation decision

Locks coordinate participating callers only. Caller-owned handles escaping the
closure, independently linked copies and direct CFITSIO use need application
coordination. Recovery from a panic releases admission; it does not repair native
state corrupted by foreign code. Pure Rust structural/XISF validation never takes
this native gate.

Before shared full FITS decoding is enabled, either introduce enforceable native
allocation hooks and audit every codec/cache, or move decoding to an isolated
worker with suitable platform resource controls. A library-wide estimated ledger
alone cannot stop a malformed compressed stream from expanding inside CFITSIO.
That choice and compressed-FITS benchmark coverage remain the next allocation
step; this increment resolves cross-caller concurrency only.

## Native source audit

Audited the actual Cargo registry dependency: `fitsio-sys 0.5.7`, whose bundled
`ext/cfitsio/fitsio.h:37` identifies CFITSIO **3.49**. This is distinct from the
current upstream version. Re-audit these internals when changing the dependency.

- `build.rs:78` enables reentrancy for the Autotools build; the CMake path requests
  pthreads at line 90. Detection uses the linked function rather than assuming
  build flags or operating system prove the capability.
- `ext/cfitsio/fitscore.c:7322`: `fits_is_reentrant` returns the compiled constant
  without accessing mutable state. The official
  [threading contract](https://heasarc.gsfc.nasa.gov/docs/software/fitsio/c/c_user/node15.html)
  requires independent handles for concurrent readers.
- `ext/cfitsio/imcompress.c:5896`: cache arrays are sized by the number of tile
  columns. Lines 6851–6900 allocate and retain decoded data per cache bin. A
  one-tile estimate does not cover all simultaneously retained tiles.
- `ext/cfitsio/imcompress.c:6334`: GZIP passes `realloc` into
  `uncompress2mem_from_mem`, before checking the decoded byte count against the
  expected tile shape. There is no reservation callback or maximum-output argument
  at this call site. Increasing an estimated budget cannot enforce that boundary.
- `ext/cfitsio/zlib/zcompress.c:182`: the inflation helper grows output by
  `BUFFINCR` through the supplied realloc function until stream completion; its
  `buffsize` argument describes current capacity, not a maximum.

All production CFITSIO entry points found in the workspace's shared crates are
`astro-io/src/fits.rs`, `astro-io/src/validation/fits.rs`, and
`astro-metadata/src/fits_parser.rs`. Private raw-card helpers run inside their
public caller's admission scope. Test fixture writers now follow the same protocol.

## Verification result

The exclusion test first failed with unguarded admission, then passed with the
gate. Four deterministic gate tests cover forced serial/reentrant modes, nesting,
waiting and panic cleanup. The cross-crate test runs 80 operations over four
workers: image loading, path-based metadata, caller-owned handle helpers and
full native GZIP FITS validation, all using independent handles. Existing native
codec and checksum fixtures also pass.

Passed locally: 105 workspace all-target tests, four doctests, formatting, Clippy
with warnings denied, documentation with warnings denied and release build. One
preexisting SEP test remains ignored. Git whitespace checks passed in both repos.
AstroMuninn changes only record integration requirements; its runtime dependency
and monitoring behavior have not changed. Native allocation/RSS bounds, actual
Windows/Linux execution and performance under foreground contention are unverified.

Subsequent [benchmark completion work](BenchmarkCompletion.md) records bounded
GZIP scaling/profiles, real captures on three volumes, CLI cancellation and an
automated responsiveness proxy under bounded contention. It does not establish
native allocation ceilings, Windows/Linux behavior or actual GUI/pressure response.
