//! Low-level image writers. They consume a complete list of image descriptors;
//! deciding which source metadata may be mapped or discarded belongs to the caller.

mod xisf;
pub use xisf::{
    write_xisf_u16, write_xisf_u16_with_metadata, write_xisf_u16_with_properties,
    xisf_output_bound_u16, xisf_output_bound_with_metadata, xisf_output_bound_with_properties,
    XisfCodec, XisfKeywordSpec, XisfPropertySpec, XisfPropertyValue,
};

use anyhow::{bail, Context, Result};
use fitsio::{
    images::{ImageDescription, ImageType},
    FitsFile,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// One two-dimensional monochrome UInt16 image in output order.
#[derive(Debug, Clone, Copy)]
pub struct U16ImageSpec<'a> {
    pub id: Option<&'a str>,
    pub width: usize,
    pub height: usize,
}

impl U16ImageSpec<'_> {
    fn sample_count(self) -> Result<usize> {
        if self.width == 0 || self.height == 0 {
            bail!("UInt16 image dimensions must be nonzero");
        }
        self.width
            .checked_mul(self.height)
            .context("UInt16 image sample count overflows")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitsCodec {
    Uncompressed,
    Rice,
    Gzip1,
    Gzip2,
}

/// An unknown native bound is not usable as a disk reservation. Uncompressed
/// output has a finite bound for this writer's fixed UInt16 HDU schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedSizeBound {
    Finite(u64),
    Unknown(&'static str),
}

impl EncodedSizeBound {
    /// An unknown native bound is never eligible for preflight disk admission.
    pub fn is_resource_safe(self) -> bool {
        matches!(self, Self::Finite(_))
    }
}

pub fn fits_output_bound_u16(
    images: &[U16ImageSpec<'_>],
    codec: FitsCodec,
) -> Result<EncodedSizeBound> {
    fits_output_bound_with_cards(images, codec, 0)
}

/// Complete FITS bound, allowing `extra_cards` additional 80-byte
/// cards anywhere among the HDUs. The fixed writer creates at most 16 cards
/// per image HDU including END, unsigned scaling, EXTNAME and checksums. All
/// extra cards are charged against every HDU, deliberately overcounting when
/// their distribution is not yet known. FITS headers and image data occupy
/// separate 2880-byte records.
pub fn fits_output_bound_with_cards(
    images: &[U16ImageSpec<'_>],
    codec: FitsCodec,
    extra_cards: u64,
) -> Result<EncodedSizeBound> {
    validate_fits_specs(images)?;
    if codec != FitsCodec::Uncompressed {
        // CFITSIO 3.49 uses one full-width row tile for these UInt16 codecs.
        // Rice's maximum is 2*n + n/32 + 6 bytes. For GZIP, charge zlib's
        // general gzip-wrapped deflateBound: n + ceil(n/8) + ceil(n/64) + 23.
        // A row has an 8-byte VLA descriptor; charge up to one FITS block of
        // heap alignment per row. The fixed CFITSIO BINTABLE/Z* writer path
        // plus translated source-writer cards uses fewer than 64 cards before
        // caller metadata, which is charged to every image HDU.
        let cards = 64u64
            .checked_add(extra_cards)
            .context("FITS compressed card count overflows")?;
        let header_records = cards
            .checked_mul(80)
            .context("FITS compressed header overflows")?
            .div_ceil(2880);
        let mut total = 5760u64; // Blank primary HDU: at most two FITS records.
        for image in images {
            let width = u64::try_from(image.width)?;
            let height = u64::try_from(image.height)?;
            let raw_tile = width.checked_mul(2).context("FITS tile size overflows")?;
            if raw_tile > 16 << 20 {
                bail!("Compressed FITS row tile exceeds 16 MiB native working limit");
            }
            let tile_bound = match codec {
                FitsCodec::Rice => raw_tile
                    .checked_add(width / 32)
                    .and_then(|n| n.checked_add(6)),
                FitsCodec::Gzip1 | FitsCodec::Gzip2 => raw_tile
                    .checked_add(raw_tile.div_ceil(8))
                    .and_then(|n| n.checked_add(raw_tile.div_ceil(64)))
                    .and_then(|n| n.checked_add(23)),
                FitsCodec::Uncompressed => None,
            }
            .context("FITS compressed tile bound overflows")?;
            let per_row = tile_bound
                .checked_add(8 + 2880)
                .context("FITS row bound overflows")?;
            let data = per_row
                .checked_mul(height)
                .context("FITS compressed heap bound overflows")?;
            let hdu = header_records
                .checked_add(data.div_ceil(2880))
                .and_then(|records| records.checked_mul(2880))
                .context("FITS compressed HDU bound overflows")?;
            total = total
                .checked_add(hdu)
                .context("FITS complete compressed bound overflows")?;
        }
        return Ok(EncodedSizeBound::Finite(total));
    }
    let header_cards = 16u64
        .checked_add(extra_cards)
        .context("FITS card count overflows")?;
    let header_bytes = header_cards
        .checked_mul(80)
        .context("FITS header bytes overflow")?;
    let header_records = header_bytes.div_ceil(2880);
    let mut total = 0u64;
    for image in images {
        let pixels = u64::try_from(image.sample_count()?)?;
        let data_bytes = pixels.checked_mul(2).context("FITS pixel bytes overflow")?;
        let hdu = header_records
            .checked_add(data_bytes.div_ceil(2880))
            .and_then(|records| records.checked_mul(2880))
            .context("FITS complete HDU bound overflows")?;
        total = total
            .checked_add(hdu)
            .context("FITS complete output bound overflows")?;
    }
    Ok(EncodedSizeBound::Finite(total))
}

impl FitsCodec {
    fn native(self) -> Option<i32> {
        match self {
            Self::Uncompressed => None,
            Self::Rice => Some(fitsio::sys::RICE_1 as i32),
            Self::Gzip1 => Some(fitsio::sys::GZIP_1 as i32),
            Self::Gzip2 => Some(fitsio::sys::GZIP_2 as i32),
        }
    }
}

/// Write every supplied image using exact logical UInt16 samples. The callback
/// fills bounded chunks and may stream from another source. The destination
/// must not exist; on a failure after creation, this function removes its own
/// partial output. No source metadata is implicitly copied or discarded.
pub fn write_fits_u16(
    path: &Path,
    images: &[U16ImageSpec<'_>],
    codec: FitsCodec,
    mut read: impl FnMut(usize, u64, &mut [u16]) -> Result<()>,
) -> Result<()> {
    validate_fits_specs(images)?;
    if codec != FitsCodec::Uncompressed && images.iter().any(|image| image.width > (16 << 20) / 2) {
        bail!("Compressed FITS row tile exceeds 16 MiB native working limit");
    }
    if path.exists() {
        bail!("FITS output already exists: {}", path.display());
    }
    match codec.native() {
        None => write_uncompressed_fits(path, images, &mut read),
        Some(native) => {
            let scratch = Scratch::new(path)?;
            write_uncompressed_fits(&scratch.path, images, &mut read)?;
            compress_fits(&scratch.path, path, images.len(), native)
        }
    }
}

fn validate_fits_specs(images: &[U16ImageSpec<'_>]) -> Result<()> {
    if images.is_empty() {
        bail!("FITS writer requires at least one image");
    }
    for (index, image) in images.iter().enumerate() {
        image.sample_count()?;
        std::os::raw::c_long::try_from(image.width)
            .context("FITS width exceeds native axis range")?;
        std::os::raw::c_long::try_from(image.height)
            .context("FITS height exceeds native axis range")?;
        if index > 0 && image.id.is_none() {
            bail!("FITS extension image {index} requires an explicit identifier");
        }
        if let Some(id) = image.id {
            if id.is_empty() || id.len() > 68 || !id.bytes().all(|byte| (32..=126).contains(&byte))
            {
                bail!("FITS image identifier must be 1..=68 printable ASCII bytes");
            }
        }
    }
    Ok(())
}

fn write_uncompressed_fits(
    path: &Path,
    images: &[U16ImageSpec<'_>],
    read: &mut impl FnMut(usize, u64, &mut [u16]) -> Result<()>,
) -> Result<()> {
    let mut created = false;
    let result = crate::fits::backend::with_cfitsio(|| {
        let first = ImageDescription {
            data_type: ImageType::UnsignedShort,
            // fitsio reverses Rust dimension order when writing FITS axes.
            dimensions: &[images[0].height, images[0].width],
        };
        let mut file = FitsFile::create(path).with_custom_primary(&first).open()?;
        created = true;
        for (index, image) in images.iter().enumerate() {
            let hdu = if index == 0 {
                file.primary_hdu()?
            } else {
                let description = ImageDescription {
                    data_type: ImageType::UnsignedShort,
                    dimensions: &[image.height, image.width],
                };
                file.create_image(
                    image.id.context("Missing FITS extension ID")?.to_owned(),
                    &description,
                )?
            };
            if index == 0 {
                if let Some(id) = image.id {
                    hdu.write_key(&mut file, "EXTNAME", id)?;
                }
            }
            let count = image.sample_count()?;
            let mut buffer = [0u16; 65_536];
            for start in (0..count).step_by(buffer.len()) {
                let size = (count - start).min(buffer.len());
                read(index, start as u64, &mut buffer[..size])?;
                hdu.write_section(&mut file, start, start + size, &buffer[..size])?;
            }
            let mut status = 0;
            // SAFETY: this is the selected, writable HDU; native handle is
            // live within the coordinated backend gate.
            unsafe { fitsio::sys::ffpcks(file.as_raw(), &mut status) };
            fitsio::errors::check_status(status)?;
        }
        Ok(())
    });
    if result.is_err() && created {
        let _ = fs::remove_file(path);
    }
    result
}

fn compress_fits(source: &Path, target: &Path, image_count: usize, codec: i32) -> Result<()> {
    let mut created = false;
    let result = crate::fits::backend::with_cfitsio(|| {
        let mut input = FitsFile::open(source)?;
        let mut output = FitsFile::create(target).open()?;
        created = true;
        for index in 0..image_count {
            let _ = input.hdu(index)?;
            let mut status = 0;
            // SAFETY: both independent handles stay alive within the backend
            // gate. The input HDU is selected before native compression.
            unsafe {
                fitsio::sys::fits_set_compression_type(output.as_raw(), codec, &mut status);
                if status == 0 {
                    fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
                }
            }
            fitsio::errors::check_status(status)
                .with_context(|| format!("Failed to compress FITS image {index}"))?;
            // CFITSIO may copy source metadata into the compressed table;
            // refresh the checksum for the physical output HDU.
            let mut checksum_status = 0;
            unsafe { fitsio::sys::ffpcks(output.as_raw(), &mut checksum_status) };
            fitsio::errors::check_status(checksum_status)?;
        }
        Ok(())
    });
    if result.is_err() && created {
        let _ = fs::remove_file(target);
    }
    result
}

struct Scratch {
    directory: PathBuf,
    path: PathBuf,
}

impl Scratch {
    fn new(target: &Path) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let name = target
            .file_name()
            .context("FITS output has no filename")?
            .to_string_lossy();
        for _ in 0..32 {
            let directory = parent.join(format!(
                ".{name}.p1-scratch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        path: directory.join("input.fits"),
                        directory,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("Failed to create FITS scratch directory"),
            }
        }
        bail!("Could not select a unique FITS compression scratch path")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.directory);
    }
}
