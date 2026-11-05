//! Low-level image writers. They consume a complete list of image descriptors;
//! deciding which source metadata may be mapped or discarded belongs to P2.

mod xisf;
pub use xisf::{
    write_xisf_u16, write_xisf_u16_with_properties, xisf_output_bound_u16,
    xisf_output_bound_with_properties, XisfCodec, XisfPropertySpec, XisfPropertyValue,
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

/// Native CFITSIO output has no proven whole-file worst-case bound in P1.
/// A caller must not use an expected compression ratio as a disk reservation.
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
    _codec: FitsCodec,
) -> Result<EncodedSizeBound> {
    validate_fits_specs(images)?;
    Ok(EncodedSizeBound::Unknown(
        "CFITSIO whole-file header, tile table and heap worst-case bound is not established",
    ))
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
            dimensions: &[images[0].width, images[0].height],
        };
        let mut file = FitsFile::create(path).with_custom_primary(&first).open()?;
        created = true;
        for (index, image) in images.iter().enumerate() {
            let hdu = if index == 0 {
                file.primary_hdu()?
            } else {
                let description = ImageDescription {
                    data_type: ImageType::UnsignedShort,
                    dimensions: &[image.width, image.height],
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
