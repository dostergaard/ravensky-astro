# P7-A RavenSky Astro publication preparation evidence

## Release documentation and master integration closure, 2026-11-20

The official crates.io version and dependency APIs, downloaded archives,
embedded `.cargo_vcs_info.json`, and docs.rs build pages were checked again
at closure. All four versions are non-yanked, declare `MIT OR Apache-2.0`
and Rust 1.94, and have the expected `^0.7.0` shared-crate dependency edges.
Each downloaded archive matched both the registry checksum and the approved
package hash. The packages originated from successive commits:

| Published crate | Source commit embedded in archive | Archive bytes | SHA-256 | docs.rs build |
| --- | --- | ---: | --- | --- |
| `astro-io 0.7.0` | `5e25bd7a20781aafdc5f94dc22f4f1a343913a0c` | 132,834 | `347071fb00b76c080a647053ce3397908c54cf87a6afa6752510015a924eb9c7` | [5405037](https://docs.rs/crate/astro-io/0.7.0/builds/5405037), succeeded |
| `astro-metadata 0.7.0` | `d905555217fdfe2f4cba3261f7d9a8d6dcf68a5d` | 51,828 | `bf9419762cd7c5a518095491c0e116bdbf0f820e6b11a8903e2933411f694560` | [5405373](https://docs.rs/crate/astro-metadata/0.7.0/builds/5405373), succeeded |
| `astro-metrics 0.7.0` | `2c3345af901acd15709dbdf15d321b19bdc94124` | 20,653 | `d01669e7229477bb6fdf997b58a7e366945c05d61215062541c6fb93cf886fa0` | [5405744](https://docs.rs/crate/astro-metrics/0.7.0/builds/5405744), succeeded |
| `ravensky-astro 0.7.0` | `6b1eeede00089a7cdc5488d75406d500d94efd6d` | 164,265 | `f07ca6796366ba5966429d2e3098f6fc1e9d3a557690fce270d883ca8ed8f5ea` | [5405807](https://docs.rs/crate/ravensky-astro/0.7.0/builds/5405807), succeeded |

An independent temporary Rust 1.94 consumer declared only
`ravensky-astro = "=0.7.0"` and passed `cargo check --locked`. Its lockfile
and Cargo metadata resolved one registry-sourced 0.7.0 copy of each crate,
with the four checksums above and no path or Git patch. This is a public
registry graph check, separate from the successful candidate CI
[run 44721965333](https://github.com/dostergaard/ravensky-astro/actions/runs/44721965333)
at implementation commit `5e25bd7a20781aafdc5f94dc22f4f1a343913a0c`.
Later branch commits before this closure changed publication evidence only.

`CHANGELOG.md` now dates 0.7.0 to 2026-11-20. `RELEASING.md` states that all
four crates are published and links the completed
`docs/releases/RavenSkyAstro-0.7.0.md`. The original `-draft.md` remains as
a historical publication candidate. The final notes preserve the exact
monochrome UInt16 writer limit, metadata impact review, and the boundary
between library operations and application-level conversion transactions.
The README links the final notes and describes native CFITSIO/SEP backends
accurately. P7-A's earlier plans and publication checkpoints remain in their
historical context below.

Post-documentation local validation on macOS ARM64 with Rust 1.94 and
`fitsio/src-cmake` passed: `cargo fmt --all --check`, strict workspace
all-target/all-feature Clippy (`-D warnings`), 217 workspace all-target
tests with three established ignores, and four workspace doctests.
`cargo metadata --locked` confirmed the four public 0.7.0 packages, Rust
1.94 and `astro-bench`'s unpublishable status. All changed local Markdown
links resolved and `git diff --check` passed. No Rust source or manifest
changed in this closure. The already published crate archives are immutable;
their embedded README files reflect their individual publication commits,
while the current Git README and final release notes are post-publication
documentation.

The established Git tag form is `v0.x.y` (`v0.2.0`, `v0.3.0`, `v0.5.0`,
`v0.6.1`, `v0.6.2`). The proposed `v0.7.0` tag target is the final
release-documentation closure commit containing this section, after it is
fast-forwarded to `master` and verified on `origin/master`. It is not any
single crate's source commit: those four immutable archives retain the
distinct revisions in the table. No tag or GitHub Release is authorized yet.
The exact full closure-commit SHA is reported after the ordinary master push
in the release-closure run result; a tracked file cannot embed its own Git
commit hash. At closure, `master`, `origin/master` and the preparation branch
are verified at this same commit. The original checkout's unrelated
`.gitignore` edit remains uncommitted and byte-identical to its preintegration
state.

### RavenSky Astro worktree inventory

This inventory was taken before the authorized fast-forward. The only HEAD
that changes during integration is the original `master` worktree, from
`4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2` to the closure commit.
The release-preparation worktree advances to the same closure commit when
this record is committed. Clean ancestor worktrees may be considered for
removal only after AstroMuninn v1.0.0 is released and both repositories are
reviewed together. No worktree or branch was removed or pruned.

| Worktree under RavenSky workspace | Branch | Preintegration HEAD | State and release relationship |
| --- | --- | --- | --- |
| `ravensky-astro/` | `master` | `4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2` | Unrelated modified `.gitignore`; preserve checkout and edit |
| `.worktrees/ravensky-astro-p0` | `research/image-conversion-compression-p0` | `03f28b7dcb51cb16dc3f9c16fd75bea5ba334fa9` | Clean but divergent: two commits not reachable from the release branch; review before any cleanup |
| `.worktrees/ravensky-astro-p1` | `feature/image-conversion-compression-p1` | `406ff61853489eec2c86e82fcc84cc9e34be5729` | Clean ancestor; possible later removal |
| `.worktrees/ravensky-astro-p2` | `feature/image-conversion-compression-p2` | `ef4688baf2b927bc1004cf5b8f8994d0c04a431a` | Clean ancestor; possible later removal |
| `.worktrees/ravensky-astro-p3` | `feature/image-conversion-compression-p3` | `ef4688baf2b927bc1004cf5b8f8994d0c04a431a` | Clean ancestor; possible later removal |
| `.worktrees/ravensky-astro-p4` | `feature/image-conversion-compression-p4` | `5205f133ed9d22ea47f3879d3904d8f778a6d64c` | Clean ancestor; possible later removal |
| `.worktrees/ravensky-astro-p5` | `feature/image-conversion-compression-p5` | `59c6de1d5f53b182b1c1033acd423d53d7fc5796` | Clean ancestor; possible later removal |
| `.worktrees/ravensky-astro-p7-preparation` | `release/ravensky-astro-p7-preparation` | `07e0d5a0fa7474ff5b090f1185af795ed681d0ae` | Clean release worktree; possible later removal after integration |
| `.worktrees/ravensky-astro-v1-baseline` | `integration/astromuninn-v1.0.0-baseline` | `4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2` | Clean ancestor; retain through AstroMuninn release, then consider removal |

Actual worktree cleanup is deferred until after AstroMuninn v1.0.0 release.
At that point, inventory both repositories again, preserve necessary release
history and outstanding user changes, review P0's unique commits, and only
then decide which worktrees and branches to remove.

## `ravensky-astro 0.7.0` publication and registry-only graph, 2026-11-20

The user separately approved the exact `ravensky-astro 0.7.0` candidate
from clean `release/ravensky-astro-p7-preparation` commit
`6b1eeede00089a7cdc5488d75406d500d94efd6d`. Local and remote refs
matched before upload; `origin/master` remained
`4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2`. The original `master`
checkout retained its unrelated `.gitignore` edit. The official
[version API](https://crates.io/api/v1/crates/ravensky-astro/0.7.0)
returned HTTP 404 before upload, while all three published 0.7.0
prerequisites were available with their approved checksums.

`cargo +1.94.0 package --locked -p ravensky-astro` verified a fresh package
against registry dependencies. Its normalized manifest required `astro-io`,
`astro-metadata` and `astro-metrics` at 0.7.0 with no active path or Git
override. The extracted package's Cargo metadata resolved exactly one
registry-sourced 0.7.0 copy of each shared crate; its lockfile recorded
their published checksums and no 0.6.2 shared crate. The package contained
all three public re-exports, README, rustdoc, examples, dual licenses,
source and expected documentation. Its 42 regular archive members had safe
paths, and inspection found no machine paths, credentials, work records or
official XISF specification. The extracted package's Rust 1.94 all-target
tests passed (19 tests) and its doctest passed using the registry graph.

The final `cargo +1.94.0 publish --dry-run --locked -p ravensky-astro`
passed, including package verification, and produced a byte-identical
archive. Its only warning was the expected dry-run upload abort. The
approved `ravensky-astro-0.7.0.crate` was 164,265 bytes, SHA-256
`f07ca6796366ba5966429d2e3098f6fc1e9d3a557690fce270d883ca8ed8f5ea`.
HEAD and the worktree remained unchanged before upload. With the user's
specific authorization, `cargo +1.94.0 publish --locked -p ravensky-astro`
completed; Cargo reported upload and publication at `crates-io`.

The [published version](https://crates.io/crates/ravensky-astro/0.7.0),
[version API](https://crates.io/api/v1/crates/ravensky-astro/0.7.0) and
[dependency API](https://crates.io/api/v1/crates/ravensky-astro/0.7.0/dependencies)
report the approved SHA-256, 164,265 bytes, license `MIT OR Apache-2.0`,
Rust 1.94, the project repository, non-yanked 0.7.0 and all three
`^0.7.0` shared dependencies. The
[published archive](https://static.crates.io/crates/ravensky-astro/ravensky-astro-0.7.0.crate)
was downloaded and byte-compared with the approved candidate. Its
`.cargo_vcs_info.json` points to exactly `6b1eeed...` at the repository
root. [docs.rs build 5405807](https://docs.rs/crate/ravensky-astro/0.7.0/builds/5405807)
reported `All builds succeeded`, and the versioned rustdoc page rendered.
The initial docs.rs HTTP 404 immediately after upload was pending ingestion,
not a build failure.

An independent temporary consumer outside the RavenSky workspace declared
only `ravensky-astro = "=0.7.0"`, with no path, Git or patch override.
`cargo +1.94.0 generate-lockfile` and `cargo +1.94.0 check --locked`
passed. Its Cargo metadata resolved exactly one registry-sourced 0.7.0 copy
of each of the four public crates; its lockfile checksums matched the four
approved published archives. This confirms the complete released graph
builds from the public registry on Rust 1.94. It does not replace the prior
platform CI or native AstroMuninn product qualification.

All four public crates are now published and verified. No merge, release
tag, GitHub Release, binary publication or AstroMuninn change was made.
Before a public tag or merge, review and update release guidance that still
describes 0.7.0 as proposed, safely integrate the branch while preserving
the original checkout's `.gitignore` edit, and obtain the separate approval
required by `RELEASING.md` for a public tag.
AstroMuninn P7-B remains a later dedicated-branch task.

## `astro-metrics 0.7.0` publication, 2026-11-20

The user separately approved the exact `astro-metrics 0.7.0` candidate
from clean `release/ravensky-astro-p7-preparation` commit
`2c3345af901acd15709dbdf15d321b19bdc94124`. Local and remote refs
matched before upload. The official crates.io version API returned HTTP 404
for `astro-metrics 0.7.0`; the published `astro-io` and `astro-metadata`
0.7.0 versions were available with their approved checksums. The
`astro-metrics` source tree was identical to the tested
`5e25bd7a20781aafdc5f94dc22f4f1a343913a0c` implementation; only
publication evidence changed since that CI run. Its normalized manifest
required both shared crates at 0.7.0, with no path or Git override. The
extracted package's Cargo metadata resolved exactly one registry-sourced
copy of each; its packaged lockfile recorded their official checksums.

`cargo +1.94.0 package --locked -p astro-metrics` verified the fresh package
against registry dependencies. The extracted package's Rust 1.94 tests with
`fitsio/src-cmake` passed: 5 passed, 1 established ignore. The final
`cargo +1.94.0 publish --dry-run --locked -p astro-metrics` passed and
produced a byte-identical archive. Its only warning was the expected dry-run
upload abort. The approved `astro-metrics-0.7.0.crate` was 20,653 bytes,
SHA-256 `d01669e7229477bb6fdf997b58a7e366945c05d61215062541c6fb93cf886fa0`.
HEAD and the worktree remained unchanged before upload. With the user's
specific authorization, `cargo +1.94.0 publish --locked -p astro-metrics`
completed; Cargo reported upload and publication at `crates-io`.

The [published version](https://crates.io/crates/astro-metrics/0.7.0) and
[version API](https://crates.io/api/v1/crates/astro-metrics/0.7.0) report
the approved SHA-256, 20,653 bytes, license `MIT OR Apache-2.0`, Rust 1.94,
the project repository and a non-yanked 0.7.0 version. The
[registry dependencies](https://crates.io/api/v1/crates/astro-metrics/0.7.0/dependencies)
include `astro-io ^0.7.0` and `astro-metadata ^0.7.0` and match the
normalized manifest. The [published archive](https://static.crates.io/crates/astro-metrics/astro-metrics-0.7.0.crate)
was downloaded and byte-compared with the approved local candidate. Its 13
regular, safe-path files include source, README, dual licenses, normalized
manifest and `.cargo_vcs_info.json` pointing to exactly `2c3345a...` and
`astro-metrics`. No unexpected local or credential-marked files were found.

At 2026-11-20 12:14 UTC, docs.rs returned HTTP 404 for the new version's
[build page](https://docs.rs/crate/astro-metrics/0.7.0/builds) and
[rustdoc URL](https://docs.rs/astro-metrics/0.7.0/astro_metrics/). This was
immediately after publication and indicated pending docs.rs ingestion, not
an observed build failure. Shortly afterward,
[build 5405744](https://docs.rs/crate/astro-metrics/0.7.0/builds/5405744)
reported `All builds succeeded`, and the versioned rustdoc page rendered.

Only `astro-metrics` was uploaded in this step. `ravensky-astro` remains
unpublished. No merge, tag, binary release, or AstroMuninn change was made.
This evidence-only commit changes the branch's VCS identity and future
archive hashes; prepare the facade from its exact new commit and obtain its
separate publication approval.

## `astro-metadata 0.7.0` publication, 2026-11-20

The user separately approved the exact `astro-metadata 0.7.0` candidate
from clean `release/ravensky-astro-p7-preparation` commit
`d905555217fdfe2f4cba3261f7d9a8d6dcf68a5d`. The local and remote
branch refs matched. Before upload, the official
[crates.io version API](https://crates.io/api/v1/crates/astro-metadata/0.7.0)
returned HTTP 404; `astro-io 0.7.0` was available with its approved checksum.
The prior `cargo +1.94.0 package --locked -p astro-metadata` verification
downloaded `astro-io 0.7.0` from crates.io, and Cargo metadata for the
extracted package identified that dependency as registry sourced. The
normalized manifest has `astro-io = "0.7.0"` with no path or Git override.

The final `RUSTUP_TOOLCHAIN=1.94.0 cargo publish --dry-run --locked -p
astro-metadata` passed, including package verification. Its upload archive
was byte-identical to the separately generated 51,828-byte candidate, SHA-256
`bf9419762cd7c5a518095491c0e116bdbf0f820e6b11a8903e2933411f694560`.
HEAD and the worktree remained unchanged before upload. With the user's
specific authorization, `RUSTUP_TOOLCHAIN=1.94.0 cargo publish --locked -p
astro-metadata` completed; Cargo reported upload and publication at registry
`crates-io`.

The [published version](https://crates.io/crates/astro-metadata/0.7.0) and
[version API](https://crates.io/api/v1/crates/astro-metadata/0.7.0) report
the approved SHA-256, 51,828 bytes, license `MIT OR Apache-2.0`, Rust 1.94,
the project repository, and a non-yanked 0.7.0 version. The
[registry dependencies](https://crates.io/api/v1/crates/astro-metadata/0.7.0/dependencies)
include `astro-io ^0.7.0` and match the normalized manifest. The
[downloaded published archive](https://static.crates.io/crates/astro-metadata/astro-metadata-0.7.0.crate)
was byte-compared with the approved local candidate. Its 18 files include
the source, example, tests, README, both license files, normalized manifest,
and `.cargo_vcs_info.json` pointing to exactly `d905555...` and
`astro-metadata`. Its packaged lockfile records the registry `astro-io 0.7.0`
checksum `347071fb00b76c080a647053ce3397908c54cf87a6afa6752510015a924eb9c7`.

At 2026-11-20 11:48 UTC, docs.rs returned HTTP 404 for the new version's
[build page](https://docs.rs/crate/astro-metadata/0.7.0/builds) and
[rustdoc URL](https://docs.rs/astro-metadata/0.7.0/astro_metadata/). This was
immediately after publication and indicated pending docs.rs ingestion, not
an observed build failure. Shortly afterward,
[build 5405373](https://docs.rs/crate/astro-metadata/0.7.0/builds/5405373)
reported `All builds succeeded`, and the versioned rustdoc page rendered.

Only `astro-metadata` was uploaded in this step. `astro-metrics` and
`ravensky-astro` remain unpublished. No merge, tag, binary release, or
AstroMuninn change was made. This evidence-only commit changes the branch's
VCS identity and therefore future package hashes; prepare and obtain
separate approval for each remaining crate from its exact new commit.

## `astro-io 0.7.0` publication, 2026-11-20

The final qualified release candidate was commit
`5e25bd7a20781aafdc5f94dc22f4f1a343913a0c` on
`release/ravensky-astro-p7-preparation`, with
[full candidate CI run 44721965333](https://github.com/dostergaard/ravensky-astro/actions/runs/44721965333)
passing Linux x86-64 GNU, macOS ARM64, Windows x86-64 GNU, and retired
naming jobs. The branch worktree was clean and `git ls-remote` confirmed
`origin/release/ravensky-astro-p7-preparation` at the same commit. The original
RavenSky `master` checkout retained its unrelated `.gitignore` edit;
AstroMuninn was untouched. Before upload, the official
[crates.io version API](https://crates.io/api/v1/crates/astro-io/0.7.0)
returned HTTP 404 while the crate's latest version was 0.6.2.

`cargo +1.94.0 package --locked --workspace --exclude astro-bench --features
fitsio/src-cmake` regenerated and verified the candidate packages. Its
`astro-io-0.7.0.crate` SHA-256 was
`347071fb00b76c080a647053ce3397908c54cf87a6afa6752510015a924eb9c7`,
matching the user-approved archive. `RUSTUP_TOOLCHAIN=1.94.0 cargo publish
--dry-run --locked -p astro-io` passed, including package verification, and
its upload archive had the same SHA-256. The worktree and HEAD remained clean
and unchanged. With the user's specific authorization, `RUSTUP_TOOLCHAIN=1.94.0
cargo publish --locked -p astro-io` completed successfully; Cargo reported
both upload and publication at registry `crates-io`.

The [published version](https://crates.io/crates/astro-io/0.7.0) and
[version API](https://crates.io/api/v1/crates/astro-io/0.7.0) report 0.7.0,
checksum `347071fb00b76c080a647053ce3397908c54cf87a6afa6752510015a924eb9c7`,
license `MIT OR Apache-2.0`, Rust version 1.94, and repository
`https://github.com/dostergaard/ravensky-astro`; the version is not yanked.
The [published archive](https://static.crates.io/crates/astro-io/astro-io-0.7.0.crate)
is 132,834 bytes and was downloaded and byte-compared with the approved local
package. It contains 38 files, including source, README, both primary
license files and the three third-party notices. Its
`.cargo_vcs_info.json` identifies exactly commit `5e25bd7...` and path
`astro-io`. The [registry dependency metadata](https://crates.io/api/v1/crates/astro-io/0.7.0/dependencies)
matches the packaged manifest: external normal dependencies and dev-only
`fs4`; no unpublished RavenSky sibling dependency.

At 2026-11-20 11:08 UTC, the official
[docs.rs build page](https://docs.rs/crate/astro-io/0.7.0/builds) showed
`in progress`; [version documentation](https://docs.rs/astro-io/0.7.0/astro_io/)
redirected to the crate landing page, which said the build would be available
soon. Shortly afterward,
[build 5405037](https://docs.rs/crate/astro-io/0.7.0/builds/5405037)
showed `All builds succeeded`, and the versioned rustdoc URL returned the
rendered `astro_io` documentation. docs.rs used its own Rust nightly for
rustdoc; the separate candidate CI and publication checks used Rust 1.94.

`astro-metadata`, `astro-metrics`, and `ravensky-astro` were not published.
No merge, tag, binary release, or AstroMuninn change was made. This evidence
update changes the release branch commit and thus Cargo's embedded VCS
identity for future package archives; requalify and hash each remaining crate
before seeking its separate publication authorization.

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
Initial inspection of bundled CFITSIO 3.49 source in `fitsio-sys 0.5.7`
found process-global mutable bitstream state in `fits_hcompress.c`
(`noutchar`, `bitcount`, `bitbuffer`) and `fits_hdecompress.c` (`nextchar`).
The two direct native oracle fixtures were temporarily serialized with a
test-only mutex. A later complete source review found CFITSIO's own `FFLOCK`
protection around those state accesses; see the review below. The temporary
test mutex is removed in that review. No production API or writer behavior
changed, and the CI test matrix was not narrowed or serialized.

Local Rust 1.94 `cargo fmt --all --check`, focused parallel harness execution
of both HCOMPRESS tests (2 passed), and strict all-target/all-feature
`astro-io` Clippy with `fitsio/src-cmake` passed after the fix. The next
candidate commit included the test-only correction and this evidence. It
subsequently received complete [CI run 44610247661](https://github.com/dostergaard/ravensky-astro/actions/runs/44610247661)
below. Cargo embeds Git commit identity in `.cargo_vcs_info.json`, so each
new committed candidate needs its own archive hashes.

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

At this checkpoint, the native HCOMPRESS state was known to be shared, but
the surrounding CFITSIO lock and initialization path had not yet been traced.
The follow-up review below resolves that question. No production concurrency
behavior changed in this candidate.

Documentation commit `b16bbc268c954516d18ecb544482305f408be65e`
received [CI run 44611228993](https://github.com/dostergaard/ravensky-astro/actions/runs/44611228993):
Linux x86-64 GNU, macOS ARM64, Windows x86-64 GNU and retired naming all
passed. Its locally generated archives were inspected and hashed from the
clean commit; Linux CI verified package build. The local archive generation
used `cargo package --offline --locked --no-verify` because the host's
temporary directory denied a bundled CFITSIO Autotools operation during
local tarball verification. The b16bbc2 archive SHA-256 values, in
publication order, are `fb91d22f76bbc3d162e7c334a71f712776054435d6caec522ad3fae52b562b69`,
`02f775b82c8d8adcba4caec9e05c3a2ff451402e41f1f940c8366cea5d5387cc`,
`b57719830613351095dddbfc1b7f1cc2cf1eb342c380a02115751110c6c53e30`,
and `d19cb8bb140c2e28e1902e9e5cfa09890e800380a7ca545011c154dae38cbe86`.

## P7-A final HCOMPRESS concurrency source review

This review corrects the earlier inference that process-global HCOMPRESS
state implied an unprotected production race. In bundled `fitsio-sys 0.5.7`
(CFITSIO 3.49), `ext/cfitsio/fitsio2.h:20-36` defines `FFLOCK`/`FFUNLOCK`
as calls on `Fitsio_Lock` when `_REENTRANT` is set, and as no-ops otherwise.
`fits_hcompress.c:115-120,162-167` locks the 32-bit and 64-bit encoder
sections that use `noutchar`, `noutmax`, `bitcount` and bit buffers.
`fits_hdecompress.c:106-108,149-151` locks both decode sections that use
`nextchar` and input bit buffers. The transforms outside those sections use
caller-owned arrays. No separate high-level mutex exists in `imcompress.c`;
it reaches these locked native functions at lines 2107 and 6314-6318.

`cfileio.c:67-109` initializes the recursive `Fitsio_Lock` through
`fitsio_init_lock`; `fits_init_cfitsio` at line 4329 calls it. The FITS open
and create entry points call `fits_init_cfitsio` before normal image work
(for example lines 628 and 3989). Direct `fits_hcompress` and
`fits_hdecompress` calls do not invoke that initialization themselves.
`fitscore.c:7322-7334` shows that `fits_is_reentrant()` merely reports the
`_REENTRANT` build macro and explicitly warns that it may not suffice for
every function. The bundled Autotools path requests `--enable-reentrant`;
the CMake path sets `USE_PTHREADS=ON`, which defines `_REENTRANT` in
`CMakeLists.txt:64-67`. The local CMake-linked Rust 1.94 test reported
`is_reentrant() == true`. If a supported build reports false, RavenSky's
`with_cfitsio` gate admits only one thread at a time.

The production decode path is:

1. `astro-io::fits::load_fits` or `astro-io::typed::read_u16_chunk` opens an
   independent `FitsFile` and calls `FitsHdu::read_image` or `read_section`.
2. `fitsio 0.21.9` delegates through `ReadImage::read_section`,
   `fits_read_img` and CFITSIO `ffgpv`. Its `getcol*.c` dispatch reaches
   `fits_read_compressed_img` or `fits_read_compressed_pixels` for compressed
   image HDUs.
3. `imcompress.c` calls `imcomp_decompress_tile`, which calls the internally
   locked `fits_hdecompress` or `fits_hdecompress64` for `HCOMPRESS_1`.

`astro-io::typed::inventory` reads HDU information and raw cards only;
`astro-metadata::fits_parser` selects image HDUs and reads headers only.
They do not decompress HCOMPRESS pixels. Full FITS validation dispatches to
RavenSky's Rust HCOMPRESS decoder, with no native calls. Exact typed
comparison reaches the native path through `read_u16_chunk` after Rust
validation. This distinction preserves concurrent unrelated FITS work and
pure-Rust validation without a new library-wide lock.

The original direct oracle tests bypassed file open and therefore had no
guarantee that CFITSIO's lock had been initialized. They now explicitly call
`fits_init_cfitsio` before native codec work, then allow Rust's harness to
run them concurrently without the temporary test mutex. A new direct test
runs four workers × 32 lossless encode/decode cycles with distinct signed
samples and exact pixel assertions. A cross-crate integration test uses two
disposable 64×64 UInt16 HCOMPRESS files and four workers × 24 iterations,
alternating same-file and different-file reads through independent handles.
It exercises `load_fits`, `read_u16_chunk`, direct `fitsio` readback, metadata
extraction, and full validation. Every logical pixel is checked; fixed FNV-1a
UInt16 checksums are `e36fa1e56e700325` and `3b9125d5e279c325`, and the
source files must remain byte-identical. Twenty local repeats of each test
group passed on the reentrant bundled CMake build. Repetition is supporting
evidence, not proof of thread safety; the lock coverage in source is the
decisive evidence.

The precise trigger of the earlier intermittent Windows failure remains an
inference: direct native calls may have encountered an uninitialized mutex,
but that single failed run cannot isolate every possibility. The tests now
initialize CFITSIO explicitly, and full cross-platform candidate CI must
confirm the change. No production synchronization or public API changed.
Calls by unrelated users of CFITSIO are outside RavenSky's handle-lifetime
gate; they must initialize the backend, use independent handles, and on
non-reentrant builds coordinate native access. Re-audit a different CFITSIO
version or separately linked copy rather than assuming these source details
apply to it.

Local Rust 1.94.0 qualification on macOS ARM64 with the bundled
`fitsio/src-cmake` build passed: `cargo fmt --all --check`, `git diff --check`,
strict all-target/all-feature workspace Clippy, full all-target workspace
tests, workspace doctests, strict `RUSTDOCFLAGS=-D warnings` documentation,
and optimized workspace example builds. The complete suite included the new
native and cross-crate regression tests. The candidate still requires clean
commit package verification and the existing full GitHub Actions workflow;
this local result is not a claim of Windows or Linux execution.
