use crate::checkpoint;
use anyhow::{ensure, Context, Result};
use astro_io::validation::{validate_file, ValidationLevel, ValidationOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const CHUNK: usize = 65536;
const MANIFEST_LIMIT: u64 = 1024 * 1024;
mod fits_gzip;
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Container/codec workloads without codec-internal parallelism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Ordinary, uncompressed FITS.
    Fits,
    /// Tiled FITS with GZIP_1 compression.
    FitsGzip,
    /// Tiled FITS with GZIP_2 byte shuffling and compression.
    FitsGzip2,
    /// Uncompressed monolithic XISF.
    Xisf,
    /// XISF with one zlib-compressed attachment.
    Zlib,
    /// XISF with one Zstandard-compressed attachment.
    Zstd,
}
impl Encoding {
    pub(crate) fn is_fits(self) -> bool {
        matches!(self, Self::Fits | Self::FitsGzip | Self::FitsGzip2)
    }
    fn is_tiled(self) -> bool {
        matches!(self, Self::FitsGzip | Self::FitsGzip2)
    }
    fn generator_version(self) -> u32 {
        if self.is_tiled() {
            2
        } else {
            1
        }
    }
}
/// Deterministic pixel distribution; synthetic data is not a camera compatibility test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Seeded xorshift noise, deliberately difficult to compress.
    Noise,
    /// Smooth horizontal gradient, deliberately easy to compress.
    Gradient,
}
/// Recipe for UInt16 monochrome frames. All bounds are checked before use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Pixel width, greater than zero.
    pub width: u32,
    /// Pixel height, greater than zero.
    pub height: u32,
    /// Number of files, 1..=256.
    pub frames: usize,
    /// Container and optional compression.
    pub encoding: Encoding,
    /// Full-width FITS tile height; None means the whole image. Only valid for
    /// FitsGzip/FitsGzip2, with 1..=height rows and at most 16,384 tiles/image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_rows: Option<u32>,
    /// Pixel distribution.
    pub pattern: Pattern,
    /// Reproducible noise seed (all u64 values accepted).
    pub seed: u64,
    /// Maximum scratch image bytes plus a reserved 1 MiB manifest allowance.
    pub max_disk_bytes: u64,
}
impl Default for Recipe {
    fn default() -> Self {
        Self {
            width: 2048,
            height: 2048,
            frames: 4,
            encoding: Encoding::Fits,
            tile_rows: None,
            pattern: Pattern::Noise,
            seed: 42,
            max_disk_bytes: 512 * 1024 * 1024,
        }
    }
}
impl Recipe {
    /// Check frame/disk bounds and return decoded bytes per image.
    ///
    /// This initial harness caps each image at 64 MiB and total scratch at 8 GiB.
    /// Conservative disk preflight allows codec expansion and metadata overhead.
    pub fn validate(&self) -> Result<u64> {
        ensure!(
            self.width > 0 && self.height > 0,
            "dimensions must be positive"
        );
        ensure!(
            (1..=256).contains(&self.frames),
            "frames must be in 1..=256"
        );
        let decoded = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|n| n.checked_mul(2))
            .context("image size overflow")?;
        ensure!(
            decoded <= 64 * 1024 * 1024,
            "initial fixtures are limited to 64 MiB per image"
        );
        ensure!(
            self.max_disk_bytes <= 8 * 1024 * 1024 * 1024,
            "scratch quota exceeds 8 GiB"
        );
        ensure!(
            self.tile_rows.is_none() || self.encoding.is_tiled(),
            "tile_rows requires FITS GZIP encoding"
        );
        let tiles = if self.encoding.is_tiled() {
            let rows = self.tile_rows.unwrap_or(self.height);
            ensure!(
                rows > 0 && rows <= self.height,
                "tile_rows must be in 1..=height"
            );
            let tiles = u64::from(self.height.div_ceil(rows));
            ensure!(
                tiles <= 16_384,
                "at most 16384 tiles per image are supported"
            );
            tiles
        } else {
            0
        };
        // Per tile: descriptor, GZIP framing and conservative small-block expansion.
        let maximum =
            (decoded + decoded / 100 + 16384 + tiles * 128) * self.frames as u64 + MANIFEST_LIMIT;
        ensure!(
            maximum <= self.max_disk_bytes,
            "recipe exceeds scratch quota (requires up to {maximum} bytes)"
        );
        Ok(decoded)
    }
}
/// Fingerprint and byte accounting for one generated file; names are plain basenames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    /// Relative generated filename.
    pub name: String,
    /// Physical container length.
    pub stored_bytes: u64,
    /// Declared UInt16 image bytes.
    pub decoded_bytes: u64,
    /// SHA-256 of the complete physical file.
    pub sha256: String,
}
/// Portable description of generated inputs. Format and generator versions are explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Manifest format version, currently 1.
    pub schema_version: u32,
    /// Pixel/container recipe version: 1 for original encodings, 2 for tiled FITS.
    pub generator_version: u32,
    /// Original recipe, including generation quota.
    pub recipe: Recipe,
    /// Ordered frame identities and expected lengths.
    pub files: Vec<FileRecord>,
}

