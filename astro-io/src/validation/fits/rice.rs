//! Bounded Rice entropy decoding. No pixel array or native decoder is retained.
use super::*;
use crate::validation::input::{decode_error, Input};

struct Bits<R> {
    input: R,
    byte: u8,
    remaining: u32,
    consumed: u64,
}
impl<R: Read> Bits<R> {
    fn bits(&mut self, mut n: u32) -> Result<u32> {
        let mut result = 0u32;
        while n > 0 {
            if self.remaining == 0 {
                let mut byte = [0];
                self.input.read_exact(&mut byte).map_err(decode_error)?;
                self.byte = byte[0];
                self.remaining = 8;
                self.consumed += 1;
            }
            let take = n.min(self.remaining);
            self.remaining -= take;
            result = (result << take) | ((self.byte as u32 >> self.remaining) & ((1 << take) - 1));
            n -= take;
        }
        Ok(result)
    }
    fn unary(&mut self, maximum: u64) -> Result<u64> {
        let mut zeros = 0;
        loop {
            if self.remaining == 0 {
                self.bits(1)?;
                self.remaining += 1;
            }
            let window = (self.byte << (8 - self.remaining)).leading_zeros();
            let available = window.min(self.remaining);
            zeros += available as u64;
            if zeros > maximum {
                return Err(integrity("Rice difference exceeds sample width"));
            }
            self.remaining -= available;
            if available < window || self.remaining == 0 {
                continue;
            }
            self.remaining -= 1;
            return Ok(zeros);
        }
    }
}

fn decode(
    input: impl Read,
    length: u64,
    pixels: u64,
    bytepix: u64,
    block: u64,
    mut emit: impl FnMut(u32),
) -> Result<()> {
    let (fsbits, fsmax, width) = match bytepix {
        1 => (3, 6, 8),
        2 => (4, 14, 16),
        4 => (5, 25, 32),
        _ => return Err(unsupported("Rice BYTEPIX must be 1, 2 or 4")),
    };
    if block == 0 || block > 65536 {
        return Err(invalid("Rice BLOCKSIZE must be in 1..=65536"));
    }
    let mask = u32::MAX >> (32 - width);
    let mut bits = Bits {
        input,
        byte: 0,
        remaining: 0,
        consumed: 0,
    };
    let mut previous = bits.bits(width)?;
    let mut done = 0;
    while done < pixels {
        let code = bits.bits(fsbits)?;
        if code > fsmax + 1 {
            return Err(integrity("invalid Rice coding parameter"));
        }
        for _ in 0..block.min(pixels - done) {
            let difference = if code == 0 {
                0
            } else if code == fsmax + 1 {
                bits.bits(width)?
            } else {
                let fs = code - 1;
                let high = bits.unary((mask >> fs) as u64)? as u32;
                (high << fs) | bits.bits(fs)?
            };
            let signed = if difference & 1 == 0 {
                difference >> 1
            } else {
                !(difference >> 1)
            };
            previous = previous.wrapping_add(signed) & mask;
            emit(previous);
            done += 1;
        }
    }
    if bits.consumed != length || (bits.byte as u32 & ((1 << bits.remaining) - 1)) != 0 {
        return Err(integrity("trailing Rice bytes or nonzero padding bits"));
    }
    Ok(())
}

pub(super) fn validate(
    c: &mut Context<'_>,
    offset: u64,
    length: u64,
    pixels: u64,
    bytepix: u64,
    block: u64,
) -> Result<()> {
    // Input checks cancellation at every bounded read. Zero-difference runs are
    // bounded by BLOCKSIZE, so an arbitrarily large run cannot delay the next read.
    decode(
        Input::attached(c, offset, length)?,
        length,
        pixels,
        bytepix,
        block,
        |_| {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    extern "C" {
        fn fits_rcomp(a: *mut i32, nx: i32, c: *mut u8, clen: i32, nblock: i32) -> i32;
        fn fits_rcomp_short(a: *mut i16, nx: i32, c: *mut u8, clen: i32, nblock: i32) -> i32;
        fn fits_rcomp_byte(a: *mut i8, nx: i32, c: *mut u8, clen: i32, nblock: i32) -> i32;
    }

    #[test]
    fn streaming_rice_matches_native_signed_and_wrapping_pixels() {
        crate::fits::backend::with_cfitsio(|| {
            for bytepix in [1, 2, 4] {
                for block in [1, 16, 32, 67] {
                    for seed in [0u32, 1, 0x12345678] {
                        let mut state = seed;
                        let mut source: Vec<i32> = (0..137)
                            .map(|_| {
                                state ^= state << 13;
                                state ^= state >> 17;
                                state ^= state << 5;
                                state as i32
                            })
                            .collect();
                        let mut shorts: Vec<i16> = source.iter().map(|&n| n as i16).collect();
                        let mut bytes: Vec<i8> = source.iter().map(|&n| n as i8).collect();
                        let mut compressed = vec![0; source.len() * 8 + 1024];
                        // SAFETY: positive bounded array/block lengths and output
                        // capacity are supplied to the test-only native encoder.
                        let count = unsafe {
                            match bytepix {
                                1 => fits_rcomp_byte(
                                    bytes.as_mut_ptr(),
                                    137,
                                    compressed.as_mut_ptr(),
                                    compressed.len() as i32,
                                    block,
                                ),
                                2 => fits_rcomp_short(
                                    shorts.as_mut_ptr(),
                                    137,
                                    compressed.as_mut_ptr(),
                                    compressed.len() as i32,
                                    block,
                                ),
                                _ => fits_rcomp(
                                    source.as_mut_ptr(),
                                    137,
                                    compressed.as_mut_ptr(),
                                    compressed.len() as i32,
                                    block,
                                ),
                            }
                        };
                        assert!(count > 0);
                        compressed.truncate(count as usize);
                        let mut output = Vec::new();
                        decode(
                            &compressed[..],
                            count as u64,
                            137,
                            bytepix,
                            block as u64,
                            |v| output.push(v),
                        )
                        .unwrap();
                        let mask = u32::MAX >> (32 - bytepix * 8);
                        assert_eq!(
                            output,
                            source.iter().map(|&n| n as u32 & mask).collect::<Vec<_>>()
                        );
                        for end in 0..compressed.len() {
                            assert!(decode(
                                &compressed[..end],
                                end as u64,
                                137,
                                bytepix,
                                block as u64,
                                |_| {}
                            )
                            .is_err());
                        }
                        compressed.push(0);
                        assert!(decode(
                            &compressed[..],
                            compressed.len() as u64,
                            137,
                            bytepix,
                            block as u64,
                            |_| {}
                        )
                        .is_err());
                    }
                }
            }
        });
    }
}
