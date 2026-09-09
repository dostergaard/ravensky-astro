//! Named compressed-table columns and bounded per-tile dispatch.
use super::*;

#[derive(Clone, Copy)]
struct Column {
    offset: u64,
    descriptor: usize,
    dtype: u8,
}
struct Columns {
    compressed: Column,
    gzip: Option<Column>,
    raw: Option<Column>,
    scale: Option<Column>,
    zero: Option<Column>,
    mask: Option<Column>,
}
fn columns(h: &BTreeMap<String, String>) -> Result<Columns> {
    let mut selected = [None; 7];
    let names = [
        "COMPRESSED_DATA",
        "GZIP_COMPRESSED_DATA",
        "UNCOMPRESSED_DATA",
        "ZSCALE",
        "ZZERO",
        "NULL_PIXEL_MASK",
        "ZBLANK",
    ];
    let mut offset = 0;
    for i in 1..=number(h, "TFIELDS")? {
        let form = h
            .get(&format!("TFORM{i}"))
            .ok_or_else(|| invalid("missing TFORM"))?;
        let digits = form.bytes().take_while(u8::is_ascii_digit).count();
        let repeat = if digits == 0 {
            1
        } else {
            form[..digits]
                .parse::<u64>()
                .map_err(|_| invalid("invalid TFORM repeat"))?
        };
        let code = *form
            .as_bytes()
            .get(digits)
            .ok_or_else(|| invalid("missing column type"))?;
        let descriptor = match code {
            b'P' => 8,
            b'Q' => 16,
            _ => 0,
        };
        let dtype = if descriptor > 0 {
            *form
                .as_bytes()
                .get(digits + 1)
                .ok_or_else(|| invalid("missing element type"))?
        } else {
            code
        };
        if let Some(role) = h
            .get(&format!("TTYPE{i}"))
            .and_then(|name| names.iter().position(|n| name.eq_ignore_ascii_case(n)))
        {
            if repeat != 1 || selected[role].is_some() {
                return Err(invalid("duplicate or repeated compressed-image column"));
            }
            if h.contains_key(&format!("TSCAL{i}"))
                || h.contains_key(&format!("TZERO{i}"))
                || h.contains_key(&format!("TNULL{i}"))
            {
                return Err(unsupported(
                    "scaled or nullable compressed-image table columns",
                ));
            }
            let valid = match role {
                0 => descriptor > 0 && matches!(dtype, b'B' | b'I' | b'J'),
                1 | 5 => descriptor > 0 && dtype == b'B',
                2 => descriptor > 0 && matches!(dtype, b'B' | b'I' | b'J' | b'K' | b'E' | b'D'),
                3 | 4 => descriptor == 0 && matches!(dtype, b'E' | b'D'),
                6 => descriptor == 0 && dtype == b'J',
                _ => false,
            };
            if !valid {
                return Err(invalid(format!("invalid {} column type", names[role])));
            }
            selected[role] = Some(Column {
                offset,
                descriptor,
                dtype,
            });
        }
        offset = add(
            offset,
            if descriptor > 0 {
                mul(repeat, descriptor as u64)?
            } else {
                type_size(dtype, repeat)?
            },
        )?;
    }
    if selected[1].is_some() && selected[2].is_some() {
        return Err(invalid(
            "both compressed and uncompressed fallback columns present",
        ));
    }
    if selected[3].is_some() != selected[4].is_some() {
        return Err(invalid("quantization requires both ZSCALE and ZZERO"));
    }
    Ok(Columns {
        compressed: selected[0].ok_or_else(|| invalid("missing COMPRESSED_DATA column"))?,
        gzip: selected[1],
        raw: selected[2],
        scale: selected[3],
        zero: selected[4],
        mask: selected[5],
    })
}

pub(super) fn parameter<'a>(
    h: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<Option<&'a str>> {
    let mut result = None;
    for (key, value) in h {
        if let Some(index) = key.strip_prefix("ZNAME") {
            if value.eq_ignore_ascii_case(name) {
                if result.is_some() {
                    return Err(invalid(format!("duplicate compression parameter {name}")));
                }
                result = Some(
                    h.get(&format!("ZVAL{index}"))
                        .ok_or_else(|| invalid("compression parameter missing ZVAL"))?
                        .as_str(),
                );
            }
        }
    }
    Ok(result)
}
fn integer_parameter(h: &BTreeMap<String, String>, name: &str, default: u64) -> Result<u64> {
    parameter(h, name)?.map_or(Ok(default), |s| {
        s.parse().map_err(|_| invalid(format!("invalid {name}")))
    })
}

