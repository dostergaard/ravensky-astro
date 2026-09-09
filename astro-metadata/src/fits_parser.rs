//! Parser for FITS file headers
//!
//! This module provides functions to extract metadata from FITS file headers
//! and convert it into the AstroMetadata structure.

use anyhow::{Context, Result};
use astro_io::fits::{header_cards_to_map, read_header_cards};
use chrono::{DateTime, NaiveDateTime, Utc};
use fitsio::FitsFile;
use log::warn;
use std::collections::HashMap;
use std::path::Path;

use super::coordinates::populate_header_coordinates;
use super::types::{
    AstroMetadata, Detector, Environment, Equipment, Exposure, Filter, Mount, WcsData,
};

pub use super::coordinates::parse_sexagesimal;

/// Extract metadata from a FITS file path
pub fn extract_metadata_from_path(path: &Path) -> Result<AstroMetadata> {
    astro_io::fits::backend::with_cfitsio(|| {
        let mut fits_file = FitsFile::open(path).context("Failed to open FITS file")?;
        extract_metadata(&mut fits_file)
    })
}

/// Extract metadata from a FITS file
///
/// Native calls share astro-io's CFITSIO gate. On non-reentrant builds, enclose
/// open, use and drop of the caller-owned handle in
/// [`astro_io::fits::backend::with_cfitsio`].
pub fn extract_metadata(fits_file: &mut FitsFile) -> Result<AstroMetadata> {
    astro_io::fits::backend::with_cfitsio(|| {
        let hdu = fits_file.primary_hdu()?;
        let mut metadata = AstroMetadata::default();
        let raw_header_cards = read_header_cards(fits_file, hdu.number)
            .context("Failed to extract FITS header cards")?;
        let raw_headers = header_cards_to_map(&raw_header_cards);

        // Parse equipment information
        parse_equipment(&mut metadata.equipment, &raw_headers);

        // Parse detector information
        parse_detector(&mut metadata.detector, &raw_headers, &hdu.info);

        // Parse filter information
        parse_filter(&mut metadata.filter, &raw_headers);

        // Parse exposure information
        parse_exposure(&mut metadata.exposure, &raw_headers);

        // Parse mount information
        metadata.mount = parse_mount(&raw_headers);

        // Parse environment information
        metadata.environment = parse_environment(&raw_headers);

        // Parse WCS information
        metadata.wcs = parse_wcs(&raw_headers);

        // Store the canonical lossless cards plus the compatibility lookup map.
        metadata.raw_header_cards = raw_header_cards;
        metadata.raw_headers = raw_headers;

        // Calculate session date
        metadata.calculate_session_date();

        Ok(metadata)
    })
}

/// Parse equipment information from FITS headers
fn parse_equipment(equipment: &mut Equipment, headers: &HashMap<String, String>) {
    equipment.telescope_name = get_string_header(headers, &["TELESCOP"]);
    equipment.focal_length = get_float_header(headers, &["FOCALLEN"]);
    equipment.aperture = get_float_header(headers, &["APERTURE"]);

    // Calculate focal ratio if not directly available
    if equipment.focal_ratio.is_none() {
        if let (Some(focal_length), Some(aperture)) = (equipment.focal_length, equipment.aperture) {
            if aperture > 0.0 {
                equipment.focal_ratio = Some(focal_length / aperture);
            }
        }
    }

    // Try to extract reducer/flattener info from INSTRUME
    if let Some(instrume) = get_string_header(headers, &["INSTRUME"]) {
        if instrume.contains("reducer") || instrume.contains("flattener") {
            equipment.reducer_flattener = Some(instrume);
        }
    }

    equipment.mount_model = get_string_header(headers, &["MOUNT"]);

    // Focuser information
    equipment.focuser_position = get_int_header(headers, &["FOCPOS", "FOCUSPOS"]);
    equipment.focuser_temperature = get_float_header(headers, &["FOCTEMP", "FOCUSTEMP"]);
}