/// A generated scratch set. Only the creating instance owns cleanup.
///
/// Drop performs best-effort cleanup; use [`Self::cleanup`] to report cleanup errors.
/// Abrupt process termination can leave the clearly named directory behind.
pub struct FixtureSet {
    directory: PathBuf,
    manifest: Manifest,
    owned: bool,
}
impl FixtureSet {
    /// Generate, sync, fully validate and fingerprint frames outside measurement.
    /// Returns errors for invalid bounds, cancellation, codec/validation or filesystem failure.
    /// The parent must already exist. Failed preparation removes only its own scratch set.
    pub fn generate(parent: &Path, recipe: Recipe, cancel: &AtomicBool) -> Result<Self> {
        let decoded = recipe.validate()?;
        checkpoint(cancel)?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let directory = parent.join(format!(
            "astro-bench-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).context("create benchmark scratch directory")?;
        let mut set = Self {
            directory,
            owned: true,
            manifest: Manifest {
                schema_version: 1,
                generator_version: recipe.encoding.generator_version(),
                recipe,
                files: Vec::new(),
            },
        };
        let mut remaining = set.manifest.recipe.max_disk_bytes - MANIFEST_LIMIT;
        for index in 0..set.manifest.recipe.frames {
            checkpoint(cancel)?;
            let name = filename(index, set.manifest.recipe.encoding);
            let path = set.directory.join(&name);
            generate_file(&path, &set.manifest.recipe, index, remaining, cancel)?;
            let stored_bytes = fs::metadata(&path)?.len();
            remaining = remaining
                .checked_sub(stored_bytes)
                .context("scratch quota exceeded")?;
            let report = validate_file(
                &path,
                &ValidationOptions::default().with_level(ValidationLevel::Full),
                Some(cancel),
            )?;
            ensure!(
                report.image_count() == 1 && report.checksums().unchecked() == 0,
                "fixture did not pass full validation"
            );
            set.manifest.files.push(FileRecord {
                name,
                stored_bytes,
                decoded_bytes: decoded,
                sha256: hash_file(&path, 0, cancel)?,
            });
        }
        let bytes = serde_json::to_vec_pretty(&set.manifest)?;
        ensure!(
            bytes.len() as u64 <= MANIFEST_LIMIT,
            "manifest exceeds reserved quota"
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(set.directory.join("manifest.json"))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(set)
    }
    /// Open and verify a generated set without taking cleanup ownership.
    /// Rejects unsupported versions, invalid recipes, traversal, symlinks, changed lengths
    /// and content fingerprints. Verification is outside sample timing and warms reads.
    /// Intended for owned benchmark scratch, not hostile directories modified concurrently.
    pub fn open(directory: &Path, cancel: &AtomicBool) -> Result<Self> {
        checkpoint(cancel)?;
        ensure!(
            fs::symlink_metadata(directory)?.file_type().is_dir(),
            "scratch must be a real directory"
        );
        let path = directory.join("manifest.json");
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() <= MANIFEST_LIMIT,
            "invalid manifest file"
        );
        let mut bytes = Vec::new();
        File::open(&path)?
            .take(MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= MANIFEST_LIMIT, "manifest too large");
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        ensure!(
            manifest.schema_version == 1
                && manifest.generator_version == manifest.recipe.encoding.generator_version(),
            "unsupported fixture version"
        );
        let decoded = manifest.recipe.validate()?;
        ensure!(
            manifest.files.len() == manifest.recipe.frames,
            "frame count mismatch"
        );
        let mut total = MANIFEST_LIMIT;
        for (index, entry) in manifest.files.iter().enumerate() {
            checkpoint(cancel)?;
            ensure!(
                entry.name == filename(index, manifest.recipe.encoding),
                "invalid generated filename"
            );
            ensure!(entry.decoded_bytes == decoded, "decoded length mismatch");
            total = total
                .checked_add(entry.stored_bytes)
                .context("stored size overflow")?;
            ensure!(
                total <= manifest.recipe.max_disk_bytes,
                "manifest exceeds disk quota"
            );
            let path = directory.join(&entry.name);
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                meta.file_type().is_file() && meta.len() == entry.stored_bytes,
                "fixture changed or is not a regular file"
            );
            ensure!(
                hash_file(&path, 0, cancel)? == entry.sha256,
                "fixture fingerprint mismatch: {}",
                entry.name
            );
        }
        Ok(Self {
            directory: directory.to_owned(),
            manifest,
            owned: false,
        })
    }
    /// Scratch directory, owned only when returned by `generate`.
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Validated recipe and input fingerprints.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    /// Remove this instance's owned scratch set. Non-owning instances do nothing.
    pub fn cleanup(mut self) -> Result<()> {
        if self.owned {
            fs::remove_dir_all(&self.directory).context("remove benchmark scratch directory")?;
            self.owned = false;
        }
        Ok(())
    }
}
impl Drop for FixtureSet {
    fn drop(&mut self) {
        if self.owned {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}
fn filename(index: usize, encoding: Encoding) -> String {
    format!(
        "frame-{index:04}.{}",
        if encoding.is_fits() { "fits" } else { "xisf" }
    )
}
fn hash_file(path: &Path, offset: u64, cancel: &AtomicBool) -> Result<String> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut hash = Sha256::new();
    let mut bytes = [0; CHUNK];
    loop {
        checkpoint(cancel)?;
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

// Bound every codec write, even when a future codec expands beyond preflight estimates.
struct LimitedWriter {
    file: File,
    remaining: u64,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other("scratch quota exceeded"));
        }
        let n = self.file.write(bytes)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
#[derive(Clone)]
struct Pixels<'a> {
    recipe: &'a Recipe,
    state: u64,
    offset: u64,
}
impl<'a> Pixels<'a> {
    fn new(recipe: &'a Recipe, frame: usize) -> Self {
        let state = recipe
            .seed
            .wrapping_add((frame as u64).wrapping_mul(0x9e3779b97f4a7c15))
            ^ 0xd1b54a32d192ed03;
        Self {
            recipe,
            state: if state == 0 { 1 } else { state },
            offset: 0,
        }
    }
    fn next(&mut self) -> [u8; 2] {
        let value = match self.recipe.pattern {
            Pattern::Noise => {
                self.state ^= self.state << 13;
                self.state ^= self.state >> 7;
                self.state ^= self.state << 17;
                self.state as u16
            }
            Pattern::Gradient => {
                ((self.offset % u64::from(self.recipe.width)) * 65535
                    / u64::from(self.recipe.width.max(2) - 1)) as u16
            }
        };
        self.offset += 1;
        if self.recipe.encoding.is_fits() {
            (value ^ 0x8000).to_be_bytes()
        } else {
            value.to_le_bytes()
        }
    }
}

// plane selects one byte of each sample for GZIP_2; None preserves interleaving.
fn pixel_chunk_stream(
    writer: &mut impl Write,
    pixels: &mut Pixels<'_>,
    mut count: u64,
    plane: Option<usize>,
    cancel: &AtomicBool,
) -> Result<()> {
    let sample_bytes = if plane.is_some() { 1 } else { 2 };
    let mut buffer = [0; CHUNK];
    while count > 0 {
        checkpoint(cancel)?;
        let n = count.min((CHUNK / sample_bytes) as u64) as usize;
        for i in 0..n {
            let value = pixels.next();
            if let Some(plane) = plane {
                buffer[i] = value[plane];
            } else {
                buffer[2 * i..2 * i + 2].copy_from_slice(&value);
            }
        }
        writer.write_all(&buffer[..n * sample_bytes])?;
        count -= n as u64;
    }
    Ok(())
}
fn pixels(
    writer: &mut impl Write,
    recipe: &Recipe,
    frame: usize,
    cancel: &AtomicBool,
) -> Result<()> {
    pixel_chunk_stream(
        writer,
        &mut Pixels::new(recipe, frame),
        u64::from(recipe.width) * u64::from(recipe.height),
        None,
        cancel,
    )
}
fn generate_file(
    path: &Path,
    recipe: &Recipe,
    frame: usize,
    quota: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut writer = LimitedWriter {
        file,
        remaining: quota,
    };
    let decoded = u64::from(recipe.width) * u64::from(recipe.height) * 2;
    if recipe.encoding.is_tiled() {
        return fits_gzip::generate(writer, recipe, frame, cancel);
    }
    if recipe.encoding == Encoding::Fits {
        let cards = [
            "SIMPLE  =                    T".to_owned(),
            "BITPIX  =                   16".to_owned(),
            "NAXIS   =                    2".to_owned(),
            format!("NAXIS1  = {:>20}", recipe.width),
            format!("NAXIS2  = {:>20}", recipe.height),
            "BZERO   =                32768".to_owned(),
            "BSCALE  =                    1".to_owned(),
            "END".to_owned(),
        ];
        let mut header = [b' '; 2880];
        for (i, card) in cards.iter().enumerate() {
            header[i * 80..i * 80 + card.len()].copy_from_slice(card.as_bytes());
        }
        writer.write_all(&header)?;
        pixels(&mut writer, recipe, frame, cancel)?;
        let padding = ((2880 - decoded % 2880) % 2880) as usize;
        writer.write_all(&[0; 2880][..padding])?;
    } else {
        writer.write_all(&[0; 4096])?;
        writer = match recipe.encoding {
            Encoding::Zlib => {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(writer, flate2::Compression::default());
                pixels(&mut encoder, recipe, frame, cancel)?;
                encoder.finish()?
            }
            Encoding::Zstd => {
                let mut encoder = zstd::stream::write::Encoder::new(writer, 3)?;
                pixels(&mut encoder, recipe, frame, cancel)?;
                encoder.finish()?
            }
            _ => {
                pixels(&mut writer, recipe, frame, cancel)?;
                writer
            }
        };
        writer.flush()?;
        let size = writer.file.stream_position()? - 4096;
        let digest = hash_file(path, 4096, cancel)?;
        let compression = match recipe.encoding {
            Encoding::Zlib => format!(" compression=\"zlib:{decoded}\""),
            Encoding::Zstd => format!(" compression=\"zstd:{decoded}\""),
            _ => String::new(),
        };
        let xml = format!(
            r#"<?xml version="1.0"?><xisf version="1.0" xmlns="http://www.pixinsight.com/xisf"><Image geometry="{}:{}:1" sampleFormat="UInt16" colorSpace="Gray" byteOrder="little" location="attachment:4096:{size}" checksum="sha256:{digest}"{compression}/></xisf>"#,
            recipe.width, recipe.height
        );
        ensure!(xml.len() <= 4080, "generated XISF header too large");
        writer.file.seek(SeekFrom::Start(0))?;
        // Replaces already-accounted placeholder bytes, never extends the file.
        writer.file.write_all(b"XISF0100")?;
        writer.file.write_all(&(xml.len() as u32).to_le_bytes())?;
        writer.file.write_all(&[0; 4])?;
        writer.file.write_all(xml.as_bytes())?;
    }
    writer.file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_writes_cannot_exceed_remaining_scratch_quota() {
        let cancel = AtomicBool::new(false);
        let owner = FixtureSet::generate(
            &std::env::temp_dir(),
            Recipe {
                width: 8,
                height: 8,
                frames: 1,
                ..Recipe::default()
            },
            &cancel,
        )
        .unwrap();
        for encoding in [Encoding::FitsGzip, Encoding::FitsGzip2] {
            let recipe = Recipe {
                width: 8,
                height: 8,
                frames: 1,
                encoding,
                tile_rows: Some(1),
                ..Recipe::default()
            };
            let path = owner.directory().join(format!("quota-{encoding:?}.fits"));
            let quota = 5760 + 8 * 16 + 12;
            assert!(generate_file(&path, &recipe, 0, quota, &cancel).is_err());
            assert!(fs::metadata(&path).unwrap().len() <= quota);
        }
        let directory = owner.directory().to_owned();
        owner.cleanup().unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn pixel_stream_stops_between_chunks_when_cancelled() {
        struct CancelWriter<'a> {
            cancel: &'a AtomicBool,
            count: usize,
        }
        impl Write for CancelWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.count += bytes.len();
                self.cancel.store(true, Ordering::Relaxed);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let cancel = AtomicBool::new(false);
        let mut writer = CancelWriter {
            cancel: &cancel,
            count: 0,
        };
        assert!(pixels(&mut writer, &Recipe::default(), 0, &cancel).is_err());
        assert_eq!(writer.count, CHUNK);
    }
}
