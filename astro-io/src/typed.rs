//! Raw container inventory and exact typed access for transformation clients.
//!
//! Inventory records format facts and source associations without deciding
//! whether a cross-format metadata mapping is faithful. Callers must account
//! for every listed image and ancillary structure before writing a new file.

pub mod capability;
pub mod compare;
pub mod metadata;
pub mod write;

use crate::fits::{self, FitsHeaderCard};
use crate::xisf::codec::Compression;
use crate::xisf::structural::{
    checked_range, visit_xml, BlockLocation, ImageDescriptor, VisitError, XmlEvent,
};
use anyhow::{anyhow, bail, Context, Result};
use fitsio::{hdu::HduInfo, sys::ffghadll, FitsFile};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Hard limits applied before an inventory allocates source-controlled records.
#[derive(Debug, Clone, Copy)]
pub struct InventoryLimits {
    pub max_xml_bytes: u64,
    pub max_hdus: usize,
    pub max_header_cards: usize,
    pub max_elements: usize,
    pub max_images: usize,
}

impl Default for InventoryLimits {
    fn default() -> Self {
        Self {
            max_xml_bytes: 64 << 20,
            max_hdus: 10_000,
            max_header_cards: 1_000_000,
            max_elements: 1_000_000,
            max_images: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerFormat {
    Fits,
    Xisf,
}

/// A checked physical byte range in the source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteExtent {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncillaryReference {
    FitsHdu(usize),
    XisfElement(usize),
}

/// One logical image, in source order. `source_index` is an HDU or XML element
/// index, depending on `format`; it remains stable within this inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRecord {
    pub source_index: usize,
    pub id: Option<String>,
    pub axes: Vec<u64>,
    pub sample_format: String,
    pub physical_format: String,
    pub compression: Option<String>,
    pub attachment: Option<ByteExtent>,
    pub source_attributes: BTreeMap<String, String>,
}

/// All physical FITS HDUs, including empty primary HDUs and non-image tables.
/// Raw cards retain source ordering and duplicate keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitsHduRecord {
    pub index: usize,
    pub kind: String,
    pub header: ByteExtent,
    pub data: ByteExtent,
    pub cards: Vec<FitsHeaderCard>,
    pub image_index: Option<usize>,
}

/// One core XISF XML element, including properties and ancillary data nodes.
/// `parent_image_index` identifies the containing image when there is one.
/// The original XML is retained separately for lexical facts and comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XisfElementRecord {
    pub index: usize,
    pub parent_index: Option<usize>,
    pub parent_image_index: Option<usize>,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub text: String,
    pub attachment: Option<ByteExtent>,
}

#[derive(Debug, Clone)]
pub struct ContainerInventory {
    pub format: ContainerFormat,
    pub images: Vec<ImageRecord>,
    pub fits_hdus: Vec<FitsHduRecord>,
    pub xisf_elements: Vec<XisfElementRecord>,
    /// Exact source XML, including unknown core elements, comments and order.
    pub xisf_xml: Option<String>,
    source_stamp: SourceObservation,
}

/// A cheap binding between this inventory and one local file observation.
/// Application transactions still require stronger source checks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceObservation {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl SourceObservation {
    fn capture(path: &Path) -> Result<Self> {
        let metadata = path.metadata()?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino()),
        })
    }
}

impl ContainerInventory {
    fn ensure_source(&self, path: &Path) -> Result<()> {
        if SourceObservation::capture(path)? != self.source_stamp {
            bail!("Source changed since container inventory was built");
        }
        Ok(())
    }
}

/// Per-call limit for exact pixel access. The caller controls aggregate
/// admission when several readers operate concurrently.
#[derive(Debug, Clone, Copy)]
pub struct PixelReadLimits {
    pub max_chunk_samples: usize,
    /// Each XISF subblock is bounded; total image size is limited by available
    /// scratch storage, the format, and the caller's resource admission.
    pub max_decoded_subblock_bytes: u64,
    pub max_stored_subblock_bytes: u64,
}

impl Default for PixelReadLimits {
    fn default() -> Self {
        Self {
            max_chunk_samples: 65_536,
            max_decoded_subblock_bytes: 16 << 20,
            max_stored_subblock_bytes: 32 << 20,
        }
    }
}

/// Owned decoded XISF spool. The file is removed on drop, including after a
/// failed decode. The caller must reserve its declared decoded image size on
/// the selected scratch filesystem before opening a large reader.
struct DecodedSpool {
    path: PathBuf,
    file: File,
    shuffled: bool,
    samples: u64,
}

