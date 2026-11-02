# Compressed FITS metadata handoff

## Goal and status

Fix GZIP_2 tiled-image metadata extraction in shared crates and `.fz` discovery in AstroMuninn CLI/Lite. The user supplied three files after the Phase 1 gate. Implementation and local validation are complete on both `bugfix/compressed-fits-metadata` branches. Publication, release, and merge are outside this task.

## Decisions and evidence

- Both repositories are on `bugfix/compressed-fits-metadata`.
- The shared repository was clean at branch creation. AstroMuninn had extensive unrelated uncommitted configuration-editor and vendored changes; preserve them.
- Three ignored real samples are under `ravensky-astro/tests/data/nina-compressed-fits/`; all yield the current empty metadata dump. Physical header inspection shows HDU 0 `NAXIS=0`, HDU 1 `ZIMAGE=T`, `ZCMPTYPE=GZIP_2`, logical 8288×5648, and observational keywords in HDU 1.
- `astro-metadata` primary-only selection is the confirmed metadata defect. `astro-io` loader is also primary-only. Validator is an independent all-HDU path. AstroMuninn core uses final-extension checks, omitting `.fz`.
- `fitsio` 0.21.9 exposes HDU 1 as a logical `ImageInfo`; raw card reads retain the physical binary-table header. `astro-io` now selects the first nonempty image with at least two axes. `astro-metadata` parses selected-image values with primary fallback, retains all raw cards, and errors when no usable image exists. `astromuninn-core` recognizes the full FITS `.fz` suffixes in one path classifier and retains the underlying FIT/FITS/FTS display label.
- Validation remains independent and unchanged. Real files pass structural and full validation with image count one; metadata extraction does not decompress pixels.
- AstroMuninn's Cargo dependencies still point to published shared crates. Integration validation used temporary Cargo patches and restored its original dirty lockfile. A future coordinated shared release/dependency update is required for a standalone product build to include the metadata fix.

## Validation and next steps

See EVIDENCE.md for exact commands and results. Review and commit only task changes; the AstroMuninn worktree contains pre-existing unrelated edits, including in `workflow.rs`, `README.md`, and the Lite manual. Do not stage those unrelated hunks. Preserve the ignored capture files. Before a released product can use the shared fix, publish a compatible shared-crate version and update AstroMuninn dependencies in a separately authorized release task.
