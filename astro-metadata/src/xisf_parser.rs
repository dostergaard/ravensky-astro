//! Parser for XISF file metadata
//!
//! This module provides functions to extract metadata from XISF files
//! and convert it into the AstroMetadata structure.

use anyhow::{Context, Result};
use astro_io::fits::{header_cards_to_map, FitsHeaderCard};
use astro_io::xisf::{read_metadata_records, XisfImageMetadata, XisfMetadataRecords};
use chrono::{DateTime, NaiveDateTime, Utc};
use log::warn;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use super::coordinates::populate_header_coordinates;
use super::types::{AstroMetadata, AttachmentInfo, ColorManagement, DisplayFunction, XisfMetadata};

/// Extract metadata from an XISF file
pub fn extract_metadata<R: Read + Seek>(reader: &mut R) -> Result<AstroMetadata> {
    let records = read_metadata_records(reader).context("Failed to read XISF metadata records")?;
    let mut metadata = AstroMetadata::default();
    metadata.detector.binning_x = 1;
    metadata.detector.binning_y = 1;
    let mut raw_header_cards = Vec::new();

    // Initialize XISF metadata
    let mut xisf_metadata = XisfMetadata {
        version: "1.0".to_string(),
        creator: None,
        creation_time: None,
        block_alignment: None,
    };

    // Syntax and raw lexical preservation stop at `astro-io`; every operation
    // below is semantic projection into the existing public metadata model.
    extract_fits_keywords(&records, &mut metadata, &mut raw_header_cards);
    extract_image_attributes(&records, &mut metadata);
    extract_xisf_metadata(&records, &mut xisf_metadata);
    extract_color_management(&records, &mut metadata);
    extract_attachments(&records, &mut metadata);

    // Store raw headers and XISF metadata
    metadata.raw_headers = header_cards_to_map(&raw_header_cards);
    populate_header_coordinates(&mut metadata.exposure, &metadata.raw_headers);
    metadata.raw_header_cards = raw_header_cards;
    metadata.xisf = Some(xisf_metadata);

    // Calculate session date
    metadata.calculate_session_date();

    Ok(metadata)
}

/// Extract metadata from an XISF file path
pub fn extract_metadata_from_path(path: &Path) -> Result<AstroMetadata> {
    let mut file = File::open(path).context("Failed to open XISF file")?;
    extract_metadata(&mut file)
}

/// Interpret ordered raw FITSKeyword records without changing their source
/// order or duplicate behavior. The semantic copy is normalized for the
/// existing metadata fields; the `astro-io` record remains untouched.
fn extract_fits_keywords(
    records: &XisfMetadataRecords,
    metadata: &mut AstroMetadata,
    raw_header_cards: &mut Vec<FitsHeaderCard>,
) {
    for keyword in records.fits_keywords() {
        let clean_value = normalize_fits_keyword_value(keyword.value());
        let card_index = raw_header_cards.len() + 1;
        raw_header_cards.push(FitsHeaderCard {
            hdu_index: 0,
            card_index,
            keyword: keyword.name().to_string(),
            value: Some(clean_value.clone()),
            comment: keyword.comment().map(str::to_string),
            raw_card: None,
        });
        process_fits_keyword(metadata, keyword.name(), &clean_value);
    }
}

fn normalize_fits_keyword_value(value: &str) -> String {
    let value = value.trim();
    match value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        Some(value) => value.trim_end().to_string(),
        None => value.to_string(),
    }
}

fn extract_image_attributes(records: &XisfMetadataRecords, metadata: &mut AstroMetadata) {
    // Creator interpretation is document-level and must not depend on whether
    // the file also contains an Image element.
    if let Some(creator_app) = property_value(records, "XISF:CreatorApplication") {
        if let Some(ref mut env) = metadata.environment {
            env.software_version = Some(creator_app.to_string());
        } else {
            metadata.environment = Some(super::types::Environment {
                software_version: Some(creator_app.to_string()),
                ..Default::default()
            });
        }
    }

    let Some(image) = records.images().first() else {
        return;
    };
    if let Some(geometry) = image.geometry() {
        let parts: Vec<&str> = geometry.split(':').collect();
        if parts.len() >= 2 {
            metadata.detector.width = parts[0].parse().unwrap_or(0);
            metadata.detector.height = parts[1].parse().unwrap_or(0);
        }
    }
}

