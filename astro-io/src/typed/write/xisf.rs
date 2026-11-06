//! Monolithic XISF writer with checked, calculated attachment offsets.

use super::{Scratch, U16ImageSpec};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const CHUNK_SAMPLES: usize = 32_768;
const ALIGNMENT: u64 = 4096;
const MAX_SUBBLOCKS_PER_IMAGE: usize = 1_000_000;
const MAX_TOTAL_SUBBLOCKS: usize = 100_000;
const MAX_SCALAR_METADATA_BYTES: usize = 4 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XisfCodec {
    Uncompressed,
    Zstd,
    Zlib,
    Lz4,
}

/// Narrow, lossless XISF property-writing primitives. Arbitrary property
/// types remain visible in inventory and must be rejected or handled by P2.
#[derive(Debug, Clone, Copy)]
pub enum XisfPropertyValue<'a> {
    String(&'a str),
    ByteArray(&'a [u8]),
    /// Copy a checked byte extent incrementally from a source file.
    ByteArrayFile {
        path: &'a Path,
        offset: u64,
        length: u64,
    },
    /// Length-only form for output-bound calculation. The writer rejects it:
    /// callers must supply the bytes when actually serializing the property.
    ByteArrayLength(u64),
}

#[derive(Debug, Clone, Copy)]
pub struct XisfPropertySpec<'a> {
    /// `None` places the property on the root; `Some(i)` associates it with
    /// image `i` in the supplied image list.
    pub image_index: Option<usize>,
    pub id: &'a str,
    pub value: XisfPropertyValue<'a>,
}

/// One literal FITS-style keyword represented by a native XISF
/// `FITSKeyword` element. The value retains FITS quoting/number spelling.
#[derive(Debug, Clone, Copy)]
pub struct XisfKeywordSpec<'a> {
    pub image_index: Option<usize>,
    pub name: &'a str,
    pub value: &'a str,
    pub comment: Option<&'a str>,
}

impl XisfCodec {
    fn label(self) -> Option<&'static str> {
        match self {
            Self::Uncompressed => None,
            Self::Zstd => Some("zstd"),
            Self::Zlib => Some("zlib"),
            Self::Lz4 => Some("lz4"),
        }
    }
}

struct Payload {
    spool_start: u64,
    stored: u64,
    decoded: u64,
    checksum: String,
    subblocks: Vec<(u64, u64)>,
}

struct ShuffleScratch {
    path: std::path::PathBuf,
    file: File,
}

impl ShuffleScratch {
    fn new(directory: &Path, name: &str) -> Result<Self> {
        let path = directory.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self { path, file })
    }
}

