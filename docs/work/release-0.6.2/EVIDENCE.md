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

CI, GitHub release, crates.io publication, and independent registry-consumer
validation remain to be recorded after publication.
