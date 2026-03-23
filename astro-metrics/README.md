# astro-metrics

Statistical metrics and analysis for astronomical images.

## Overview

`astro-metrics` provides functionality for analyzing astronomical images, including star detection, measurement, and quality assessment. It uses the Source Extractor as a Library (SEP) for efficient star detection and implements various metrics for evaluating image quality.

## Features

- Star detection using Source Extractor (SEP)
- Star measurements (FWHM, eccentricity, elongation, etc.)
- Background analysis (median, RMS, uniformity)
- Quality scoring for image comparison
- Comprehensive metrics for astronomical image evaluation

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
astro-metrics = "0.4.0"
```

## API Reference

### Core Types

#### StarMetrics

```rust
pub struct StarMetrics {
    pub x: f64,                // X centroid position
    pub y: f64,                // Y centroid position
    pub flux: f32,             // Total flux
    pub peak: f32,             // Peak pixel value
    pub a: f32,                // Semi-major axis
    pub b: f32,                // Semi-minor axis
    pub theta: f32,            // Position angle in radians
    pub eccentricity: f32,     // Eccentricity (derived from a and b)
    pub fwhm: f32,             // Full Width at Half Maximum (derived from a and b)
    pub kron_radius: f32,      // Kron radius (radius containing 50% of flux)
    pub flux_auto: f32,        // Total flux in automatic aperture
    pub fluxerr_auto: f32,     // Error on flux_auto
    pub npix: usize,           // Number of pixels in the object
    pub elongation: f32,       // Elongation (a/b, alternative to eccentricity)
    pub flag: u8,              // Extraction flag
}
```

Key methods:
```rust
impl StarMetrics {
    /// Calculate FWHM as average of semi-major and semi-minor axes
    pub fn calc_fwhm(&mut self)
    
    /// Calculate eccentricity from semi-major and semi-minor axes
    pub fn calc_eccentricity(&mut self)
}
```

#### StarStats

```rust
pub struct StarStats {
    pub count: usize,                  // Total number of stars detected
    pub median_fwhm: f32,              // Median FWHM across all stars
    pub median_eccentricity: f32,      // Median eccentricity across all stars
    pub fwhm_std_dev: f32,             // Standard deviation of FWHM
    pub eccentricity_std_dev: f32,     // Standard deviation of eccentricity
    pub median_kron_radius: f32,       // Median Kron radius
    pub median_flux: f32,              // Median flux
    pub median_snr: f32,               // Median signal-to-noise ratio
    pub median_elongation: f32,        // Median elongation
    pub flagged_fraction: f32,         // Fraction of stars with flag != 0
    pub kron_radius_std_dev: f32,      // Standard deviation of Kron radius
    pub flux_std_dev: f32,             // Standard deviation of flux
    pub snr_std_dev: f32,              // Standard deviation of SNR
}
```

Key methods:
```rust
impl StarStats {
    /// Calculate aggregate statistics from a collection of star metrics
    pub fn from_stars(stars: &[StarMetrics], max_stars: Option<usize>) -> Self
}
```

#### BackgroundMetrics

```rust
pub struct BackgroundMetrics {
    pub median: f32,       // Median background level
    pub rms: f32,          // Root mean square of background
    pub min: f32,          // Minimum background value
    pub max: f32,          // Maximum background value
    pub uniformity: f32,   // Background uniformity (0-1, higher is better)
}
```

#### QualityScores

```rust
pub struct QualityScores {
    pub fwhm: f32,             // FWHM score (higher means better focus/seeing)
    pub eccentricity: f32,     // Eccentricity score (higher means rounder stars)
    pub background: f32,       // Background score (higher means better background)
    pub kron_radius: f32,      // Kron radius score (higher means tighter stars)
    pub snr: f32,              // SNR score (higher means better signal-to-noise ratio)
    pub flag: f32,             // Flag score (higher means fewer flagged stars)
    pub overall: f32,          // Overall quality score (weighted average of scores)
}
```

#### QualityWeights

```rust
pub struct QualityWeights {
    pub fwhm: f32,             // Weight for FWHM score (default: 0.3)
    pub eccentricity: f32,     // Weight for eccentricity score (default: 0.2)
    pub background: f32,       // Weight for background score (default: 0.2)
    pub kron_radius: f32,      // Weight for Kron radius score (default: 0.15)
    pub snr: f32,              // Weight for SNR score (default: 0.1)
    pub flag: f32,             // Weight for flag score (default: 0.05)
}
```

#### FrameQualityMetrics

```rust
pub struct FrameQualityMetrics {
    pub frame_id: String,          // Identifier for the frame
    pub star_stats: StarStats,     // Star statistics
    pub background: BackgroundMetrics, // Background metrics
    pub scores: QualityScores,     // Quality scores
}
```

### SEP Detection

```rust
/// Detect stars using SEP's built-in background estimation and object detection
pub fn detect_stars_with_sep_background(
    data: &[f32],
    width: usize,
    height: usize,
    max_stars: Option<usize>,
) -> Result<(StarStats, BackgroundMetrics)>

