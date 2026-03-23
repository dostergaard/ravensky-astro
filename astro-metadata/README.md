# astro-metadata

Metadata extraction and handling for astronomical images.

## Overview

`astro-metadata` provides functionality for extracting and processing metadata from astronomical image formats, including FITS and XISF. It handles parsing of headers, extraction of equipment information, exposure details, and more.

## Features

- Comprehensive metadata type definitions
- FITS header parsing
- XISF header parsing
- Equipment information (telescope, camera, etc.)
- Exposure details
- Filter information
- Environmental data
- Coordinate and timing utilities

## Windows FITS Path-Length Note

On Windows, FITS file access in AstroMuninn and the ravensky-astro FITS APIs depends on CFITSIO (via `fitsio` / `fitsio-sys`). CFITSIO currently opens disk files using its `fopen`-based path handling (`file_openfile`), which in this environment follows the classic Windows path-length boundary.

Use full FITS paths shorter than 260 characters (`< 260`). At 260 or more, FITS open calls may fail.

This limitation is specific to FITS access through CFITSIO. XISF handling is not affected.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
astro-metadata = "0.4.0"
```

## API Reference

### Core Types

#### AstroMetadata

```rust
pub struct AstroMetadata {
    pub equipment: Equipment,
    pub detector: Detector,
    pub filter: Filter,
    pub exposure: Exposure,
    pub mount: Option<Mount>,
    pub environment: Option<Environment>,
    pub wcs: Option<WcsData>,
    pub xisf: Option<XisfMetadata>,
    pub color_management: Option<ColorManagement>,
    pub attachments: Vec<AttachmentInfo>,
    pub raw_header_cards: Vec<FitsHeaderCard>,
    pub raw_headers: HashMap<String, String>,
}
```

Key methods:
```rust
impl AstroMetadata {
    /// Check if we have enough information to calculate plate scale
    pub fn can_calculate_plate_scale(&self) -> bool

    /// Calculate plate scale in arcsec/pixel
    pub fn plate_scale(&self) -> Option<f32>
    
    /// Calculate field of view in arcminutes
    pub fn field_of_view(&self) -> Option<(f32, f32)>
    
    /// Calculate the session date using location information if available
    pub fn calculate_session_date(&mut self)
}
```

#### Equipment

```rust
pub struct Equipment {
    pub telescope_name: Option<String>,
    pub focal_length: Option<f32>,
    pub aperture: Option<f32>,
    pub focal_ratio: Option<f32>,
    pub reducer_flattener: Option<String>,
    pub mount_model: Option<String>,
    pub focuser_position: Option<i32>,
    pub focuser_temperature: Option<f32>,
}
```

#### Detector

```rust
pub struct Detector {
    pub camera_name: Option<String>,
    pub pixel_size: Option<f32>,
    pub width: usize,
    pub height: usize,
    pub binning_x: usize,
    pub binning_y: usize,
    pub gain: Option<f32>,
    pub offset: Option<i32>,
    pub readout_mode: Option<String>,
    pub usb_limit: Option<String>,
    pub read_noise: Option<f32>,
    pub full_well: Option<f32>,
    pub temperature: Option<f32>,
    pub temp_setpoint: Option<f32>,
    pub cooler_power: Option<f32>,
    pub cooler_status: Option<String>,
    pub rotator_angle: Option<f32>,
}
```

#### Filter

```rust
pub struct Filter {
    pub name: Option<String>,
    pub position: Option<usize>,
    pub wavelength: Option<f32>,
}
```

#### Exposure

```rust
pub struct Exposure {
    pub object_name: Option<String>,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub date_obs: Option<DateTime<Utc>>,
    pub session_date: Option<DateTime<Utc>>,
    pub exposure_time: Option<f32>,
    pub frame_type: Option<String>,
    pub sequence_id: Option<String>,
    pub frame_number: Option<usize>,
    pub dither_offset_x: Option<f32>,
    pub dither_offset_y: Option<f32>,
    pub project_name: Option<String>,
    pub session_id: Option<String>,
}
```

### FITS Parser

```rust
/// Extract metadata from a FITS file path
pub fn extract_metadata_from_path(path: &Path) -> Result<AstroMetadata>

/// Extract metadata from a FITS file
pub fn extract_metadata(fits_file: &mut FitsFile) -> Result<AstroMetadata>

/// Parse sexagesimal format (HH MM SS or DD MM SS) to decimal degrees
pub fn parse_sexagesimal(value: &str) -> Option<f64>
```

- **Parameters**:
  - `path`: Path to the FITS file
  - `fits_file`: Open FITS file handle
  - `value`: String in sexagesimal format (e.g., "12 34 56" for RA or "-45 12 34" for DEC)
- **Returns**:
  - `AstroMetadata`: Extracted metadata structure
  - `f64`: Decimal degrees (for parse_sexagesimal)
- **Errors**:
  - If the file cannot be opened
  - If required headers cannot be read
  - On Windows, FITS open may fail when the full pathname is 260 characters or longer due to CFITSIO `fopen` path handling.

### XISF Parser

```rust
/// Extract metadata from an XISF file path
pub fn extract_metadata_from_path(path: &Path) -> Result<AstroMetadata>

/// Extract metadata from an XISF file
pub fn extract_metadata<R: Read + Seek>(reader: &mut R) -> Result<AstroMetadata>
```

- **Parameters**:
  - `path`: Path to the XISF file
  - `reader`: Reader for the XISF file
- **Returns**:
  - `AstroMetadata`: Extracted metadata structure
- **Errors**:
  - If the file cannot be opened
  - If the XISF signature is invalid
  - If the XML header cannot be parsed

## Usage Examples

### Extracting metadata from a FITS file

```rust
use astro_metadata::fits_parser;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/path/to/image.fits");
    let metadata = fits_parser::extract_metadata_from_path(path)?;
    
    // Access equipment information
    if let Some(focal_length) = metadata.equipment.focal_length {
        println!("Focal length: {} mm", focal_length);
    }
    
    // Access exposure information
    if let Some(exp_time) = metadata.exposure.exposure_time {
        println!("Exposure time: {} seconds", exp_time);
    }
    
    // Calculate plate scale
    if let Some(plate_scale) = metadata.plate_scale() {
        println!("Plate scale: {} arcsec/pixel", plate_scale);
    }
    
    // Calculate field of view
    if let Some((width, height)) = metadata.field_of_view() {
        println!("Field of view: {:.2}' × {:.2}'", width, height);
    }
    
    Ok(())
}
```

### Parsing sexagesimal coordinates

```rust
use astro_metadata::fits_parser::parse_sexagesimal;

fn main() {
    // Parse right ascension: "12 34 56" (12h 34m 56s)
    let ra_deg = parse_sexagesimal("12 34 56").map(|ra| ra * 15.0); // Convert hours to degrees
    println!("RA: {:?} degrees", ra_deg);
    
    // Parse declination: "-45 12 34" (-45° 12' 34")
    let dec_deg = parse_sexagesimal("-45 12 34");
    println!("DEC: {:?} degrees", dec_deg);
}
```

## License

This project is dual-licensed under the MIT License or the Apache License, Version 2.0.
