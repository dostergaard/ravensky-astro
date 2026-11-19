# AstroMuninn v1.0.0 P7 release preparation plan

## Objective

Prepare the enhanced RavenSky Astro public crates for a reviewed crates.io
release, then define the later AstroMuninn registry transition and unsigned
product qualification. This assignment stops before any crate upload, product
version change, release tag or binary publication.

## Constraints and current state

RavenSky Astro `master` and `origin/master` were verified at
`4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2`; the original checkout has
an unrelated `.gitignore` edit. AstroMuninn `master` and `origin/master` were
verified at `b1f0a12dd68daa660cd8890f86292845980a09f3` and are clean.
Work proceeds only in the clean RavenSky worktree on
`release/ravensky-astro-p7-preparation`. The four public crates are
`astro-io`, `astro-metadata`, `astro-metrics` and `ravensky-astro`; official
crates.io metadata reports 0.6.2 as their latest published version.
`astro-bench` has `publish = false`.

The P5 source adds typed exact UInt16 FITS/XISF access and metadata fidelity
APIs to `astro-io` and `astro-metadata`. AstroMuninn currently pins those two
crates to the verified Git revision because registry 0.6.2 predates the work.
The existing four-crate release line uses one workspace version and local
version-qualified dependency edges. Version choice must account for public
API compatibility and avoiding duplicate old/new shared types in consumers.

## Proposed design and affected areas

1. Compare exported APIs and behavior against tag `v0.6.2`, including enum
   exhaustiveness, trait and signature changes, MSRV, features and external
   dependencies. Recommend a version for each public crate before editing
   manifests. Keep `astro-bench` unpublished.
2. Update only needed RavenSky manifests, public READMEs/rustdoc, changelog and
   release guidance. Document separately what can be read, inventoried,
   written, compressed, exactly compared and cross-format converted. Preserve
   MIT/Apache-2.0 licensing and avoid bundling the official XISF specification
   in crate archives. No format, transaction, resource or concurrency behavior
   changes are planned.
3. Qualify the actual archive contents and local package verification in
   dependency order. Attempt nonuploading `cargo publish --dry-run` only where
   registry prerequisites make it meaningful. Record blocked dry-runs exactly;
   never substitute an old sibling version to make one pass.
4. Record P7-B's future AstroMuninn manifest/lockfile, product version,
   configuration and journal checks without changing that repository. Record
   P7-C's existing unsigned artifact channels, native install matrix,
   checksum and final release-approval gates.

Affected RavenSky files are expected to include `Cargo.toml`, crate manifests,
crate READMEs, `CHANGELOG.md`, `RELEASING.md`, and this work record. Source
changes are limited to documentation or examples if inspection proves them
necessary. AstroMuninn is read-only in this assignment.

## Execution and verification

1. Audit Git refs, worktrees, tags, registry versions and owners, manifests,
   internal dependency graph, CI, prior release evidence and P0–P6 handoffs.
2. Produce a version and compatibility matrix; resolve any genuine breaking
   issue before finalizing version-qualified dependency edges.
3. Prepare manifests and accurate public documentation and examples; check
   package metadata, licenses and the XISF attribution boundary.
4. Run formatting, all-target/all-feature workspace tests, doc tests, strict
   Clippy, rustdoc, optimized workspace/examples builds and `git diff --check`.
   Compare with the integrated 197 passed / 3 ignored baseline.
5. Use fresh package target directories for `cargo package --list`, package
   archives, Cargo verification and archive-content inspection. Audit archive
   members and hashes with the existing release tooling. Run viable
   `cargo publish --dry-run` commands; record registry-unavailable sibling
   dependencies as a gate rather than a pass.
6. Commit focused local preparation changes, leave the worktree clean, and
   present exact artifacts, limitations and proposed publication commands for
   separate approval. Do not push this branch unless separately authorized.

## Independent stage gates

- **P7-A:** Version/API review, crate docs/manifests, local tests, verified
  archives and recorded dry-runs complete. Stop before first upload; user
  approves exact crate names, versions and dependency order.
- **P7-B:** Only after published crates are verified, replace AstroMuninn's
  Git overrides with registry dependencies while retaining the vendored
  `fitsio-sys` patch; set CLI/workspace and Lite to 1.0.0; verify existing
  config and schema-1 job compatibility with locked clean builds.
- **P7-C:** Only after P7-B, qualify unsigned distributable artifacts and
  clean/upgrade installs on supported platforms, generate SHA-256 checksums,
  review public documentation and obtain separate final tag/publication
  approval. Native Windows results require an actual Windows environment.

## Risks and open questions

`astro-metadata` depends on `astro-io`, `astro-metrics` depends on both, and
the facade re-exports all three. Registry availability may block dry-runs of
later crates until earlier new versions are published. Cargo package archives
must be checked from fresh output paths because the 0.6.2 release observed a
reused-target archive defect. The current CI toolchain is Rust 1.94; the host
uses 1.100, so minimum-version qualification is a separate check. Owner/API
availability and cross-platform CI may limit the prepublication claim. No
new performance or native product acceptance is inferred from this work.

## P7-B execution plan after registry publication

1. Confirm all four 0.7.0 registry versions and docs.rs builds, then verify an
   independent consumer resolves the published packages without Git patches.
