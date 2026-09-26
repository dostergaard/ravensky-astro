use super::*;

use super::super::input::decode_error as error;
use crate::xisf::codec::{decode_zstd_frame, zstd_frame_requirements};

pub(super) fn zlib(
    c: &mut Context<'_>,
    storage: &Storage,
    offset: u64,
    input: u64,
    expected: u64,
) -> Result<()> {
    // Includes inflater history/state; flate2/miniz allocation is internally
    // infallible and the allowance is not an OS/native allocator interception.
    let _codec = c.memory.reserve(1024 * 1024)?;
    let mut output = c.memory.buffer(65536)?;
    let mut source = storage.input(c, offset, input)?;
    let mut decoder = flate2::Decompress::new(true);
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let cap = expected
            .saturating_sub(before_out)
            .saturating_add(1)
            .min(output.len() as u64) as usize;
        let bytes = source.fill_buf().map_err(error)?;
        let status = decoder
            .decompress(bytes, &mut output[..cap], flate2::FlushDecompress::None)
            .map_err(|e| integrity(format!("zlib decode: {e}")))?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let produced = decoder.total_out() - before_out;
        source.consume(consumed);
        if decoder.total_out() > expected {
            return Err(integrity("decoded payload exceeds declared size"));
        }
        if status == flate2::Status::StreamEnd {
            if decoder.total_in() != input {
                return Err(integrity("trailing or unused zlib bytes"));
            }
            if decoder.total_out() != expected {
                return Err(integrity("decoded payload shorter than declared size"));
            }
            return Ok(());
        }
        if consumed == 0 && produced == 0 {
            return Err(integrity("truncated or stalled zlib stream"));
        }
    }
}

pub(super) fn zstd(
    c: &mut Context<'_>,
    storage: &Storage,
    offset: u64,
    input: u64,
    expected: u64,
) -> Result<()> {
    let cancel = c.cancel;
    let memory = c.memory.clone();
    let mut output = memory.buffer(65536)?;
    if input == 0 {
        return Err(integrity("empty Zstandard subblock"));
    }
    c.checkpoint()?;
    c.structure()?;
    let mut source = storage.input(c, offset, input)?;
    let requirements =
        zstd_frame_requirements(source.fill_buf().map_err(error)?).map_err(super::zstd_error)?;
    // Reserve the rounded native history requirement plus context/block
    // scratch before creating the native decoder.
    let _codec = memory.reserve(add(requirements.history_bytes(), 1024 * 1024)?)?;
    decode_zstd_frame(
        source,
        expected,
        requirements,
        &mut output,
        || {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(io::Error::other(ValidationError::new(
                    ValidationErrorKind::Cancelled,
                    "validation cancelled",
                )));
            }
            Ok(())
        },
        |_| Ok(()),
    )
    .map_err(error)
}
