# Validation and benchmark evidence

Status: historical evidence audited 2026-09-09; release closeout checks and actual
publication results are recorded below as completed. [PLAN.md](PLAN.md) is the
settled contract; [HANDOFF.md](HANDOFF.md) owns the operational handoff.

## Provenance and interpretation

Raw evidence is retained in `docs/benchmarks/`, with source/compiler/build/script
fingerprints and input identities. Earlier reports apply to their recorded code
and conditions; later codec support must not be retroactively attributed to them.
The [closeout audit](records/retained-evidence-audit.json) recomputed current codec
counts, maximum RSS, speedups, cancellation timings and all 14 supplied fixture
hashes from retained artifacts. Tests are correctness evidence; timings are
separate observations. **No timing, worker count or RSS measurement is a universal
default, guarantee, hardware recommendation or OS-enforced memory limit.**

## Correctness coverage

| Area | Evidence and observations |
| --- | --- |
| FITS/XISF structure | Generated fixtures in `astro-io/tests/validation.rs` cover layouts, HDUs/local blocks, bounds, truncation, limits, unknown features and changes during validation. |
| Full payloads | Checksum corruption, stored-data checks before decompression, exact codec extents/output, supported XISF zlib/LZ4/LZ4HC/Zstandard/shuffle and digest variants. |
| Compressed FITS | `validation/compressed.rs` covers native-generated integer/float/quantized/fallback tiles, masks, malformed descriptors, truncations, mutations, bounded random HCOMPRESS input and shared contention/cancellation. |
| Algorithm parity | Rice/HCOMPRESS unit tests compare decoded samples against native-generated/oracle values, including signed/wrapping values, odd geometry and lossy HCOMPRESS. PLIO native masks and hand-built instructions verify its representation contract. |
| Resource lifetime | `astro-io/tests/resources.rs`, validator unit/integration tests and benchmark workloads check shared accounting, limits/busy distinctions, errors, cancellation and cleanup. |
| Native consumers | `astro-metadata/tests/fits_concurrency.rs` exercises loader/header/metadata and validation callers. Native reentrancy gate tests cover serial/reentrant/nested/unwind behavior. |
| Supplied fixtures | 14 binary-preserved files in `astro-bench/tests/test_data`; checksum_false is rejected at Full; two incomplete XISF thumbnail fixtures remain incomplete; the supplied Rice files now pass managed Full validation. Random groups remain unsupported. |

Unknown checksums/codecs in Full are not counted as successful validation. Source
preservation is tested both by validator observations and by independent before/
after benchmark hashes. No invalid capture was repaired to manufacture a pass.

## Earlier baseline and completion investigation

See [initial baseline](../../benchmarks/2026-09-06-m4-max/README.md),
[streaming comparison](../../benchmarks/2026-09-06-m4-max-streaming/README.md),
[completion record](../../benchmarks/2026-09-07-m4-max-completion/README.md) and its
[machine-readable audit](../../benchmarks/2026-09-07-m4-max-completion/audit.json).
The initial and streaming checkpoints each retain 216-sample comparisons. The
later completion audit records:

- 960 scaling samples, eight UInt16 inputs and worker/tile/size/order variants;
  maximum unprofiled RSS 9.53125 MiB (reported conservatively as below 9.54 MiB).
- 40 bounded-contention samples with a short-task/scheduling proxy, not a human
  application-switching assessment. Two competitors each hash a touched 32 MiB
  payload; this is not induced system RAM pressure.
- 10 CLI cancellation trials with child reaping and scratch cleanup; maximum
  38.842 ms, below the predeclared two-second local target.
- 270 real-capture samples: 240 successful and 30 explicit rejections. Three
  older local FITS captures have invalid HDU checksums; independent CFITSIO 4.7.0
  verification found valid data checksums (+1) but invalid HDU checksums (−1).
- 15 additional successful samples of three 714.29 MiB FITS files, maximum RSS
  6.33 MiB. No source was copied, repaired or modified by the probes.
