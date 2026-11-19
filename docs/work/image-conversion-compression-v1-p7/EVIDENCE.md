# P7-A RavenSky Astro publication preparation evidence

## Starting audit and publication inventory, 2026-11-19

The release worktree starts at RavenSky Astro
`4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2` on
`release/ravensky-astro-p7-preparation`. Its original checked-out `master`
has only an unrelated unstaged `.gitignore` edit. `origin/master` matched the
starting commit by `git ls-remote`. AstroMuninn `master` and `origin/master`
matched `b1f0a12dd68daa660cd8890f86292845980a09f3` and its checkout was
clean. The signed/annotated status of prior tags was not relied upon; `v0.6.2`
resolves to release source commit `0d8c57a053cf922cbe8075fc77e82b69192f9aeb`.
The previous publication order and matching published archives are recorded in
`docs/work/release-0.6.2/EVIDENCE.md`.

The official crates.io API reported 0.6.2 as the latest non-yanked version of
each of the four public crates. Its public owner list names `dostergaard` for
each. This establishes registry ownership, not proof that this machine has a
working upload credential. No credential contents were read.

| Crate | Published | Proposed | API/role assessment | Why a new package is needed |
| --- | --- | --- | --- | --- |
| `astro-io` | 0.6.2 | **0.7.0** | Additive typed inventory, bounded exact UInt16 reader/writers, comparison, codec capability, XISF Revision 2 raw-subblock handling; no existing exported signature removed | Substantial new public capability, not a 0.6.2 bugfix; source for AstroMuninn |
| `astro-metadata` | 0.6.2 | **0.7.0** | Additive `fidelity` semantic inventory, impact ledger, metadata comparison and provenance; existing metadata surface retained | Depends on `astro-io 0.7.0` and must expose the tested fidelity APIs |
| `astro-metrics` | 0.6.2 | **0.7.0** | Source-level public API unchanged; dependency edges move to the new shared pair | Avoids old 0.6.2 shared crates in the facade graph and preserves the coordinated release line |
| `ravensky-astro` | 0.6.2 | **0.7.0** | Thin re-export surface unchanged, but re-exports new crate versions and APIs | Facade must select a single aligned 0.7.0 graph |
| `astro-bench` | unpublished | **unpublished** | Synthetic benchmark and measurement tooling, `publish = false` | Not a public release artifact |

The root workspace declares one version and three version-qualified path
dependencies. `astro-metadata` depends on `astro-io`; `astro-metrics` depends on
both; the facade depends on all three. Publication order is therefore
`astro-io` → `astro-metadata` → `astro-metrics` → `ravensky-astro`.
The published 0.6.2 crates cannot be overwritten. A 0.6.3 release would signal
a patch line while introducing substantial new public modules. Cargo's
pre-1.0 compatibility convention treats 0.7 as a new compatibility line; it
is a deliberate choice for these public APIs and dependency identities, not
a claim that an existing exported signature was removed. The coupled metrics
and facade updates are required to avoid two versions of the shared types in
the public release graph. `astro-bench` inherits the workspace version for
local testing but remains unpublishable.

All five workspace packages inherit Rust 1.94, the GitHub repository and
MIT OR Apache-2.0 license declaration. The four public crates also declare
the project homepage, their matching docs.rs pages and README files. None declares a crate feature
or optional dependency. `astro-io` has no internal library dependency;
`astro-metadata` depends on `astro-io`; `astro-metrics` depends on both;
`ravensky-astro` depends on all three; unpublished `astro-bench` depends on
`astro-io`. The remaining direct dependencies are external registry crates
listed in each manifest, including `fitsio`/CFITSIO for FITS and `sep-sys` for
metrics. The facade publicly re-exports the three libraries. Tag history is
`v0.2.0`, `v0.3.0`, `v0.5.0`, `v0.6.1`, `v0.6.2`; registry versions also include
the published 0.6.0 and 0.6.2 packages.

## API compatibility assessment against `v0.6.2`

`git diff v0.6.2..master` shows only `pub mod typed` added to `astro-io`'s
existing public module tree and `pub mod fidelity` added to `astro-metadata`'s.
The other changes to existing FITS/XISF source are implementation or
`pub(crate)` visibility changes; no existing exported function signature,
public field, trait bound, enum variant or Cargo feature changed. The new
modules contain public types/functions, including `ContainerInventory`,
`U16ChunkReader`, `write_fits_u16`, `write_xisf_u16`,
`compare_u16_containers`, `ImpactLedger`, `inspect`, `compare_metadata_files`
and provenance types. No crate exposes a new feature flag. Rust MSRV remains
1.94, and the normal FITS path still uses CFITSIO via `fitsio`; it is not a
pure-Rust backend.