/// Project raw document/property facts into XISF-specific semantic metadata.
fn extract_xisf_metadata(records: &XisfMetadataRecords, xisf_metadata: &mut XisfMetadata) {
    // Extract XISF version
    if let Some(version) = records.version() {
        xisf_metadata.version = version.to_string();
    }

    // Extract creator application
    if let Some(creator_app) = property_value(records, "XISF:CreatorApplication") {
        xisf_metadata.creator = Some(creator_app.to_string());
    }

    // Extract creation time
    if let Some(creation_time) = property_value(records, "XISF:CreationTime") {
        xisf_metadata.creation_time = parse_date_time(creation_time);
    }

    // Extract block alignment
    if let Some(block_alignment) = records.block_alignment() {
        xisf_metadata.block_alignment = block_alignment.parse::<usize>().ok();
    }
}

/// Project raw image/property facts into the existing color metadata model.
fn extract_color_management(records: &XisfMetadataRecords, metadata: &mut AstroMetadata) {
    let mut color_management = ColorManagement::default();
    let mut has_color_info = false;
    let image = records.images().first();

    // Extract color space
    if let Some(color_space) = image.and_then(XisfImageMetadata::color_space) {
        color_management.color_space = Some(color_space.to_string());
        has_color_info = true;
    }

    // Extract ICC profile if present
    if records
        .properties()
        .iter()
        .any(|property| property.id() == "ICCProfile" && property.value().is_some())
    {
        // In a real implementation, we would decode the base64 data here
        // For now, we'll just note that it exists
        color_management.icc_profile = Some(Vec::new());
        has_color_info = true;
    }

    // Extract display function information
    if let Some(display_function_type) = image.and_then(XisfImageMetadata::display_function) {
        let mut display_function = DisplayFunction {
            function_type: Some(display_function_type.to_string()),
            ..Default::default()
        };

        // Extract display function parameters
        if let Some(params) = image.and_then(XisfImageMetadata::display_parameters) {
            let param_pairs: Vec<&str> = params.split(';').collect();
            let mut parameters = HashMap::new();

            for pair in param_pairs {
                let kv: Vec<&str> = pair.split('=').collect();
                if kv.len() == 2 {
                    if let Ok(value) = kv[1].parse::<f64>() {
                        parameters.insert(kv[0].to_string(), value);
                    }
                }
            }

            display_function.parameters = parameters;
        }

        color_management.display_function = Some(display_function);
        has_color_info = true;
    }

    // Only set color_management if we found any color information
    if has_color_info {
        metadata.color_management = Some(color_management);
    }
}

/// Project ordered image facts into the existing attachment metadata model.
fn extract_attachments(records: &XisfMetadataRecords, metadata: &mut AstroMetadata) {
    let mut attachments = Vec::new();

    for image in records.images() {
        let mut attachment = AttachmentInfo {
            id: image
                .id()
                .map(str::to_string)
                .unwrap_or_else(|| format!("image{}", attachments.len())),
            geometry: image.geometry().unwrap_or_default().to_string(),
            sample_format: image.sample_format().unwrap_or("UInt16").to_string(),
            bits_per_sample: image
                .bits_per_sample()
                .and_then(|value| value.parse().ok())
                .unwrap_or(16),
            ..Default::default()
        };

        if let Some(compression) = image.compression() {
            attachment.compression = Some(compression.to_string());
            if let Some(params) = image.compression_parameters() {
                for pair in params.split(';') {
                    let kv: Vec<&str> = pair.split('=').collect();
                    if kv.len() == 2 {
                        attachment
                            .compression_parameters
                            .insert(kv[0].to_string(), kv[1].to_string());
                    }
                }
            }
        }

        if let Some(checksum_type) = image.checksum_type() {
            attachment.checksum_type = Some(checksum_type.to_string());
            attachment.checksum = image.checksum().map(str::to_string);
        }

        if let Some(resolution_x) = image.x_resolution() {
            attachment.resolution_x = resolution_x.parse::<f64>().ok();
            attachment.resolution_y = image
                .y_resolution()
                .and_then(|value| value.parse::<f64>().ok());
            attachment.resolution_unit = image.resolution_unit().map(str::to_string);
        }

        attachments.push(attachment);
    }

    // If we found at least one attachment, update the metadata
    if !attachments.is_empty() {
        metadata.attachments = attachments;
    }
}

