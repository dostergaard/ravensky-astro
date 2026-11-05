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
const MAX_SHUFFLED_IMAGE_BYTES: usize = 16 << 20;
const ALIGNMENT: u64 = 4096;
const MAX_SUBBLOCKS_PER_IMAGE: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XisfCodec {
    Uncompressed,
    Zstd,
    Zlib,
    Lz4,
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

/// Write a complete sequence of UInt16 images without fixed attachment
/// offsets. Compressed data use independently decodable subblocks and Revision
/// 2 raw fallback. The callback fills bounded pixel chunks. Whole-block
/// shuffling is limited to 16 MiB decoded per image. The output must not exist.
pub fn write_xisf_u16(
    path: &Path,
    images: &[U16ImageSpec<'_>],
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
    for image in images {
        image.sample_count()?;
        if image.sample_count()?.div_ceil(CHUNK_SAMPLES) > MAX_SUBBLOCKS_PER_IMAGE {
            bail!("XISF image exceeds writer subblock count limit");
        }
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
            let byte_count = count
                .checked_mul(2)
                .context("XISF shuffled byte count overflows")?;
            if byte_count > MAX_SHUFFLED_IMAGE_BYTES {
                bail!("XISF shuffled image exceeds 16 MiB writer limit");
            }
            let mut raw = Vec::new();
            raw.try_reserve_exact(byte_count)
                .context("Failed to allocate XISF shuffle input")?;
            let mut pixels = [0u16; CHUNK_SAMPLES];
            for start in (0..count).step_by(CHUNK_SAMPLES) {
                let size = (count - start).min(CHUNK_SAMPLES);
                read(index, start as u64, &mut pixels[..size])?;
                for value in &pixels[..size] {
                    raw.extend_from_slice(&value.to_le_bytes());
                }
            }
            let mut shuffled_bytes = vec![0u8; byte_count];
            for sample in 0..count {
                shuffled_bytes[sample] = raw[2 * sample];
                shuffled_bytes[count + sample] = raw[2 * sample + 1];
            }
            for part in shuffled_bytes.chunks(CHUNK_SAMPLES * 2) {
                write_part(&mut spool, &mut hasher, &mut subblocks, codec, part)?;
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
    drop(spool);
    let (xml, offsets) = layout(images, &payloads, codec, shuffled)?;
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
    if images.is_empty() || images.len() > 10_000 {
        bail!("XISF writer requires 1 to 10000 images");
    }
    if shuffled && codec == XisfCodec::Uncompressed {
        bail!("XISF shuffle requires compression");
    }
    let mut payloads = Vec::new();
    for image in images {
        let count = image.sample_count()?;
        let byte_count = count
            .checked_mul(2)
            .context("XISF decoded byte count overflows")?;
        if shuffled && byte_count > MAX_SHUFFLED_IMAGE_BYTES {
            bail!("XISF shuffled image exceeds 16 MiB writer limit");
        }
        let parts = count.div_ceil(CHUNK_SAMPLES);
        if parts > MAX_SUBBLOCKS_PER_IMAGE {
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
    let (_, offsets) = layout(images, &payloads, codec, shuffled)?;
    offsets
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(payloads.last().map_or(0, |payload| payload.stored))
        .context("XISF output bound overflows")
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
    payloads: &[Payload],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<(String, Vec<u64>)> {
    let mut offsets = vec![0u64; images.len()];
    for _ in 0..16 {
        let xml = build_xml(images, payloads, &offsets, codec, shuffled)?;
        let mut cursor = align(
            16u64
                .checked_add(u64::try_from(xml.len())?)
                .context("XISF XML end overflows")?,
        )?;
        let mut next = Vec::with_capacity(images.len());
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
    payloads: &[Payload],
    offsets: &[u64],
    codec: XisfCodec,
    shuffled: bool,
) -> Result<String> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xisf version=\"1.0\">");
    for ((image, payload), offset) in images.iter().zip(payloads).zip(offsets) {
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
        xml.push_str(&format!(" checksum=\"sha-256:{}\"/>", payload.checksum));
    }
    xml.push_str("</xisf>");
    if xml.len() > 64 << 20 {
        bail!("XISF XML exceeds writer limit");
    }
    Ok(xml)
}

fn escape_xml(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
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