Existing `load_fits` and `load_xisf` signatures remain. The XISF decoder now
accepts specification Revision 2 equal-length raw compression subblocks and
rejects older nonconformant equal-length compressed encodings rather than
guessing their meaning. The new typed writer's exact transport contract is
two-dimensional monochrome UInt16; parsing, metadata inspection and file
validation cover broader structures but do not imply they can be rewritten.
Metadata translation is separately classified and may require explicit loss
approval. This is a behavioral expansion with a documented strictness edge,
not a universal FITS/XISF conversion promise.

No `cargo-semver-checks` or `cargo-public-api` executable is installed on this
host. The source-level comparison and release tests are the available API
review; an automated external-consumer semver diff remains unverified.

## CI and qualification scope

The latest two RavenSky `master` GitHub Rust runs for the integration commits
were cancelled, including the run at `4689ec4`; a green cross-platform CI
result is therefore not yet available for this preparation branch. Local
qualification and package evidence are recorded below as they complete.
The previous integrated local baseline passed 197 tests with three ignored,
strict Clippy, formatting and optimized workspace builds on macOS ARM64.

No crate is uploaded during this preparation assignment.

## Prepared 0.7.0 candidate and local qualification

The candidate changes the root workspace and four public package versions to
0.7.0, updates all internal path-plus-version edges to 0.7.0, and updates the
lockfile's five workspace package identities only. `astro-bench` still inherits
the workspace version for benchmark provenance but remains `publish = false`;
no external dependency was added or upgraded for this preparation. Public
crate metadata now carries science keywords/categories. Each subcrate has a
copy of the root MIT and Apache license texts (with one trailing blank line
removed from each Apache copy for `git diff --check`); `astro-io` also
retains its CFITSIO, HCOMPRESS and fitskit notices.

READMEs and new buildable, read-only examples distinguish inventory, validation,
exact UInt16 pixels, metadata impact and application-level conversion. Public
rustdoc and capability/error strings no longer use internal P1–P4 labels.
The draft release notes are `docs/releases/RavenSkyAstro-0.7.0-draft.md`; the
changelog and current release procedure were updated. The root package excludes
work records, benchmark evidence, local/configuration material, large image
fixtures and the official XISF specification. Subcrate README links to repo
material use stable repository URLs rather than paths outside their archives.

On macOS ARM64 with Rust 1.94.0 and `fitsio/src-cmake` for local CFITSIO build:

- `cargo +1.94.0 fmt --all --check`: passed after formatting the two examples.
- `cargo +1.94.0 test --locked --workspace --all-targets --all-features --features fitsio/src-cmake -- --test-threads=1`: 215 passed, 0 failed, 3 intentionally ignored across 26 test executables. An immediate rerun captured the exact count and no warnings in `/private/tmp/ravensky-p7-all-target-tests.log`.
- `cargo +1.94.0 test --locked --workspace --all-features --features fitsio/src-cmake --doc`: 4 passed, 0 failed.
- `cargo +1.94.0 clippy --locked --workspace --all-targets --all-features --features fitsio/src-cmake -- -D warnings`: passed.
- `RUSTDOCFLAGS='-D warnings' cargo +1.94.0 doc --locked --workspace --all-features --features fitsio/src-cmake --no-deps`: passed.
- `cargo +1.94.0 build --locked --release --workspace --all-features --features fitsio/src-cmake --examples`: passed, including the two new public API examples.
- `git diff --check`: passed. Later README and module-doc edits have no Rust behavior effect; the final clean commit is checked again below.

The ordinary full workspace package verification using Cargo 1.94.0 passed for
all four candidates from a fresh precommit target directory,
`/private/tmp/ravensky-p7-precommit-package`, with `--locked --workspace
--exclude astro-bench --allow-dirty`. Cargo used its temporary local registry
to resolve the unpublished 0.7.0 sibling crates. It packaged and verified:

| Crate | Files | Compressed archive bytes | Local package verification |
| --- | ---: | ---: | --- |
| `astro-io` | 38 | 132,082 | Passed |
| `astro-metadata` | 18 | 50,692 | Passed |
| `astro-metrics` | 13 | 20,667 | Passed |
| `ravensky-astro` | 42 | 163,545 | Passed |

