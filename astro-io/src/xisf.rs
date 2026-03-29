//! XISF file loader
//!
//! This module provides functionality to load pixel data from XISF files.
//! XISF (Extensible Image Serialization Format) is an XML-based format used by PixInsight.
//!
//! The current loader intentionally supports a narrow, explicit subset:
//! uncompressed, single-channel, attachment-backed `UInt16` images.
//! Malformed or unsupported files return an error instead of producing
//! placeholder image data.

use anyhow::{bail, Context, Result};
use log::debug;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageDataBlock {
    width: usize,
    height: usize,
    data_offset: u64,
    data_size: usize,
}

/// Read an XISF file and return its pixel data, width, and height.
///
/// The current implementation supports uncompressed, single-channel,
/// attachment-backed `UInt16` XISF images.
pub fn load_xisf(path: &Path) -> Result<(Vec<f32>, usize, usize)> {
    debug!("Loading XISF file: {}", path.display());
    let file = File::open(path).context("Failed to open XISF file")?;
    let mut reader = BufReader::new(file);
    load_xisf_from_reader(&mut reader)
        .with_context(|| format!("Failed to load XISF image from {}", path.display()))
}

fn load_xisf_from_reader<R: Read + Seek>(reader: &mut R) -> Result<(Vec<f32>, usize, usize)> {
    let mut signature = [0u8; 8];
    reader
        .read_exact(&mut signature)
        .context("Failed to read XISF signature")?;

    if &signature != b"XISF0100" {
        bail!("Invalid XISF signature");
    }

    let mut header_size_bytes = [0u8; 4];
    reader
        .read_exact(&mut header_size_bytes)
        .context("Failed to read header size")?;
    let header_size = u32::from_le_bytes(header_size_bytes) as usize;

    let xml_content = extract_xml_content(reader, header_size)?;
    let image = parse_image_data_block(&xml_content)?;

    debug!(
        "Parsed XISF image layout: {}x{}, offset={}, size={}",
        image.width, image.height, image.data_offset, image.data_size
    );

    reader
        .seek(SeekFrom::Start(image.data_offset))
        .context("Failed to seek to image data")?;

    let mut data = vec![0u8; image.data_size];
    reader
        .read_exact(&mut data)
        .context("Failed to read image data")?;

    let pixels = read_pixel_data(&data, image.width, image.height)?;

    Ok((pixels, image.width, image.height))
}

/// Extract XML content from the XISF header
fn extract_xml_content<R: Read>(reader: &mut R, header_size: usize) -> Result<String> {
    let mut header_data = vec![0u8; header_size];
    reader
        .read_exact(&mut header_data)
        .context("Failed to read XML header")?;

    let xml_start = header_data
        .windows(5)
        .position(|window| window == b"<?xml")
        .context("XISF header does not contain an XML declaration")?;

    let actual_size = header_data[xml_start..]
        .iter()
        .position(|&b| b == 0)
        .map(|pos| xml_start + pos)
        .unwrap_or(header_data.len());

    let xml_bytes = &header_data[xml_start..actual_size];
    let xml_content = std::str::from_utf8(xml_bytes)
        .context("Failed to decode XISF XML header as UTF-8")?
        .to_string();

    Ok(xml_content)
}

fn parse_image_data_block(xml: &str) -> Result<ImageDataBlock> {
    let image_tag = extract_first_image_tag(xml)?;
    let (width, height, channels) = parse_geometry(image_tag)?;
    let sample_format = extract_attribute(image_tag, "sampleFormat")
        .context("XISF image is missing required sampleFormat attribute")?;

    if sample_format != "UInt16" {
        bail!(
            "Unsupported XISF sampleFormat '{}'; only UInt16 images are currently supported",
            sample_format
        );
    }

    if let Some(compression) = extract_attribute(image_tag, "compression") {
        bail!(
            "Unsupported compressed XISF image '{}'; only uncompressed attachment-backed images are currently supported",
            compression
        );
    }

    if channels != 1 {
        bail!(
            "Unsupported XISF geometry '{}': only single-channel images are currently supported",
            extract_attribute(image_tag, "geometry").unwrap_or_default()
        );
    }

    let location = extract_attribute(image_tag, "location")
        .context("XISF image is missing required location attribute")?;
    let (data_offset, data_size) = parse_attachment_location(&location)?;

    Ok(ImageDataBlock {
        width,
        height,
        data_offset,
        data_size,
    })
}

