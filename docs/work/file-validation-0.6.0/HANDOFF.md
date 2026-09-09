# RavenSky validation release → AstroMuninn handoff

Status: **RELEASE COMPLETE — READY FOR ASTROMUNINN** as of 2026-09-09. All four
0.6.1 crates are published, registry-verified and documented successfully on
docs.rs. Tag `v0.6.1` and its GitHub release identify the exact published source
commit. No AstroMuninn files were changed.

Read [PLAN.md](PLAN.md) for the settled architecture,
[EVIDENCE.md](EVIDENCE.md) for verified observations, and
[RELEASING.md](../../../RELEASING.md) for release procedure.

## Release identity

- Repair PR: [#3](https://github.com/dostergaard/ravensky-astro/pull/3), merged to
  `master` after all configured CI jobs passed in
  [run 34410913809](https://github.com/dostergaard/ravensky-astro/actions/runs/34410913809).
- Published source and merge commit:
  `4bc4660ccdfd607611eb998019e93eda399f69d1`.
- Published coordinated versions: `astro-io`, `astro-metadata`, `astro-metrics`,
  and `ravensky-astro` 0.6.1, in dependency order on 2026-09-09.
- `astro-bench` 0.6.1 is intentionally unpublished (`publish = false`). Clone the
  repository for its library/CLI; installing the facade does not install it.
- Annotated tag: [`v0.6.1`](https://github.com/dostergaard/ravensky-astro/tree/v0.6.1),
  peeled to `4bc4660`; [GitHub release](https://github.com/dostergaard/ravensky-astro/releases/tag/v0.6.1)
  published at 22:23:41 UTC.
- [Release evidence](EVIDENCE.md#061-publication-hosted-docs-and-closeout)
  retains archive hashes, registry identifiers and timestamps, docs.rs build IDs,
  and the registry-only consumer result.
- The earlier 0.6.0 source commit remains `84a0f9e`; do not republish, yank, or
  retrospectively tag that superseded release.

## Hosted-documentation repair

`fitsio-sys 0.5.7` calls autotools with `.insource(true)`; `autotools 0.2.7`
then attempts to create `ext/cfitsio/configure.prev`. docs.rs mounts crate sources
read-only, so this fails with `ReadOnlyFilesystem` (OS error 30). Both the direct
I/O and facade build logs confirm this cause; all four versioned API pages were
unavailable at the final check. Local rustdoc and Linux CI pass because their
source trees are writable. This is not a validator correctness failure.

The earlier isolated [CMake diagnostic](records/docs-cmake-probe.log) successfully built
documentation, with warnings denied, for all four published crates using the
existing `fitsio/src-cmake` feature and a read-only copy of `fitsio-sys 0.5.7`.
This was pre-deployment evidence; the actual 0.6.1 docs.rs builds now confirm it.
The diagnostic's manifest/lock are retained beside its log. No dependency code
or production build defaults were changed to run it.

The final repair uses each publishable crate's `[package.metadata.docs.rs]` to
pass the dependency feature selector `fitsio/src-cmake` and limits hosted docs to
the supported `x86_64-unknown-linux-gnu` target. `astro-io` and `astro-metadata`
already depended directly on `fitsio`. Cargo only accepts a dependency feature
selector from a package with a direct dependency edge, so configuration-only
direct `fitsio` edges were added to `astro-metrics` and `ravensky-astro`. Those
edges add no package or feature to their ordinary resolved graphs because the
same configured dependency was already present transitively.

This shape deliberately does **not** add a RavenSky docs feature. Default and
`--all-features` graphs remain identical and select only `fitsio-src`/autotools.
Only the docs-oriented command adds `src-cmake`; Cargo unifies that additive
feature on the existing `fitsio` instance, and `fitsio-sys 0.5.7` intentionally
chooses its CMake branch when both source-build features are present. Therefore
ordinary native behavior and public Rust APIs are unchanged. AstroMuninn's
patched `fitsio-sys 0.5.5` does not expose `src-cmake`, but it is unaffected:
docs.rs package metadata is not applied to dependents and no RavenSky feature can
forward the selector into an AstroMuninn build.

The latest available `fitsio` release checked during this repair is 0.21.10; it
still depends on `fitsio-sys 0.5`, and the latest `fitsio-sys` remains 0.5.7 with
the same autotools source-write behavior. It already exposes the usable CMake
alternative, but no available upstream release makes RavenSky's existing
`fitsio-src` selection docs.rs-safe by default. Published 0.6.0 manifests are
immutable, so a coordinated 0.6.1 is required to deploy the metadata.

A fresh probe vendored all locked third-party sources into an isolated temporary
tree, made that tree recursively read-only, and built docs for each publishable
crate with `DOCS_RS=1`, warnings denied, offline mode, and its production feature
selector. All four succeeded without any third-party modification. See
[EVIDENCE.md](EVIDENCE.md#061-hosted-documentation-repair) for commands and graph
results. CI then passed on Linux GNU, macOS ARM64 and the documented Windows GNU
subset. Clean packages from the merged commit were audited and matched the
downloaded registry archives byte-for-byte. A fresh registry-only consumer also
passed the public validation, metadata, I/O, metrics and facade smoke test.

Hosted builds [4395656](https://docs.rs/crate/astro-io/0.6.1/builds/4395656),
[4395657](https://docs.rs/crate/astro-metadata/0.6.1/builds/4395657),
[4395664](https://docs.rs/crate/astro-metrics/0.6.1/builds/4395664), and
[4395675](https://docs.rs/crate/ravensky-astro/0.6.1/builds/4395675) all succeeded.
Every versioned API page returned HTTP 200. This closes the only 0.6.0 blocker.

Do not try to republish or yank 0.6.0 or 0.6.1, move `v0.6.1`, or create a
retrospective 0.6.0 release tag. AstroMuninn integration may now begin from the
published 0.6.1 registry dependencies.

Workspace housekeeping: the merged `feature/file-validation` branch was deleted
locally and on origin after the user explicitly approved deletion. Its history
remains reachable from `master`. The initial automatic approval rejection was
resolved by that explicit authorization; no cleanup approval remains pending.
Unrelated branches are retained. Intentionally ignored local material
remains: `target/`, private `tests/data/` captures, `docs/fits_standard40aa-le.pdf`,
Python `__pycache__` directories and Finder `.DS_Store` files. These are not release
changes. The workspace-level tooling venv and user-maintained `AGENTS.md` remain
outside this repository's changes.

## Components and APIs

| Component | Role |
| --- | --- |
| `astro-io/src/validation/mod.rs` | Public options/limits, levels, reports, typed errors, file stamps and entry points |
| `astro-io/src/validation/resources.rs` | Caller-owned `MemoryBudget`/`MemoryReservation`, nonblocking admission and telemetry |
| `astro-io/src/validation/input.rs` | Shared bounded compressed input and cancellation-aware reads |
| `astro-io/src/validation/fits/` | Managed GZIP/Rice/PLIO/HCOMPRESS and named tile/fallback/mask dispatch |
| `astro-io/src/validation/xisf*` | Structural/block/checksum traversal and streamed zlib/Zstandard; admitted indivisible LZ4/inline buffers |
| `astro-io/src/fits/backend.rs` | Coordinated native loader/metadata access; not used by validation |
| `astro-bench/` and `docs/BenchmarkGuide.md` | Optional fixture/measurement library, CLI, capture probe, scripts and standalone guide |
| `astro-io/licenses/` | Retained adapted HCOMPRESS/fitskit/native algorithm notices |
| `docs/benchmarks/` | Immutable staged measurements, successful/rejected inputs and reproduction conditions |

Use `validate_file_with_budget(path, options, cancel, &budget)` for concurrent
application work. Clones share one budget; avoid separate whole-machine budgets
per file/session. `ResourceBusy` is retryable admission contention, not corruption;
release worker/state reservations before requeueing. `ResourceLimit` is a file or
stage that cannot fit the configured limits. `ValidationReport::stamp()` must be
rechecked before execution; size/mtime/available identity do not eliminate writer
races. Validation is read-only, additive and distinct from pixel loading.

FITS validation has no native fallback. GZIP/Rice/PLIO stream; HCOMPRESS admits
tile coefficients and scratch. Unknown extensions fail explicitly. Quantized
payload integrity is checked without rendering dequantized/smoothed output or
asserting scientific fidelity. The 64 GiB decoded limit is not RAM allocation.
Working accounting is not hard RSS enforcement or guaranteed recovery from every
OS/allocator OOM. Cancellation is cooperative, with blocking I/O limitations.

Existing image-loader and metadata contracts remain unchanged. CFITSIO direct
users must cover the entire handle lifetime with the backend closure protocol.
Reentrant builds permit independent native handles; non-reentrant calls serialize.
Native image-array/cache memory remains outside validator accounting.

## Platform and test limitations

Linux GNU and macOS ARM64 CI cover all crates. Windows GNU covers only `astro-io`,
`astro-metadata`, `astro-bench`. `sep-sys 1.3.0` calls POSIX `rand_r`, so
`astro-metrics` and the facade do not compile in that Windows job. Do not hide
that limitation by weakening warnings or treating excluded crates as passing.

Windows MSVC is not validated by the GNU job. AstroMuninn Lite uses MSVC and its
vendored `fitsio-sys 0.5.5` patch; actual MSVC release-target builds remain a
downstream requirement. The isolated patched-backend macOS test passes do not
establish MSVC compatibility. The standard shared-crate lock uses `0.5.7`.

Two intentionally ignored tests remain: `test_load_xisf_real_sample` needs an
untracked acquisition capture in `tests/data`; run explicitly when present.
`test_detect_stars_sep` is a pre-existing ignored initialization-dependent SEP
test and is not counted as passed. Normal generated-fixture tests require neither
that local capture nor the external drives.

No automatic calibration/scheduler, actual foreground-switching guarantee,
network-storage benchmark, induced low-memory pressure result or all-producer/
maximum-size compatibility claim is delivered. Local four-worker speedups and
105.73 MiB peak RSS are observations, not release defaults.

## AstroMuninn starting point

The application repository is on `feature/consolidate-monitor-policy`, commit
`5215a5bf5aae839db4615c628959c35f2245f145` at handoff preparation. Core/CLI version
is 0.9.2; `astro-metadata = "0.5.0"` remains in workspace dependencies, with no
direct `astro-io` core dependency. No application manifest/code update is part of
this closeout.

Read its `AGENTS.md`,
[MonitorPolicyDesign.md](../../../../astromuninn/docs/MonitorPolicyDesign.md),
[ImageValidationDesign.md](../../../../astromuninn/docs/ImageValidationDesign.md)
and [Lite roadmap](../../../../astromuninn/docs/AstroMuninnLiteRoadmap.md).
Inspect the current code; the designs are approved intent, not implemented monitor
behavior. `crates/astromuninn-core/src/organizer.rs::monitor_directory` and
`apps/astromuninn-lite/src-tauri/src/commands.rs::run_monitor_loop` still duplicate
policy. Config serialization belongs to `crates/astromuninn-core/src/config.rs`.

Recommended first steps in a fresh session:

1. Verify this release is published and inspect both repositories' clean state.
   In AstroMuninn, update the registry `astro-metadata` requirement to 0.6.1; add
   `astro-io = "0.6.1"` to workspace/core when wiring validation. Refresh the lock
   and check that the vendored backend patch is actually selected. Do not leak
   development path overrides into release manifests.
2. Create an AstroMuninn task plan/handoff/evidence directory. Add backward-
   compatible monitor/resource config with field validation and preservation
   when Lite saves unrelated settings. Settings remain manually editable in
   `config.json` until an editor exists; reload at monitoring start.
3. Implement a shared core coordinator with injected clock/telemetry tests,
   bounded pending queues and fair CPU/memory/storage admission. Waiting retries
   must not occupy workers or prevent discovery of new files. Recheck source
   observations and preserve existing collision/overwrite/copy/move semantics.
4. Route CLI and Lite through the shared state/events/cancellation contract;
   remove the duplicate policies. Measure the actual shipped native backend and
   resource pressure/foreground responsiveness before selecting product defaults.
5. Verify CLI GNU Windows and Lite MSVC separately, plus Linux/macOS workflows.
   Use a normal registry dependency graph for release actions.

Approved product decisions that should not be reopened without new evidence:

- Ten seconds between completed discovery passes, five seconds unchanged quiet
  time, and a five-minute timeout for an unchanged file awaiting validation.
- Include files present at startup; structural checks by default, Full via config.
- Changed size/mtime resets readiness and reactivates skipped/processed files.
- New/ready files continue while another waits or times out; do not overlap
  discovery passes or wait for every active job before the next pass.
- Destination unavailable/full aborts monitoring with a descriptive explanation
  and instructions to fix the cause and restart; no automatic restart.
- Cancellation stops new scheduling, cancels validation cooperatively, and lets
  already-active copies/moves reach safe completion before reporting stopped.
- Automatic bounded concurrency is the release direction; serial is diagnostic.
  Fairness, finite telemetry fallbacks, storage caps, queue limits and aggregate
  memory must be designed together. Do not infer a universal worker count from
  the M4 Max results.
- AstroMuninn remains donationware with Buy Me a Coffee support; no Pro/licensing
  branch or tier machinery should return as part of integration.

Initial commands, from the RavenSky workspace (inspect before updating):

```sh
git -C ravensky-astro status --short --branch
git -C ravensky-astro show v0.6.1 --stat
git -C ravensky-astro tag --list 'v0.6.*'
git -C astromuninn status --short --branch
cd astromuninn
cargo tree -i fitsio-sys
cargo fmt --all --check
cargo clippy --workspace --exclude astromuninn-lite --all-targets --all-features -- -D warnings
cargo test --workspace --exclude astromuninn-lite
cargo check -p astromuninn-lite
```

After dependency integration, repeat these with the new lockfile and use the
repository's platform CI/release build instructions. Do not run release publishing
jobs merely to test an unfinished monitor. See the evidence document for exact
benchmark commands and the difference between successful and rejected captures.
