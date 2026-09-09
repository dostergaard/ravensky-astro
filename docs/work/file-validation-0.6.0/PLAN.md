# File validation and benchmarks — authoritative retrospective plan

This records the settled design approved for 0.6.0 on 2026-09-09. Earlier
implementation documents describe useful checkpoints, not competing current
contracts. [HANDOFF.md](HANDOFF.md) owns release state and downstream actions;
[EVIDENCE.md](EVIDENCE.md) distinguishes observations from guarantees.

Closeout result: the original 0.6.0 documentation failure was repaired by the
coordinated 0.6.1 patch release. Local, CI, archive, registry-only consumer and
actual docs.rs verification all passed. `v0.6.1` and its GitHub release identify
published source commit `4bc4660`. RavenSky is ready for AstroMuninn integration;
no AstroMuninn implementation was started during this closeout.

## 0.6.1 hosted-documentation repair

The completed repair uses a coordinated 0.6.1 patch release. Each
publishable manifest supplies docs.rs metadata that passes
`fitsio/src-cmake` directly to Cargo and builds only the supported Linux GNU
documentation target. `astro-metrics` and the facade add configuration-only
direct `fitsio` dependency edges because Cargo cannot select a transitive
dependency feature from those package roots. The dependency and its
`fitsio-src` feature were already present transitively, so ordinary resolution
and backend selection remain unchanged.

Do not replace this with a RavenSky documentation feature: Cargo's
`--all-features` would then select CMake for ordinary validation builds and could
forward a feature that AstroMuninn's patched `fitsio-sys 0.5.5` does not expose.
The direct-selector metadata is intentionally outside RavenSky's feature surface.
Published 0.6.0 manifests cannot be changed, and no currently released upstream
version removes the failing autotools source write, so 0.6.1 is the required
deployment vehicle.

## Problem, goals and boundaries

AstroMuninn's separate CLI/Lite monitor loops inferred readiness from unchanged
size/mtime alone. A producer paused mid-write could pass that heuristic. The shared
crates needed independent structural completeness checks and optional full
validation, efficient enough for concurrent background use. The original
validator then needed enforceable managed working-memory admission instead of
whole-image buffers and estimated native compressed-FITS allocations.

Provide general-purpose, read-only FITS/XISF validation, typed failures,
cooperative cancellation, source observations and reusable benchmark primitives.
Keep correctness, bounded resource use, throughput and responsiveness visible in
design and verification. Serial processing is a diagnostic baseline, not a
universal release restriction.

Non-goals: producer locking, proof that acquisition has finished forever,
scientific image-quality assessment, image rendering, automatic application
calibration, monitor scheduling, GUI configuration, file moves/copies, metadata
normalization changes, licensing changes or a whole-process/OS memory quota.
This closeout must not implement the AstroMuninn feature.

## Settled API and behavior