/// Parse detector information from FITS headers
fn parse_detector(
    detector: &mut Detector,
    headers: &HashMap<String, String>,
    hdu_info: &fitsio::hdu::HduInfo,
) {
    detector.camera_name = get_string_header(headers, &["INSTRUME", "CAMERA"]);
    detector.pixel_size = get_float_header(headers, &["PIXSIZE", "XPIXSZ"]);

    // Get dimensions from NAXIS1/NAXIS2 headers
    if let Some(naxis1) = get_int_header(headers, &["NAXIS1"]) {
        detector.width = naxis1 as usize;
    }

    if let Some(naxis2) = get_int_header(headers, &["NAXIS2"]) {
        detector.height = naxis2 as usize;
    }

    // If dimensions are not in headers, try to get them from HDU info
    if detector.width == 0 || detector.height == 0 {
        if let fitsio::hdu::HduInfo::ImageInfo { shape, .. } = hdu_info {
            if shape.len() >= 2 {
                // FITS standard: first dimension is y (height), second is x (width)
                detector.height = shape[0];
                detector.width = shape[1];
            }
        }
    }

    // Binning
    detector.binning_x = get_int_header(headers, &["XBINNING"]).unwrap_or(1) as usize;
    detector.binning_y = get_int_header(headers, &["YBINNING"]).unwrap_or(1) as usize;

    // Camera settings
    detector.gain = get_float_header(headers, &["GAIN", "EGAIN"]);
    detector.offset = get_int_header(headers, &["OFFSET", "CCDOFFST"]);
    detector.readout_mode = get_string_header(headers, &["READOUT", "READOUTM"]);
    detector.usb_limit = get_string_header(headers, &["USBLIMIT", "USBTRFC"]);
    detector.read_noise = get_float_header(headers, &["RDNOISE"]);
    detector.temperature = get_float_header(headers, &["CCD-TEMP", "CCDTEMP"]);
    detector.temp_setpoint = get_float_header(headers, &["CCD-TEMP-SETPOINT", "SET-TEMP"]);
    detector.cooler_power = get_float_header(headers, &["COOL-PWR", "COOLPWR"]);
    detector.cooler_status = get_string_header(headers, &["COOL-STAT", "COOLSTAT"]);
    detector.rotator_angle = get_float_header(headers, &["ROTANG", "ROTPA", "ROTATANG"]);
}

/// Parse filter information from FITS headers
fn parse_filter(filter: &mut Filter, headers: &HashMap<String, String>) {
    filter.name = get_string_header(headers, &["FILTER"]);

    // Try to get filter position
    if let Some(pos_str) = get_string_header(headers, &["FILTERID", "FLTPOS"]) {
        if let Ok(pos) = pos_str.parse::<usize>() {
            filter.position = Some(pos);
        }
    }

    // Filter wavelength is rarely in FITS headers, but we'll check anyway
    filter.wavelength = get_float_header(headers, &["WAVELENG", "WAVELEN"]);
}

/// Parse exposure information from FITS headers
fn parse_exposure(exposure: &mut Exposure, headers: &HashMap<String, String>) {
    exposure.object_name = get_string_header(headers, &["OBJECT"]);

    populate_header_coordinates(exposure, headers);

    // Parse date/time
    if let Some(date_str) = get_string_header(headers, &["DATE-OBS"]) {
        exposure.date_obs = parse_date_time(&date_str);
    }

    // Exposure time
    exposure.exposure_time = get_float_header(headers, &["EXPTIME", "EXPOSURE"]);

    // Frame type
    exposure.frame_type = get_string_header(headers, &["IMAGETYP", "FRAME"]);

    // Sequence information
    exposure.sequence_id = get_string_header(headers, &["SEQID", "SEQFILE"]);

    if let Some(frame_num_str) = get_string_header(headers, &["FRAMENUM", "SEQNUM"]) {
        if let Ok(frame_num) = frame_num_str.parse::<usize>() {
            exposure.frame_number = Some(frame_num);
        }
    }

    // Dither offsets
    exposure.dither_offset_x = get_float_header(headers, &["DX", "DITHX"]);
    exposure.dither_offset_y = get_float_header(headers, &["DY", "DITHY"]);

    // Scheduler information
    exposure.project_name = get_string_header(headers, &["PROJECT", "PROJNAME"]);
    exposure.session_id = get_string_header(headers, &["SESSIONID", "SESSID"]);
}

