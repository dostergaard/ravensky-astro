//! Checked PLIO instruction execution; runs are counted without pixel allocation.
use super::*;
use crate::validation::input::{decode_error, Input};

fn word(input: &mut impl Read) -> Result<i16> {
    let mut bytes = [0; 2];
    input.read_exact(&mut bytes).map_err(decode_error)?;
    Ok(i16::from_be_bytes(bytes))
}

pub(super) fn validate(c: &mut Context<'_>, offset: u64, length: u64, pixels: u64) -> Result<()> {
    if length < 6 || !length.is_multiple_of(2) {
        return Err(integrity("invalid PLIO line-list extent"));
    }
    let mut input = Input::attached(c, offset, length)?;
    let _buffer = word(&mut input)?;
    let header = word(&mut input)?;
    let version = word(&mut input)?;
    let (words, first) = if version > 0 {
        (version as u64, 3)
    } else {
        if version != -100 || header != 7 || length < 14 {
            return Err(unsupported("unknown PLIO line-list header"));
        }
        let low = word(&mut input)?;
        let high = word(&mut input)?;
        if low < 0 || high < 0 {
            return Err(integrity("negative PLIO line-list size"));
        }
        word(&mut input)?;
        word(&mut input)?;
        (low as u64 + ((high as u64) << 15), 7)
    };
    if words != length / 2 || words < first {
        return Err(integrity("PLIO line-list size mismatch"));
    }
    let mut cursor = first;
    let mut emitted = 0u64;
    let mut high = 1i64;
    while cursor < words {
        let instruction = word(&mut input)?;
        cursor += 1;
        if instruction < 0 {
            return Err(integrity("invalid PLIO instruction"));
        }
        let data = (instruction & 4095) as i64;
        let opcode = instruction >> 12;
        let count = match opcode {
            0 | 4 | 5 => {
                if data == 0 {
                    return Err(integrity("zero-length PLIO run"));
                }
                data as u64
            }
            1 => {
                if cursor == words {
                    return Err(integrity("truncated PLIO high value"));
                }
                let upper = word(&mut input)?;
                cursor += 1;
                if upper < 0 {
                    return Err(integrity("negative PLIO high value"));
                }
                high = ((upper as i64) << 12) + data;
                0
            }
            2 | 6 => {
                high += data;
                u64::from(opcode == 6)
            }
            3 | 7 => {
                high -= data;
                u64::from(opcode == 7)
            }
            _ => return Err(integrity("unknown PLIO opcode")),
        };
        if !(0..=0xffffff).contains(&high) {
            return Err(integrity("PLIO value exceeds unsigned 24-bit range"));
        }
        emitted = add(emitted, count)?;
        if emitted > pixels {
            return Err(integrity("PLIO run exceeds declared tile"));
        }
    }
    // PLIO implicitly fills the unwritten tail with zeros.
    Ok(())
}