Archive inspection confirmed source, README, manifest, both licenses, new
examples where applicable, and the three `astro-io` third-party notices.
Normalized registry manifests require exactly `0.7.0` on internal edges.
No archive contained the official XISF specification, `docs/work`, benchmark
reports, `.continue`, `.agents`, `.github`, local test image archives, or
machine-specific/credential markers. The archive members are regular files
with safe relative paths. These precommit archive hashes are deliberately not
a final publication identity; Cargo's VCS metadata changes with the final
commit and the final clean archives must be regenerated.

`cargo +1.94.0 publish --dry-run --locked --allow-dirty -p astro-io` passed and
ended with Cargo's expected `warning: aborting upload due to dry run`.
Equivalent nonuploading dry-runs for `astro-metadata`, `astro-metrics` and
`ravensky-astro` stopped during registry resolution because crates.io has no
`astro-io = ^0.7.0` yet. This is the intended dependency gate, not a package
verification failure. After approval, repeat each downstream dry-run only
once its prerequisites are published and indexed. No real upload was invoked.

The public crates.io owner API lists `dostergaard` for all four crates. This
does not verify that the local machine's token is configured or authorized;
no credential was inspected at that initial checkpoint. At that point, no P7
branch had been pushed, and the latest `master` integration CI runs were
cancelled. The subsequent candidate CI results are recorded below. Native
AstroMuninn Windows qualification remains a later product gate.

## Read-only AstroMuninn P7-B/P7-C findings

AstroMuninn root `Cargo.toml` declares workspace/CLI/core 0.11.0 and registry
requirements `astro-io`/`astro-metadata` 0.6.2 with Git patches pinned to
RavenSky commit `4689ec4`; the independent `fitsio-sys` patch points to
`vendor/fitsio-sys`. Lite's Cargo manifest and `tauri.conf.json` both declare
0.4.0. Lite About reads `CARGO_PKG_VERSION`; the CLI uses Clap's package
version. The release workflow checks CLI workspace and both Lite version
sources against their respective tags. `astromuninn-core` inherits workspace
version and uses it for provenance, but configuration and job schema versions
are separate values; changing the product version must not change them.