/// Parse mount information from FITS headers
fn parse_mount(headers: &HashMap<String, String>) -> Option<Mount> {
    // Check if we have any mount information
    if !headers.contains_key("PIERSIDE")
        && !headers.contains_key("MFLIP")
        && !headers.contains_key("GUIDERMS")
        && !headers.contains_key("SITELAT")
        && !headers.contains_key("OBSLAT")
    {
        return None;
    }

    let mut mount = Mount {
        pier_side: get_string_header(headers, &["PIERSIDE"]),
        latitude: get_float_header(headers, &["SITELAT", "OBSLAT"]).map(|v| v as f64),
        longitude: get_float_header(headers, &["SITELONG", "OBSLONG"]).map(|v| v as f64),
        height: get_float_header(headers, &["SITEELEV", "OBSELEV"]).map(|v| v as f64),
        guide_camera: get_string_header(headers, &["GUIDECAM"]),
        guide_rms: get_float_header(headers, &["GUIDERMS"]),
        guide_scale: get_float_header(headers, &["GUIDESCALE"]),
        peak_ra_error: get_float_header(headers, &["PEAKRA", "PEAKRAER"]),
        peak_dec_error: get_float_header(headers, &["PEAKDEC", "PEAKDCER"]),
        ..Default::default()
    };

    // Parse meridian flip
    if let Some(mflip_str) = get_string_header(headers, &["MFLIP", "MFOC"]) {
        mount.meridian_flip = Some(mflip_str.to_lowercase() == "true" || mflip_str == "1");
    }

    // Parse dither enabled
    if let Some(dither_str) = get_string_header(headers, &["DITHER"]) {
        mount.dither_enabled = Some(dither_str.to_lowercase() == "true" || dither_str == "1");
    }

    Some(mount)
}

/// Parse environment information from FITS headers
fn parse_environment(headers: &HashMap<String, String>) -> Option<Environment> {
    // Check if we have any environment information
    if !headers.contains_key("AMB_TEMP")
        && !headers.contains_key("HUMIDITY")
        && !headers.contains_key("NINA-VERSION")
        && !headers.contains_key("EKOS-VERSION")
        && !headers.contains_key("SQM")
    {
        return None;
    }

    let mut env = Environment {
        ambient_temp: get_float_header(headers, &["AMB_TEMP", "AMBTEMP"]),
        humidity: get_float_header(headers, &["HUMIDITY"]),
        dew_heater_power: get_float_header(headers, &["DEWPOWER", "DEWPWR"]),
        voltage: get_float_header(headers, &["VOLTAGE", "SYSVOLT"]),
        current: get_float_header(headers, &["CURRENT", "SYSCURR"]),
        sqm: get_float_header(headers, &["SQM", "SQMMAG", "SKYQUAL"]),
        ..Default::default()
    };

    // Software version
    if let Some(nina_ver) = get_string_header(headers, &["NINA-VERSION"]) {
        env.software_version = Some(format!("NINA {}", nina_ver));
    } else if let Some(ekos_ver) = get_string_header(headers, &["EKOS-VERSION"]) {
        env.software_version = Some(format!("EKOS {}", ekos_ver));
    } else if let Some(software) = get_string_header(headers, &["SWCREATE", "SOFTWARE"]) {
        env.software_version = Some(software);
    }

    Some(env)
}

/// Parse WCS information from FITS headers
fn parse_wcs(headers: &HashMap<String, String>) -> Option<WcsData> {
    // Check if we have any WCS information
    if !headers.contains_key("CRPIX1")
        && !headers.contains_key("CRPIX2")
        && !headers.contains_key("CRVAL1")
        && !headers.contains_key("CRVAL2")
    {
        return None;
    }

    let wcs = WcsData {
        // Reference pixel coordinates
        crpix1: get_float_header(headers, &["CRPIX1"]).map(|v| v as f64),
        crpix2: get_float_header(headers, &["CRPIX2"]).map(|v| v as f64),
        // Reference pixel values (usually RA/DEC in degrees)
        crval1: get_float_header(headers, &["CRVAL1"]).map(|v| v as f64),
        crval2: get_float_header(headers, &["CRVAL2"]).map(|v| v as f64),
        // CD matrix elements (transformation matrix)
        cd1_1: get_float_header(headers, &["CD1_1"]).map(|v| v as f64),
        cd1_2: get_float_header(headers, &["CD1_2"]).map(|v| v as f64),
        cd2_1: get_float_header(headers, &["CD2_1"]).map(|v| v as f64),
        cd2_2: get_float_header(headers, &["CD2_2"]).map(|v| v as f64),
        // Coordinate system
        ctype1: get_string_header(headers, &["CTYPE1"]),
        ctype2: get_string_header(headers, &["CTYPE2"]),
        ..Default::default()
    };

    // Note: If we need to store equinox information, we would need to add
    // an equinox field to the WcsData struct

    Some(wcs)
}

