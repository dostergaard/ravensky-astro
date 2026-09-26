//! Shared private XISF compression primitives.
//!
//! This module owns format rules that must not drift between the pixel loader
//! and full validator: compression/subblock syntax, exact-one-frame Zstandard
//! decoding, and whole-block byte unshuffling. Resource admission remains a
//! caller policy; all sizes returned here are checked format facts.

use std::fmt;
use std::io::{self, BufRead, Cursor, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Invalid,
    Limit,
}

#[derive(Debug)]
pub(crate) struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Invalid,
            message: message.into(),
        }
    }

    fn limit(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Limit,
            message: message.into(),
        }
    }

    pub(crate) fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Subblock {
    pub(crate) stored: u64,
    pub(crate) decoded: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Compression {
    codec: String,
    decoded: u64,
    shuffle: Option<u64>,
    subblocks: Vec<Subblock>,
}

impl Compression {
    /// Parse the complete XISF compression and optional subblock descriptors.
    ///
    /// Accepted subblocks account for both the stored extent and declared
    /// decoded length exactly. This is a format parser, not codec capability
    /// policy: callers still decide which base codec they support.
    pub(crate) fn parse(
        compression: Option<&str>,
        subblocks: Option<&str>,
        stored: u64,
    ) -> Result<Option<Self>, Error> {
        let Some(value) = compression else {
            if subblocks.is_some() {
                return Err(Error::invalid("subblocks without compression"));
            }
            return Ok(None);
        };
        let mut pieces = value.split(':');
        let codec_piece = pieces
            .next()
            .ok_or_else(|| Error::invalid("invalid compression descriptor"))?;
        let decoded_piece = pieces
            .next()
            .ok_or_else(|| Error::invalid("invalid compression descriptor"))?;
        let shuffle_piece = pieces.next();
        if pieces.next().is_some() {
            return Err(Error::invalid("invalid compression descriptor"));
        }
        let (codec_value, shuffled) = codec_piece
            .strip_suffix("+sh")
            .map_or((codec_piece, false), |codec| (codec, true));
        let shuffle = if shuffled {
            let size =
                natural(shuffle_piece.ok_or_else(|| Error::invalid("missing shuffle item size"))?)?;
            if size == 0 {
                return Err(Error::invalid("zero shuffle item size"));
            }
            Some(size)
        } else {
            if shuffle_piece.is_some() {
                return Err(Error::invalid("unexpected compression parameter"));
            }
            None
        };
        if codec_value.is_empty() {
            return Err(Error::invalid("missing compression codec"));
        }
        let mut codec = String::new();
        codec
            .try_reserve_exact(codec_value.len())
            .map_err(|_| Error::limit("compression codec allocation failed"))?;
        codec.push_str(codec_value);
        let decoded = natural(decoded_piece)?;

        let mut parsed = Vec::new();
        if let Some(value) = subblocks {
            if value.is_empty() {
                return Err(Error::invalid("empty compression subblock table"));
            }
            for part in value.split(':') {
                let (stored_part, decoded_part) = part
                    .split_once(',')
                    .ok_or_else(|| Error::invalid("invalid compression subblock"))?;
                let part = Subblock {
                    stored: natural(stored_part)?,
                    decoded: natural(decoded_part)?,
                };
                if part.stored == 0 || part.decoded == 0 {
                    return Err(Error::invalid("empty compression subblock"));
                }
                parsed
                    .try_reserve(1)
                    .map_err(|_| Error::limit("subblock descriptor allocation failed"))?;
                parsed.push(part);
            }
        } else {
            parsed
                .try_reserve_exact(1)
                .map_err(|_| Error::limit("subblock descriptor allocation failed"))?;
            parsed.push(Subblock { stored, decoded });
        }

        let (stored_total, decoded_total) = checked_subblock_totals(&parsed)?;
        if stored_total != stored || decoded_total != decoded {
            return Err(Error::invalid(
                "subblocks do not match stored/decoded lengths",
            ));
        }
        if shuffle.is_some_and(|size| size > decoded) {
            return Err(Error::invalid("shuffle item size exceeds payload"));
        }
        Ok(Some(Self {
            codec,
            decoded,
            shuffle,
            subblocks: parsed,
        }))
    }

    pub(crate) fn codec(&self) -> &str {
        &self.codec
    }

    pub(crate) fn decoded(&self) -> u64 {
        self.decoded
    }

    pub(crate) fn shuffle(&self) -> Option<u64> {
        self.shuffle
    }

    pub(crate) fn subblocks(&self) -> &[Subblock] {
        &self.subblocks
    }
}

fn natural(value: &str) -> Result<u64, Error> {
    value
        .parse()
        .map_err(|_| Error::invalid(format!("invalid unsigned integer {value}")))
}

fn checked_subblock_totals(parts: &[Subblock]) -> Result<(u64, u64), Error> {
    parts
        .iter()
        .try_fold((0u64, 0u64), |(stored, decoded), part| {
            Ok((
                stored
                    .checked_add(part.stored)
                    .ok_or_else(|| Error::invalid("compressed subblock byte count overflow"))?,
                decoded
                    .checked_add(part.decoded)
                    .ok_or_else(|| Error::invalid("decoded subblock byte count overflow"))?,
            ))
        })
}

/// A bounded buffered source whose logical consumption excludes read-ahead.
///
/// Exact logical consumption is part of the XISF one-frame rule: the decoder
/// must stop at the end of its first frame with no byte left in the declared
/// compression subblock.
pub(crate) trait BoundedBufRead: BufRead {
    fn logical_len(&self) -> u64;
    fn logical_consumed(&self) -> u64;
}

impl<T: AsRef<[u8]>> BoundedBufRead for Cursor<T> {
    fn logical_len(&self) -> u64 {
        self.get_ref().as_ref().len() as u64
    }

    fn logical_consumed(&self) -> u64 {
        self.position()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZstdFrameRequirements {
    history_bytes: u64,
    window_log: u32,
}

impl ZstdFrameRequirements {
    pub(crate) fn history_bytes(self) -> u64 {
        self.history_bytes
    }
}

/// Inspect the Zstandard frame header without constructing a native decoder.
/// Skippable frames are not XISF compression frames and are rejected here.
pub(crate) fn zstd_frame_requirements(bytes: &[u8]) -> Result<ZstdFrameRequirements, Error> {
    fn little(bytes: &[u8], start: usize, length: usize) -> Result<u64, Error> {
        let end = start
            .checked_add(length)
            .ok_or_else(|| Error::invalid("truncated Zstandard frame header"))?;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| Error::invalid("truncated Zstandard frame header"))?;
        Ok(slice.iter().enumerate().fold(0, |result, (i, byte)| {
            result | (u64::from(*byte) << (8 * i))
        }))
    }

    let magic = little(bytes, 0, 4)?;
    if magic & 0xfffffff0 == 0x184d2a50 {
        return Err(Error::invalid(
            "skippable Zstandard frame is not an XISF compression frame",
        ));
    }
    if magic != 0xfd2fb528 {
        return Err(Error::invalid("invalid Zstandard frame signature"));
    }
    let descriptor = little(bytes, 4, 1)? as u8;
    if descriptor & 8 != 0 {
        return Err(Error::invalid("reserved Zstandard frame flag"));
    }
    let single = descriptor & 32 != 0;
    let dictionary = [0, 1, 2, 4][(descriptor & 3) as usize];
    let content = match descriptor >> 6 {
        0 => usize::from(single),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let content_offset = 5usize
        .checked_add(usize::from(!single))
        .and_then(|offset| offset.checked_add(dictionary))
        .ok_or_else(|| Error::invalid("truncated Zstandard frame header"))?;
    let size = little(bytes, content_offset, content)?;
    let size = if content == 2 {
        size.checked_add(256)
            .ok_or_else(|| Error::limit("Zstandard frame size exceeds address space"))?
    } else {
        size
    };
    let window = if single {
        size
    } else {
        let descriptor = little(bytes, 5, 1)?;
        let exponent = 10 + (descriptor >> 3);
        let base = 1u64
            .checked_shl(exponent as u32)
            .ok_or_else(|| Error::limit("Zstandard window exceeds address space"))?;
        base.checked_add((base / 8) * (descriptor & 7))
            .ok_or_else(|| Error::limit("Zstandard window exceeds address space"))?
    };
    let history_bytes = window
        .max(1024)
        .checked_next_power_of_two()
        .ok_or_else(|| Error::limit("Zstandard history exceeds address space"))?;
    Ok(ZstdFrameRequirements {
        history_bytes,
        window_log: history_bytes.ilog2(),
    })
}

/// Decode exactly one Zstandard frame from exactly one XISF subblock.
pub(crate) fn decode_zstd_frame<R>(
    source: R,
    expected: u64,
    requirements: ZstdFrameRequirements,
    scratch: &mut [u8],
    mut checkpoint: impl FnMut() -> io::Result<()>,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()>
where
    R: BoundedBufRead,
{
    if source.logical_len() == 0 {
        return Err(invalid_data("empty Zstandard subblock"));
    }
    if scratch.is_empty() {
        return Err(invalid_data("empty Zstandard output scratch buffer"));
    }
    let input = source.logical_len();
    let mut decoder = zstd::stream::read::Decoder::with_buffer(source)
        .map_err(|error| invalid_data(format!("Zstandard decoder: {error}")))?
        .single_frame();
    decoder
        .window_log_max(requirements.window_log)
        .map_err(|error| invalid_data(format!("Zstandard window limit: {error}")))?;

    let mut decoded = 0u64;
    loop {
        checkpoint()?;
        let capacity = expected
            .saturating_sub(decoded)
            .saturating_add(1)
            .min(scratch.len() as u64) as usize;
        let count = decoder
            .read(&mut scratch[..capacity])
            .map_err(|error| invalid_data(format!("Zstandard decode: {error}")))?;
        decoded = decoded
            .checked_add(count as u64)
            .ok_or_else(|| invalid_data("Zstandard decoded byte count overflow"))?;
        if decoded > expected {
            return Err(invalid_data(
                "Zstandard decoded payload exceeds declared size",
            ));
        }
        if count == 0 {
            break;
        }
        consume(&scratch[..count])?;
    }
    decoder
        .finish_frame()
        .map_err(|error| invalid_data(format!("truncated Zstandard frame: {error}")))?;
    let source = decoder.finish();
    if source.logical_consumed() != input {
        return Err(invalid_data(
            "trailing data or multiple Zstandard frames in one XISF subblock",
        ));
    }
    if decoded != expected {
        return Err(invalid_data(
            "Zstandard decoded payload shorter than declared size",
        ));
    }
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Reverse XISF byte shuffling over the complete decompressed data block.
pub(crate) fn unshuffle(input: &[u8], item_size: usize) -> Result<Vec<u8>, Error> {
    if item_size == 0 {
        return Err(Error::invalid("zero shuffle item size"));
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if item_size > input.len() {
        return Err(Error::invalid("shuffle item size exceeds payload"));
    }
    let items = input.len() / item_size;
    let complete = items
        .checked_mul(item_size)
        .ok_or_else(|| Error::invalid("shuffle layout overflows address space"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(input.len())
        .map_err(|_| Error::limit("unshuffle output allocation failed"))?;
    output.resize(input.len(), 0);

    for (target, output_byte) in output[..complete].iter_mut().enumerate() {
        let source = unshuffle_source_index(target, item_size, items)?;
        *output_byte = input[source];
    }
    output[complete..].copy_from_slice(&input[complete..]);
    Ok(output)
}

fn unshuffle_source_index(target: usize, item_size: usize, items: usize) -> Result<usize, Error> {
    if item_size == 0 {
        return Err(Error::invalid("zero shuffle item size"));
    }
    let byte = target % item_size;
    let item = target / item_size;
    byte.checked_mul(items)
        .and_then(|plane| plane.checked_add(item))
        .ok_or_else(|| Error::invalid("shuffle source index overflow"))
}

#[cfg(kani)]
mod proofs {
    use super::*;

    /// All possible Zstandard header prefixes up to the maximum 18 bytes read
    /// by the production inspector are panic-free. Any accepted prefix starts
    /// with standard (not skippable) magic and yields a power-of-two history.
    #[kani::proof]
    #[kani::unwind(20)]
    fn zstd_frame_header_is_bounded_and_standard() {
        let bytes: [u8; 18] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());

        if let Ok(requirements) = zstd_frame_requirements(&bytes[..length]) {
            let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            assert_eq!(magic, 0xfd2fb528);
            assert!(requirements.history_bytes() >= 1024);
            assert!(requirements.history_bytes().is_power_of_two());
        }
    }

    /// Checked subblock accumulation agrees with a wider arithmetic oracle for
    /// every list of up to three full-width extents.
    #[kani::proof]
    #[kani::unwind(4)]
    fn subblock_totals_never_wrap() {
        let parts = [
            Subblock {
                stored: kani::any(),
                decoded: kani::any(),
            },
            Subblock {
                stored: kani::any(),
                decoded: kani::any(),
            },
            Subblock {
                stored: kani::any(),
                decoded: kani::any(),
            },
        ];
        let length: usize = kani::any();
        kani::assume(length <= parts.len());
        let selected = &parts[..length];
        let stored = selected
            .iter()
            .fold(0u128, |sum, part| sum + u128::from(part.stored));
        let decoded = selected
            .iter()
            .fold(0u128, |sum, part| sum + u128::from(part.decoded));

        match checked_subblock_totals(selected) {
            Ok((actual_stored, actual_decoded)) => {
                assert!(stored <= u128::from(u64::MAX));
                assert!(decoded <= u128::from(u64::MAX));
                assert_eq!(u128::from(actual_stored), stored);
                assert_eq!(u128::from(actual_decoded), decoded);
            }
            Err(_) => {
                assert!(stored > u128::from(u64::MAX) || decoded > u128::from(u64::MAX));
            }
        }
    }

    /// Every in-range target for bounded valid layouts maps to an in-range
    /// source through the exact index calculation used by production
    /// unshuffle.
    #[kani::proof]
    fn unshuffle_indices_are_in_bounds() {
        let item_size: usize = kani::any();
        let items: usize = kani::any();
        let target: usize = kani::any();
        kani::assume((1..=8).contains(&item_size));
        kani::assume(items <= 8);
        kani::assume(items <= usize::MAX / item_size);
        let complete = items * item_size;
        kani::assume(target < complete);

        let source = unshuffle_source_index(target, item_size, items).unwrap();
        assert!(source < complete);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_subblocks_are_checked_and_accounted_exactly() {
        let compression = Compression::parse(Some("zstd+sh:8:2"), Some("3,3:5,5"), 8)
            .unwrap()
            .unwrap();
        assert_eq!(compression.codec(), "zstd");
        assert_eq!(compression.decoded(), 8);
        assert_eq!(compression.shuffle(), Some(2));
        assert_eq!(compression.subblocks().len(), 2);

        let unknown = Compression::parse(Some("zstd+sh+sh:8:2"), None, 4)
            .unwrap()
            .unwrap();
        assert_eq!(unknown.codec(), "zstd+sh");
        assert_eq!(unknown.shuffle(), Some(2));

        for result in [
            Compression::parse(Some("zstd:8"), Some("3,3:4,5"), 8),
            Compression::parse(Some("zstd:2"), Some("18446744073709551615,1:1,1"), 1),
            Compression::parse(None, Some("1,1"), 1),
            Compression::parse(Some("zstd+sh:8:0"), None, 8),
        ] {
            assert!(result.is_err());
        }
    }

    #[test]
    fn unshuffle_matches_the_specification_golden_pattern_and_tail() {
        assert_eq!(
            unshuffle(&[0x01, 0x03, 0xa0, 0xc0, 0x02, 0x04, 0xb0, 0xd0], 2).unwrap(),
            [0x01, 0x02, 0x03, 0x04, 0xa0, 0xb0, 0xc0, 0xd0]
        );
        assert_eq!(
            unshuffle(&[0, 3, 6, 1, 4, 7, 2, 5, 8, 9], 3).unwrap(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
        assert_eq!(unshuffle(&[], 2).unwrap(), []);
        assert_eq!(unshuffle(&[], usize::MAX).unwrap(), []);
        assert_eq!(unshuffle(&[1, 2, 3], 1).unwrap(), [1, 2, 3]);
        assert!(unshuffle(&[1], 0).is_err());
        assert!(unshuffle(&[1], usize::MAX).is_err());
    }

    #[test]
    fn zstd_frame_header_rejects_skippable_truncated_and_reserved_forms() {
        assert_eq!(
            zstd_frame_requirements(&[0x28, 0xb5, 0x2f, 0xfd, 0x20, 17])
                .unwrap()
                .history_bytes(),
            1024
        );
        assert!(zstd_frame_requirements(&[0x50, 0x2a, 0x4d, 0x18, 0, 0, 0, 0]).is_err());
        assert!(zstd_frame_requirements(&[0x28, 0xb5, 0x2f]).is_err());
        assert!(zstd_frame_requirements(&[0x28, 0xb5, 0x2f, 0xfd, 8, 0]).is_err());
    }
}