fn payload(
    c: &mut Context<'_>,
    column: Column,
    row: u64,
    heap: u64,
    heap_size: u64,
) -> Result<(u64, u64)> {
    let mut bytes = [0; 16];
    c.read(add(row, column.offset)?, &mut bytes[..column.descriptor])?;
    let (count, offset) = if column.descriptor == 8 {
        (
            i32::from_be_bytes(bytes[..4].try_into().map_err(|_| invalid("descriptor"))?) as i64,
            i32::from_be_bytes(bytes[4..8].try_into().map_err(|_| invalid("descriptor"))?) as i64,
        )
    } else {
        (
            i64::from_be_bytes(bytes[..8].try_into().map_err(|_| invalid("descriptor"))?),
            i64::from_be_bytes(bytes[8..].try_into().map_err(|_| invalid("descriptor"))?),
        )
    };
    if count < 0 || offset < 0 {
        return Err(invalid("negative compressed tile descriptor"));
    }
    let length = type_size(column.dtype, count as u64)?;
    if length > 0 && add(offset as u64, length)? > heap_size {
        return Err(invalid("compressed tile exceeds heap"));
    }
    Ok((add(heap, offset as u64)?, length))
}
fn scalar(
    c: &mut Context<'_>,
    h: &BTreeMap<String, String>,
    row: u64,
    column: Option<Column>,
    name: &str,
) -> Result<Option<f64>> {
    let value = if let Some(column) = column {
        let mut bytes = [0; 8];
        let width = if column.dtype == b'E' { 4 } else { 8 };
        c.read(add(row, column.offset)?, &mut bytes[..width])?;
        Some(if width == 4 {
            f32::from_be_bytes(bytes[..4].try_into().map_err(|_| invalid("scalar"))?) as f64
        } else {
            f64::from_be_bytes(bytes)
        })
    } else if let Some(value) = h.get(name) {
        Some(
            value
                .replace('D', "E")
                .parse()
                .map_err(|_| invalid(format!("invalid {name}")))?,
        )
    } else {
        None
    };
    if value.is_some_and(|v: f64| !v.is_finite()) {
        return Err(invalid(format!("nonfinite {name}")));
    }
    Ok(value)
}