/// Process a FITS keyword and update metadata
fn process_fits_keyword(metadata: &mut AstroMetadata, name: &str, value: &str) {
    match name {
        // Equipment information
        "TELESCOP" => metadata.equipment.telescope_name = Some(value.to_string()),
        "FOCALLEN" => metadata.equipment.focal_length = value.parse().ok(),
        "APERTURE" => metadata.equipment.aperture = value.parse().ok(),
        "FOCRATIO" => metadata.equipment.focal_ratio = value.parse().ok(),

        // Detector information
        "INSTRUME" | "CAMERA" => metadata.detector.camera_name = Some(value.to_string()),
        "XPIXSZ" | "PIXSIZE" => metadata.detector.pixel_size = value.parse().ok(),
        "XBINNING" => metadata.detector.binning_x = value.parse().unwrap_or(1),
        "YBINNING" => metadata.detector.binning_y = value.parse().unwrap_or(1),
        "GAIN" | "EGAIN" => metadata.detector.gain = value.parse().ok(),
        "RDNOISE" => metadata.detector.read_noise = value.parse().ok(),
        "CCD-TEMP" | "CCDTEMP" => metadata.detector.temperature = value.parse().ok(),
        "SET-TEMP" => metadata.detector.temp_setpoint = value.parse().ok(),

        // Filter information
        "FILTER" => metadata.filter.name = Some(value.to_string()),

        // Exposure information
        "OBJECT" => metadata.exposure.object_name = Some(value.to_string()),
        "RA" | "OBJCTRA" | "DEC" | "OBJCTDEC" => {}
        "DATE-OBS" => metadata.exposure.date_obs = parse_date_time(value),
        "EXPTIME" | "EXPOSURE" => metadata.exposure.exposure_time = value.parse().ok(),
        "IMAGETYP" | "FRAME" => metadata.exposure.frame_type = Some(value.to_string()),

        // Mount information
        "PIERSIDE" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.pier_side = Some(value.to_string());
            } else {
                metadata.mount = Some(super::types::Mount {
                    pier_side: Some(value.to_string()),
                    ..Default::default()
                });
            }
        }

        // Environment information
        "AMB_TEMP" | "AMBTEMP" => {
            if let Some(ref mut env) = metadata.environment {
                env.ambient_temp = value.parse().ok();
            } else {
                metadata.environment = Some(super::types::Environment {
                    ambient_temp: value.parse().ok(),
                    ..Default::default()
                });
            }
        }
        "HUMIDITY" => {
            if let Some(ref mut env) = metadata.environment {
                env.humidity = value.parse().ok();
            } else {
                metadata.environment = Some(super::types::Environment {
                    humidity: value.parse().ok(),
                    ..Default::default()
                });
            }
        }

        // WCS information
        "CRPIX1" => {
            if let Some(ref mut wcs) = metadata.wcs {
                wcs.crpix1 = value.parse().ok();
            } else {
                metadata.wcs = Some(super::types::WcsData {
                    crpix1: value.parse().ok(),
                    ..Default::default()
                });
            }
        }
        "CRPIX2" => {
            if let Some(ref mut wcs) = metadata.wcs {
                wcs.crpix2 = value.parse().ok();
            } else {
                metadata.wcs = Some(super::types::WcsData {
                    crpix2: value.parse().ok(),
                    ..Default::default()
                });
            }
        }

        // Observatory location
        "SITELAT" | "OBSLAT" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.latitude = value.parse().ok();
            } else {
                metadata.mount = Some(super::types::Mount {
                    latitude: value.parse().ok(),
                    ..Default::default()
                });
            }
        }
        "SITELONG" | "OBSLONG" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.longitude = value.parse().ok();
            } else {
                metadata.mount = Some(super::types::Mount {
                    longitude: value.parse().ok(),
                    ..Default::default()
                });
            }
        }
        "SITEELEV" | "OBSELEV" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.height = value.parse().ok();
            } else {
                metadata.mount = Some(super::types::Mount {
                    height: value.parse().ok(),
                    ..Default::default()
                });
            }
        }

        // Detector information
        "OFFSET" | "CCDOFFST" => metadata.detector.offset = value.parse().ok(),
        "READOUT" | "READOUTM" => metadata.detector.readout_mode = Some(value.to_string()),
        "USBLIMIT" | "USBTRFC" => metadata.detector.usb_limit = Some(value.to_string()),
        "ROTANG" | "ROTPA" | "ROTATANG" => metadata.detector.rotator_angle = value.parse().ok(),

        // Equipment information
        "FOCPOS" | "FOCUSPOS" => metadata.equipment.focuser_position = value.parse().ok(),
        "FOCTEMP" | "FOCUSTEMP" => metadata.equipment.focuser_temperature = value.parse().ok(),

        // Mount information
        "PEAKRA" | "PEAKRAER" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.peak_ra_error = value.parse().ok();
            } else {
                metadata.mount = Some(super::types::Mount {
                    peak_ra_error: value.parse().ok(),
                    ..Default::default()
                });
            }
        }
        "PEAKDEC" | "PEAKDCER" => {
            if let Some(ref mut mount) = metadata.mount {
                mount.peak_dec_error = value.parse().ok();
            } else {
                metadata.mount = Some(super::types::Mount {
                    peak_dec_error: value.parse().ok(),
                    ..Default::default()
                });
            }
        }

        // Environment information
        "SQM" | "SQMMAG" | "SKYQUAL" => {
            if let Some(ref mut env) = metadata.environment {
                env.sqm = value.parse().ok();
            } else {
                metadata.environment = Some(super::types::Environment {
                    sqm: value.parse().ok(),
                    ..Default::default()
                });
            }
        }

        // Exposure information
        "PROJECT" | "PROJNAME" => metadata.exposure.project_name = Some(value.to_string()),
        "SESSIONID" | "SESSID" => metadata.exposure.session_id = Some(value.to_string()),

        // Ignore other keywords
        _ => {}
    }
}

