use super::*;
use std::collections::BTreeMap;
mod gzip;
mod hcompress;
mod plio;
mod rice;
mod tiles;

fn padded(n: u64) -> Result<u64> {
    mul(add(n, 2879)? / 2880, 2880)
}
fn number(h: &BTreeMap<String, String>, key: &str) -> Result<u64> {
    h.get(key)
        .ok_or_else(|| invalid(format!("missing {key}")))?
        .parse()
        .map_err(|_| invalid(format!("invalid nonnegative integer {key}")))
}
fn optional(h: &BTreeMap<String, String>, key: &str, default: u64) -> Result<u64> {
    if h.contains_key(key) {
        number(h, key)
    } else {
        Ok(default)
    }
}
fn value(card: &[u8]) -> Result<String> {
    let s = std::str::from_utf8(&card[10..])
        .map_err(|_| invalid("non-ASCII structural card"))?
        .trim();
    if let Some(s) = s.strip_prefix('\'') {
        let end = s
            .find('\'')
            .ok_or_else(|| invalid("unterminated structural string"))?;
        Ok(s[..end].trim().to_string())
    } else {
        Ok(s.split('/').next().unwrap_or("").trim().to_string())
    }
}
pub(super) fn validate(c: &mut Context<'_>) -> Result<()> {
    let mut offset = 0;
    let mut index = 0;
    while offset < c.stamp.size {
        c.checkpoint()?;
        let start = offset;
        let mut header_memory = c.memory.reserve(0)?;
        let mut h = BTreeMap::new();
        let mut cards = 0u64;
        loop {
            c.header(80)?;
            c.structure()?;
            let mut card = [0; 80];
            c.read(offset, &mut card)?;
            offset = add(offset, 80)?;
            if !card.iter().all(|b| (32..=126).contains(b)) {
                return Err(invalid(format!("HDU {index}: non-ASCII header card")));
            }
            let key = std::str::from_utf8(&card[..8])
                .map_err(|_| invalid("invalid card keyword"))?
                .trim();
            let required = match cards {
                0 => Some(if index == 0 { "SIMPLE" } else { "XTENSION" }.to_string()),
                1 => Some("BITPIX".to_string()),
                2 => Some("NAXIS".to_string()),
                _ => {
                    let axes = number(&h, "NAXIS")?;
                    if axes > 999 {
                        return Err(invalid("NAXIS exceeds FITS limit"));
                    }
                    if cards < 3 + axes {
                        Some(format!("NAXIS{}", cards - 2))
                    } else if index > 0 && cards == 3 + axes {
                        Some("PCOUNT".to_string())
                    } else if index > 0 && cards == 4 + axes {
                        Some("GCOUNT".to_string())
                    } else {
                        None
                    }
                }
            };
            if required.as_deref().is_some_and(|required| required != key) {
                return Err(invalid(format!(
                    "HDU {index}: misplaced mandatory card {key}"
                )));
            }
            cards += 1;
            if key == "END" {
                if card[8..].iter().any(|b| *b != b' ') {
                    return Err(invalid("invalid END card"));
                }
                break;
            }
            if matches!(
                key,
                "SIMPLE"
                    | "XTENSION"
                    | "BITPIX"
                    | "PCOUNT"
                    | "GCOUNT"
                    | "TFIELDS"
                    | "THEAP"
                    | "CHECKSUM"
                    | "DATASUM"
                    | "GROUPS"
                    | "ZIMAGE"
                    | "ZBITPIX"
                    | "ZCMPTYPE"
                    | "ZQUANTIZ"
                    | "ZSCALE"
                    | "ZZERO"
                    | "ZMASKCMP"
            ) || key.starts_with("NAXIS")
                || key.starts_with("TFORM")
                || key.starts_with("TTYPE")
                || key.starts_with("TBCOL")
                || key.starts_with("ZTILE")
                || key.starts_with("ZNAXIS")
                || key.starts_with("ZNAME")
                || key.starts_with("ZVAL")
                || key.starts_with("TSCAL")
                || key.starts_with("TZERO")
                || key.starts_with("TNULL")
                || matches!(key, "ZDITHER0" | "ZBLANK")
            {
                if &card[8..10] != b"= " {
                    return Err(invalid(format!("invalid {key} card")));
                }
                header_memory.grow(1024)?;
                if h.insert(key.to_string(), value(&card)?).is_some() {
                    return Err(invalid(format!("duplicate structural keyword {key}")));
                }
            }
        }
        let header_size = padded(mul(cards, 80)?)?;
        c.header(header_size - cards * 80)?;
        let data = add(start, header_size)?;
        c.extent(start, header_size)?;
        if index == 0 && h.get("SIMPLE").map(String::as_str) != Some("T") {
            return Err(unsupported("SIMPLE is not true"));
        }
        if h.get("GROUPS").map(String::as_str) == Some("T") {
            return Err(unsupported("FITS random groups are not supported"));
        }
        let bitpix: i64 = h
            .get("BITPIX")
            .ok_or_else(|| invalid("missing BITPIX"))?
            .parse()
            .map_err(|_| invalid("invalid BITPIX"))?;
        if ![8, 16, 32, 64, -32, -64].contains(&bitpix) {
            return Err(invalid("unsupported BITPIX value"));
        }
        let ndim = number(&h, "NAXIS")?;
        if ndim > 999 {
            return Err(invalid("NAXIS exceeds FITS limit"));
        }
        let mut elements = if ndim == 0 { 0 } else { 1 };
        for axis in 1..=ndim {
            elements = mul(elements, number(&h, &format!("NAXIS{axis}"))?)?;
        }
        let kind = if index == 0 {
            "IMAGE"
        } else {
            h.get("XTENSION").map(String::as_str).unwrap_or("")
        };
        let pcount = if index == 0 {
            optional(&h, "PCOUNT", 0)?
        } else {
            number(&h, "PCOUNT")?
        };
        let gcount = if index == 0 {
            optional(&h, "GCOUNT", 1)?
        } else {
            number(&h, "GCOUNT")?
        };
        if gcount != 1 {
            return Err(unsupported("FITS group count other than one"));
        }
        match kind {
            "IMAGE" => {
                if pcount != 0 {
                    return Err(invalid("image PCOUNT must be zero"));
                }
                if elements > 0 {
                    c.images += 1;
                }
            }
            "TABLE" | "BINTABLE" => {
                if bitpix != 8 || ndim != 2 {
                    return Err(invalid("invalid table dimensions/BITPIX"));
                }
            }
            _ => return Err(unsupported(format!("FITS extension {kind}"))),
        }
        let size = mul(add(elements, pcount)?, bitpix.unsigned_abs() / 8)?;
        c.extent(data, padded(size)?)?;

        if kind == "BINTABLE" {
            binary_table(c, &h, data, size)?;
        }
        if kind == "TABLE" {
            ascii_table(&h)?;
        }
        // Bound declared work before checksum I/O; defer payload decoding until
        // checksums have been verified.
        let layout = if h.get("ZIMAGE").map(String::as_str) == Some("T") {
            c.images += 1;
            if kind != "BINTABLE" {
                return Err(invalid("ZIMAGE requires a binary table"));
            }
            Some(tile_layout(c, &h)?)
        } else {
            c.decoded(size)?;
            None
        };
        if let Some(expected) = h.get("DATASUM") {
            expected
                .parse::<u32>()
                .map_err(|_| invalid("invalid DATASUM"))?;
        }
        if let Some(expected) = h.get("CHECKSUM") {
            if expected.len() != 16 || !expected.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err(invalid("invalid CHECKSUM encoding"));
            }
        }
        for key in ["DATASUM", "CHECKSUM"] {
            if h.contains_key(key) {
                c.checksums.present += 1;
            }
        }
        if c.options.level == ValidationLevel::Full {
            if let Some(expected) = h.get("DATASUM") {
                let expected: u32 = expected.parse().map_err(|_| invalid("invalid DATASUM"))?;
                let mut sum = Sum::default();
                c.stream(data, padded(size)?, |b| sum.update(b))?;
                if sum.finish() != expected {
                    return Err(integrity(format!("HDU {index}: DATASUM mismatch")));
                }
                c.checksums.verified += 1;
            }
            if h.contains_key("CHECKSUM") {
                let mut sum = Sum::default();
                c.stream(start, add(header_size, padded(size)?)?, |b| sum.update(b))?;
                if sum.finish() != u32::MAX {
                    return Err(integrity(format!("HDU {index}: CHECKSUM mismatch")));
                }
                c.checksums.verified += 1;
            }
        }
        if let Some(layout) = layout {
            validate_tiles(c, &h, index, data, layout)?;
        }
        offset = add(data, padded(size)?)?;
        index += 1;
    }
    Ok(())
}
#[derive(Default)]
struct Sum {
    sum: u64,
    word: u32,
    n: u8,
}
impl Sum {
    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.word = (self.word << 8) | b as u32;
            self.n += 1;
            if self.n == 4 {
                self.sum += self.word as u64;
                self.sum = (self.sum & 0xffff_ffff) + (self.sum >> 32);
                self.word = 0;
                self.n = 0;
            }
        }
    }
    fn finish(mut self) -> u32 {
        if self.n != 0 {
            self.sum += (self.word << ((4 - self.n) * 8)) as u64;
        }
        while self.sum >> 32 != 0 {
            self.sum = (self.sum & 0xffff_ffff) + (self.sum >> 32);
        }
        self.sum as u32
    }
}
fn type_size(code: u8, count: u64) -> Result<u64> {
    match code {
        b'X' => Ok(add(count, 7)? / 8),
        b'L' | b'A' | b'B' => Ok(count),
        b'I' => mul(count, 2),
        b'J' | b'E' => mul(count, 4),
        b'K' | b'D' | b'C' => mul(count, 8),
        b'M' => mul(count, 16),
        _ => Err(unsupported(format!("binary column type {}", code as char))),
    }
}
fn binary_table(
    c: &mut Context<'_>,
    h: &BTreeMap<String, String>,
    data: u64,
    size: u64,
) -> Result<()> {
    let fields = number(h, "TFIELDS")?;
    if fields > 999 {
        return Err(invalid("too many table fields"));
    }
    let row = number(h, "NAXIS1")?;
    let rows = number(h, "NAXIS2")?;
    let table = mul(row, rows)?;
    let heap = optional(h, "THEAP", table)?;
    if heap < table || heap > size {
        return Err(invalid("invalid table heap offset"));
    }
    let mut column = 0;
    for field in 1..=fields {
        c.structure()?;
        let form = h
            .get(&format!("TFORM{field}"))
            .ok_or_else(|| invalid("missing TFORM"))?;
        let digits = form.bytes().take_while(u8::is_ascii_digit).count();
        let repeat = if digits == 0 {
            1
        } else {
            form[..digits]
                .parse()
                .map_err(|_| invalid("invalid TFORM repeat"))?
        };
        let code = *form
            .as_bytes()
            .get(digits)
            .ok_or_else(|| invalid("missing TFORM type"))?;
        let width = if code == b'P' || code == b'Q' {
            if repeat > 1 {
                return Err(invalid("heap descriptor repeat must be zero or one"));
            }
            mul(repeat, if code == b'P' { 8 } else { 16 })?
        } else {
            type_size(code, repeat)?
        };
        if add(column, width)? > row {
            return Err(invalid("columns exceed table row length"));
        }
        if code == b'P' || code == b'Q' {
            let dtype = *form
                .as_bytes()
                .get(digits + 1)
                .ok_or_else(|| invalid("missing heap element type"))?;
            type_size(dtype, 0)?;
            for r in 0..rows {
                for item in 0..repeat {
                    c.structure()?;
                    let n = if code == b'P' { 8 } else { 16 };
                    let mut bytes = [0; 16];
                    let pos = add(add(add(data, mul(r, row)?)?, column)?, mul(item, n)?)?;
                    c.read(pos, &mut bytes[..n as usize])?;
                    let (count, offset) = if code == b'P' {
                        (
                            i32::from_be_bytes(
                                bytes[..4].try_into().map_err(|_| invalid("descriptor"))?,
                            ) as i64,
                            i32::from_be_bytes(
                                bytes[4..8].try_into().map_err(|_| invalid("descriptor"))?,
                            ) as i64,
                        )
                    } else {
                        (
                            i64::from_be_bytes(
                                bytes[..8].try_into().map_err(|_| invalid("descriptor"))?,
                            ),
                            i64::from_be_bytes(
                                bytes[8..].try_into().map_err(|_| invalid("descriptor"))?,
                            ),
                        )
                    };
                    if count < 0 || offset < 0 {
                        return Err(invalid("negative heap descriptor"));
                    }
                    if count > 0
                        && add(offset as u64, type_size(dtype, count as u64)?)? > size - heap
                    {
                        return Err(invalid("heap descriptor exceeds declared heap"));
                    }
                }
            }
        }
        column = add(column, width)?;
    }
    if column != row {
        return Err(invalid("column widths do not match row length"));
    }
    Ok(())
}
fn ascii_table(h: &BTreeMap<String, String>) -> Result<()> {
    if number(h, "PCOUNT")? != 0 {
        return Err(invalid("ASCII table PCOUNT must be zero"));
    }
    let row = number(h, "NAXIS1")?;
    let fields = number(h, "TFIELDS")?;
    if fields > 999 {
        return Err(invalid("too many ASCII table fields"));
    }
    let mut ranges = Vec::new();
    for field in 1..=fields {
        let start = number(h, &format!("TBCOL{field}"))?
            .checked_sub(1)
            .ok_or_else(|| invalid("TBCOL is one-based"))?;
        let form = h
            .get(&format!("TFORM{field}"))
            .ok_or_else(|| invalid("missing ASCII TFORM"))?;
        let code = form
            .as_bytes()
            .first()
            .copied()
            .ok_or_else(|| invalid("empty ASCII TFORM"))?;
        if !matches!(code, b'A' | b'I' | b'F' | b'E' | b'D') {
            return Err(unsupported("ASCII table column type"));
        }
        let rest = &form[1..];
        let (width, precision) = match rest.split_once('.') {
            Some((w, p)) => (w, Some(p)),
            None => (rest, None),
        };
        let width: u64 = width
            .parse()
            .map_err(|_| invalid("invalid ASCII column width"))?;
        if width == 0 {
            return Err(invalid("zero ASCII column width"));
        }
        if matches!(code, b'F' | b'E' | b'D') {
            precision
                .ok_or_else(|| invalid("missing ASCII precision"))?
                .parse::<u64>()
                .map_err(|_| invalid("invalid ASCII precision"))?;
        } else if precision.is_some() {
            return Err(invalid("unexpected ASCII precision"));
        }
        let end = add(start, width)?;
        if end > row || ranges.iter().any(|&(a, b)| start < b && end > a) {
            return Err(invalid("ASCII columns overlap or exceed row"));
        }
        ranges.push((start, end));
    }
    Ok(())
}

