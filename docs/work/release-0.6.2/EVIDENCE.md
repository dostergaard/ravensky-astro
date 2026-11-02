# RavenSky Astro 0.6.2 release evidence

Prepublication checks on macOS ARM64, Rust 1.94.0, 2026-11-02:

- `cargo +1.94.0 fmt --all --check`: passed.
- `cargo +1.94.0 clippy --locked --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo +1.94.0 test --locked --workspace`: passed.
- `cargo +1.94.0 test --locked -p astro-metadata --test compressed_fits` with `ASTRO_NINA_FITS_FIXTURES` set to the absolute path of `tests/data/nina-compressed-fits`: 5 passed, including the three supplied captures.
- `cargo +1.94.0 build --locked --release --workspace --all-features --examples`: passed.
- `RUSTDOCFLAGS='-D warnings' cargo +1.94.0 doc --locked --workspace --exclude astro-bench --all-features --no-deps`: passed.
- `cargo +1.94.0 package --locked --workspace --exclude astro-bench --allow-dirty`: packaged and verified all four public crates through Cargo's temporary registry.

The first focused test and release-example build attempts hit the restricted
sandbox's CFITSIO native-build permission error. Both passed when rerun with the
required build permission. The ignored real capture files were not included in
the `ravensky-astro` package archive.

## Publication

Release source commit `0d8c57a053cf922cbe8075fc77e82b69192f9aeb` passed
[GitHub CI run 41474331600](https://github.com/dostergaard/ravensky-astro/actions/runs/41474331600)
on Linux x86-64 GNU, macOS ARM64, and the configured Windows GNU subset.
Annotated tag `v0.6.2` resolves to that commit. The
[GitHub release](https://github.com/dostergaard/ravensky-astro/releases/tag/v0.6.2)
was published before crates.io uploads, as requested.

The first `cargo +1.94.0 package` output in the reused target directory passed
Cargo verification but an independent gzip audit found trailing bytes in three
archives. A fresh `cargo package --locked --workspace --exclude astro-bench`
using stable Cargo in `/private/tmp/ravensky-astro-062-package` produced four
complete gzip streams. The existing `verify_archives.py` audit confirmed clean
VCS identity, source bytes, internal dependency versions, docs.rs metadata, and
archive structure. `cargo publish --locked` then published in dependency order:

| Crate | Published archive SHA-256 |
| --- | --- |
| `astro-io 0.6.2` | `93d958eab651452ce18f658a0e9cce5f12d3c86a29a59ae407bd94d8441f3604` |
| `astro-metadata 0.6.2` | `4ba69196d5d2d456fada2659d24f24719d01e3f949b0a13d9ea8f9aa0d0cb83c` |
| `astro-metrics 0.6.2` | `b34325099d6c245c774fef225862eabe76e8b61afd3a315de7925615c452cadf` |
| `ravensky-astro 0.6.2` | `a105bde6689fcd00dd266100ceda2f5859f71f79afc1f26fe2fc75558084cedd` |

Each archive downloaded from crates.io matched its independently audited local
archive byte-for-byte. Cargo verified each dependent crate against the newly
published registry dependencies before uploading it. Hosted docs.rs completion
and an AstroMuninn dependency update are separate follow-up checks.