impl DecodedSpool {
    fn create(directory: &Path) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..64 {
            let path = directory.join(format!(
                ".astro-io-decode-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file,
                        shuffled: false,
                        samples: 0,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("Could not create XISF decode scratch"),
            }
        }
        bail!("Could not allocate a unique XISF decode scratch path")
    }
}

impl Drop for DecodedSpool {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Reusable exact reader. Compressed XISF is decoded into an owned spool with
/// bounded per-subblock memory; byte-shuffled images remain in byte-plane order.
pub struct U16ChunkReader<'a> {
    path: &'a Path,
    inventory: &'a ContainerInventory,
    image_index: usize,
    limits: PixelReadLimits,
    decoded_xisf: Option<DecodedSpool>,
    big_endian: bool,
}

impl<'a> U16ChunkReader<'a> {
    pub fn open(
        path: &'a Path,
        inventory: &'a ContainerInventory,
        image_index: usize,
        limits: PixelReadLimits,
    ) -> Result<Self> {
        Self::open_in(path, inventory, image_index, limits, &std::env::temp_dir())
    }

    /// Select the scratch filesystem explicitly so an application can reserve
    /// and monitor the decoded image's full byte count there.
    pub fn open_in(
        path: &'a Path,
        inventory: &'a ContainerInventory,
        image_index: usize,
        limits: PixelReadLimits,
        scratch_directory: &Path,
    ) -> Result<Self> {
        inventory.ensure_source(path)?;
        let image = inventory
            .images
            .get(image_index)
            .context("Image index is not inventoried")?;
        let mut decoded_xisf = None;
        let mut big_endian = false;
        if inventory.format == ContainerFormat::Xisf {
            big_endian =
                image.source_attributes.get("byteOrder").map(String::as_str) == Some("big");
            if image.compression.is_some() {
                if image.sample_format != "UInt16" || image.axes.len() != 3 || image.axes[2] != 1 {
                    bail!("Unsupported compressed XISF exact UInt16 representation");
                }
                let count = image.axes.iter().try_fold(1u64, |count, axis| {
                    count
                        .checked_mul(*axis)
                        .context("Image sample count overflows")
                })?;
                let expected = count.checked_mul(2).context("XISF byte count overflows")?;
                let attachment = image
                    .attachment
                    .context("XISF image is not attachment-backed")?;
                let compression = Compression::parse(
                    image.compression.as_deref(),
                    image.source_attributes.get("subblocks").map(String::as_str),
                    attachment.length,
                )?
                .context("Missing XISF compression descriptor")?;
                if compression.decoded() != expected {
                    bail!("XISF compression length does not match image geometry");
                }
                if compression.shuffle().is_some_and(|size| size != 2) {
                    bail!("XISF UInt16 shuffle item size must be two bytes");
                }
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(attachment.offset))?;
                let mut spool = DecodedSpool::create(scratch_directory)?;
                crate::xisf::decode_compressed_image_into(
                    &mut file,
                    &compression,
                    expected,
                    limits.max_stored_subblock_bytes,
                    limits.max_decoded_subblock_bytes,
                    &mut spool.file,
                )?;
                spool.file.flush()?;
                spool.shuffled = compression.shuffle().is_some();
                spool.samples = count;
                decoded_xisf = Some(spool);
            }
        }
        Ok(Self {
            path,
            inventory,
            image_index,
            limits,
            decoded_xisf,
            big_endian,
        })
    }

    pub fn read_chunk(&mut self, start: u64, output: &mut [u16]) -> Result<()> {
        let Some(decoded) = &mut self.decoded_xisf else {
            return read_u16_chunk(
                self.path,
                self.inventory,
                self.image_index,
                start,
                output,
                self.limits,
            );
        };
        self.inventory.ensure_source(self.path)?;
        if self.limits.max_chunk_samples == 0 || output.len() > self.limits.max_chunk_samples {
            bail!("UInt16 chunk exceeds configured sample limit");
        }
        let byte_start =
            usize::try_from(start.checked_mul(2).context("XISF byte offset overflows")?)?;
        let byte_count = output
            .len()
            .checked_mul(2)
            .context("XISF chunk byte count overflows")?;
        let byte_end = byte_start
            .checked_add(byte_count)
            .context("XISF chunk byte range overflows")?;
        let image_bytes = decoded
            .samples
            .checked_mul(2)
            .context("XISF size overflows")?;
        if byte_end as u64 > image_bytes {
            bail!("UInt16 chunk exceeds decoded image");
        }
        let mut bytes = vec![0u8; byte_count];
        if decoded.shuffled {
            let sample_start = start;
            let sample_count = output.len();
            let mut high = vec![0u8; sample_count];
            decoded.file.seek(SeekFrom::Start(sample_start))?;
            decoded.file.read_exact(&mut bytes[..sample_count])?;
            decoded
                .file
                .seek(SeekFrom::Start(decoded.samples + sample_start))?;
            decoded.file.read_exact(&mut high)?;
            for (index, sample) in output.iter_mut().enumerate() {
                let pair = [bytes[index], high[index]];
                *sample = if self.big_endian {
                    u16::from_be_bytes(pair)
                } else {
                    u16::from_le_bytes(pair)
                };
            }
        } else {
            decoded.file.seek(SeekFrom::Start(byte_start as u64))?;
            decoded.file.read_exact(&mut bytes)?;
            for (sample, pair) in output.iter_mut().zip(bytes.as_chunks::<2>().0) {
                *sample = if self.big_endian {
                    u16::from_be_bytes(*pair)
                } else {
                    u16::from_le_bytes(*pair)
                };
            }
        }
        Ok(())
    }
}