- 1,255 successful primary unprofiled samples total, plus 30 rejected samples.
  Two instrumented profile runs and the initial aborted capture attempt (45
  structural successes and one failed full sample) are retained but not pooled.

Machine: Apple M4 Max, 36 GiB RAM, 14 logical/physical cores, macOS 26.6.2 ARM64,
Rust 1.94, optimized release builds. Earlier completion runs used AC power with
low-power mode off. Cache/desktop/core placement were uncontrolled. No builds or
tests ran concurrently with measurements. Swap snapshots/counters stayed zero;
snapshots are not continuous pressure telemetry. Source hash
`1b0a1b651e6c3a84b4a204268dfe9aa3eebf5556e0f754d5f268c16c3e7e4c1d`
ties the earlier measured sources together despite intervening tooling commits.

| Storage | Source context | Selected stored data |
| --- | --- | ---: |
| Internal APFS SSD | Ignored local `tests/data`, three FITS + three XISF | 96.24 MiB |
| User-described mechanical HDD | `/Volumes/WD_ElementsHD/RavenSkyTestFiles/QA` | 535.80 MiB |
| User-described external SSD | `/Volumes/ap_projects/0Working`, recursive selection | 672.82 MiB |
| External SSD large-file extension | Supplied `NGC7000/app`, three FITS | 2,142.86 MiB |

No network storage or user-owned Windows/Linux machine was available. Subsequent
hosted CI supplies build/test evidence, not equivalent storage timings.

## Managed compressed-FITS matrix

[Raw matrix and conditions](../../benchmarks/2026-09-08-managed-fits/README.md):
six cases × 1/2/4 workers × three repetitions = 54 samples, **432 operations**.
All succeeded, all source hashes matched, and reservations returned to zero.
Independent `fpack` 1.7.0 / CFITSIO 4.070 generated four 8-MiB-pixel files per case;
two validation passes per sample. Generation and hashing are outside timed work.
512 MiB shared admission and `min(256 MiB, 512 MiB / workers)` per call. Cache
likely warm but uncontrolled; this matrix did not measure power mode.

| Case | Four-worker speedup over serial | Maximum RSS (MiB) |
| --- | ---: | ---: |
| Rice noise / 32 rows | 3.496× | 6.72 |
| Rice gradient / 32 rows | 3.478× | 6.94 |
| PLIO mask / 32 rows | 3.045× | 6.56 |
| HCOMPRESS noise / 32 rows | 3.447× | 10.14 |
| HCOMPRESS noise / whole tile | 3.464× | **105.734375** |
| HCOMPRESS quantized float / whole tile | 3.311× | 79.47 |

Peak whole-tile reservations were about 289.51 MiB across four workers; all
samples passed predeclared RSS <192 MiB and reservations ≤512 MiB targets.
This is about **3–3.5× throughput** at four workers locally. Large HCOMPRESS tiles
consume admitted coefficient buffers; the small streamed-codec RSS cannot be
generalized to every tile. This matrix is not a native-versus-managed comparison.

Six separate whole-image HCOMPRESS SIGINT probes (1/4 workers, 256 passes,
signal one second after launch) stopped in 0.775–1.744 ms, all **below 1.8 ms**
and the 250 ms local target. The maximum recomputed value is 1.743541972 ms.
Sources were unchanged; cancellation did not emit a success report. Shutdown
includes process teardown and has no codec-phase handshake. Blocking I/O,
different hardware, foreground switching and pressure remain separate concerns.

## Reproduction

Use [BenchmarkGuide.md](../../BenchmarkGuide.md) for complete standalone Rust/CLI
examples, options and report interpretation. From the repository root:

```sh
cargo build --locked --release -p astro-bench --bin astro-bench --example capture_probe
python3 astro-bench/scripts/benchmark_suite.py --help
python3 astro-bench/scripts/capture_matrix.py --help
python3 docs/benchmarks/2026-09-08-managed-fits/run.py --output target/managed-fits-new
python3 docs/benchmarks/2026-09-08-managed-fits/cancel.py --output target/cancel-new
```