/// Detect stars using the SEP library and return detailed measurements for each star
pub fn detect_stars_sep(
    data: &[f32],
    width: usize,
    height: usize,
    background: f32,
    std_dev: f32,
    max_stars: Option<usize>,
) -> Result<StarStats>
```

- **Parameters**:
  - `data`: Flattened pixel data as 32-bit floats
  - `width`: Width of the image in pixels
  - `height`: Height of the image in pixels
  - `max_stars`: Optional maximum number of stars to use for statistics
  - `background`: Background level (for detect_stars_sep)
  - `std_dev`: Background standard deviation (for detect_stars_sep)
- **Returns**:
  - `StarStats`: Statistics about detected stars
  - `BackgroundMetrics`: Background metrics
- **Errors**:
  - If the image is too small
  - If SEP encounters an error during detection

### Quality Metrics

```rust
/// Calculate quality scores for a frame
pub fn calculate_quality_scores(
    star_stats: &StarStats,
    background: &BackgroundMetrics,
) -> QualityScores

/// Calculate overall quality score from individual scores and weights
pub fn calculate_overall_score(
    fwhm_score: f32,
    eccentricity_score: f32,
    background_score: f32,
    kron_score: f32,
    snr_score: f32,
    flag_score: f32,
    weights: &QualityWeights,
) -> f32

/// Create frame quality metrics for an image
pub fn create_frame_metrics(
    path: &Path,
    star_stats: StarStats,
    background: BackgroundMetrics,
) -> FrameQualityMetrics

/// Create frame quality metrics with custom weights
pub fn create_frame_metrics_with_weights(
    path: &Path,
    star_stats: StarStats,
    background: BackgroundMetrics,
    weights: QualityWeights,
) -> FrameQualityMetrics
```

## Usage Examples

### Detecting stars and calculating quality metrics

```rust
use astro_metrics::sep_detect;
use astro_metrics::quality_metrics;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Assume we have image data loaded from somewhere
    let image_data: Vec<f32> = vec![/* ... */];
    let width = 1024;
    let height = 768;
    
    // Detect stars and analyze background
    let (star_stats, background) = sep_detect::detect_stars_with_sep_background(
        &image_data, width, height, None)?;
    
    println!("Found {} stars", star_stats.count);
    println!("Median FWHM: {:.2} pixels", star_stats.median_fwhm);
    println!("Background uniformity: {:.3}", background.uniformity);
    
    // Calculate quality scores
    let scores = quality_metrics::calculate_quality_scores(&star_stats, &background);
    
    println!("FWHM score: {:.3}", scores.fwhm);
    println!("Eccentricity score: {:.3}", scores.eccentricity);
    println!("Background score: {:.3}", scores.background);
    println!("Overall quality score: {:.3}", scores.overall);
    
    // Create frame metrics
    let path = Path::new("/path/to/image.fits");
    let frame_metrics = quality_metrics::create_frame_metrics(
        path, star_stats, background);
    
    println!("Frame ID: {}", frame_metrics.frame_id);
    
    Ok(())
}
```

### Using custom weights for quality scoring

```rust
use astro_metrics::quality_metrics::{self, QualityWeights};
use astro_metrics::sep_detect;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Assume we have image data loaded from somewhere
    let image_data: Vec<f32> = vec![/* ... */];
    let width = 1024;
    let height = 768;
    
    // Detect stars and analyze background
    let (star_stats, background) = sep_detect::detect_stars_with_sep_background(
        &image_data, width, height, None)?;
    
    // Create custom weights
    let weights = QualityWeights {
        fwhm: 0.5,             // Prioritize focus quality
        eccentricity: 0.2,
        background: 0.1,
        kron_radius: 0.1,
        snr: 0.05,
        flag: 0.05,
    };
    
    // Create frame metrics with custom weights
    let path = Path::new("/path/to/image.fits");
    let frame_metrics = quality_metrics::create_frame_metrics_with_weights(
        path, star_stats, background, weights);
    
    println!("Overall quality score: {:.3}", frame_metrics.scores.overall);
    
    Ok(())
}
```

## Additional Documentation

For more detailed information about the quality metrics and how they're calculated, see the [Quality Metrics Documentation](../docs/QualityMetrics.md).

## License

This project is dual-licensed under the MIT License or the Apache License, Version 2.0.