/// Read a bounded range of exact logical UInt16 samples into caller storage.
/// The source inventory must describe the same unchanged file. Unsupported
/// representation, geometry, or compression returns an error instead of
/// converting samples. FITS uses CFITSIO's exact UInt16 scaling convention.
pub fn read_u16_chunk(
    path: &Path,
    inventory: &ContainerInventory,
    image_index: usize,
    start: u64,
    output: &mut [u16],
    limits: PixelReadLimits,
) -> Result<()> {
    inventory.ensure_source(path)?;
    if limits.max_chunk_samples == 0 || output.len() > limits.max_chunk_samples {
        bail!("UInt16 chunk exceeds configured sample limit");
    }
    let image = inventory
        .images
        .get(image_index)
        .context("Image index is not inventoried")?;
    if image.sample_format != "UInt16" {
        bail!(
            "Unsupported exact UInt16 source representation: {}",
            image.sample_format
        );
    }
    let count = image.axes.iter().try_fold(1u64, |count, axis| {
        count
            .checked_mul(*axis)
            .context("Image sample count overflows")
    })?;
    let end = start
        .checked_add(output.len() as u64)
        .context("UInt16 chunk range overflows")?;
    if end > count {
        bail!("UInt16 chunk exceeds image sample extent");
    }
    match inventory.format {
        ContainerFormat::Fits => {
            if image.axes.len() != 2 {
                bail!("Unsupported FITS UInt16 geometry");
            }
            fits::backend::with_cfitsio(|| {
                let mut file = FitsFile::open(path)?;
                let hdu = file.hdu(image.source_index)?;
                let begin =
                    usize::try_from(start).context("FITS sample index exceeds address space")?;
                let finish =
                    usize::try_from(end).context("FITS sample index exceeds address space")?;
                let pixels: Vec<u16> = hdu.read_section(&mut file, begin, finish)?;
                if pixels.len() != output.len() {
                    bail!("FITS UInt16 short read");
                }
                output.copy_from_slice(&pixels);
                Ok(())
            })
        }
        ContainerFormat::Xisf => {
            if image.axes.len() != 3 || image.axes[2] != 1 {
                bail!("Unsupported XISF UInt16 geometry");
            }
            let attachment = image
                .attachment
                .context("XISF image is not attachment-backed")?;
            let expected = count.checked_mul(2).context("XISF byte count overflows")?;
            let order = image
                .source_attributes
                .get("byteOrder")
                .map(String::as_str)
                .unwrap_or("little");
            if !matches!(order, "little" | "big") {
                bail!("Unsupported XISF byte order {order}");
            }
            let mut file = File::open(path)?;
            if let Some(descriptor) = &image.compression {
                let _ = descriptor;
                return U16ChunkReader::open(path, inventory, image_index, limits)?
                    .read_chunk(start, output);
            }
            if attachment.length != expected {
                bail!("XISF UInt16 attachment length mismatch");
            }
            let byte_start = start.checked_mul(2).context("XISF byte offset overflows")?;
            let offset = attachment
                .offset
                .checked_add(byte_start)
                .context("XISF byte offset overflows")?;
            file.seek(SeekFrom::Start(offset))?;
            let mut buffer = [0u8; 65_536];
            for chunk in output.chunks_mut(buffer.len() / 2) {
                let bytes = &mut buffer[..chunk.len() * 2];
                file.read_exact(bytes)?;
                for (sample, pair) in chunk.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *sample = if order == "little" {
                        u16::from_le_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_be_bytes([pair[0], pair[1]])
                    };
                }
            }
            Ok(())
        }
    }
}