pub(super) fn validate(
    c: &mut Context<'_>,
    h: &BTreeMap<String, String>,
    data: u64,
    bitpix: i64,
) -> Result<()> {
    let columns = columns(h)?;
    let row_size = number(h, "NAXIS1")?;
    let rows = number(h, "NAXIS2")?;
    let table = mul(row_size, rows)?;
    let heap_relative = optional(h, "THEAP", table)?;
    let heap_size = add(table, number(h, "PCOUNT")?)?
        .checked_sub(heap_relative)
        .ok_or_else(|| invalid("invalid heap"))?;
    let heap = add(data, heap_relative)?;
    let ndim = number(h, "ZNAXIS")?;
    let _reservation = c.memory.reserve(mul(ndim, 24)?)?;
    let mut axes = Vec::new();
    axes.try_reserve_exact(ndim as usize)
        .map_err(|_| limit("tile geometry allocation failed"))?;
    for axis in 1..=ndim {
        let size = number(h, &format!("ZNAXIS{axis}"))?;
        let tile = optional(h, &format!("ZTILE{axis}"), if axis == 1 { size } else { 1 })?;
        axes.push((size, tile, size.div_ceil(tile)));
    }
    let codec = h
        .get("ZCMPTYPE")
        .ok_or_else(|| invalid("missing ZCMPTYPE"))?
        .as_str();
    let block = integer_parameter(h, "BLOCKSIZE", 32)?;
    let bytepix = integer_parameter(h, "BYTEPIX", 4)?;
    if codec == "HCOMPRESS_1" {
        if integer_parameter(h, "SMOOTH", 0)? > 1 {
            return Err(invalid("HCOMPRESS SMOOTH must be zero or one"));
        }
        if let Some(scale) = parameter(h, "SCALE")? {
            let scale: f64 = scale
                .replace('D', "E")
                .parse()
                .map_err(|_| invalid("invalid HCOMPRESS SCALE"))?;
            if !scale.is_finite() {
                return Err(invalid("nonfinite HCOMPRESS SCALE"));
            }
        }
    }
    if let Some(blank) = h.get("ZBLANK") {
        blank
            .parse::<i32>()
            .map_err(|_| invalid("ZBLANK must be a signed 32-bit integer"))?;
    }
    if let Some(quantization) = h.get("ZQUANTIZ") {
        match quantization.as_str() {
            "NO_DITHER" | "NONE" => {}
            "SUBTRACTIVE_DITHER_1" | "SUBTRACTIVE_DITHER_2" => {
                if !(1..=10000).contains(&number(h, "ZDITHER0")?) {
                    return Err(invalid("ZDITHER0 must be in 1..=10000"));
                }
            }
            _ => return Err(unsupported(format!("quantization {quantization}"))),
        }
    }
    if columns.mask.is_some() != h.contains_key("ZMASKCMP") {
        return Err(invalid("null mask column and ZMASKCMP must occur together"));
    }
    for row in 0..rows {
        c.checkpoint()?;
        let start = add(data, mul(row, row_size)?)?;
        let mut index = row;
        let mut pixels = 1;
        let mut dims = [1u64; 2];
        for (axis, &(size, tile, bins)) in axes.iter().enumerate() {
            let dimension = tile.min(size - mul(index % bins, tile)?);
            pixels = mul(pixels, dimension)?;
            if axis < 2 {
                dims[axis] = dimension;
            }
            index /= bins;
        }
        let main = payload(c, columns.compressed, start, heap, heap_size)?;
        let fallback_gzip = columns
            .gzip
            .map(|col| payload(c, col, start, heap, heap_size))
            .transpose()?;
        let fallback_raw = columns
            .raw
            .map(|col| payload(c, col, start, heap, heap_size))
            .transpose()?;
        let active = usize::from(main.1 > 0)
            + usize::from(fallback_gzip.is_some_and(|p| p.1 > 0))
            + usize::from(fallback_raw.is_some_and(|p| p.1 > 0));
        if active != 1 {
            return Err(invalid(
                "tile must have exactly one nonempty primary or fallback payload",
            ));
        }
        let original_bytes = mul(pixels, bitpix.unsigned_abs() / 8)?;
        if main.1 > 0 {
            let scale = scalar(c, h, start, columns.scale, "ZSCALE")?;
            let zero = scalar(c, h, start, columns.zero, "ZZERO")?;
            if scale.is_some() != zero.is_some() {
                return Err(invalid("quantization requires scale and zero"));
            }
            let quantized = bitpix < 0 && scale.is_some_and(|v| v != 0.0);
            let bytes = if quantized {
                4
            } else {
                bitpix.unsigned_abs() / 8
            };
            match codec {
                "GZIP_1" | "GZIP_2" => {
                    if columns.compressed.dtype != b'B' {
                        return Err(invalid("GZIP requires byte descriptors"));
                    }
                    gzip::stream(c, main.0, main.1, mul(pixels, bytes)?)?;
                }
                "RICE_1" | "RICE_ONE" => {
                    if bitpix < 0 && !quantized {
                        return Err(unsupported("Rice float tile without quantization"));
                    }
                    if bytepix != bytes || columns.compressed.dtype != b'B' {
                        return Err(invalid(
                            "Rice BYTEPIX or column type disagrees with samples",
                        ));
                    }
                    rice::validate(c, main.0, main.1, pixels, bytepix, block)?;
                }
                "PLIO_1" => {
                    if bitpix < 0 && !quantized {
                        return Err(unsupported("PLIO float tile without quantization"));
                    }
                    if columns.compressed.dtype != b'I' {
                        return Err(invalid("PLIO requires 16-bit descriptors"));
                    }
                    plio::validate(c, main.0, main.1, pixels)?;
                }
                "HCOMPRESS_1" => {
                    if pixels != mul(dims[0], dims[1])? {
                        return Err(unsupported(
                            "HCOMPRESS tile has more than two nontrivial axes",
                        ));
                    }
                    if bitpix < 0 && !quantized {
                        return Err(unsupported("HCOMPRESS float tile without quantization"));
                    }
                    if columns.compressed.dtype != b'B' {
                        return Err(invalid("HCOMPRESS requires byte descriptors"));
                    }
                    hcompress::validate(c, main.0, main.1, dims, !matches!(bitpix, 8 | 16))?;
                }
                "NOCOMPRESS" => {
                    if main.1 != mul(pixels, bytes)? {
                        return Err(integrity("uncompressed tile size mismatch"));
                    }
                    c.stream(main.0, main.1, |_| {})?;
                }
                _ => return Err(unsupported(format!("FITS codec {codec}"))),
            }
        } else if let Some((offset, length)) = fallback_gzip.filter(|p| p.1 > 0) {
            gzip::stream(c, offset, length, original_bytes)?;
        } else if let Some((offset, length)) = fallback_raw.filter(|p| p.1 > 0) {
            let expected_type = match bitpix {
                8 => b'B',
                16 => b'I',
                32 => b'J',
                64 => b'K',
                -32 => b'E',
                -64 => b'D',
                _ => return Err(invalid("invalid BITPIX")),
            };
            if columns.raw.is_none_or(|col| col.dtype != expected_type) || length != original_bytes
            {
                return Err(integrity("raw fallback type or size mismatch"));
            }
            c.stream(offset, length, |_| {})?;
        }
        if let Some(mask) = columns.mask {
            let (offset, length) = payload(c, mask, start, heap, heap_size)?;
            if length > 0 {
                c.decoded(pixels)?;
                match h.get("ZMASKCMP").map(String::as_str) {
                    Some("GZIP_1" | "GZIP_2") => gzip::stream(c, offset, length, pixels)?,
                    Some("RICE_1" | "RICE_ONE") => {
                        rice::validate(c, offset, length, pixels, 1, 32)?
                    }
                    Some("PLIO_1") => plio::validate(c, offset, length, pixels)?,
                    _ => return Err(unsupported("null mask compression codec")),
                }
            }
        }
    }
    Ok(())
}
