use super::*;

use super::super::input::decode_error as error;

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

// Zstandard format's frame header determines required history before decoder
// allocation. The native decoder still validates the complete header/frame.
// https://github.com/facebook/zstd/blob/v1.5.7/doc/zstd_compression_format.md
fn frame_window(bytes: &[u8]) -> Result<u64> {
    fn little(bytes: &[u8], start: usize, length: usize) -> Result<u64> {
        let slice = bytes
            .get(start..start + length)
            .ok_or_else(|| integrity("truncated Zstandard frame header"))?;
        let mut result = 0;
        for (i, b) in slice.iter().enumerate() {
            result |= u64::from(*b) << (8 * i);
        }
        Ok(result)
    }
    let magic = little(bytes, 0, 4)?;
    if magic & 0xfffffff0 == 0x184d2a50 {
        little(bytes, 4, 4)?;
        return Ok(0);
    }
    if magic != 0xfd2fb528 {
        return Err(integrity(
            "invalid Zstandard frame signature or trailing bytes",
        ));
    }
    let descriptor = little(bytes, 4, 1)? as u8;
    if descriptor & 8 != 0 {
        return Err(integrity("reserved Zstandard frame flag"));
    }
    let single = descriptor & 32 != 0;
    let dictionary = [0, 1, 2, 4][(descriptor & 3) as usize];
    let content = match descriptor >> 6 {
        0 => usize::from(single),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let content_offset = 5 + usize::from(!single) + dictionary;
    let size = little(bytes, content_offset, content)?;
    let size = if content == 2 { size + 256 } else { size };
    if single {
        return Ok(size);
    }
    let window = little(bytes, 5, 1)?;
    let base = 1u64 << (10 + (window >> 3));
    Ok(base + (base / 8) * (window & 7))
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
    let mut position = 0;
    let mut decoded = 0;
    if input == 0 {
        return Err(integrity("empty Zstandard subblock"));
    }
    while position < input {
        c.checkpoint()?;
        c.structure()?;
        let mut source = storage.input(c, offset + position, input - position)?;
        let window = frame_window(source.fill_buf().map_err(error)?)?;
        let history = window
            .max(1024)
            .checked_next_power_of_two()
            .ok_or_else(|| limit("Zstandard history exceeds address space"))?;
        // Round up the configured native window limit and reserve that amount
        // plus context/block scratch before native decoder creation.
        let _codec = memory.reserve(add(history, 1024 * 1024)?)?;
        let mut decoder = zstd::stream::read::Decoder::with_buffer(source)
            .map_err(error)?
            .single_frame();
        decoder
            .window_log_max(history.ilog2())
            .map_err(|e| limit(format!("Zstandard window limit: {e}")))?;
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(ValidationError::new(
                    ValidationErrorKind::Cancelled,
                    "validation cancelled",
                ));
            }
            let cap = expected
                .saturating_sub(decoded)
                .saturating_add(1)
                .min(output.len() as u64) as usize;
            let n = decoder.read(&mut output[..cap]).map_err(error)?;
            decoded = add(decoded, n as u64)?;
            if decoded > expected {
                return Err(integrity("decoded payload exceeds declared size"));
            }
            if n == 0 {
                break;
            }
        }
        decoder.finish_frame().map_err(error)?;
        let source = decoder.finish();
        if source.consumed == 0 {
            return Err(integrity("Zstandard frame made no progress"));
        }
        position = add(position, source.consumed)?;
    }
    if decoded != expected {
        return Err(integrity("decoded payload shorter than declared size"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_history_sizes_cover_single_segment_dictionary_and_window_forms() {
        assert_eq!(
            frame_window(&[0x28, 0xb5, 0x2f, 0xfd, 0x20, 17]).unwrap(),
            17
        );
        assert_eq!(
            frame_window(&[0x28, 0xb5, 0x2f, 0xfd, 0x61, 7, 0, 0]).unwrap(),
            256
        );
        assert_eq!(
            frame_window(&[0x28, 0xb5, 0x2f, 0xfd, 0, 0x53]).unwrap(),
            1024 * 1024 + 3 * 128 * 1024
        );
        assert!(frame_window(&[0x28, 0xb5, 0x2f, 0xfd, 0x60, 1]).is_err());
        assert!(frame_window(&[0x28, 0xb5, 0x2f, 0xfd, 8, 0]).is_err());
    }
}
