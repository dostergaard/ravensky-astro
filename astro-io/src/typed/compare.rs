//! Exact image comparison after independent full container validation.

use super::{read_u16_chunk, ContainerFormat, ContainerInventory, ImageRecord, PixelReadLimits};
use crate::validation::{validate_file, ValidationLevel, ValidationOptions};
use anyhow::{bail, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageMismatch {
    ImageCount {
        source: usize,
        output: usize,
    },
    Identity {
        index: usize,
        source: Option<String>,
        output: Option<String>,
    },
    Geometry {
        index: usize,
        source: Vec<u64>,
        output: Vec<u64>,
    },
    SampleFormat {
        index: usize,
        source: String,
        output: String,
    },
    Pixel {
        image: usize,
        sample: u64,
        source: u16,
        output: u16,
    },
}

/// Compare every UInt16 image's identity, geometry, type and logical samples.
/// The existing independent validators first check both containers at full
/// level, including XISF checksums and codec framing. Other sample types are
/// explicitly unsupported by this initial comparator.
pub fn compare_u16_containers(
    source_path: &Path,
    source: &ContainerInventory,
    output_path: &Path,
    output: &ContainerInventory,
    limits: PixelReadLimits,
) -> Result<Option<ImageMismatch>> {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    validate_file(source_path, &options, None)?;
    validate_file(output_path, &options, None)?;
    if source.images.len() != output.images.len() {
        return Ok(Some(ImageMismatch::ImageCount {
            source: source.images.len(),
            output: output.images.len(),
        }));
    }
    if limits.max_chunk_samples == 0 || limits.max_chunk_samples > 1_048_576 {
        bail!("Comparison chunk sample limit must be within 1..=1048576");
    }
    for (index, (left, right)) in source.images.iter().zip(&output.images).enumerate() {
        let left_id = canonical_id(source, index);
        let right_id = canonical_id(output, index);
        if left_id != right_id {
            return Ok(Some(ImageMismatch::Identity {
                index,
                source: left_id.map(str::to_owned),
                output: right_id.map(str::to_owned),
            }));
        }
        let left_shape = canonical_geometry(source.format, left);
        let right_shape = canonical_geometry(output.format, right);
        if left_shape != right_shape {
            return Ok(Some(ImageMismatch::Geometry {
                index,
                source: left_shape,
                output: right_shape,
            }));
        }
        if left.sample_format != right.sample_format {
            return Ok(Some(ImageMismatch::SampleFormat {
                index,
                source: left.sample_format.clone(),
                output: right.sample_format.clone(),
            }));
        }
        if left.sample_format != "UInt16" {
            bail!("Exact comparator does not support {}", left.sample_format);
        }
        if left_shape.len() != 3 || left_shape[2] != 1 {
            bail!("Exact comparator requires two-dimensional monochrome UInt16 images");
        }
        let count = left_shape[0]
            .checked_mul(left_shape[1])
            .ok_or_else(|| anyhow::anyhow!("Comparison sample count overflows"))?;
        let capacity = usize::try_from(count.min(limits.max_chunk_samples as u64))?;
        let mut left_pixels = vec![0u16; capacity];
        let mut right_pixels = vec![0u16; capacity];
        let mut start = 0u64;
        while start < count {
            let size = usize::try_from((count - start).min(capacity as u64))?;
            read_u16_chunk(
                source_path,
                source,
                index,
                start,
                &mut left_pixels[..size],
                limits,
            )?;
            read_u16_chunk(
                output_path,
                output,
                index,
                start,
                &mut right_pixels[..size],
                limits,
            )?;
            if let Some(position) = left_pixels[..size]
                .iter()
                .zip(&right_pixels[..size])
                .position(|(a, b)| a != b)
            {
                return Ok(Some(ImageMismatch::Pixel {
                    image: index,
                    sample: start + position as u64,
                    source: left_pixels[position],
                    output: right_pixels[position],
                }));
            }
            start += size as u64;
        }
    }
    Ok(None)
}

fn canonical_geometry(format: ContainerFormat, image: &ImageRecord) -> Vec<u64> {
    match (format, image.axes.as_slice()) {
        (ContainerFormat::Fits, [width, height]) => vec![*width, *height, 1],
        _ => image.axes.clone(),
    }
}

fn canonical_id<'a>(inventory: &'a ContainerInventory, index: usize) -> Option<&'a str> {
    let id = inventory.images[index].id.as_deref();
    if inventory.format == ContainerFormat::Fits && index == 0 && id == Some("_PRIMARY") {
        None
    } else {
        id
    }
}