The authoritative public interface is
[`astro_io::validation`](../../../astro-io/src/validation/mod.rs), with the
[coverage contract](../../../astro-io/README.md#file-validation).

| Area | Contract |
| --- | --- |
| Entry points | `validate_file` creates a private allowance; `validate_file_with_budget` accepts a caller-owned shared `MemoryBudget`. Both use the same validation implementation. |
| Default level | `ValidationOptions::default()` selects `Structural`; format is detected from bytes, not filename suffixes. |
| Structural FITS | Check mandatory headers, checked dimensions/byte lengths, padded HDU traversal, table columns/heap descriptors and tiled-image extents. Record undecompressed codecs. |
| Structural XISF | Check prefix/XML, geometry/sample formats, local references and declared block/subblock extents. Bound parsing and reject external/unsafe XML constructs. |
| Full | Read all physical bytes including gaps/padding, decode supported compressed payloads, require exact extents/output, and verify declared checksums. Never silently downgrade to structural success. |
| Checksums | FITS DATASUM/CHECKSUM over stored data/HDU; XISF SHA-1/SHA-2/SHA-3 over stored block bytes. Missing optional checksums remain legal and visible in the report. |
| Success report | Completed level, `FileStamp`, image/structure counts, managed I/O bytes, checksum coverage, undecoded codec information and peak reservations. |
| Errors | `Incomplete`, `InvalidStructure`, `IntegrityMismatch`, `Unsupported`, `ResourceLimit`, `ResourceBusy`, `ChangedDuringValidation`, `Cancelled`, `Io`. The enum is non-exhaustive. |

Cancellation uses the caller's optional `AtomicBool`, checked between bounded
reads, allocations/stages and managed codec work. It stops validation cooperatively;
it does not interrupt a blocking OS read instantly. Owned reservations unwind on
success, error, cancellation and panic. A callback is not required to run a worker.

Source observations compare length, mtime and available identity information
before/after reads and against the path. The public stamp is returned for a
consumer recheck before using a result. This does not close the race with a writer
after validation or guarantee detecting an adversarial same-stamp rewrite.
Validators never repair or rewrite input. Benchmark probes additionally hash
sources before/after measurement and distinguish rejected samples from throughput.

## Resource and concurrency design

[`resources.rs`](../../../astro-io/src/validation/resources.rs) provides cloneable
shared accounting and non-cloneable lifetime-owned reservations. `try_reserve`
never waits. `ResourceBusy` means active reservations temporarily occupy capacity;
`ResourceLimit` means the requirement cannot fit the call/shared allowance even
without other callers. The application must release/requeue outside worker slots;
no validator waits while holding a partial reservation.

Defaults are 64 MiB accumulated header bytes, 256 MiB working allowance,
100,000 structures and 64 GiB accumulated declared decoded data. **64 GiB is a
data-volume limit, not a RAM allocation or a physical-file-size maximum.** Budgets
do not allocate their capacity. Managed buffers are admitted before fallible
allocation. Parser/codec overhead has conservative allowances; some XML/map/codec
internals still allocate infallibly. Accounting is not RSS, available-memory
telemetry, or an OS reservation.

File I/O uses at most 64 KiB chunks. XISF zlib/Zstandard streams discard output;
Zstandard frame history is admitted and its native window limit enforced. Inline
storage and raw LZ4/LZ4HC blocks retain admitted buffers where indivisible decoding
requires them. XML allowances account for parser/string overlap and nodes.

One validation call uses its caller worker, without a hidden worker pool. The
application coordinates CPU, memory, storage and bounded queues across sessions;
it owns fairness, pressure feedback, QoS, source/collision ordering and policy.
No GPU/SIMD layer was added speculatively. Existing codec support and measured
end-to-end concurrency were used; portable behavior remains the contract.

### Compressed-FITS strategy

All supported full-validation layouts use managed decoding. There is no CFITSIO
validation fallback, including for standalone calls.

| Codec/layout | Implementation and working-set contract |
| --- | --- |
| GZIP_1/GZIP_2 | Stream stored input/output, verify CRC/ISIZE, exact decoded length and stream end; optional headers capped at 64 KiB. Reversible shuffle needs no rendered pixel array. |
| Rice | Checked bit/unary codes for BYTEPIX 1/2/4 and bounded coding blocks; stream differences without retaining pixels; exact consumption/padding. BYTEPIX 8 unsupported. |
| PLIO | Checked line-list headers/opcodes and 24-bit nonnegative values; count runs within tile extent without pixel allocation; unwritten tail is implicitly zero. |
| HCOMPRESS | Adapted managed 32/64-bit coefficient/inverse transform. Preflight geometry/bitplanes/scale, reserve stored bytes plus `16 × tile_pixels + 64 KiB`, allocate fallibly and check cancellation. Recheck buffered header against admission observation. |
| Table profiles | P/Q descriptors, named/reordered columns, edge tiles, quantized Float32/64, GZIP/raw fallback columns and GZIP/Rice/PLIO null masks. Exactly one primary/fallback payload per row. |

Quantization metadata/seeds and the encoded integer representation are validated;
the API does not render dequantized pixels or optional HCOMPRESS smoothing, or
judge lossy scientific fidelity. Unknown extensions and transformed/nullable
compressed table columns fail explicitly. Present FITS checksums precede payload
decoding; declared-size preflight still precedes checksum I/O. See the complete
[codec support table](../../../astro-io/README.md#compressed-fits-coverage).

## Compatibility and ownership

The APIs are additive. Published crate versions advance together from 0.5.0 to
0.6.0 under the existing workspace relationship. Loaders, metadata precedence,
coordinate behavior and pixel-return contracts remain unchanged. XISF validation
supports more formats/channels than the intentionally narrower existing loader;
do not substitute the loader for the readiness API.

CFITSIO remains in loaders and metadata helpers. Its
[`fits::backend`](../../../astro-io/src/fits/backend.rs) gate detects reentrancy:
independent handles can run concurrently on reentrant builds; non-reentrant builds
serialize participating native callers. Direct fitsio callers must cover open,
operations and drop with the public closure protocol. Validation does not acquire
this gate. Native loader arrays/caches are outside validator resource accounting.

Publish in order: `astro-io` → `astro-metadata` → `astro-metrics` → `ravensky-astro`.
All require 0.6.0 internal dependencies; Rust minimum remains 1.94. `astro-bench`
inherits 0.6.0 locally but `publish = false`; consumers clone the workspace to use
its library/CLI. No benchmark dependency is introduced into production crates.

## Historical decisions and superseded approaches

- `b513716`: initial validator/benchmark foundation based on published `dbf4bba`.
- `eb06530`: caller-owned reservations and XISF streaming replaced large buffers.
- `348aceb`: coordinated existing CFITSIO callers; native allocation estimates
  still were not a hard bound at this checkpoint.
- `28979fc` / `79da7f8`: bounded integer GZIP and matching synthetic benchmark
  recipes established the first managed compressed-FITS subset.
- `13ab8c6` through `c9c047d`: scaling, contention, real captures, rejected-sample
  records, preserved binary fixtures and standalone usage guide.
- `fd39899`: managed Rice/PLIO/HCOMPRESS and general tile dispatch removed the
  remaining estimated native validation path. `603c8c4` retained its measurements.
- `6237264` through `2e9ba95`: versions/package/CI preparation, explicit Windows
  scope, local-fixture opt-in and native-handle test cleanup.

Direct native Rice/PLIO/HCOMPRESS decode was rejected because low-level input
bounds were incomplete or absent. Allocator interposition would couple consumers
to backend builds; process isolation would require portable worker distribution
and OS-specific controls. Audited `ricecomp 0.5.0`, `hcompress 0.4.0` and
`fitskit 0.3.0` were not drop-in validators: allocation, malformed-input, exact
consumption and cancellation contracts needed changes. HCOMPRESS algorithm code
was adapted with upstream notices retained in `astro-io/licenses/`.

The bundled CFITSIO 3.49 PLIO fixture encoder overran its estimated output for
high-delta positive 32-bit data. Native fixtures now use mask-like values with
safe capacity margin; malformed instruction streams are hand-built. Production
validation never calls that encoder. Details and references are retained in
[the implementation audit](../../CompressedFitsResourceImplementation.md).

## Verification and platform constraints

Use deterministic tests for resource lifetime/contention, read-only behavior,
source changes, cancellation, malformed/truncated streams and supported profiles.
Use native-generated fixtures and pixel parity where codecs were adapted.
Keep noisy timing experiments separate. Repeated serial/fixed-worker matrices,
fresh child-process RSS, source hashes and SIGINT observations establish local
evidence; automatic scheduling/foreground switching needs later product evidence.

Linux GNU and macOS ARM64 CI cover the complete workspace; Windows GNU covers
I/O, metadata and benchmarks. Existing `sep-sys 1.3.0` uses POSIX `rand_r`, so
metrics/facade Windows compilation remains unsupported. Windows MSVC has no
new library CI evidence; AstroMuninn's patched backend must be tested in actual
product builds. No network-storage, induced OS-memory-pressure or universal
64-GiB-file claim follows from the retained local measurements.

## Approved closeout execution

1. Reconstruct these three task documents and audit retained artifacts.
2. Refresh formatting, Clippy, tests/doctests, optimized builds, rustdoc, Python
   tests, local-capture and vendored-backend evidence; review package/dependency
   versions and notices. Fix only release-blocking defects.
3. Commit task docs and required preparation; require current CI on the PR head.
   Merge PR #2 with a merge commit to preserve rollback history; fast-forward
   local `master`. The user's 2026-09-09 instruction supplies release approval.
4. Set the actual release date on `master` as prescribed by `RELEASING.md`, commit,
   verify clean packages in a fresh directory, push and verify release CI.
5. Check registry state, publish only absent intended versions in dependency
   order, verify visibility/clean consumer/docs, and tag the release `v0.6.0`.
6. Persist actual identifiers/results, push documentation closeout, and leave
   AstroMuninn untouched for a fresh development session.

No unresolved product design decision blocks this release. Registry credentials,
registry/docs availability and CI are external completion gates; record actual
failures rather than claiming publication. Follow the settled monitor design in
the next session, with config-file settings and bounded concurrency intact.