/// Helper function to get a string value from headers
fn get_string_header(headers: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = get_header_value(headers, key) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Helper function to get a float value from headers
fn get_float_header(headers: &HashMap<String, String>, keys: &[&str]) -> Option<f32> {
    for key in keys {
        if let Some(value) = get_header_value(headers, key) {
            if let Ok(float_val) = value.parse::<f32>() {
                return Some(float_val);
            }
        }
    }
    None
}

/// Helper function to get an integer value from headers
fn get_int_header(headers: &HashMap<String, String>, keys: &[&str]) -> Option<i32> {
    for key in keys {
        if let Some(value) = get_header_value(headers, key) {
            if let Ok(int_val) = value.parse::<i32>() {
                return Some(int_val);
            }
        }
    }
    None
}

fn get_header_value<'a>(headers: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    headers.get(key).map(String::as_str).or_else(|| {
        headers
            .iter()
            .find(|(header_key, _)| header_key.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    })
}

/// Helper function to parse date/time strings
fn parse_date_time(date_str: &str) -> Option<DateTime<Utc>> {
    // Try different date formats
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.fZ", // ISO 8601 with Z suffix
        "%Y-%m-%dT%H:%M:%SZ",    // ISO 8601 with Z suffix, no fractional seconds
        "%Y-%m-%dT%H:%M:%S%.f",  // ISO 8601 with fractional seconds
        "%Y-%m-%dT%H:%M:%S",     // ISO 8601 without fractional seconds
        "%Y-%m-%d %H:%M:%S%.f",  // Space-separated with fractional seconds
        "%Y-%m-%d %H:%M:%S",     // Space-separated without fractional seconds
    ];

    for format in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, format) {
            return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
        }
    }

    warn!("Failed to parse date string: {}", date_str);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use fitsio::errors::check_status;
    use fitsio::sys::{fits_write_comment, fits_write_record};
    use fitsio::FitsFile;
    use std::ffi::CString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_parse_sexagesimal() {
        // Test RA format (HH MM SS)
        assert_eq!(parse_sexagesimal("12 30 45"), Some(12.5125));

        // Test DEC format (DD MM SS)
        assert_eq!(parse_sexagesimal("-45 30 15"), Some(-45.50416666666667));

        // Test with zero values
        assert_eq!(parse_sexagesimal("0 0 0"), Some(0.0));

        // Test with invalid input
        assert_eq!(parse_sexagesimal("not a coordinate"), None);
        assert_eq!(parse_sexagesimal("12 30"), None); // Not enough parts
    }

    #[test]
    fn parse_exposure_preserves_numeric_ra_in_degrees() {
        let headers = HashMap::from([
            ("RA".to_owned(), "237.502568422944".to_owned()),
            ("OBJCTRA".to_owned(), "15 50 01".to_owned()),
        ]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert_eq!(exposure.ra, Some(237.502568422944));
    }

    #[test]
    fn parse_exposure_preserves_each_header_coordinate_pair_independently() {
        let headers = HashMap::from([
            ("RA".to_owned(), "237.502568422944".to_owned()),
            ("DEC".to_owned(), "43.8990616679659".to_owned()),
            ("OBJCTRA".to_owned(), "15 50 01".to_owned()),
            ("OBJCTDEC".to_owned(), "+43 53 57".to_owned()),
        ]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert_eq!(
            exposure.header_coordinates.ra_dec.ra,
            Some(237.502568422944)
        );
        assert_eq!(
            exposure.header_coordinates.ra_dec.dec,
            Some(43.8990616679659)
        );
        assert!(
            (exposure.header_coordinates.objctra_objctdec.ra.unwrap() - 237.50416666666666).abs()
                < 0.000_000_001
        );
        assert!(
            (exposure.header_coordinates.objctra_objctdec.dec.unwrap() - 43.899166666666666).abs()
                < 0.000_000_001
        );

        assert_eq!(exposure.ra, exposure.header_coordinates.ra_dec.ra);
        assert_eq!(exposure.dec, exposure.header_coordinates.ra_dec.dec);
    }

    #[test]
    fn parse_exposure_falls_back_to_sexagesimal_objctra_in_hours() {
        let headers = HashMap::from([("OBJCTRA".to_owned(), "15 50 01".to_owned())]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert_eq!(exposure.ra, Some(237.50416666666666));
    }

    #[test]
    fn parse_exposure_converts_high_sexagesimal_objctra_to_degrees() {
        let headers = HashMap::from([("OBJCTRA".to_owned(), "23 59 59".to_owned())]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert!((exposure.ra.unwrap() - 359.99583333333334).abs() < 0.000_000_001);
    }

    #[test]
    fn parse_exposure_rejects_out_of_range_right_ascension() {
        let headers = HashMap::from([("RA".to_owned(), "360.0".to_owned())]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert_eq!(exposure.ra, None);
    }

    #[test]
    fn parse_exposure_rejects_out_of_range_declinations_from_both_sources() {
        let headers = HashMap::from([
            ("DEC".to_owned(), "90.1".to_owned()),
            ("OBJCTDEC".to_owned(), "-90 00 01".to_owned()),
        ]);
        let mut exposure = Exposure::default();

        parse_exposure(&mut exposure, &headers);

        assert_eq!(exposure.header_coordinates.ra_dec.dec, None);
        assert_eq!(exposure.header_coordinates.objctra_objctdec.dec, None);
        assert_eq!(exposure.dec, None);
    }

    #[test]
    fn parse_exposure_accepts_right_ascension_degree_boundaries() {
        for value in ["0.0", "359.9"] {
            let headers = HashMap::from([("RA".to_owned(), value.to_owned())]);
            let mut exposure = Exposure::default();

            parse_exposure(&mut exposure, &headers);

            assert_eq!(exposure.ra, Some(value.parse().unwrap()));
        }
    }

    #[test]
    fn test_extract_metadata_preserves_duplicate_cards() -> Result<()> {
        astro_io::fits::backend::with_cfitsio(|| {
            let path = unique_temp_fits_path("metadata");
            let mut file = FitsFile::create(&path).open()?;
            let hdu = file.primary_hdu()?;

            hdu.write_key(&mut file, "OBJECT", "M42".to_string())?;
            hdu.write_key(&mut file, "EXPTIME", 300.0f32)?;
            append_duplicate_test_records(&mut file)?;
            drop(file);

            let metadata = extract_metadata_from_path(&path)?;

            assert_eq!(metadata.exposure.object_name.as_deref(), Some("M42"));
            assert_eq!(metadata.exposure.exposure_time, Some(300.0));
            assert_eq!(metadata.raw_headers.get("OBJECT"), Some(&"M42".to_string()));
            assert_eq!(metadata.raw_headers.get("DUPKEY"), Some(&"two".to_string()));
            assert_eq!(
                metadata
                    .raw_header_cards
                    .iter()
                    .filter(|card| card.keyword == "DUPKEY")
                    .count(),
                2
            );
            assert!(metadata.raw_header_cards.iter().any(|card| {
                card.keyword == "COMMENT"
                    && card
                        .raw_card
                        .as_deref()
                        .is_some_and(|raw| raw.contains("metadata parser comment"))
            }));

            fs::remove_file(path)?;
            Ok(())
        })
    }

    fn append_duplicate_test_records(file: &mut FitsFile) -> Result<()> {
        let mut status = 0;
        let raw_fits = unsafe { file.as_raw() };
        let comment = CString::new("metadata parser comment")?;
        let duplicate_one = CString::new("DUPKEY  = 'one'")?;
        let duplicate_two = CString::new("DUPKEY  = 'two'")?;

        unsafe {
            fits_write_comment(raw_fits, comment.as_ptr(), &mut status);
            fits_write_record(raw_fits, duplicate_one.as_ptr(), &mut status);
            fits_write_record(raw_fits, duplicate_two.as_ptr(), &mut status);
        }

        check_status(status)?;
        Ok(())
    }

    fn unique_temp_fits_path(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "astro-metadata-{prefix}-{}-{timestamp}.fits",
            std::process::id()
        ))
    }
}