`release-product.yml` builds CLI macOS ARM64/Linux x86-64 tarballs and Windows
x86-64 GNU ZIP through cargo-dist. Lite uses Tauri macOS ARM64 DMG, Windows
x86-64 MSVC NSIS setup EXE and Linux x86-64 AppImage. The public downloads
channel stages named assets and SHA256SUMS. Existing docs describe unsigned
installation. The public docs identify architectures but do not state a
minimum macOS or Windows OS version; P7-C must establish and test an honest
version floor rather than infer it from CI runner names. The Lite macOS
install script currently strips quarantine from
the installed app silently; its installation page also recommends manual
`xattr -dr` removal. That conflicts with the approved P7-C per-app security
workflow and must be changed during P7-C before product publication. The
Windows installer script invokes `Unblock-File`; evaluate this during native
Windows qualification. Apple's current user guidance supports System Settings
> Privacy & Security > Open Anyway after the user first attempts to open a
trusted downloaded app. Microsoft's current Smart App Control FAQ says Windows
11 may block unsigned apps without a per-app exception. Sources:
[Apple](https://support.apple.com/en-us/102445),
[Microsoft](https://support.microsoft.com/en-gb/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions).

No AstroMuninn files, versions, lockfile, build products, tags or distribution
artifacts were changed or created in this assignment. Detailed later-stage
steps and the native installation matrix are in PLAN.md.

## Remote candidate CI qualification, 2026-11-19

The repository has one GitHub Actions workflow, `.github/workflows/rust.yml`.
Its push event is restricted to `master`/`main`; it also supports pull requests
into those branches and manual `workflow_dispatch`. It contains no crates.io
upload, release-tagging, binary-publication or GitHub Release step. The normal
push of `release/ravensky-astro-p7-preparation` to `origin` started no workflow;
`gh run list --branch` showed only the manually dispatched Rust runs below.
`master` and its unrelated `.gitignore` edit were untouched.

[First manual candidate run 44607491633](https://github.com/dostergaard/ravensky-astro/actions/runs/44607491633)
used `1c73ceabb66c8720fb38095a32eaae8bb454b9e4`. Retired naming and Linux
x86-64 GNU passed, including Linux four-crate package verification. Windows
x86-64 GNU failed at strict Clippy with Rust E0282: `astro-bench`'s
`examples/p1_typed_io.rs` left a Unix-only `peak_rss = None` without an
inferred type on Windows; its later build/test/doc steps were skipped. GitHub
cancelled macOS ARM64 before runner allocation, with annotation
`The configuration 'macos-14-arm64-us-default' is not supported`; no macOS
checks ran. The overall run conclusion was `cancelled`, not a pass.

The narrow correction commit
`6267d1121d44714c984a0961c43e6fd93e7c295b` gives the benchmark example
an explicit `Option<u64>` on both cfg paths and moves the macOS ARM64 runner
from `macos-14` to GitHub's currently listed `macos-15` ARM64 standard runner.
The target remains `aarch64-apple-darwin` and the full macOS job remains in
the matrix. GitHub's [current runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
lists `macos-15` for Apple Silicon. Local Rust 1.94 formatting, strict
`astro-bench` all-target/all-feature Clippy, and affected benchmark tests
passed (24 tests across the package and examples, no ignores). No public
crate implementation or dependency requirement changed.

[Second manual candidate run 44608168246](https://github.com/dostergaard/ravensky-astro/actions/runs/44608168246)
used the correction commit and completed `success`:

| Job | Result | Qualification |
| --- | --- | --- |
| Retired naming | Passed | Canonical crate naming check |
| Linux x86-64 GNU | Passed | Rust 1.94; fmt, strict Clippy, optimized examples, 215 tests/3 ignored, 4 doctests, strict rustdoc, four-crate package verification |
| Windows x86-64 GNU | Passed | Rust 1.94; fmt, strict Clippy, optimized examples, configured `astro-io`/`astro-metadata`/`astro-bench` subset with 187 tests/2 ignored, 3 doctests, strict rustdoc |
| macOS ARM64 | Passed | Rust 1.94; fmt, strict Clippy, optimized examples, 215 tests/3 ignored, 4 doctests, strict rustdoc |

The Linux-only package step was correctly skipped on Windows and macOS.
Windows GNU does not build `astro-metrics`/the facade because the existing
`sep-sys 1.3.0` POSIX `rand_r` dependency is unsupported there. The MSYS2
setup action emitted a MINGW64 deprecation warning; GitHub also noted that
macOS ARM64 runner capacity can delay queueing. Neither warning failed a
check. The exact completed job logs were inspected; there were no test or
Rustdoc failures. This is library CI, not native AstroMuninn Windows MSVC
installation evidence.

Clean Cargo 1.94.0 `cargo package --locked --workspace --exclude astro-bench`
from correction commit `6267d11` passed all four packages. Archive inspection
confirmed the same file counts, dual licenses, README/source, examples and
`astro-io` third-party notices as the initial candidate. All normalized
internal requirements remain 0.7.0; the official XISF document, local paths,
credential markers and work artifacts remain excluded. Checksums at this
code-and-workflow commit are:

| Archive | Files | Bytes | SHA-256 |
| --- | ---: | ---: | --- |
| `astro-io-0.7.0.crate` | 38 | 132,064 | `6e4aaaa66745342f4cf38b7328fb91d3eb6c13bef352a8d2291a1bbb2da49d84` |
| `astro-metadata-0.7.0.crate` | 18 | 50,682 | `42385ee2eae9e65e9ba52af2531a75e8713ac2e03b954639764bbc66843e73ca` |
| `astro-metrics-0.7.0.crate` | 13 | 20,655 | `678d2e3b62c988ec95b6fe2ed9e2389c6489fb1ecd402c314f7f704bb9d6082d` |
| `ravensky-astro-0.7.0.crate` | 42 | 163,462 | `182275c21f35c70adea897555df13e9f681c29ba82d3f20118910ad2ddb93088` |

These hashes identify the 6267d11 archive set. A later documentation-only
evidence commit changes Cargo's `.cargo_vcs_info.json` and cascades sibling
checksums in packaged lockfiles, so regenerate and report final archive
hashes from the exact final commit before publication.

A Cargo `credentials.toml` file is present and has a nonempty default-registry
token entry; no token value was displayed or copied. No token is set in the
checked Cargo environment variables. The official crates.io owner API lists
`dostergaard` for each of `astro-io`, `astro-metadata`, `astro-metrics` and
`ravensky-astro`. File presence and public ownership do not prove which
account the token represents or that an upload will be authorized. No upload
was attempted.

## Final-commit Windows HCOMPRESS oracle race and correction

[Third manual candidate run 44609029486](https://github.com/dostergaard/ravensky-astro/actions/runs/44609029486)
used documentation-only commit
`1590d7ea577be5f097ef66a5b14ac1c82551e2c8`. Retired naming, Linux
x86-64 GNU and macOS ARM64 passed their complete configured jobs, including
all Rust 1.94 checks and Linux four-crate packaging. Windows GNU passed
formatting, strict Clippy and optimized builds, then failed two
`astro-io::validation::fits::hcompress` unit tests. The Windows unit result
was 50 passed, 2 failed, 1 ignored; Windows doctests and rustdoc were skipped
because the test step failed. The run conclusion was `failure`, not a pass.

The failures were corrupted native oracle results from direct
`fits_hcompress`/`fits_hdecompress` calls in tests run concurrently by Rust's
ordinary test harness. The previous [run 44608168246](https://github.com/dostergaard/ravensky-astro/actions/runs/44608168246)
passed those tests without a source change, establishing intermittence.
Inspection of bundled CFITSIO 3.49 source in `fitsio-sys 0.5.7` found
process-global mutable bitstream state in `fits_hcompress.c` (`noutchar`,
`bitcount`, `bitbuffer`) and `fits_hdecompress.c` (`nextchar`). The two direct
native oracle fixtures were therefore serialized with a test-only mutex;
all test cases and assertions remain enabled. No production API or writer
behavior changed, and the CI test matrix was not narrowed or serialized.

Local Rust 1.94 `cargo fmt --all --check`, focused parallel harness execution
of both HCOMPRESS tests (2 passed), and strict all-target/all-feature
`astro-io` Clippy with `fitsio/src-cmake` passed after the fix. The next
candidate commit includes the test-only correction and this evidence; it
must receive a complete new CI run before the approval checkpoint. Final
archive hashes must be regenerated from that exact commit because Cargo
embeds Git commit identity in `.cargo_vcs_info.json`.

## Qualified HCOMPRESS correction candidate

Commit `d091319580004840ef2b28bfd15f04d3053c0096` contains the
test-only HCOMPRESS oracle serialization and this evidence. Its
[manual candidate CI run 44610247661](https://github.com/dostergaard/ravensky-astro/actions/runs/44610247661)
completed `success`: retired naming, Linux x86-64 GNU, macOS ARM64 and
Windows x86-64 GNU all passed. Each configured platform passed Rust 1.94
formatting, strict Clippy, optimized examples, tests, doctests and strict
rustdoc. Linux also passed four-crate package verification. Package
verification was intentionally skipped on macOS and Windows, and only the
configured three-package subset ran on Windows because `sep-sys` does not
support that target. Native setup steps for other platforms were skipped as
configured. No test or documentation job was cancelled in this run.

A fresh Cargo 1.94 `cargo package --locked --workspace --exclude astro-bench`
verification and archive inspection from this clean commit passed. The
archives retain dual licenses, README/source and relevant examples;
`astro-io` retains the CFITSIO/HCOMPRESS/fitskit notices. Internal normalized
requirements are 0.7.0, and documentation sources, work artifacts, local
paths and credential markers are absent. Archive identity is:

| Archive | Files | Bytes | SHA-256 |
| --- | ---: | ---: | --- |
| `astro-io-0.7.0.crate` | 38 | 132,237 | `d77948cc89c80d9c81a692ebfc6791b3dca34ae85831630bc2408211264a1b80` |
| `astro-metadata-0.7.0.crate` | 18 | 50,685 | `91a65e7a402f56ce0d49cdc34206ec09c7945072bcef28a51ea2f545ce4495e9` |
| `astro-metrics-0.7.0.crate` | 13 | 20,664 | `4485817bfdbd65fd2a29e0cf1c797c494aae6caf47a51548d2a3890c3a6084c9` |
| `ravensky-astro-0.7.0.crate` | 42 | 163,475 | `ffd8ebca2698f04b8a26a324f5baffdd6a36f2d784552590846376fd7494b3d5` |

The bundled CFITSIO source also shows that high-level compressed image
operations call the same native HCOMPRESS functions with process-global
mutable state. This CI result proves the direct test fixtures no longer race;
it does not establish safety of concurrent production HCOMPRESS reads through
independent CFITSIO handles. The existing `with_cfitsio` gate admits such
calls when `fits_is_reentrant()` reports true. Assess that native concurrency
contract before relying on concurrent HCOMPRESS reads in a release. No
production concurrency behavior was changed in this candidate.
