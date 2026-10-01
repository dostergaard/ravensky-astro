// ravensky-astro/src/lib.rs
//! RavenSky Astro
//!
//! Copyright (c) 2025 Dean Ostergaard
//!
//! Licensed under the MIT OR Apache-2.0 license.
//!
//! Core libraries for astronomical image processing and analysis.
//!
//! This crate provides a collection of tools for working with astronomical images,
//! organized into three main modules:
//!
//! - [`io`]: File I/O operations for astronomical image formats (FITS, XISF)
//! - [`metadata`]: Metadata extraction and handling for astronomical images
//! - [`metrics`]: Statistical analysis and quality metrics for astronomical images
//!
//! # Examples
//!
//! ```no_run
//! use ravensky_astro::io;
//! use ravensky_astro::metadata;
//! use ravensky_astro::metrics;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load metadata from a FITS file
//! let path = Path::new("/path/to/image.fits");
//! let metadata = metadata::fits_parser::extract_metadata_from_path(path)?;
//!
//! // Extract star metrics
//! let (image_data, width, height) = io::fits::load_fits(path)?;
//! let (star_stats, background) = metrics::sep_detect::detect_stars_with_sep_background(
//!     &image_data, width, height, None)?;
//!
//! // Calculate quality scores
//! let scores = metrics::quality_metrics::calculate_quality_scores(&star_stats, &background);
//! println!("Overall quality score: {}", scores.overall);
//! # Ok(())
//! # }
//! ```

pub use astro_io as io;
pub use astro_metadata as metadata;
pub use astro_metrics as metrics;
