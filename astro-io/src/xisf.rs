//! XISF file loader
//!
//! This module provides functionality to load pixel data from XISF files.
//! XISF (Extensible Image Serialization Format) is an XML-based format used by PixInsight.
//!
//! The current loader intentionally supports a narrow, explicit subset:
//! uncompressed, single-channel, attachment-backed `UInt16` images.
//! Malformed or unsupported files return an error instead of producing
//! placeholder image data.

pub(crate) mod structural;

use anyhow::{anyhow, bail, Context, Result};
use log::debug;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use structural::{
    checked_range, visit_xml, BlockLocation, ImageDescriptor, MonolithicEnvelope, VisitError,
    XmlEvent, PREFIX_LEN,
};

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
    let source_len = reader
        .seek(SeekFrom::End(0))
        .context("Failed to determine XISF source extent")?;
    reader
        .seek(SeekFrom::Start(0))
        .context("Failed to seek to XISF prefix")?;
    let prefix_len = source_len.min(PREFIX_LEN) as usize;
    let mut prefix = vec![0u8; prefix_len];
    reader
        .read_exact(&mut prefix)
        .context("Failed to read XISF monolithic prefix")?;
    let envelope = MonolithicEnvelope::parse(&prefix, source_len).map_err(anyhow::Error::new)?;
    let xml_content = read_xml_header(reader, &envelope)?;
    let image = parse_image_data_block(&xml_content, source_len, envelope.xml_end())?;

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

fn read_xml_header<R: Read + Seek>(
    reader: &mut R,
    envelope: &MonolithicEnvelope,
) -> Result<Vec<u8>> {
    let range = envelope.xml_range();
    let header_size = usize::try_from(envelope.xml_len())
        .context("XISF XML header exceeds this platform's address space")?;
    let mut header_data = vec![0u8; header_size];
    reader
        .seek(SeekFrom::Start(range.start))
        .context("Failed to seek to XISF XML header")?;
    reader
        .read_exact(&mut header_data)
        .context("Failed to read XML header")?;
    Ok(header_data)
}

