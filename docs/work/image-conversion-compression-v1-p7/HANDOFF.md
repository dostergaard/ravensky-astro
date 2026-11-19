# P7-A release preparation handoff

## Goal and current status

P7-A prepares `astro-io`, `astro-metadata`, `astro-metrics` and
`ravensky-astro` 0.7.0 for separate publication approval. The release
worktree is `.worktrees/ravensky-astro-p7-preparation` on
`release/ravensky-astro-p7-preparation` from RavenSky Astro
`4689ec495ff7a8d70af84b24d17f4eb36d5cb3e2`. The original `master`
checkout has an unrelated `.gitignore` edit and was not touched. AstroMuninn
`master` remains `b1f0a12dd68daa660cd8890f86292845980a09f3` and was used
read-only. No crates were uploaded, no tag/remote branch was created, and no
AstroMuninn version or dependency was changed.

## Decisions and evidence

The four public crates advance together from registry 0.6.2 to proposed
0.7.0. `astro-io` and `astro-metadata` add public typed/fidelity modules;
`astro-metrics` and the facade need 0.7.0 package identities because their
internal edges and re-exports must select the same graph. Existing API
signatures remain; an automated semver tool was unavailable. `astro-bench`
remains unpublished. See EVIDENCE.md for the version matrix and manual API
review. The publication order is `astro-io` → `astro-metadata` →
`astro-metrics` → `ravensky-astro`.

Rust 1.94 full all-target tests, doctests, strict Clippy, strict rustdoc,
optimized examples, Cargo four-package verification and package-content
inspection passed. `astro-io` passed a nonuploading crates.io dry-run. The
three downstream dry-runs are blocked until `astro-io 0.7.0` is in the
registry; local multi-package verification passed for them. Latest `master`
CI runs were cancelled, and no candidate branch CI has run. Public owner
metadata names `dostergaard`; upload credential availability remains
unverified. Package and test conditions and limitations are in EVIDENCE.md.

## Follow-up and stop condition

After the final local commit, regenerate four clean archives in a fresh
package target directory; inspect them and record their exact hashes and
commit in the user checkpoint. Check `git diff --check`, branch cleanliness,
remote refs and the unchanged original checkout. Then stop and request
separate approval for the exact crates.io uploads. Do not run a real
`cargo publish`, push a tag, push the P7 branch, or change AstroMuninn as
part of this preparation assignment.

After explicit publication approval in a later task, repeat downstream
registry dry-runs as prerequisites appear, upload in dependency order,
verify registry archives/docs.rs and an independent registry-only consumer.
P7-B then updates AstroMuninn registry requirements/1.0.0 metadata and tests
old config/journals; P7-C separately qualifies unsigned distributables and
actual installs. PLAN.md contains those detailed future steps. The Lite
installer's current automatic quarantine removal is a P7-C correction, not
an action taken here.