2. On a dedicated AstroMuninn P7-B branch, change root `Cargo.toml` workspace
   `astro-io` and `astro-metadata` requirements to 0.7.0 and remove only their
   two `[patch.crates-io]` Git entries. Retain the unrelated vendored
   `fitsio-sys` path patch. Update `Cargo.lock` and inspect `cargo metadata`
   source IDs for core, CLI, Lite, and the vendored benchmark consumer; no
   stray 0.6.2 shared crate should remain. Strengthen CI's
   `scripts/ci/verify-ravensky-backend.py` or a focused companion check to
   assert registry source plus the intended `fitsio-sys` patch.
3. Change root workspace version from 0.11.0 to 1.0.0 (inherited by CLI and
   `astromuninn-core`). Change Lite's independent version in
   `apps/astromuninn-lite/src-tauri/Cargo.toml` and `tauri.conf.json` from
   0.4.0 to 1.0.0. Check CLI Clap `--version`, Lite About (via
   `CARGO_PKG_VERSION`), Tauri bundle metadata, workflow tag/version checks,
   and installer asset naming. Retain the core crate's workspace inheritance;
   `vendor/astro-bench` and data/journal schema versions are independent and
   must not be advanced simply to match the product number.
4. Update current product docs, manuals, versioned release notes and public
   downloads instructions, including `README.md`, `RELEASING.md`,
   `docs/AstroMuninnLiteUserManual.md`, and the public-downloads bootstrap
   pages. Preserve historical versioned notes as history. Check release
   scripts/workflows for exact version assumptions before changing them.
5. Test old configuration and saved IMAGETYP mappings, default and source
   directories, schema-1 conversion plan/journal and job history/Resume
   against existing fixtures and representative upgrade copies. Run full
   locked core/CLI/Lite tests, strict Clippy, platform CI and clean-checkout
   builds with the vendored native backend. Do not introduce config or journal
   migration solely because the product version changes.

P7-B is complete only after registry-only dependency resolution, 1.0.0
metadata consistency, compatibility regression evidence, clean-checkout
builds and review; it does not by itself authorize product publication.

## P7-C unsigned distribution and native qualification plan

The existing CLI `cargo-dist` flow produces Apple Silicon macOS and Linux
x86-64 `.tar.gz` archives and Windows x86-64 GNU `.zip`; Lite's Tauri flow
produces Apple Silicon `.dmg`, Windows x86-64 MSVC NSIS `setup.exe`, and
Linux x86-64 `.AppImage`. The public-downloads repository stages assets and
`SHA256SUMS.txt`. No MSI or signed/notarized artifact is currently in the
release matrix; do not invent one.

Before a final release tag, build unsigned candidates from a clean reviewed
commit and inspect bundled versions, licenses, executable permissions,
installer contents and hashes. Test CLI invocation and Lite installation
from the actual downloaded artifacts on supported macOS and Windows hosts,
with Linux smoke coverage for the existing Linux artifacts. The current
macOS ARM64 host can perform native macOS clean/upgrade and external-volume
checks; Windows NSIS and CLI behavior require native Windows evidence or a
separately approved qualified tester. Backend cross-compilation does not
count as native installation evidence. Current public docs specify supported
architectures but no minimum macOS or Windows release number; define and test
the OS version floor during P7-C before advertising one.

| Scenario | macOS ARM64 | Windows x86-64 | Linux x86-64 |
| --- | --- | --- | --- |
| Clean CLI and Lite installation, first launch, version/About | Native host | Native Windows required | Existing artifact smoke |
| Upgrade over prior CLI/Lite release; retained config, mappings, history, Resume | Native host | Native Windows required | Existing artifact smoke |
| Organize Copy/Move, Convert Scan/Execute, fallback, permissions | Native host | Native Windows required | Existing artifact smoke |
| External volume, recovery, uninstall without deleting user records | Native host | Native Windows required | Environment dependent |
| SHA-256 match against downloaded `SHA256SUMS.txt` | All artifacts | All artifacts | All artifacts |

Preserve the November 18 native acceptance records for source freshness,
lossless compression fallback, config persistence, byte-identical Organize,
compressed-FITS `.fz` option, PixInsight XISF interoperability, Scan guidance
and temporary lock-file cleanup. They do not cover every OS or pixel/codec
combination. Include known limitations and support contact in public docs.

The current Lite install shell script silently removes `com.apple.quarantine`
from the installed app, and its install page recommends an `xattr` removal
command. P7-C must replace that behavior and guidance with Apple's explicit
per-app System Settings > Privacy & Security > Open Anyway workflow, after a
user reviews the downloaded source and checksum, following
[Apple's current guidance](https://support.apple.com/en-us/102445). Do not disable Gatekeeper
or automatically strip quarantine. The Windows install script uses
`Unblock-File` on the downloaded installer; review that behavior and its
warning text with native Windows qualification. Explain that SmartScreen
may warn and Windows 11 Smart App Control can block an unsigned app without a
per-app exception, as described in [Microsoft's Smart App Control FAQ](https://support.microsoft.com/en-gb/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions).
Do not promise universal override or change system policy.
Checksums detect accidental or malicious alteration only when the checksum
source is trusted; they are not code signing.

Before product publication, update installation and user manuals, format and
loss boundaries, Organize/Convert distinction, fallback and compressed-FITS
filename behavior, schema/config compatibility, recovery, third-party notices,
checksums and support guidance. Obtain separate final approval for the exact
release artifacts, tag(s), hashes and public release notes. No signing fees or
paid services are part of this plan.