fn parse_image_data_block(xml: &[u8], source_len: u64, xml_end: u64) -> Result<ImageDataBlock> {
    let mut descriptor = None;
    match visit_xml(xml, |event| {
        let XmlEvent::Element(element) = event else {
            return Ok(());
        };
        if !element.namespace.is_core() {
            return Err(anyhow!("Unsupported XML namespace on {}", element.name));
        }
        if element.name == "Image" && descriptor.is_none() {
            descriptor = Some(ImageDescriptor::parse(&element).map_err(anyhow::Error::new)?);
        }
        Ok(())
    }) {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(anyhow::Error::new(error)),
        Err(VisitError::Consumer(error)) => return Err(error),
    }
    let descriptor = descriptor.context("XISF header is missing an Image element")?;
    let dimensions = descriptor.geometry.dimensions();
    if dimensions.len() != 3 {
        bail!(
            "Invalid XISF geometry; expected width:height:channels, got {} dimensions",
            dimensions.len()
        );
    }
    let (width, height, channels) = (
        usize::try_from(dimensions[0]).context("XISF image width exceeds address space")?,
        usize::try_from(dimensions[1]).context("XISF image height exceeds address space")?,
        dimensions[2],
    );

    if descriptor.sample_format != "UInt16" {
        bail!(
            "Unsupported XISF sampleFormat '{}'; only UInt16 images are currently supported",
            descriptor.sample_format
        );
    }

    if let Some(compression) = descriptor.compression {
        bail!(
            "Unsupported compressed XISF image '{}'; only uncompressed attachment-backed images are currently supported",
            compression
        );
    }

    if channels != 1 {
        bail!("Unsupported XISF geometry: only single-channel images are currently supported");
    }

    let (data_offset, data_size) = match descriptor.location {
        BlockLocation::Attachment { offset, size } => {
            if offset < xml_end {
                bail!("XISF attachment overlaps the XML header");
            }
            checked_range(offset, size, source_len, "XISF image attachment")
                .map_err(anyhow::Error::new)?;
            let size = usize::try_from(size)
                .context("XISF image attachment exceeds this platform's address space")?;
            (offset, size)
        }
        BlockLocation::Inline { .. } => {
            bail!("Unsupported inline XISF image; only attachment-backed images are currently supported")
        }
        BlockLocation::Embedded => {
            bail!("Unsupported embedded XISF image; only attachment-backed images are currently supported")
        }
        BlockLocation::Other(location) => bail!(
            "Unsupported XISF location '{}'; only attachment-backed images are currently supported",
            location
        ),
    };

    let expected = descriptor.expected_bytes().map_err(anyhow::Error::new)?;
    if (data_size as u64) < expected {
        bail!(
            "XISF image payload is truncated: geometry requires {expected} bytes, location declares {data_size}"
        );
    }
    if data_size as u64 > expected {
        bail!(
            "XISF image payload has trailing bytes: geometry requires {expected} bytes, location declares {data_size}"
        );
    }

    Ok(ImageDataBlock {
        width,
        height,
        data_offset,
        data_size,
    })
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
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn load_xisf_rejects_legacy_twelve_byte_prefix() {
        let xml = r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#;
        let path = temp_xisf_path();
        fs::write(
            &path,
            build_legacy_twelve_byte_xisf(xml, &u16_payload(&[0, 1, 2, 3])),
        )
        .unwrap();

        let error = load_xisf(&path).expect_err("the historical 12-byte form must be rejected");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("reserved"));
    }

    #[test]
    fn load_xisf_rejects_nonzero_reserved_bytes() {
        let xml = r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#;
        let mut bytes = build_xisf_bytes(xml, &u16_payload(&[0, 1, 2, 3]));
        bytes[12] = 1;
        let path = temp_xisf_path();
        fs::write(&path, bytes).unwrap();

        let error = load_xisf(&path).expect_err("reserved prefix bytes must be zero");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("reserved"));
    }

    #[test]
    fn load_xisf_rejects_each_truncated_prefix_section() {
        for (length, expected) in [(7, "signature"), (11, "length"), (15, "reserved")] {
            let path = temp_xisf_path();
            fs::write(&path, &b"XISF0100\x01\0\0\0\0\0\0\0"[..length]).unwrap();

            let error = load_xisf(&path).expect_err("truncated prefix must fail");
            fs::remove_file(&path).unwrap();

            assert!(
                format!("{error:#}").contains(expected),
                "length {length} should identify the truncated {expected} field: {error:#}"
            );
        }
    }

    #[test]
    fn load_xisf_rejects_invalid_utf8_and_malformed_xml() {
        let mut invalid_utf8 = build_xisf_bytes(
            r#"<xisf><Image geometry="1:1:1" sampleFormat="UInt16" location="attachment:512:2"/></xisf>"#,
            &[0, 0],
        );
        invalid_utf8[16] = 0xff;
        let invalid_utf8_path = temp_xisf_path();
        fs::write(&invalid_utf8_path, invalid_utf8).unwrap();
        let error = load_xisf(&invalid_utf8_path).expect_err("invalid UTF-8 must fail");
        fs::remove_file(&invalid_utf8_path).unwrap();
        assert!(format!("{error:#}").contains("UTF-8"));

        let malformed_path = write_temp_xisf(
            r#"<xisf><Image geometry="1:1:1" sampleFormat="UInt16" location="attachment:512:2"/>"#,
            &[0, 0],
        );
        let error = load_xisf(&malformed_path).expect_err("malformed XML must fail");
        fs::remove_file(&malformed_path).unwrap();
        assert!(format!("{error:#}").contains("XML"));
    }

    #[test]
    fn load_xisf_accepts_qualified_core_namespace_elements() {
        let path = write_temp_xisf(
            r#"<x:xisf xmlns:x="http://www.pixinsight.com/xisf"><x:Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8"/></x:xisf>"#,
            &u16_payload(&[0, 32768, 65535, 16384]),
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("qualified core elements should load");
        assert_eq!((width, height, pixels.len()), (2, 2, 4));
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
    #[ignore = "requires the local capture in tests/data; run explicitly where available"]
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
        const PREFIX_LEN: usize = 16;

        assert!(PREFIX_LEN + xml.len() <= DATA_OFFSET);

        let mut bytes = Vec::with_capacity(DATA_OFFSET + payload.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(xml.as_bytes());
        bytes.resize(DATA_OFFSET, 0);
        bytes.extend_from_slice(payload);

        bytes
    }

    fn build_legacy_twelve_byte_xisf(xml: &str, payload: &[u8]) -> Vec<u8> {
        const DATA_OFFSET: usize = 512;
        let mut bytes = Vec::with_capacity(DATA_OFFSET + payload.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(xml.as_bytes());
        bytes.resize(DATA_OFFSET, 0);
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