fn extract_first_image_tag(xml: &str) -> Result<&str> {
    let start = xml
        .find("<Image ")
        .context("XISF header is missing an <Image> element")?;
    let end = xml[start..]
        .find('>')
        .map(|pos| start + pos + 1)
        .context("XISF <Image> element is not terminated")?;
    Ok(&xml[start..end])
}

fn parse_geometry(image_tag: &str) -> Result<(usize, usize, usize)> {
    let geometry = extract_attribute(image_tag, "geometry")
        .context("XISF image is missing required geometry attribute")?;
    let parts: Vec<&str> = geometry.split(':').collect();
    if parts.len() != 3 {
        bail!(
            "Invalid XISF geometry '{}'; expected width:height:channels",
            geometry
        );
    }

    let width = parts[0]
        .parse::<usize>()
        .with_context(|| format!("Invalid XISF width in geometry '{}'", geometry))?;
    let height = parts[1]
        .parse::<usize>()
        .with_context(|| format!("Invalid XISF height in geometry '{}'", geometry))?;
    let channels = parts[2]
        .parse::<usize>()
        .with_context(|| format!("Invalid XISF channel count in geometry '{}'", geometry))?;

    if width == 0 || height == 0 || channels == 0 {
        bail!(
            "Invalid XISF geometry '{}'; width, height, and channels must all be non-zero",
            geometry
        );
    }

    Ok((width, height, channels))
}

fn parse_attachment_location(location: &str) -> Result<(u64, usize)> {
    let parts: Vec<&str> = location.split(':').collect();
    if parts.len() != 3 {
        bail!(
            "Invalid XISF location '{}'; expected attachment:offset:size",
            location
        );
    }

    if parts[0] != "attachment" {
        bail!(
            "Unsupported XISF location '{}'; only attachment-backed images are currently supported",
            location
        );
    }

    let data_offset = parts[1]
        .parse::<u64>()
        .with_context(|| format!("Invalid XISF attachment offset in '{}'", location))?;
    let data_size = parts[2]
        .parse::<usize>()
        .with_context(|| format!("Invalid XISF attachment size in '{}'", location))?;

    Ok((data_offset, data_size))
}

/// Extract an attribute value from XML content
fn extract_attribute(xml: &str, attr_name: &str) -> Option<String> {
    let search_pattern = format!("{}=\"", attr_name);

    if let Some(start_pos) = xml.find(&search_pattern) {
        let start = start_pos + search_pattern.len();
        if let Some(end_pos) = xml[start..].find('"') {
            return Some(xml[start..start + end_pos].to_string());
        }
    }

    None
}