/// Return the first matching Property value, preserving the historical
/// first-record precedence while normalizing only surrounding XML text space.
fn property_value<'a>(records: &'a XisfMetadataRecords, property_id: &str) -> Option<&'a str> {
    records
        .properties()
        .iter()
        .find(|property| property.id() == property_id)
        .and_then(|property| property.value())
        .map(str::trim)
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
    use super::extract_metadata;
    use chrono::{TimeZone, Utc};
    use std::io::Cursor;

    fn xisf(xml: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16 + xml.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(xml.as_bytes());
        bytes
    }

    #[test]
    fn test_creation_time_does_not_override_observation_date() {
        let xml = concat!(
            "<?xml version=\"1.0\"?>",
            "<xisf version=\"1.0\">",
            "<Image geometry=\"2:2:1\" sampleFormat=\"UInt16\">",
            "<FITSKeyword name=\"DATE-OBS\" value=\"2024-09-04T08:39:13.204\"/>",
            "<FITSKeyword name=\"EXPTIME\" value=\"60.\"/>",
            "</Image>",
            "<Property id=\"XISF:CreationTime\" type=\"String\">2024-09-06T10:31:17</Property>",
            "<Property id=\"XISF:CreatorApplication\" type=\"String\">PixInsight 1.8.9-3</Property>",
            "</xisf>"
        );

        let metadata =
            extract_metadata(&mut Cursor::new(xisf(xml))).expect("metadata should parse");

        assert_eq!(
            metadata.exposure.date_obs,
            Some(
                Utc.with_ymd_and_hms(2024, 9, 4, 8, 39, 13).unwrap()
                    + chrono::Duration::milliseconds(204)
            )
        );
        assert_eq!(
            metadata.xisf.as_ref().and_then(|xisf| xisf.creation_time),
            Some(Utc.with_ymd_and_hms(2024, 9, 6, 10, 31, 17).unwrap())
        );
    }

    #[test]
    fn test_missing_binning_defaults_to_one() {
        let xml = concat!(
            "<?xml version=\"1.0\"?>",
            "<xisf version=\"1.0\">",
            "<Image geometry=\"2:2:1\" sampleFormat=\"UInt16\">",
            "<FITSKeyword name=\"OBJECT\" value=\"M31\"/>",
            "</Image>",
            "</xisf>"
        );

        let metadata =
            extract_metadata(&mut Cursor::new(xisf(xml))).expect("metadata should parse");

        assert_eq!(metadata.detector.binning_x, 1);
        assert_eq!(metadata.detector.binning_y, 1);
    }

    #[test]
    fn document_properties_do_not_require_an_image() {
        let xml = concat!(
            r#"<xisf version="1.0">"#,
            r#"<Property id="XISF:CreatorApplication" type="String">Producer</Property>"#,
            r#"</xisf>"#,
        );

        let metadata =
            extract_metadata(&mut Cursor::new(xisf(xml))).expect("metadata should parse");

        assert_eq!(
            metadata
                .environment
                .as_ref()
                .and_then(|environment| environment.software_version.as_deref()),
            Some("Producer")
        );
        assert_eq!(
            metadata
                .xisf
                .as_ref()
                .and_then(|xisf| xisf.creator.as_deref()),
            Some("Producer")
        );
        assert!(metadata.attachments.is_empty());
    }

    #[test]
    fn test_coordinate_keyword_pairs_are_preserved_independently() {
        let xml = concat!(
            "<?xml version=\"1.0\"?>",
            "<xisf version=\"1.0\">",
            "<Image geometry=\"2:2:1\" sampleFormat=\"UInt16\">",
            "<FITSKeyword name=\"RA\" value=\"237.502568422944\"/>",
            "<FITSKeyword name=\"DEC\" value=\"43.8990616679659\"/>",
            "<FITSKeyword name=\"OBJCTRA\" value=\"15 50 01\"/>",
            "<FITSKeyword name=\"OBJCTDEC\" value=\"+43 53 57\"/>",
            "</Image>",
            "</xisf>"
        );

        let metadata =
            extract_metadata(&mut Cursor::new(xisf(xml))).expect("metadata should parse");

        assert_eq!(
            metadata.exposure.header_coordinates.ra_dec.ra,
            Some(237.502568422944)
        );
        assert_eq!(
            metadata.exposure.header_coordinates.ra_dec.dec,
            Some(43.8990616679659)
        );
        assert!(
            (metadata
                .exposure
                .header_coordinates
                .objctra_objctdec
                .ra
                .unwrap()
                - 237.50416666666666)
                .abs()
                < 0.000_000_001
        );
        assert!(
            (metadata
                .exposure
                .header_coordinates
                .objctra_objctdec
                .dec
                .unwrap()
                - 43.899166666666666)
                .abs()
                < 0.000_000_001
        );
    }

    #[test]
    fn shared_records_preserve_semantics_precedence_and_attachment_projection() {
        let xml = concat!(
            r#"<xisf version="1.0" blockAlignment="4096">"#,
            r#"<Property id="XISF:CreatorApplication" type="String">PixInsight 1.9</Property>"#,
            r#"<Property id="XISF:CreatorApplication" type="String">Later Producer</Property>"#,
            r#"<Property id="XISF:CreationTime" type="TimePoint" value="2024-09-06T10:31:17"/>"#,
            r#"<Property id="ICCProfile" type="ByteArray">AA==</Property>"#,
            r#"<Image id="main" geometry="10:20:1" sampleFormat="UInt16" colorSpace="Gray" bitsPerSample="16" compression="zlib:6" compressionParameters="itemSize=2;level=6" checksumType="sha256" checksum="abcd" xResolution="72" yResolution="73" resolutionUnit="inch" displayFunction="STF" displayParameters="m=0.5;s=1">"#,
            r#"<FITSKeyword name="TELESCOP" value="Scope"/>"#,
            r#"<FITSKeyword name="INSTRUME" value="Camera"/>"#,
            r#"<FITSKeyword name="XPIXSZ" value="3.76"/>"#,
            r#"<FITSKeyword name="XBINNING" value="2"/>"#,
            r#"<FITSKeyword name="FILTER" value="L"/>"#,
            r#"<FITSKeyword name="DATE-OBS" value="2024-09-04T08:39:13"/>"#,
            r#"<FITSKeyword name="EXPTIME" value="60"/>"#,
            r#"<FITSKeyword name="PIERSIDE" value="East"/>"#,
            r#"<FITSKeyword name="AMBTEMP" value="12.5"/>"#,
            r#"<FITSKeyword name="CRPIX1" value="5.5"/>"#,
            r#"<FITSKeyword name="CRPIX2" value="6.5"/>"#,
            r#"<FITSKeyword name="OBJECT" value="M31"/>"#,
            r#"<FITSKeyword name="OBJECT" value="  &apos;M31 &amp; M32   &apos;  " comment="escaped &amp; ordered"/>"#,
            r#"</Image></xisf>"#,
        );

        let metadata =
            extract_metadata(&mut Cursor::new(xisf(xml))).expect("metadata should parse");

        assert_eq!(metadata.equipment.telescope_name.as_deref(), Some("Scope"));
        assert_eq!(metadata.detector.camera_name.as_deref(), Some("Camera"));
        assert_eq!(metadata.detector.pixel_size, Some(3.76));
        assert_eq!(metadata.detector.binning_x, 2);
        assert_eq!(metadata.filter.name.as_deref(), Some("L"));
        assert_eq!(metadata.exposure.exposure_time, Some(60.0));
        assert_eq!(metadata.exposure.object_name.as_deref(), Some("M31 & M32"));
        assert_eq!(metadata.raw_header_cards.len(), 13);
        assert_eq!(metadata.raw_header_cards[11].value.as_deref(), Some("M31"));
        assert_eq!(
            metadata.raw_header_cards[12].comment.as_deref(),
            Some("escaped & ordered")
        );
        assert_eq!(
            metadata.raw_headers.get("OBJECT").map(String::as_str),
            Some("M31 & M32")
        );

        let xisf = metadata.xisf.as_ref().unwrap();
        assert_eq!(xisf.version, "1.0");
        assert_eq!(xisf.creator.as_deref(), Some("PixInsight 1.9"));
        assert_eq!(xisf.block_alignment, Some(4096));
        assert_eq!(
            metadata
                .environment
                .as_ref()
                .and_then(|environment| environment.software_version.as_deref()),
            Some("PixInsight 1.9")
        );
        let mount = metadata.mount.as_ref().unwrap();
        assert_eq!(mount.pier_side.as_deref(), Some("East"));
        assert_eq!(
            metadata.environment.as_ref().unwrap().ambient_temp,
            Some(12.5)
        );
        let wcs = metadata.wcs.as_ref().unwrap();
        assert_eq!(wcs.crpix1, Some(5.5));
        assert_eq!(wcs.crpix2, Some(6.5));
        assert_eq!(
            (metadata.detector.width, metadata.detector.height),
            (10, 20)
        );
        let color = metadata.color_management.as_ref().unwrap();
        assert_eq!(color.color_space.as_deref(), Some("Gray"));
        assert!(color.icc_profile.is_some());
        assert_eq!(
            color
                .display_function
                .as_ref()
                .and_then(|display| display.function_type.as_deref()),
            Some("STF")
        );
        assert_eq!(metadata.attachments.len(), 1);
        assert_eq!(metadata.attachments[0].id, "main");
        assert_eq!(metadata.attachments[0].geometry, "10:20:1");
        assert_eq!(metadata.attachments[0].sample_format, "UInt16");
        assert_eq!(metadata.attachments[0].bits_per_sample, 16);
        assert_eq!(
            metadata.attachments[0].compression.as_deref(),
            Some("zlib:6")
        );
        assert_eq!(metadata.attachments[0].checksum.as_deref(), Some("abcd"));
        assert_eq!(metadata.attachments[0].resolution_x, Some(72.0));
        assert_eq!(metadata.attachments[0].resolution_y, Some(73.0));
    }

    #[test]
    fn structural_and_metadata_record_errors_are_not_empty_success() {
        let malformed = xisf(r#"<xisf><Image></xisf>"#);
        assert!(extract_metadata(&mut Cursor::new(malformed)).is_err());

        let mut invalid_utf8 = xisf("<xisf/> ");
        let last = invalid_utf8.len() - 1;
        invalid_utf8[last] = 0xff;
        assert!(extract_metadata(&mut Cursor::new(invalid_utf8)).is_err());

        let malformed_keyword = xisf(r#"<xisf><Image><FITSKeyword name="OBJECT"/></Image></xisf>"#);
        assert!(extract_metadata(&mut Cursor::new(malformed_keyword)).is_err());

        let xml = r#"<xisf><Image><FITSKeyword name="OBJECT" value="M31"/></Image></xisf>"#;
        let mut legacy = Vec::with_capacity(12 + xml.len());
        legacy.extend_from_slice(b"XISF0100");
        legacy.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        legacy.extend_from_slice(xml.as_bytes());
        assert!(extract_metadata(&mut Cursor::new(legacy)).is_err());
    }
}