Choose new output directories and preserve diagnostic failures. Capture matrices
accept an explicit directory, optional recursion, per-format selection and
`--allow-rejected` for known-invalid input. The large-file extension uses the
three sorted FITS inputs, one pass, 1/2/4 workers and five fresh processes per
worker count. Preserve first-observed versus repeated-hash timing distinctions.
No unsafe cache purges or source alterations are part of reproduction.

## Prior verification checkpoints

`RELEASING.md` records 131 workspace tests and four doctests passing locally,
nine Python tests, formatting, Clippy/rustdoc with warnings denied and optimized
builds. Two tests are intentionally ignored: the local-only XISF capture loader
test and the pre-existing SEP test whose comment requires proper initialization.
The XISF test was run explicitly and passed (3856 × 2180, 8,406,080 pixels).
The SEP ignored test is not counted as passed and was not enabled during closeout.

An isolated copy with AstroMuninn's unchanged vendored `fitsio-sys 0.5.5` passed
108 applicable I/O/metadata/benchmark tests on macOS; the normal RavenSky lock
uses `fitsio-sys 0.5.7`. This is not a Windows MSVC result.

Four clean 0.6.0 package archives from `6237264` passed Cargo verification and
inspection in a fresh `target/package-check-060`. Notices were present and root
benchmark artifacts excluded. Cargo 1.94 offline multi-package verification hit
`no hash listed`; online verification worked. Reusing larger old archives also
left trailing bytes locally; fresh archives passed complete gzip-consumption
checks. Do not publish a stale preflight archive. Fresh release verification below
supersedes this checkpoint for the actual released contents.