/// Read a bounded physical slice of an inventoried non-image structure.
/// FITS HDU data include table/heap bytes and padding as reported by CFITSIO;
/// XISF references are attachment bytes as stored, before decompression.
pub fn read_ancillary_chunk(
    path: &Path,
    inventory: &ContainerInventory,
    reference: AncillaryReference,
    start: u64,
    output: &mut [u8],
    max_chunk_bytes: usize,
) -> Result<()> {
    inventory.ensure_source(path)?;
    if max_chunk_bytes == 0 || output.len() > max_chunk_bytes {
        bail!("Ancillary read exceeds configured byte limit");
    }
    let extent = match (inventory.format, reference) {
        (ContainerFormat::Fits, AncillaryReference::FitsHdu(index)) => {
            let hdu = inventory
                .fits_hdus
                .get(index)
                .context("FITS HDU is not inventoried")?;
            if hdu.image_index.is_some() {
                bail!("FITS HDU is an image, not ancillary content");
            }
            hdu.data
        }
        (ContainerFormat::Xisf, AncillaryReference::XisfElement(index)) => {
            let element = inventory
                .xisf_elements
                .get(index)
                .context("XISF element is not inventoried")?;
            // A Thumbnail is display ancillary content, not one of the
            // mandatory science Image arrays. Its bytes still need bounded
            // access so metadata policy can identify and verify its loss.
            if element.name == "Image" {
                bail!("XISF element is an image, not ancillary content");
            }
            element
                .attachment
                .context("XISF element has no attached binary content")?
        }
        _ => bail!("Ancillary reference does not match source format"),
    };
    let end = start
        .checked_add(output.len() as u64)
        .context("Ancillary range overflows")?;
    if end > extent.length {
        bail!("Ancillary range exceeds inventoried extent");
    }
    let absolute = extent
        .offset
        .checked_add(start)
        .context("Ancillary offset overflows")?;
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(absolute))?;
    file.read_exact(output)?;
    Ok(())
}

/// Read a bounded raw inventory. This does not decode pixels or certify a
/// semantic conversion; full validation remains a separate operation.
pub fn inventory(path: &Path, limits: InventoryLimits) -> Result<ContainerInventory> {
    let source_stamp = SourceObservation::capture(path)?;
    let mut file =
        File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)
        .context("Failed to read container signature")?;
    let found = match &magic {
        b"XISF0100" => inventory_xisf(&mut file, limits, source_stamp.clone()),
        b"SIMPLE  " => {
            fits::backend::with_cfitsio(|| inventory_fits(path, limits, source_stamp.clone()))
        }
        _ => bail!("Unsupported container signature"),
    }?;
    found.ensure_source(path)?;
    Ok(found)
}

