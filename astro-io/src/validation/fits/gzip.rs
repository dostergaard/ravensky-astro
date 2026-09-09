use super::*;
use crate::validation::input::{decode_error, Input};
use std::io::BufRead;

// This exact profile needs no quantization, null-mask or fallback-column decoder.
// A false result dispatches to the general managed column/layout validator.
pub(super) fn supported(h: &BTreeMap<String, String>, bitpix: i64) -> bool {
    matches!(bitpix, 8 | 16 | 32 | 64)
        && h.get("ZCMPTYPE")
            .is_some_and(|s| matches!(s.as_str(), "GZIP_1" | "GZIP_2"))
        && number(h, "TFIELDS").ok() == Some(1)
        && h.get("TTYPE1")
            .is_some_and(|s| s.eq_ignore_ascii_case("COMPRESSED_DATA"))
        && descriptor_width(h).is_some()
        && !h.contains_key("ZQUANTIZ")
        && !h.contains_key("ZSCALE")
        && !h.contains_key("ZZERO")
        && !h.contains_key("ZMASKCMP")
        && !h.keys().any(|key| {
            key.starts_with("TSCAL") || key.starts_with("TZERO") || key.starts_with("TNULL")
        })
}

fn descriptor_width(h: &BTreeMap<String, String>) -> Option<usize> {
    let form = h.get("TFORM1")?;
    // Optional maximum-element count is metadata, not an allocation instruction.
    let code = form.split('(').next()?;
    match code {
        "PB" | "1PB" => Some(8),
        "QB" | "1QB" => Some(16),
        _ => None,
    }
}

pub(super) fn validate(
    c: &mut Context<'_>,
    h: &BTreeMap<String, String>,
    data: u64,
    bitpix: i64,
) -> Result<()> {
    let width = descriptor_width(h).ok_or_else(|| unsupported("GZIP descriptor layout"))?;
    let row_size = number(h, "NAXIS1")?;
    let rows = number(h, "NAXIS2")?;
    let heap = optional(h, "THEAP", mul(row_size, rows)?)?;
    let heap_size = add(mul(row_size, rows)?, number(h, "PCOUNT")?)?
        .checked_sub(heap)
        .ok_or_else(|| invalid("invalid GZIP heap offset"))?;
    let dimensions = number(h, "ZNAXIS")?;
    let _geometry = c
        .memory
        .reserve(mul(dimensions, std::mem::size_of::<Axis>() as u64)?)?;
    let mut axes = Vec::new();
    axes.try_reserve_exact(dimensions as usize)
        .map_err(|_| limit("tile geometry allocation failed"))?;
    for axis in 1..=dimensions {
        let size = number(h, &format!("ZNAXIS{axis}"))?;
        let tile = optional(h, &format!("ZTILE{axis}"), if axis == 1 { size } else { 1 })?;
        axes.push(Axis {
            size,
            tile,
            bins: size.div_ceil(tile),
        });
    }
    for row in 0..rows {
        c.checkpoint()?;
        let mut descriptor = [0; 16];
        c.read(add(data, mul(row, row_size)?)?, &mut descriptor[..width])?;
        // Recheck the descriptor used here: the source may have changed since
        // binary_table inspected it. The final stamp check is an additional guard.
        let (length, offset) = if width == 8 {
            (
                u32::from_be_bytes(
                    descriptor[..4]
                        .try_into()
                        .map_err(|_| invalid("descriptor"))?,
                ) as u64,
                u32::from_be_bytes(
                    descriptor[4..8]
                        .try_into()
                        .map_err(|_| invalid("descriptor"))?,
                ) as u64,
            )
        } else {
            (
                u64::from_be_bytes(
                    descriptor[..8]
                        .try_into()
                        .map_err(|_| invalid("descriptor"))?,
                ),
                u64::from_be_bytes(
                    descriptor[8..]
                        .try_into()
                        .map_err(|_| invalid("descriptor"))?,
                ),
            )
        };
        let maximum = if width == 8 {
            i32::MAX as u64
        } else {
            i64::MAX as u64
        };
        if length > maximum || offset > maximum || add(offset, length)? > heap_size {
            return Err(invalid(
                "GZIP descriptor is negative or exceeds declared heap",
            ));
        }
        if length == 0 {
            return Err(invalid("empty compressed tile without a fallback column"));
        }
        let expected = tile_bytes(&axes, row, bitpix as u64 / 8)?;
        stream(c, add(add(data, heap)?, offset)?, length, expected)?;
    }
    Ok(())
}