[CI run 34237121494](https://github.com/dostergaard/ravensky-astro/actions/runs/34237121494)
passed on `2e9ba9505f0685839a14b5141d6faea20bbeb759`: complete Linux/macOS
workspace, Windows GNU I/O+metadata+benchmarks. Earlier failures identified SEP's
Windows `rand_r` dependency and two header tests deleting live native handles;
the latter tests now close handles first. No production loading change resulted.

## Fresh closeout and publication record

Fresh checks on 2026-09-09 passed, with complete commands and outputs retained:

- [Local command/results index](records/local-verification.json): 131 workspace
  tests, two intentionally ignored, four doctests, formatting, Clippy and rustdoc
  with warnings denied, optimized workspace/example builds and nine Python tests.
- [Explicit local XISF check](records/local-xisf.log): the ignored local capture
  loader test passed when invoked explicitly; SEP remains ignored.
- [Vendored-backend check](records/vendored-backend.json) and
  [log](records/vendored-backend.log): a fresh isolated source copy selected
  AstroMuninn's unchanged `fitsio-sys 0.5.5`, passing 108 tests with the unavailable
  local-capture test intentionally ignored. Application tracked files were unchanged.
- [Historical CI recheck](records/implementation-ci.json): actual GitHub results
  confirm all three jobs succeeded on `2e9ba95` in run `34237121494`.
- [Registry preflight](records/registry-preflight.json): all four intended 0.6.0
  versions returned 404 on 2026-09-09. No republishing or version changes are needed.

Task-document relative links, CI YAML/branch targets and manifest versions were
also checked. Production Rust code is unchanged from the approved head.

Clean review packages from `e294ba542960c1d780a5dc072e1f86bb6e823c2f` passed
`cargo package --locked --workspace --exclude astro-bench --target-dir
target/package-closeout-review`. The [build log](records/review-package.log) and
[archive audit](records/review-packages.json) retain hashes, sizes, clean source
identity, 0.6.0 dependency requirements, matched Rust sources, complete gzip ends
and notices. Reproduce inspection with [verify_archives.py](verify_archives.py):

```sh
python3 docs/work/file-validation-0.6.0/verify_archives.py \
  target/package-closeout-review/package target/package-inspection.json \
  e294ba542960c1d780a5dc072e1f86bb6e823c2f
```

Closeout CI run `34385778028` passed macOS and Windows GNU, but both Linux
attempts failed before compilation because the runner's unrelated Google Chrome
apt repository returned a hash mismatch. The smallest workflow correction checks
whether `build-essential` is already installed and only updates/installs when
needed. Installation failures still fail CI; no Rust checks were relaxed.

## Original 0.6.0 publication and documentation blocker

PR #2 merged on 2026-09-09 at 18:10:24 UTC as
`67491721de5fd95eece5456886f259efb74c1165`; see [merge record](records/merged-pr.json).
The corrected [premerge CI](records/premerge-ci.json), run `34386857280`, passed
all three jobs on `ad40e2f`. Release metadata commit
`84a0f9eeb3e588bc0d27978e717a15add962af52` dates the changelog and changes no
production source. Its [release CI](records/release-ci.json), run `34387421495`,
also passed all three configured jobs. Their Windows scope remains as above.

Fresh final packages passed:

```sh
cargo package --locked --workspace --exclude astro-bench \
  --target-dir target/package-060-release
python3 docs/work/file-validation-0.6.0/verify_archives.py \
  target/package-060-release/package target/release-packages.json \
  84a0f9eeb3e588bc0d27978e717a15add962af52
```

Retained [package log](records/package-060-release.log) and
[archive audit](records/release-packages.json) confirm all four versions, dependency
requirements, clean VCS identity, matching Rust sources, complete gzip archives,
decoder notices and raw benchmark exclusion. The checkout stayed clean throughout
publication; evidence was staged under ignored `target/` until publication finished.

Each command below used the additional argument
`--target-dir target/publish-060-closeout`, a fresh publication directory:

| Command | Registry creation time (UTC, 2026-09-09) | Retained evidence |
| --- | --- | --- |
| `cargo publish --locked -p astro-io` | 18:14:30.734969 | [log](records/publish-astro-io.log), [registry](records/registry-astro-io.json) |
| `cargo publish --locked -p astro-metadata` | 18:14:43.703995 | [log](records/publish-astro-metadata.log), [registry](records/registry-astro-metadata.json) |
| `cargo publish --locked -p astro-metrics` | 18:15:21.184354 | [log](records/publish-astro-metrics.log), [registry](records/registry-astro-metrics.json) |
| `cargo publish --locked -p ravensky-astro` | 18:15:36.634674 | [log](records/publish-ravensky-astro.log), [registry](records/registry-ravensky-astro.json) |

All commands exited successfully. Each version was confirmed visible before its
dependent was published. All four downloaded 0.6.0 archives match the registry
SHA-256 checksums **and** the final prepublication archives byte for byte; see
[published archive inspection](records/published-packages.json).
[verify-ravensky-registry.py](verify-ravensky-registry.py) reproduces the registry,
dependency and download-checksum inspection:

```sh
python3 docs/work/file-validation-0.6.0/verify-ravensky-registry.py \
  /path/to/ravensky-astro astro-io
# Repeat for astro-metadata, astro-metrics, ravensky-astro.
```

`astro-bench` was intentionally not published. No crate was yanked, republished
or given an unapproved new version.

### Independent published consumer

A fresh external Cargo project pinned all four dependencies to `=0.6.0`, with
no workspace/path patches, built in release mode and ran successfully. Its
[manifest](published-consumer/Cargo.toml), [lockfile](published-consumer/Cargo.lock),
[source](published-consumer/src/main.rs), [log](records/published-consumer.log) and
[resolution audit](records/consumer-resolution.json) are retained. The lockfile
selects only registry sources for the four crates and matches the downloaded
archive checksums. The smoke test checks Full FITS validation through the facade,
shared-budget release, metadata extraction, known loaded pixels and unchanged
source bytes. It references the metrics API and compiles/links that dependency;
it does not execute the ignored SEP initialization test.

To reproduce, copy `published-consumer/` to a fresh directory outside this Cargo
workspace, enter it and run `cargo run --release --locked`. Its small FITS fixture
uses `create_new` and is removed only after successful source-preservation checks.
This exact retained example also passed from another fresh directory with
`cargo run --release --locked --offline`; see [reproduction log](records/consumer-reproduction.log).
The first sandboxed attempt was denied the native backend's write to the Cargo
source cache; rerunning with that required filesystem access passed. It is the
same source-write requirement exposed more strictly by docs.rs, not a test failure
in validation logic.

### Hosted documentation blocker

[Final versioned-page check](records/docs-rs-060.json) found all four API paths
redirected to crate landing pages rather than rustdoc. The direct I/O build
[4392698](https://docs.rs/crate/astro-io/0.6.0/builds/4392698) and facade build
[4392717](https://docs.rs/crate/ravensky-astro/0.6.0/builds/4392717) both failed in
`fitsio-sys 0.5.7` / `autotools 0.2.7` with `ReadOnlyFilesystem`, OS error 30.
Retained [I/O log](records/docs-rs-astro-io-failure.log) and
[facade log](records/docs-rs-facade-failure.log) preserve the failure details.

Inspection of the exact dependency source identifies `.insource(true)` in
`fitsio-sys`'s `build.rs`, then `File::create(configure.prev)` at autotools line
643. This conflicts with the documented [docs.rs read-only source sandbox](https://docs.rs/about/builds).
Local writable-source rustdoc success does not establish hosted success. The
crate versions are usable and published, but hosted documentation remains failed.

A focused diagnostic copied `fitsio-sys 0.5.7` under a fresh temporary directory,
removed write permissions recursively, patched only that diagnostic consumer to
the copy, and enabled the existing `fitsio` `src-cmake` feature. With
`RUSTDOCFLAGS='-D warnings' cargo doc`, documentation for all four published crates
built successfully. The [diagnostic log](records/docs-cmake-probe.log),
[manifest](records/docs-cmake-probe.Cargo.toml.txt) and
[lockfile](records/docs-cmake-probe.Cargo.lock.txt) preserve the exact setup. It
ran on this macOS machine, not inside docs.rs; no production manifests or backend
defaults changed. This supports a repair direction without claiming hosted or
cross-platform verification. The probe's patch path and writable build output
are intentional and separate from the unpatched published-consumer verification.

Because published 0.6.0 manifests are immutable, applying documentation feature
metadata required a new patch release. This section records the superseded
0.6.0-only state; the 0.6.1 evidence below closes that blocker. No `v0.6.0` tag
or release was created, and 0.6.0 was neither republished nor yanked.

## 0.6.1 hosted-documentation repair

Investigation on 2026-09-09 established:

- `astro-io` and `astro-metadata` directly depend on workspace `fitsio` with
  `fitsio-src`; `astro-metrics` and the facade reach the same dependency through
  their RavenSky dependencies.
- The locked graph selects `fitsio 0.21.9` and `fitsio-sys 0.5.7`.
  `fitsio-src` activates `fitsio-sys/fitsio-src` and its `autotools` dependency.
- `fitsio/src-cmake` activates `fitsio-sys/src-cmake`; with both features,
  `fitsio-sys`'s existing build script selects CMake rather than autotools.
- `cargo info` reported `fitsio 0.21.10` and `fitsio-sys 0.5.7` as the latest
  available releases. Inspection of 0.21.10 confirms the same `fitsio-sys =
  "0.5"`, `fitsio-src`, and `src-cmake` arrangement. No available release fixes
  the autotools source write while retaining RavenSky's existing default feature.
- docs.rs metadata passes its `features` values to Cargo. Cargo 1.94 accepts
  `fitsio/src-cmake` for a package with a direct `fitsio` edge. It rejects that
  selector from `astro-metrics` or the facade without such an edge.

The release adds docs.rs metadata to all four publishable packages and
configuration-only direct `fitsio` edges to `astro-metrics` and the facade. The
following graph checks passed:

```sh
cargo tree --locked -e features -i fitsio-sys
cargo tree --locked --all-features -e features -i fitsio-sys
for pkg in astro-io astro-metadata astro-metrics ravensky-astro; do
  cargo tree --locked -p "$pkg" --features fitsio/src-cmake \
    -e features -i fitsio-sys -f '{p} {f}'
done
```

Default and `--all-features` both selected only
`fitsio-sys` features `autotools,fitsio-src`. Every docs-oriented package graph
selected `autotools,cmake,fitsio-src,src-cmake`; the third-party build script's
existing cfg chooses its CMake branch in that supported combination. No
RavenSky feature was added, and public Rust source/API is unchanged.

Writable-source documentation first passed for all four packages:

```sh
for pkg in astro-io astro-metadata astro-metrics ravensky-astro; do
  RUSTDOCFLAGS='-D warnings' CARGO_TARGET_DIR=target/docs-061-cmake \
    cargo doc --locked -p "$pkg" --features fitsio/src-cmake --no-deps
done
```

The decisive fresh probe used `/private/tmp/ravensky-docs-061-final-probe.KLu3xp`.
`cargo vendor --offline --versioned-dirs --sync <cmake-only-manifest> <vendor>`
created an isolated source tree including the optional CMake backend. After
`chmod -R a-w <vendor>`, the following equivalent command was run for each of the
four package names:

```sh
DOCS_RS=1 RUSTDOCFLAGS='-D warnings' \
CARGO_TARGET_DIR=<probe>/target cargo doc --locked --offline \
  -p <package> --features fitsio/src-cmake --no-deps \
  --config 'source.crates-io.replace-with="vendored-sources"' \
  --config 'source.vendored-sources.directory="<probe>/vendor"'
```

The read-only assertion for
`vendor/fitsio-sys-0.5.7/ext/cfitsio` passed before the builds. All four commands
completed successfully and generated their crate index pages. All build output
was outside the read-only source tree. The dependencies were copied without
patches and are not part of the RavenSky candidate.

The first vendor attempt omitted optional `cmake` because it was not active in
the default lock traversal; it failed before compilation with “no matching
package named `cmake`.” The final fresh probe explicitly synchronized that
unchanged registry dependency before making the source tree read-only. This was
a probe-setup correction, not a repair to third-party source.

Patch-release package, CI, registry and hosted docs results are recorded below.

### Local candidate matrix and vendored-backend compatibility

The complete local release matrix passed on the 0.6.1 candidate:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --doc
cargo build --locked --release --workspace --all-features --examples
RUSTDOCFLAGS='-D warnings' \
  cargo doc --locked --workspace --all-features --no-deps
```

Results: formatting, Clippy and rustdoc passed with warnings denied; 131 tests
passed with the same two intentional ignores; four doctests passed; and the
optimized workspace/example build passed. This all-features run used the
unchanged autotools backend, as confirmed by the graph audit above.

A fresh isolated copy at `/private/tmp/ravensky-061-vendored.4kr02W/source`
patched only dependency resolution to AstroMuninn's unchanged vendored
`fitsio-sys 0.5.5`. `cargo update --offline -p fitsio-sys --precise 0.5.5`
selected that path. `cargo tree --locked --offline --all-features -i fitsio-sys
-e features` showed only its existing `default,fitsio-src` features; no
`src-cmake` request escaped the docs.rs metadata. The following passed with 108
tests and the local-capture test intentionally ignored:

```sh
cargo test --locked --offline \
  -p astro-io -p astro-metadata -p astro-bench \
  --all-targets --all-features \
  --target-dir <ravensky-repository>/target/vendored-061
```

No AstroMuninn tracked file was changed. This macOS check confirms resolution and
applicable RavenSky behavior with the vendored backend; it is not Windows MSVC
evidence.

A fresh precommit package build also passed for all four publishable crates:

```sh
cargo package --locked --workspace --exclude astro-bench --allow-dirty \
  --target-dir target/package-061-precommit
```

Inspection of every normalized archive manifest confirmed version 0.6.1,
internal requirements 0.6.1, `package.metadata.docs.rs.features =
["fitsio/src-cmake"]`, the single Linux GNU docs target, and direct configured
`fitsio` dependencies where required. Cargo verified each archive in dependency
order through its temporary registry. These precommit archives were not
published; the following section records the clean merged-commit packages.

### 0.6.1 publication, hosted docs and closeout

PR [#3](https://github.com/dostergaard/ravensky-astro/pull/3) merged at
2026-09-09 22:14:44 UTC as
`4bc4660ccdfd607611eb998019e93eda399f69d1`. Its head `431d4c6` passed all three
configured jobs in [CI run 34410913809](https://github.com/dostergaard/ravensky-astro/actions/runs/34410913809):
Linux x86-64 GNU, macOS ARM64, and the documented Windows x86-64 GNU subset.

From clean merged `master`, `cargo package --locked --workspace --exclude
astro-bench --target-dir target/package-061-release` passed. The generalized
`verify_archives.py` audit confirmed complete gzip streams, clean VCS identity,
matching Rust and notice bytes, no path dependencies, no vendored CFITSIO source,
0.6.1 internal requirements and the intended docs.rs metadata. Final archive
hashes were:

| crate | SHA-256 | bytes |
| --- | --- | ---: |
| `astro-io` | `26beeb3388e7a3cbb1b5b49fe609839ee9c24c4a169694d039d14b731fd3ee00` | 69,758 |
| `astro-metadata` | `6be3e3f0cab87e516b22d61c5bdf72e087f48f3cac51d67ac763bbcb9f75dade` | 24,428 |
| `astro-metrics` | `9177453b81858939307106a1d94d92aaa58b8f0c57987593804e5fd870ac9d16` | 16,553 |
| `ravensky-astro` | `722be74b2a4e755b2111ac8d7d92a256908d3b1bd27484610577a9c43753433a` | 212,068 |

The crates were published in dependency order. crates.io assigned version IDs
3199637, 3199638, 3199643 and 3199652 respectively, between 22:16:56 and
22:18:32 UTC. Each was non-yanked, exposed Rust 1.94, and its downloaded archive
matched the corresponding prepublication archive byte-for-byte. Registry
dependency records showed the expected `^0.6.1` internal requirements.

A fresh external project with exact `=0.6.1` requirements and no path/workspace
patches ran successfully in release mode. It resolved all four RavenSky crates
from crates.io and exercised Full FITS validation, shared-budget release,
metadata extraction, pixel loading, the metrics API, facade use, and source-byte
preservation. See the retained [consumer log](records/published-consumer-061.log).

Actual hosted documentation then succeeded:

| crate | docs.rs build | versioned API |
| --- | --- | --- |
| `astro-io` | [4395656](https://docs.rs/crate/astro-io/0.6.1/builds/4395656) | [HTTP 200](https://docs.rs/astro-io/0.6.1/astro_io/) |
| `astro-metadata` | [4395657](https://docs.rs/crate/astro-metadata/0.6.1/builds/4395657) | [HTTP 200](https://docs.rs/astro-metadata/0.6.1/astro_metadata/) |
| `astro-metrics` | [4395664](https://docs.rs/crate/astro-metrics/0.6.1/builds/4395664) | [HTTP 200](https://docs.rs/astro-metrics/0.6.1/astro_metrics/) |
| `ravensky-astro` | [4395675](https://docs.rs/crate/ravensky-astro/0.6.1/builds/4395675) | [HTTP 200](https://docs.rs/ravensky-astro/0.6.1/ravensky_astro/) |

All four builds used rustc 1.100.0-nightly (2026-09-08) and docsrs commit
`bbe8284d494398efa390829fb7f0bf2364bcdf59`. The successful hosted results confirm
the read-only-source repair under the real service, beyond the local probe.

Annotated tag `v0.6.1` peels to the exact published source commit `4bc4660`. The
[GitHub release](https://github.com/dostergaard/ravensky-astro/releases/tag/v0.6.1)
was published at 22:23:41 UTC. The compact machine-readable
[release record](records/release-061.json) retains the PR, CI, registry, docs.rs,
tag and release identifiers.

Final status: **RELEASE COMPLETE — READY FOR ASTROMUNINN**. AstroMuninn remains
unchanged at `5215a5bf5aae839db4615c628959c35f2245f145`; its integration is the next
task, not part of this release closeout.