fn inventory_fits(
    path: &Path,
    limits: InventoryLimits,
    source_stamp: SourceObservation,
) -> Result<ContainerInventory> {
    let source_len = source_stamp.len;
    let mut file = FitsFile::open(path)?;
    let count = fits::read_num_hdus(&mut file)?;
    if count > limits.max_hdus {
        bail!(
            "FITS HDU count {count} exceeds inventory limit {}",
            limits.max_hdus
        );
    }
    let mut images = Vec::new();
    let mut fits_hdus = Vec::new();
    let mut remaining_cards = limits.max_header_cards;
    for index in 0..count {
        let hdu = file.hdu(index)?;
        let cards = fits::read_header_cards_limited(&mut file, index, remaining_cards)?;
        remaining_cards -= cards.len();
        let raw = unsafe { file.as_raw() };
        let (mut header_start, mut data_start, mut data_end, mut status) = (0i64, 0i64, 0i64, 0);
        // SAFETY: the live CFITSIO handle is current at this HDU and its whole
        // lifetime is enclosed by the coordinated backend gate.
        unsafe {
            ffghadll(
                raw,
                &mut header_start,
                &mut data_start,
                &mut data_end,
                &mut status,
            )
        };
        fitsio::errors::check_status(status)?;
        let (header_start, data_start, data_end) = (
            u64::try_from(header_start).context("Negative FITS header offset")?,
            u64::try_from(data_start).context("Negative FITS data offset")?,
            u64::try_from(data_end).context("Negative FITS data end")?,
        );
        if header_start > data_start || data_start > data_end || data_end > source_len {
            bail!("Invalid FITS HDU {index} byte extent");
        }
        let compressed = card_value(&cards, "ZIMAGE") == Some("T");
        let kind = if compressed {
            "compressed-image"
        } else {
            match &hdu.info {
                HduInfo::ImageInfo { .. } => "image",
                HduInfo::TableInfo { .. } => "table",
                HduInfo::AnyInfo => "other",
            }
        };
        let mut image_index = None;
        if let HduInfo::ImageInfo { shape, image_type } = &hdu.info {
            if !shape.is_empty() {
                if images.len() >= limits.max_images {
                    bail!(
                        "FITS image count exceeds inventory limit {}",
                        limits.max_images
                    );
                }
                let axis_prefix = if compressed { "ZNAXIS" } else { "NAXIS" };
                let axes = (1..=shape.len())
                    .map(|axis| {
                        let key = format!("{axis_prefix}{axis}");
                        card_value(&cards, &key)
                            .ok_or_else(|| anyhow!("FITS HDU {index} missing {key}"))?
                            .parse::<u64>()
                            .with_context(|| format!("Invalid FITS HDU {index} {key}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                axes.iter().try_fold(1u64, |count, axis| {
                    count
                        .checked_mul(*axis)
                        .context("FITS image sample count overflows")
                })?;
                let bitpix_key = if compressed { "ZBITPIX" } else { "BITPIX" };
                let bitpix = card_value(&cards, bitpix_key).unwrap_or("unknown");
                let bzero_key = if compressed { "ZBZERO" } else { "BZERO" };
                let bscale_key = if compressed { "ZBSCALE" } else { "BSCALE" };
                // CFITSIO's image compressor can retain the ordinary BZERO
                // and BSCALE cards on its ZIMAGE table instead of emitting
                // their Z-prefixed forms; both describe logical samples.
                let bzero = card_value(&cards, bzero_key)
                    .or_else(|| card_value(&cards, "BZERO"))
                    .unwrap_or("0");
                let bscale = card_value(&cards, bscale_key)
                    .or_else(|| card_value(&cards, "BSCALE"))
                    .unwrap_or("1");
                let blank = card_value(&cards, if compressed { "ZBLANK" } else { "BLANK" })
                    .or_else(|| card_value(&cards, "BLANK"));
                let sample_format =
                    if bitpix == "16" && bscale == "1" && bzero == "32768" && blank.is_none() {
                        "UInt16".to_owned()
                    } else {
                        format!("BITPIX={bitpix},BSCALE={bscale},BZERO={bzero},BLANK={blank:?}")
                    };
                let mut source_attributes = BTreeMap::new();
                source_attributes.insert("BITPIX".into(), bitpix.into());
                source_attributes.insert("BSCALE".into(), bscale.into());
                source_attributes.insert("BZERO".into(), bzero.into());
                if let Some(blank) = blank {
                    source_attributes.insert("BLANK".into(), blank.into());
                }
                source_attributes.insert("CFITSIO_IMAGE_TYPE".into(), format!("{image_type:?}"));
                let id = card_value(&cards, "EXTNAME").map(str::to_owned);
                image_index = Some(images.len());
                images.push(ImageRecord {
                    source_index: index,
                    id,
                    axes,
                    sample_format,
                    physical_format: if compressed { "BINTABLE" } else { "IMAGE" }.into(),
                    compression: card_value(&cards, "ZCMPTYPE").map(str::to_owned),
                    attachment: Some(ByteExtent {
                        offset: data_start,
                        length: data_end - data_start,
                    }),
                    source_attributes,
                });
            }
        }
        fits_hdus.push(FitsHduRecord {
            index,
            kind: kind.into(),
            header: ByteExtent {
                offset: header_start,
                length: data_start - header_start,
            },
            data: ByteExtent {
                offset: data_start,
                length: data_end - data_start,
            },
            cards,
            image_index,
        });
    }
    Ok(ContainerInventory {
        format: ContainerFormat::Fits,
        images,
        fits_hdus,
        xisf_elements: Vec::new(),
        xisf_xml: None,
        source_stamp,
    })
}

fn card_value<'a>(cards: &'a [FitsHeaderCard], key: &str) -> Option<&'a str> {
    cards
        .iter()
        .rev()
        .find(|card| card.keyword == key)
        .and_then(|card| card.value.as_deref())
}