struct TileLayout {
    bitpix: i64,
}

fn tile_layout(c: &mut Context<'_>, h: &BTreeMap<String, String>) -> Result<TileLayout> {
    let ndim = number(h, "ZNAXIS")?;
    if ndim == 0 || ndim > 999 {
        return Err(invalid("invalid compressed image dimensions"));
    }
    let mut pixels = 1;
    let mut tiles = 1;
    for axis in 1..=ndim {
        let n = number(h, &format!("ZNAXIS{axis}"))?;
        let t = optional(h, &format!("ZTILE{axis}"), if axis == 1 { n } else { 1 })?;
        if n == 0 || t == 0 {
            return Err(invalid("zero compressed dimension/tile"));
        }
        pixels = mul(pixels, n)?;
        tiles = mul(tiles, n.div_ceil(t))?;
    }
    let bitpix: i64 = h
        .get("ZBITPIX")
        .ok_or_else(|| invalid("missing ZBITPIX"))?
        .parse()
        .map_err(|_| invalid("invalid ZBITPIX"))?;
    if ![8, 16, 32, 64, -32, -64].contains(&bitpix) {
        return Err(invalid("invalid compressed BITPIX"));
    }
    c.decoded(mul(pixels, bitpix.unsigned_abs() / 8)?)?;
    if tiles != number(h, "NAXIS2")? {
        return Err(invalid("compressed tile count does not match table rows"));
    }
    Ok(TileLayout { bitpix })
}

fn validate_tiles(
    c: &mut Context<'_>,
    h: &BTreeMap<String, String>,
    _index: usize,
    data: u64,
    layout: TileLayout,
) -> Result<()> {
    let bitpix = layout.bitpix;
    let codec = h
        .get("ZCMPTYPE")
        .ok_or_else(|| invalid("missing ZCMPTYPE"))?;
    if c.options.level == ValidationLevel::Structural {
        return c.undecoded(codec);
    }
    if ![
        "RICE_1",
        "RICE_ONE",
        "GZIP_1",
        "GZIP_2",
        "PLIO_1",
        "HCOMPRESS_1",
        "NOCOMPRESS",
    ]
    .contains(&codec.as_str())
    {
        return Err(unsupported(format!("FITS compression codec {codec}")));
    }
    if gzip::supported(h, bitpix) {
        return gzip::validate(c, h, data, bitpix);
    }
    tiles::validate(c, h, data, bitpix)
}