/// Read pixel data from a byte buffer
fn read_pixel_data(data: &[u8], width: usize, height: usize) -> Result<Vec<f32>> {
    let pixel_count = width
        .checked_mul(height)
        .context("XISF image dimensions overflow pixel count calculation")?;
    let expected_size = pixel_count
        .checked_mul(2)
        .context("XISF image dimensions overflow byte-size calculation")?;

    if data.len() < expected_size {
        bail!(
            "XISF image payload is truncated: expected at least {} bytes, got {}",
            expected_size,
            data.len()
        );
    }

    let mut pixels = Vec::with_capacity(pixel_count);
    let mut cursor = std::io::Cursor::new(data);

    for _ in 0..pixel_count {
        let value = cursor
            .read_u16::<LittleEndian>()
            .context("Truncated XISF pixel data while decoding UInt16 samples")?;
        let float_val = value as f32 / 65535.0;
        pixels.push(float_val);
    }

    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_extract_attribute() {
        let xml =
            r#"<Image id="main" geometry="1024:768:1" sampleFormat="UInt16" colorSpace="Gray">"#;

        // Test existing attributes
        assert_eq!(
            extract_attribute(xml, "geometry"),
            Some("1024:768:1".to_string())
        );
        assert_eq!(
            extract_attribute(xml, "sampleFormat"),
            Some("UInt16".to_string())
        );
        assert_eq!(
            extract_attribute(xml, "colorSpace"),
            Some("Gray".to_string())
        );

        // Test non-existent attribute
        assert_eq!(extract_attribute(xml, "nonexistent"), None);
    }

    #[test]
    fn test_read_pixel_data() {
        // Create test data for a 2x2 image with 16-bit pixels
        let mut data = Vec::new();
        let pixels = [0u16, 32768u16, 65535u16, 16384u16];

        for pixel in &pixels {
            data.extend_from_slice(&pixel.to_le_bytes());
        }

        // Read the pixel data
        let result = read_pixel_data(&data, 2, 2).unwrap();

        // Check the results
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], 0.0);
        assert!((result[1] - 0.5).abs() < 0.001);
        assert_eq!(result[2], 1.0);
        assert!((result[3] - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_read_pixel_data_rejects_truncated_buffer() {
        let err = read_pixel_data(&[0u8; 6], 2, 2).expect_err("truncated data should fail");
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn test_extract_xml_content() {
        // Create a test header with XML content
        let mut header = vec![0u8; 100];
        let xml = b"<?xml version=\"1.0\"?><xisf><Image></Image></xisf>";
        header[10..10 + xml.len()].copy_from_slice(xml);

        // Extract the XML content
        let mut reader = Cursor::new(header);
        let result = extract_xml_content(&mut reader, 100).unwrap();

        // Check the result
        assert!(result.contains("<?xml"));
        assert!(result.contains("<xisf>"));
        assert!(result.contains("<Image>"));
    }

    #[test]
    fn test_load_xisf_reads_valid_uint16_payload() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#,
            &u16_payload(&[0, 32768, 65535, 16384]),
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("valid XISF should load");
        assert_eq!((width, height), (2, 2));
        assert_eq!(pixels.len(), 4);
        assert_eq!(pixels[0], 0.0);
        assert!((pixels[1] - 0.5).abs() < 0.001);
        assert_eq!(pixels[2], 1.0);
        assert!((pixels[3] - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_load_xisf_rejects_missing_geometry() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#,
            &u16_payload(&[0, 1, 2, 3]),
        );

        let err = load_xisf(&path).expect_err("missing geometry should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("geometry"));
    }

    #[test]
    fn test_load_xisf_rejects_missing_location() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" /></xisf>"#,
            &u16_payload(&[0, 1, 2, 3]),
        );

        let err = load_xisf(&path).expect_err("missing location should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("location"));
    }

    #[test]
    fn test_load_xisf_rejects_truncated_payload() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:6" /></xisf>"#,
            &[0u8; 6],
        );

        let err = load_xisf(&path).expect_err("truncated payload should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("truncated"));
    }

    #[test]
    fn test_load_xisf_rejects_unsupported_sample_format() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:512:16" /></xisf>"#,
            &[0u8; 16],
        );

        let err = load_xisf(&path).expect_err("unsupported sample format should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("sampleFormat"));
    }

    #[test]
    fn test_load_xisf_real_sample() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/data/2024-08-23_21-44-23_LIGHT_-10.00_60.00s_0000_a.xisf");

        let (pixels, width, height) = load_xisf(&path).expect("sample XISF should load");

        assert_eq!((width, height), (3856, 2180));
        assert_eq!(pixels.len(), width * height);
    }

    fn write_temp_xisf(xml: &str, payload: &[u8]) -> PathBuf {
        let path = temp_xisf_path();
        let bytes = build_xisf_bytes(xml, payload);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn temp_xisf_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ravensky-astro-xisf-{}-{}.xisf",
            process::id(),
            unique
        ))
    }

    fn build_xisf_bytes(xml: &str, payload: &[u8]) -> Vec<u8> {
        const DATA_OFFSET: usize = 512;
        const PREAMBLE_LEN: usize = 12;
        const HEADER_SIZE: usize = DATA_OFFSET - PREAMBLE_LEN;

        assert!(xml.len() <= HEADER_SIZE);

        let mut bytes = Vec::with_capacity(DATA_OFFSET + payload.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(HEADER_SIZE as u32).to_le_bytes());

        let mut header = vec![0u8; HEADER_SIZE];
        header[..xml.len()].copy_from_slice(xml.as_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(payload);

        bytes
    }

    fn u16_payload(values: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * 2);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}