fn inventory_xisf(
    file: &mut File,
    limits: InventoryLimits,
    source_stamp: SourceObservation,
) -> Result<ContainerInventory> {
    let header = crate::xisf::read_monolithic_header_limited(file, limits.max_xml_bytes)?;
    let mut images = Vec::new();
    let mut elements: Vec<XisfElementRecord> = Vec::new();
    let mut stack: Vec<(usize, Option<usize>)> = Vec::new();
    let result = visit_xml(&header.xml, |event| {
        match event {
            XmlEvent::Element(element) => {
                if elements.len() >= limits.max_elements {
                    bail!(
                        "XISF XML element count exceeds inventory limit {}",
                        limits.max_elements
                    );
                }
                let parent_index = stack.last().map(|entry| entry.0);
                let inherited_image = stack.last().and_then(|entry| entry.1);
                let attachment = match element
                    .attr("location")
                    .map(BlockLocation::parse)
                    .transpose()?
                {
                    Some(BlockLocation::Attachment { offset, size }) => {
                        if offset < header.xml_end {
                            bail!("XISF attachment overlaps XML header");
                        }
                        checked_range(offset, size, header.source_len, "XISF attachment")?;
                        Some(ByteExtent {
                            offset,
                            length: size,
                        })
                    }
                    _ => None,
                };
                let index = elements.len();
                let mut parent_image_index = inherited_image;
                if element.name == "Image" {
                    if images.len() >= limits.max_images {
                        bail!(
                            "XISF image count exceeds inventory limit {}",
                            limits.max_images
                        );
                    }
                    let descriptor = ImageDescriptor::parse(&element)?;
                    descriptor
                        .geometry
                        .dimensions()
                        .iter()
                        .try_fold(1u64, |count, axis| {
                            count
                                .checked_mul(*axis)
                                .context("XISF image sample count overflows")
                        })?;
                    if matches!(
                        descriptor.sample_format.as_str(),
                        "UInt8"
                            | "UInt16"
                            | "UInt32"
                            | "UInt64"
                            | "Float32"
                            | "Float64"
                            | "Complex32"
                            | "Complex64"
                    ) {
                        descriptor.sample_layout()?;
                    }
                    let image_index = images.len();
                    images.push(ImageRecord {
                        source_index: index,
                        id: element.attr("id").map(str::to_owned),
                        axes: descriptor.geometry.dimensions().to_vec(),
                        sample_format: descriptor.sample_format.clone(),
                        physical_format: match &descriptor.location {
                            BlockLocation::Attachment { .. } => "attachment",
                            BlockLocation::Inline { .. } => "inline",
                            BlockLocation::Embedded => "embedded",
                            BlockLocation::Other(_) => "unsupported-location",
                        }
                        .into(),
                        compression: descriptor.compression.clone(),
                        attachment,
                        source_attributes: element.attrs.clone(),
                    });
                    parent_image_index = Some(image_index);
                }
                elements.push(XisfElementRecord {
                    index,
                    parent_index,
                    parent_image_index,
                    name: element.name,
                    attributes: element.attrs,
                    text: String::new(),
                    attachment,
                });
                if !element.empty {
                    stack.push((index, parent_image_index));
                }
            }
            XmlEvent::Text { value, .. } => {
                if let Some((index, _)) = stack.last() {
                    elements[*index].text.push_str(&value);
                }
            }
            XmlEvent::End { .. } => {
                stack.pop();
            }
            XmlEvent::IgnoredExtension {
                element_attributes: Some(_),
            } => {
                // The structural visitor deliberately does not expose foreign
                // extension syntax. A marker ensures semantic conversion can
                // never mistake its omitted subtree for an empty document.
                if elements.len() >= limits.max_elements {
                    bail!("XISF XML element count exceeds inventory limit");
                }
                let index = elements.len();
                elements.push(XisfElementRecord {
                    index,
                    parent_index: None,
                    parent_image_index: None,
                    name: "ForeignExtension".into(),
                    attributes: BTreeMap::new(),
                    text: String::new(),
                    attachment: None,
                });
            }
            XmlEvent::IgnoredExtension { .. } => {}
        }
        Ok::<(), anyhow::Error>(())
    });
    match result {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(error.into()),
        Err(VisitError::Consumer(error)) => return Err(error),
    }
    let xisf_xml = Some(String::from_utf8(header.xml).context("XISF XML is not UTF-8")?);
    Ok(ContainerInventory {
        format: ContainerFormat::Xisf,
        images,
        fits_hdus: Vec::new(),
        xisf_elements: elements,
        xisf_xml,
        source_stamp,
    })
}
