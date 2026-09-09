# RavenSky validation release → AstroMuninn handoff

Status: **RELEASE INCOMPLETE — BLOCKED** as of 2026-09-09. PR #2 is merged and
all four 0.6.0 crates are published and independently usable. The remaining gate
is hosted API documentation: docs.rs cannot build the bundled CFITSIO autotools
backend in its read-only source sandbox. The `v0.6.0` tag and GitHub release are
not created, following the documentation-verification gate in `RELEASING.md`.
No AstroMuninn implementation or dependency changes were made.

Read [PLAN.md](PLAN.md) for the settled architecture,
[EVIDENCE.md](EVIDENCE.md) for verified observations, and
[RELEASING.md](../../../RELEASING.md) for release procedure.

## Release identity

- Approved PR: [#2](https://github.com/dostergaard/ravensky-astro/pull/2), target `master`.
- Approved implementation head: `2e9ba9505f0685839a14b5141d6faea20bbeb759`.
- Published coordinated versions: `astro-io`, `astro-metadata`, `astro-metrics`,
  `ravensky-astro` 0.6.0, in that dependency order on 2026-09-09.
- `astro-bench` 0.6.0 is intentionally unpublished (`publish = false`). Clone the
  repository for its library/CLI; installing the facade does not install it.
- Merge commit: `67491721de5fd95eece5456886f259efb74c1165` (PR #2, 18:10:24 UTC).
- Published source commit: `84a0f9eeb3e588bc0d27978e717a15add962af52` on `master`.
  All downloaded crate VCS identities and Rust sources match this clean commit.
- Release CI: [34387421495](https://github.com/dostergaard/ravensky-astro/actions/runs/34387421495),
  all three configured jobs passed. [Publication evidence](EVIDENCE.md#actual-publication-and-remaining-gate)
  retains timestamps, registry identifiers/checksums, downloaded-archive audits and
  a passing clean registry-only consumer.
- Post-publication commits preserve evidence only; find their exact current
  identity with `git log -1 master`. They do not change published 0.6.0 source.

## Resolve the release gate first

`fitsio-sys 0.5.7` calls autotools with `.insource(true)`; `autotools 0.2.7`
then attempts to create `ext/cfitsio/configure.prev`. docs.rs mounts crate sources
read-only, so this fails with `ReadOnlyFilesystem` (OS error 30). Both the direct
I/O and facade build logs confirm this cause; all four versioned API pages were
unavailable at the final check. Local rustdoc and Linux CI pass because their
source trees are writable. This is not a validator correctness failure.

The isolated [CMake diagnostic](records/docs-cmake-probe.log) successfully built
documentation, with warnings denied, for all four published crates using the
existing `fitsio/src-cmake` feature and a read-only copy of `fitsio-sys 0.5.7`.
This is evidence for a repair direction, not a deployed fix or a docs.rs result.
The diagnostic's manifest/lock are retained beside its log. No dependency code
or production build defaults were changed to run it.

Next action: obtain direction for a patch release (normally coordinated 0.6.1),
then implement a documentation build option that selects the existing CMake
backend, propagate it through the crate graph, configure docs.rs and validate
against read-only sources and the existing platform/backend matrix. Keep the
ordinary backend behavior unchanged. Feature unification and all-features builds
must be assessed, including AstroMuninn's older vendored backend. An alternative
is a compatible upstream backend fix followed by docs.rs rebuilds of 0.6.0;
that depends on an external release and has not been arranged.

Do not try to republish 0.6.0, move a release tag, silently publish an unapproved
version, or request repeated builds without changing the failing conditions.
Only after hosted documentation is verified should the remaining tag/release
steps run. The current 0.6.0 publication does not need repeating or yanking.
Local API docs remain available with `cargo doc --workspace --all-features --no-deps`.
AstroMuninn can resolve the published crates now, but the agreed closeout gate
remains incomplete; resolve it before starting the planned application phase.

Workspace housekeeping: the merged `feature/file-validation` branch is retained
locally and on origin. Automatic approval review rejected its deletion because
it required explicit branch-deletion authorization. No history was removed;
optional branch cleanup is separate from the documentation release gate.
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
   In AstroMuninn, update the registry `astro-metadata` requirement to 0.6.0; add
   `astro-io = "0.6.0"` to workspace/core when wiring validation. Refresh the lock
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
git -C ravensky-astro show 84a0f9e --stat
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