struct Axis {
    size: u64,
    tile: u64,
    bins: u64,
}

fn tile_bytes(axes: &[Axis], mut row: u64, bytes_per_pixel: u64) -> Result<u64> {
    let mut bytes = bytes_per_pixel;
    for axis in axes {
        let position = mul(row % axis.bins, axis.tile)?;
        bytes = mul(bytes, axis.tile.min(axis.size - position))?;
        row /= axis.bins;
    }
    Ok(bytes)
}

pub(super) fn stream(c: &mut Context<'_>, offset: u64, length: u64, expected: u64) -> Result<()> {
    // The header parser can retain extra/name/comment Vecs. Take bounds bytes
    // delivered during header parsing; allowances precede those allocations.
    let _codec = c.memory.reserve(1024 * 1024 + 256 * 1024)?;
    let mut output = c.memory.buffer(65536)?;
    let source = Input::attached(c, offset, length)?;
    let mut header = flate2::bufread::GzDecoder::new(source.take(65536));
    if header.header().is_none() {
        let exhausted = header.get_ref().limit() == 0;
        let error = match header.read(&mut [0; 1]) {
            Err(error) => decode_error(error),
            Ok(_) => integrity("missing GZIP header"),
        };
        return Err(if exhausted {
            limit("GZIP header exceeds 64 KiB")
        } else {
            error
        });
    }
    // Reuse flate2's header checks, but drive Deflate explicitly to require
    // StreamEnd even for malformed streams with a plausible CRC/ISIZE suffix.
    let mut source = header.into_inner().into_inner();
    let mut decoder = flate2::Decompress::new(false);
    let mut crc = flate2::Crc::new();
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let cap = expected
            .saturating_sub(before_out)
            .saturating_add(1)
            .min(output.len() as u64) as usize;
        let status = decoder
            .decompress(
                source.fill_buf().map_err(decode_error)?,
                &mut output[..cap],
                flate2::FlushDecompress::None,
            )
            .map_err(|e| integrity(format!("FITS GZIP decode: {e}")))?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let produced = (decoder.total_out() - before_out) as usize;
        source.consume(consumed);
        if decoder.total_out() > expected {
            return Err(integrity("GZIP tile exceeds declared decoded size"));
        }
        crc.update(&output[..produced]);
        if status == flate2::Status::StreamEnd {
            break;
        }
        if consumed == 0 && produced == 0 {
            return Err(integrity("truncated or stalled GZIP deflate stream"));
        }
    }
    let mut trailer = [0; 8];
    source.read_exact(&mut trailer).map_err(decode_error)?;
    let stored_crc = u32::from_le_bytes(
        trailer[..4]
            .try_into()
            .map_err(|_| integrity("GZIP trailer"))?,
    );
    let stored_size = u32::from_le_bytes(
        trailer[4..]
            .try_into()
            .map_err(|_| integrity("GZIP trailer"))?,
    );
    if stored_crc != crc.sum() || stored_size != decoder.total_out() as u32 {
        return Err(integrity("GZIP CRC32 or ISIZE mismatch"));
    }
    if source.consumed != length {
        if length - source.consumed >= 2 {
            let mut signature = [0; 2];
            source.read_exact(&mut signature).map_err(decode_error)?;
            if signature == [31, 139] {
                return Err(unsupported("concatenated GZIP members in a FITS tile"));
            }
        }
        return Err(integrity("trailing or unused GZIP tile bytes"));
    }
    if decoder.total_out() != expected {
        return Err(integrity("GZIP tile shorter than declared decoded size"));
    }
    Ok(())
}
