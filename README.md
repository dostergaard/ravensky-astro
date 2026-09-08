# ravensky-astro

![RavenSky Logo](assets/RavenSkyRavens_DocHeader.png)

**RavenSky Astro** is a modular collection of Rust crates for astronomical image I/O, metadata extraction, and quantitative image analysis.

Designed for astrophotography tools and observatory workflows, RavenSky Astro provides reusable building blocks for working with FITS and XISF data in pure Rust.

---

## Status & Scope

RavenSky Astro is actively developed and used in production within my own astrophotography tools and workflows. The APIs are designed for reuse and stability, but the ecosystem continues to evolve as new utilities and applications are built on top of it.

The goal is not just experimentation — it is to build a durable, composable foundation for astronomical image tooling in Rust.

The next deliberate cleanup pass should target boundary cleanup, XISF hardening, API surface review, and docs consolidation, because those are the clearest repo-wide needs right now.

---

## Architecture Overview

RavenSky Astro is intentionally modular. Each crate has a focused responsibility:

* **astro-io** → image file loading and saving
* **astro-metadata** → structured metadata extraction
* **astro-metrics** → statistical and quality analysis
* **astro-bench** → optional synthetic workloads and repeatable performance measurements

These crates are designed to be used independently or together, depending on your application’s needs.

Planned format-layer refactoring and migration details are documented in the [Format Architecture Plan](docs/FormatArchitecturePlan.md).

Start with the [standalone benchmark guide](docs/BenchmarkGuide.md) for setup,
commands, report interpretation, real captures and use from a separate Rust project.
No RavenSky application is required.

The [benchmark runner](astro-bench/README.md) provides generated FITS/XISF validator
and raw-I/O workloads, explicit concurrency, and JSON reports. It is separate from
production dependencies and does not change application defaults. See the
[benchmark and calibration design](docs/BenchmarkAndCalibrationDesign.md) for scope
and the path toward application-driven calibration.

---

## Windows FITS Path-Length Note

On Windows, FITS file access in AstroMuninn and the ravensky-astro FITS APIs depends on CFITSIO (via `fitsio` / `fitsio-sys`). CFITSIO currently opens disk files using its `fopen`-based path handling (`file_openfile`), which in this environment follows the classic Windows path-length boundary.

Use full FITS paths shorter than 260 characters (`< 260`). At 260 or more, FITS open calls may fail.

This limitation is specific to FITS loading and metadata access through CFITSIO.
Managed FITS validation and XISF handling do not use that path implementation.

---

## Crates

### astro-io

Handles I/O operations for astronomical image formats:

* FITS file loading and saving
* XISF file loading and saving
* Efficient image data handling

```rust
use astro_io::fits;
use astro_io::xisf;

let (pixels, width, height) = fits::load_fits(Path::new("/path/to/image.fits"))?;
```

---

### astro-metadata

Provides structured metadata extraction and handling:

* FITS header parsing
* XISF header parsing
* Equipment information (telescope, camera, filters)
* Exposure details
* Environmental data
* Plate scale calculations

```rust
use astro_metadata::fits_parser;

let metadata = fits_parser::extract_metadata_from_path(Path::new("/path/to/image.fits"))?;

if let Some(plate_scale) = metadata.plate_scale() {
    println!("Plate scale: {} arcsec/pixel", plate_scale);
}
```

---

### astro-metrics

Implements statistical and quality metrics for astronomical images:

* Star detection and measurement (via SEP)
* Star metrics (count, FWHM, eccentricity, elongation)
* Background analysis (median, RMS, uniformity)
* Composite quality scoring

```rust
use astro_metrics::sep_detect;
use astro_metrics::quality_metrics;

let (star_stats, background) =
    sep_detect::detect_stars_with_sep_background(&image_data, width, height, None)?;

let scores = quality_metrics::calculate_quality_scores(&star_stats, &background);
println!("Overall quality score: {}", scores.overall);
```

---

## Design Goals

* Pure Rust implementation
* Minimal external dependencies
* Clear crate boundaries
* Composable APIs
* Suitable for CLI tools, services, or GUI applications
* Deterministic, testable image quality metrics

---

## Installation

Add only the crates you need:

```toml
[dependencies]
astro-io = "0.6.0"
astro-metadata = "0.6.0"
astro-metrics = "0.6.0"
# Optional meta crate that re-exports all three:
ravensky-astro = "0.6.0"
```

Each crate can be used independently.

If you depend on the meta crate, import it as `ravensky_astro`:

```rust
use ravensky_astro::{io, metadata, metrics};
```

---

## Example: Load, Extract Metadata, and Score an Image

```rust
use astro_io::fits;
use astro_metadata::fits_parser;
use astro_metrics::sep_detect;
use astro_metrics::quality_metrics;

let path = Path::new("/path/to/image.fits");

let (image_data, width, height) = fits::load_fits(path)?;
let metadata = fits_parser::extract_metadata_from_path(path)?;

let (star_stats, background) =
    sep_detect::detect_stars_with_sep_background(&image_data, width, height, None)?;

let scores = quality_metrics::calculate_quality_scores(&star_stats, &background);

println!("Object: {:?}", metadata.exposure.object_name);
println!("Stars detected: {}", star_stats.count);
println!("Quality score: {:.3}", scores.overall);
```

---

## Example: Sensor Temperature QA

The `sensor_temperature_qa` example recursively evaluates FITS and XISF files
whose `CCD-TEMP` value differs from `SET-TEMP` by more than a supplied limit.
It previews matching files and asks for confirmation before moving them.

```bash
cargo run --example sensor_temperature_qa -- /path/to/input /path/to/output 0.5
```

The input directory, output directory, and decimal temperature limit can also
be supplied interactively when omitted. Moved files retain their paths relative
to the input directory—for example, `input/Lights/M31/frame.fit` becomes
`output/Lights/M31/frame.fit`. Existing output files are never overwritten.

---

## Development

Clone and build:

```bash
git clone https://github.com/dostergaard/ravensky-astro.git
cd ravensky-astro
cargo build --release
```

This workspace tracks the current stable Rust toolchain for development. The checked-in `rust-toolchain.toml` ensures `cargo`, `rustfmt`, `clippy`, and rust-analyzer all resolve through the same stable channel and component set.

Run the standing maintenance loop from the workspace root:

```bash
rustup update stable
cargo update
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
```

---

## Documentation

Additional documentation is available in the `docs/` directory:

* Quality Metrics documentation
* Supported NINA tokens
* File organization tokens

---

## Contributing

Contributions, suggestions, and issue reports are welcome.

RavenSky Astro is part of a broader effort to build open, composable tools for astrophotography workflows in Rust.

---

## Support

If RavenSky Astro is useful in your projects, consider supporting development via GitHub Sponsors or other contribution channels.

---

## License

Licensed under either:

* MIT License
* Apache License, Version 2.0