impl Drop for ShuffleScratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Write a complete sequence of UInt16 images without fixed attachment
/// offsets. Compressed data use independently decodable subblocks and Revision
/// 2 raw fallback. The callback fills bounded pixel chunks. Byte-plane
/// shuffling uses two owned scratch files, keeping memory independent of the
/// total image size. The output must not exist.
pub fn write_xisf_u16(
    path: &Path,
    images: &[U16ImageSpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
    read: impl FnMut(usize, u64, &mut [u16]) -> Result<()>,
) -> Result<()> {
    write_xisf_u16_with_properties(path, images, &[], codec, shuffled, read)
}

/// Variant with explicit root/image String and binary ByteArray properties.
/// Binary properties are stored as checksummed attachments, without loading
/// additional copies of their byte slices.
pub fn write_xisf_u16_with_properties(
    path: &Path,
    images: &[U16ImageSpec<'_>],
    properties: &[XisfPropertySpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
    read: impl FnMut(usize, u64, &mut [u16]) -> Result<()>,
) -> Result<()> {
    write_xisf_u16_with_metadata(path, images, properties, &[], codec, shuffled, read)
}

/// Variant that also writes ordered root/image `FITSKeyword` elements. A
/// semantic caller decides which cards are eligible; this function validates
/// only their format-level syntax and associations.
pub fn write_xisf_u16_with_metadata(
    path: &Path,
    images: &[U16ImageSpec<'_>],
    properties: &[XisfPropertySpec<'_>],
    keywords: &[XisfKeywordSpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
    mut read: impl FnMut(usize, u64, &mut [u16]) -> Result<()>,
) -> Result<()> {
    if images.is_empty() || images.len() > 10_000 {
        bail!("XISF writer requires 1 to 10000 images");
    }
    if shuffled && codec == XisfCodec::Uncompressed {
        bail!("XISF shuffle requires compression");
    }
    if path.exists() {
        bail!("XISF output already exists: {}", path.display());
    }
    let mut total_subblocks = 0usize;
    let mut image_id_bytes = 0usize;
    for image in images {
        let count = image.sample_count()?;
        let parts = count.div_ceil(CHUNK_SAMPLES);
        total_subblocks = total_subblocks
            .checked_add(parts)
            .context("XISF subblock count overflows")?;
        if parts > MAX_SUBBLOCKS_PER_IMAGE || total_subblocks > MAX_TOTAL_SUBBLOCKS {
            bail!("XISF image exceeds writer subblock count limit");
        }
        if image
            .id
            .is_some_and(|id| id.is_empty() || id.len() > 4096 || !valid_xml_text(id))
        {
            bail!("XISF image identifier contains invalid XML text");
        }
        image_id_bytes = image_id_bytes
            .checked_add(image.id.map_or(0, str::len))
            .context("XISF image ID bytes overflow")?;
        if image_id_bytes > MAX_SCALAR_METADATA_BYTES {
            bail!("XISF image IDs exceed writer byte limit");
        }
    }
    validate_properties(properties, images.len())?;
    validate_keywords(keywords, images.len())?;
    if properties
        .iter()
        .any(|property| matches!(property.value, XisfPropertyValue::ByteArrayLength(_)))
    {
        bail!("Length-only XISF property cannot be written");
    }
    let scratch = Scratch::new(path)?;
    let mut spool = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&scratch.path)?;
    let mut payloads = Vec::new();
    for (index, image) in images.iter().enumerate() {
        let count = image.sample_count()?;
        let decoded = u64::try_from(count)?
            .checked_mul(2)
            .context("XISF decoded byte count overflows")?;
        let spool_start = spool.stream_position()?;
        let mut hasher = Sha256::new();
        let mut subblocks = Vec::new();
        if shuffled {
            let mut low = ShuffleScratch::new(&scratch.directory, "low")?;
            let mut high = ShuffleScratch::new(&scratch.directory, "high")?;
            let mut pixels = [0u16; CHUNK_SAMPLES];
            let mut low_bytes = [0u8; CHUNK_SAMPLES];
            let mut high_bytes = [0u8; CHUNK_SAMPLES];
            for start in (0..count).step_by(CHUNK_SAMPLES) {
                let size = (count - start).min(CHUNK_SAMPLES);
                read(index, start as u64, &mut pixels[..size])?;
                for (offset, value) in pixels[..size].iter().enumerate() {
                    let pair = value.to_le_bytes();
                    low_bytes[offset] = pair[0];
                    high_bytes[offset] = pair[1];
                }
                low.file.write_all(&low_bytes[..size])?;
                high.file.write_all(&high_bytes[..size])?;
            }
            low.file.rewind()?;
            high.file.rewind()?;
            let mut part = [0u8; CHUNK_SAMPLES * 2];
            let mut filled = 0usize;
            for plane in [&mut low.file, &mut high.file] {
                let mut remaining = count;
                while remaining > 0 {
                    let size = remaining.min(part.len() - filled);
                    plane.read_exact(&mut part[filled..filled + size])?;
                    filled += size;
                    if filled == part.len() {
                        write_part(&mut spool, &mut hasher, &mut subblocks, codec, &part)?;
                        filled = 0;
                    }
                    remaining -= size;
                }
            }
            if filled > 0 {
                write_part(
                    &mut spool,
                    &mut hasher,
                    &mut subblocks,
                    codec,
                    &part[..filled],
                )?;
            }
        } else {
            let mut pixels = [0u16; CHUNK_SAMPLES];
            let mut bytes = Vec::with_capacity(CHUNK_SAMPLES * 2);
            for start in (0..count).step_by(CHUNK_SAMPLES) {
                let size = (count - start).min(CHUNK_SAMPLES);
                read(index, start as u64, &mut pixels[..size])?;
                bytes.clear();
                for value in &pixels[..size] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                write_part(&mut spool, &mut hasher, &mut subblocks, codec, &bytes)?;
            }
        }
        let stored = spool
            .stream_position()?
            .checked_sub(spool_start)
            .context("XISF spool offset underflows")?;
        payloads.push(Payload {
            spool_start,
            stored,
            decoded,
            checksum: format!("{:x}", hasher.finalize()),
            subblocks,
        });
    }
    let mut property_payloads = vec![None; properties.len()];
    for (index, property) in properties.iter().enumerate() {
        if matches!(
            property.value,
            XisfPropertyValue::ByteArray(_) | XisfPropertyValue::ByteArrayFile { .. }
        ) {
            let spool_start = spool.stream_position()?;
            let mut hasher = Sha256::new();
            let length = match property.value {
                XisfPropertyValue::ByteArray(bytes) => {
                    spool.write_all(bytes)?;
                    hasher.update(bytes);
                    u64::try_from(bytes.len())?
                }
                XisfPropertyValue::ByteArrayFile {
                    path,
                    offset,
                    length,
                } => {
                    let end = offset
                        .checked_add(length)
                        .context("Binary property extent overflows")?;
                    if end > fs::metadata(path)?.len() {
                        bail!("Binary property source extent is truncated");
                    }
                    let mut source = File::open(path)?;
                    source.seek(SeekFrom::Start(offset))?;
                    let mut remaining = length;
                    let mut buffer = [0u8; 65_536];
                    while remaining > 0 {
                        let size = usize::try_from(remaining.min(buffer.len() as u64))?;
                        source.read_exact(&mut buffer[..size])?;
                        spool.write_all(&buffer[..size])?;
                        hasher.update(&buffer[..size]);
                        remaining -= size as u64;
                    }
                    length
                }
                _ => unreachable!(),
            };
            property_payloads[index] = Some(payloads.len());
            payloads.push(Payload {
                spool_start,
                stored: length,
                decoded: length,
                checksum: format!("{:x}", hasher.finalize()),
                subblocks: Vec::new(),
            });
        }
    }
    drop(spool);
    let (xml, offsets) = layout(
        images,
        properties,
        keywords,
        &property_payloads,
        &payloads,
        codec,
        shuffled,
    )?;
    let mut created = false;
    let result = (|| -> Result<()> {
        let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
        created = true;
        output.write_all(b"XISF0100")?;
        output.write_all(
            &u32::try_from(xml.len())
                .context("XISF XML length exceeds u32")?
                .to_le_bytes(),
        )?;
        output.write_all(&[0; 4])?;
        output.write_all(xml.as_bytes())?;
        let mut source = File::open(&scratch.path)?;
        for (payload, offset) in payloads.iter().zip(offsets) {
            pad_to(&mut output, offset)?;
            source.seek(SeekFrom::Start(payload.spool_start))?;
            let copied = std::io::copy(
                &mut Read::by_ref(&mut source).take(payload.stored),
                &mut output,
            )?;
            if copied != payload.stored {
                bail!("XISF spool payload was truncated");
            }
        }
        output.flush()?;
        Ok(())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(path);
    }
    result
}

/// Conservative whole-output bound for the current XISF writer. Revision 2
/// raw fallback guarantees each encoded subblock is no larger than its raw
/// input; maximal raw lengths also maximize descriptor digit widths and
/// attachment offsets. Scratch spool bytes are at most the sum of decoded
/// image bytes and must be reserved separately by a future resource planner.
pub fn xisf_output_bound_u16(
    images: &[U16ImageSpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<u64> {
    xisf_output_bound_with_properties(images, &[], codec, shuffled)
}

/// Complete XISF output bound including explicit String and ByteArray
/// properties. The caller must additionally reserve the spool size, bounded
/// by the sum of image decoded bytes and supplied binary property lengths.
pub fn xisf_output_bound_with_properties(
    images: &[U16ImageSpec<'_>],
    properties: &[XisfPropertySpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<u64> {
    xisf_output_bound_with_metadata(images, properties, &[], codec, shuffled)
}

/// Whole-output bound including explicit FITSKeyword elements.
pub fn xisf_output_bound_with_metadata(
    images: &[U16ImageSpec<'_>],
    properties: &[XisfPropertySpec<'_>],
    keywords: &[XisfKeywordSpec<'_>],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<u64> {
    if images.is_empty() || images.len() > 10_000 {
        bail!("XISF writer requires 1 to 10000 images");
    }
    if shuffled && codec == XisfCodec::Uncompressed {
        bail!("XISF shuffle requires compression");
    }
    validate_properties(properties, images.len())?;
    validate_keywords(keywords, images.len())?;
    let mut payloads = Vec::new();
    let mut total_subblocks = 0usize;
    let mut image_id_bytes = 0usize;
    for image in images {
        let count = image.sample_count()?;
        if image
            .id
            .is_some_and(|id| id.is_empty() || id.len() > 4096 || !valid_xml_text(id))
        {
            bail!("XISF image identifier contains invalid XML text");
        }
        image_id_bytes = image_id_bytes
            .checked_add(image.id.map_or(0, str::len))
            .context("XISF image ID bytes overflow")?;
        if image_id_bytes > MAX_SCALAR_METADATA_BYTES {
            bail!("XISF image IDs exceed writer byte limit");
        }
        let byte_count = count
            .checked_mul(2)
            .context("XISF decoded byte count overflows")?;
        let parts = count.div_ceil(CHUNK_SAMPLES);
        total_subblocks = total_subblocks
            .checked_add(parts)
            .context("XISF subblock count overflows")?;
        if parts > MAX_SUBBLOCKS_PER_IMAGE || total_subblocks > MAX_TOTAL_SUBBLOCKS {
            bail!("XISF image exceeds writer subblock count limit");
        }
        let mut subblocks = Vec::new();
        if codec != XisfCodec::Uncompressed {
            for start in (0..count).step_by(CHUNK_SAMPLES) {
                let length = u64::try_from((count - start).min(CHUNK_SAMPLES))?
                    .checked_mul(2)
                    .context("XISF subblock length overflows")?;
                subblocks.push((length, length));
            }
        }
        let bytes = u64::try_from(byte_count)?;
        payloads.push(Payload {
            spool_start: 0,
            stored: bytes,
            decoded: bytes,
            checksum: "0".repeat(64),
            subblocks,
        });
    }
    let mut property_payloads = vec![None; properties.len()];
    for (index, property) in properties.iter().enumerate() {
        let length = match property.value {
            XisfPropertyValue::ByteArray(bytes) => Some(u64::try_from(bytes.len())?),
            XisfPropertyValue::ByteArrayFile { length, .. } => Some(length),
            XisfPropertyValue::ByteArrayLength(length) => Some(length),
            XisfPropertyValue::String(_) => None,
        };
        if let Some(length) = length {
            property_payloads[index] = Some(payloads.len());
            payloads.push(Payload {
                spool_start: 0,
                stored: length,
                decoded: length,
                checksum: "0".repeat(64),
                subblocks: Vec::new(),
            });
        }
    }
    let (_, offsets) = layout(
        images,
        properties,
        keywords,
        &property_payloads,
        &payloads,
        codec,
        shuffled,
    )?;
    offsets
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(payloads.last().map_or(0, |payload| payload.stored))
        .context("XISF output bound overflows")
}

fn validate_properties(properties: &[XisfPropertySpec<'_>], images: usize) -> Result<()> {
    if properties.len() > 10_000 {
        bail!("XISF property count exceeds writer limit");
    }
    let mut scalar_bytes = 0usize;
    for property in properties {
        if property.id.is_empty() || property.id.len() > 4096 || !valid_xml_text(property.id) {
            bail!("XISF property ID contains invalid XML text");
        }
        scalar_bytes = scalar_bytes
            .checked_add(property.id.len())
            .context("XISF metadata byte count overflows")?;
        if property.image_index.is_some_and(|index| index >= images) {
            bail!("XISF property refers to an absent image");
        }
        if matches!(property.value, XisfPropertyValue::ByteArray(bytes) if bytes.is_empty())
            || matches!(
                property.value,
                XisfPropertyValue::ByteArrayFile { length: 0, .. }
            )
            || matches!(property.value, XisfPropertyValue::ByteArrayLength(0))
        {
            bail!("XISF binary property must not be empty");
        }
        if matches!(property.value, XisfPropertyValue::String(value) if !valid_xml_text(value)) {
            bail!("XISF String property contains invalid XML text");
        }
        if let XisfPropertyValue::String(value) = property.value {
            scalar_bytes = scalar_bytes
                .checked_add(value.len())
                .context("XISF metadata byte count overflows")?;
        }
        if scalar_bytes > MAX_SCALAR_METADATA_BYTES {
            bail!("XISF scalar metadata exceeds writer byte limit");
        }
    }
    Ok(())
}

fn validate_keywords(keywords: &[XisfKeywordSpec<'_>], images: usize) -> Result<()> {
    if keywords.len() > 100_000 {
        bail!("XISF FITSKeyword count exceeds writer limit");
    }
    let mut bytes = 0usize;
    for keyword in keywords {
        if keyword.image_index.is_some_and(|index| index >= images) {
            bail!("XISF FITSKeyword refers to an absent image");
        }
        if keyword.name.is_empty()
            || keyword.name.len() > 8
            || !keyword.name.bytes().all(|byte| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
            || keyword.value.is_empty()
            || keyword.value.len() > 70
            || !keyword.value.bytes().all(|byte| (32..=126).contains(&byte))
            || keyword.comment.is_some_and(|comment| {
                comment.len() > 70 || !comment.bytes().all(|byte| (32..=126).contains(&byte))
            })
        {
            bail!("Invalid FITSKeyword name, value or comment");
        }
        bytes = bytes
            .checked_add(keyword.name.len())
            .and_then(|n| n.checked_add(keyword.value.len()))
            .and_then(|n| n.checked_add(keyword.comment.map_or(0, str::len)))
            .context("FITSKeyword metadata size overflows")?;
        if bytes > MAX_SCALAR_METADATA_BYTES {
            bail!("XISF FITSKeyword metadata exceeds writer byte limit");
        }
    }
    Ok(())
}

fn write_part(
    spool: &mut File,
    hash: &mut Sha256,
    subblocks: &mut Vec<(u64, u64)>,
    codec: XisfCodec,
    raw: &[u8],
) -> Result<()> {
    let compressed = match codec {
        XisfCodec::Uncompressed => None,
        XisfCodec::Zstd => Some(zstd::stream::encode_all(raw, 1)?),
        XisfCodec::Zlib => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(raw)?;
            Some(encoder.finish()?)
        }
        XisfCodec::Lz4 => Some(lz4_flex::block::compress(raw)),
    };
    // Revision 2 requires raw storage when compression does not save bytes.
    let stored = compressed
        .as_deref()
        .filter(|bytes| bytes.len() < raw.len())
        .unwrap_or(raw);
    spool.write_all(stored)?;
    hash.update(stored);
    if codec != XisfCodec::Uncompressed {
        subblocks.push((u64::try_from(stored.len())?, u64::try_from(raw.len())?));
    }
    Ok(())
}

fn layout(
    images: &[U16ImageSpec<'_>],
    properties: &[XisfPropertySpec<'_>],
    keywords: &[XisfKeywordSpec<'_>],
    property_payloads: &[Option<usize>],
    payloads: &[Payload],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<(String, Vec<u64>)> {
    let mut offsets = vec![0u64; payloads.len()];
    for _ in 0..16 {
        let xml = build_xml(
            images,
            (properties, keywords),
            property_payloads,
            payloads,
            &offsets,
            codec,
            shuffled,
        )?;
        let mut cursor = align(
            16u64
                .checked_add(u64::try_from(xml.len())?)
                .context("XISF XML end overflows")?,
        )?;
        let mut next = Vec::with_capacity(payloads.len());
        for payload in payloads {
            next.push(cursor);
            cursor = align(
                cursor
                    .checked_add(payload.stored)
                    .context("XISF attachment offset overflows")?,
            )?;
        }
        if next == offsets {
            return Ok((xml, offsets));
        }
        offsets = next;
    }
    bail!("XISF XML attachment offsets did not converge")
}

fn build_xml(
    images: &[U16ImageSpec<'_>],
    metadata: (&[XisfPropertySpec<'_>], &[XisfKeywordSpec<'_>]),
    property_payloads: &[Option<usize>],
    payloads: &[Payload],
    offsets: &[u64],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<String> {
    let (properties, keywords) = metadata;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xisf version=\"1.0\">");
    write_properties(
        &mut xml,
        None,
        properties,
        property_payloads,
        payloads,
        offsets,
    )?;
    write_keywords(&mut xml, None, keywords);
    for (image_index, ((image, payload), offset)) in
        images.iter().zip(payloads).zip(offsets).enumerate()
    {
        xml.push_str("<Image geometry=\"");
        xml.push_str(&format!("{}:{}:1", image.width, image.height));
        xml.push_str("\" sampleFormat=\"UInt16\" colorSpace=\"Gray\" byteOrder=\"little\"");
        if let Some(id) = image.id {
            xml.push_str(" id=\"");
            escape_xml(&mut xml, id);
            xml.push('"');
        }
        xml.push_str(&format!(
            " location=\"attachment:{offset}:{}\"",
            payload.stored
        ));
        if let Some(label) = codec.label() {
            if shuffled {
                xml.push_str(&format!(
                    " compression=\"{label}+sh:{}:2\"",
                    payload.decoded
                ));
            } else {
                xml.push_str(&format!(" compression=\"{label}:{}\"", payload.decoded));
            }
            xml.push_str(" subblocks=\"");
            for (index, (stored, decoded)) in payload.subblocks.iter().enumerate() {
                if index > 0 {
                    xml.push(':');
                }
                xml.push_str(&format!("{stored},{decoded}"));
            }
            xml.push('"');
        }
        xml.push_str(&format!(" checksum=\"sha-256:{}\"", payload.checksum));
        if properties
            .iter()
            .any(|property| property.image_index == Some(image_index))
            || keywords
                .iter()
                .any(|keyword| keyword.image_index == Some(image_index))
        {
            xml.push('>');
            write_properties(
                &mut xml,
                Some(image_index),
                properties,
                property_payloads,
                payloads,
                offsets,
            )?;
            write_keywords(&mut xml, Some(image_index), keywords);
            xml.push_str("</Image>");
        } else {
            xml.push_str("/>");
        }
    }
    xml.push_str("</xisf>");
    if xml.len() > 64 << 20 {
        bail!("XISF XML exceeds writer limit");
    }
    Ok(xml)
}

fn write_keywords(xml: &mut String, parent_image: Option<usize>, keywords: &[XisfKeywordSpec<'_>]) {
    for keyword in keywords
        .iter()
        .filter(|keyword| keyword.image_index == parent_image)
    {
        xml.push_str("<FITSKeyword name=\"");
        escape_xml(xml, keyword.name);
        xml.push_str("\" value=\"");
        escape_xml(xml, keyword.value);
        if let Some(comment) = keyword.comment {
            xml.push_str("\" comment=\"");
            escape_xml(xml, comment);
        }
        xml.push_str("\"/>");
    }
}

fn write_properties(
    xml: &mut String,
    parent_image: Option<usize>,
    properties: &[XisfPropertySpec<'_>],
    property_payloads: &[Option<usize>],
    payloads: &[Payload],
    offsets: &[u64],
) -> Result<()> {
    for (index, property) in properties.iter().enumerate() {
        if property.image_index != parent_image {
            continue;
        }
        xml.push_str("<Property id=\"");
        escape_xml(xml, property.id);
        match property.value {
            XisfPropertyValue::String(value) => {
                xml.push_str("\" type=\"String\" value=\"");
                escape_xml(xml, value);
                xml.push_str("\"/>");
            }
            XisfPropertyValue::ByteArray(_)
            | XisfPropertyValue::ByteArrayFile { .. }
            | XisfPropertyValue::ByteArrayLength(_) => {
                let length = match property.value {
                    XisfPropertyValue::ByteArray(bytes) => bytes.len() as u64,
                    XisfPropertyValue::ByteArrayFile { length, .. } => length,
                    XisfPropertyValue::ByteArrayLength(length) => length,
                    XisfPropertyValue::String(_) => unreachable!(),
                };
                let payload_index =
                    property_payloads[index].context("Missing binary property payload index")?;
                let payload = payloads
                    .get(payload_index)
                    .context("Missing binary property payload")?;
                let offset = offsets
                    .get(payload_index)
                    .context("Missing binary property offset")?;
                xml.push_str(&format!(
                    "\" type=\"ByteArray\" length=\"{}\" location=\"attachment:{}:{}\" checksum=\"sha-256:{}\"/>",
                    length, offset, payload.stored, payload.checksum,
                ));
            }
        }
    }
    Ok(())
}

fn escape_xml(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(ch),
        }
    }
}

fn valid_xml_text(value: &str) -> bool {
    value.chars().all(|ch| matches!(ch as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF))
}

fn align(value: u64) -> Result<u64> {
    value
        .checked_add(ALIGNMENT - 1)
        .map(|n| n / ALIGNMENT * ALIGNMENT)
        .context("XISF attachment alignment overflows")
}

fn pad_to(output: &mut File, offset: u64) -> Result<()> {
    let mut remaining = offset
        .checked_sub(output.stream_position()?)
        .context("XISF attachment overlaps header or predecessor")?;
    let zeros = [0u8; 4096];
    while remaining > 0 {
        let count = usize::try_from(remaining.min(zeros.len() as u64))?;
        output.write_all(&zeros[..count])?;
        remaining -= count as u64;
    }
    Ok(())
}
