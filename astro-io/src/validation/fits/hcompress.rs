//! Bounded HCOMPRESS decoder adapted from fitskit 0.3.0 (MIT).
//! Copyright (c) 2026 Steven Michael. See ../../../licenses/fitskit-MIT.txt.
//! Original algorithm: R. White/STScI and CFITSIO; notices retained alongside it.
//! Changes: admission before allocation, fallible growth, geometry/bitplane/input
//! checks, exact stream consumption, integer log2 and cooperative cancellation.
use super::*;

fn allocate<T: Default + Clone>(n: usize, input: &HcInput<'_>) -> Result<Vec<T>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(n)
        .map_err(|_| limit("HCOMPRESS allocation failed"))?;
    while result.len() < n {
        input.checkpoint()?;
        result.resize((result.len() + 4096).min(n), T::default());
    }
    Ok(result)
}

pub(super) fn validate(
    c: &mut Context<'_>,
    offset: u64,
    length: u64,
    dimensions: [u64; 2],
    wide: bool,
) -> Result<()> {
    if length < 25 {
        return Err(integrity("truncated HCOMPRESS header"));
    }
    let mut header = [0; 25];
    c.read(offset, &mut header)?;
    if header[..2] != [0xdd, 0x99] {
        return Err(integrity("HCOMPRESS magic mismatch"));
    }
    let nx = i32::from_be_bytes(
        header[2..6]
            .try_into()
            .map_err(|_| invalid("HCOMPRESS header"))?,
    );
    let ny = i32::from_be_bytes(
        header[6..10]
            .try_into()
            .map_err(|_| invalid("HCOMPRESS header"))?,
    );
    let scale = i32::from_be_bytes(
        header[10..14]
            .try_into()
            .map_err(|_| invalid("HCOMPRESS header"))?,
    );
    if nx <= 0 || ny <= 0 || [ny as u64, nx as u64] != dimensions || scale < 0 {
        return Err(integrity(
            "HCOMPRESS stream geometry/scale disagrees with tile",
        ));
    }
    let width = if wide { 64 } else { 32 };
    if header[22..25].iter().any(|&n| n > width) {
        return Err(integrity(
            "HCOMPRESS bitplane count exceeds coefficient width",
        ));
    }
    // Internal indexing and transform shifts remain representable. A larger
    // working set would already exceed normal limits; reject before allocation.
    if (nx as u32).max(ny as u32) > (1 << 28) {
        return Err(limit(
            "HCOMPRESS dimension exceeds supported transform range",
        ));
    }
    let pixels = mul(nx as u64, ny as u64)?;
    // Covers coefficients (<=8/pixel), quadrant scratch, and inverse-transform
    // scratch, including allocation lifetimes and rounding. No native allocations.
    let _working = c.memory.reserve(add(mul(pixels, 16)?, 65536)?)?;
    let stored =
        usize::try_from(length).map_err(|_| limit("HCOMPRESS input exceeds address space"))?;
    let mut bytes = c.memory.buffer(stored)?;
    for (index, chunk) in bytes.chunks_mut(65536).enumerate() {
        c.read(add(offset, mul(index as u64, 65536)?)?, chunk)?;
    }
    // Admission used the first header observation. Never let an intervening
    // writer substitute new dimensions before the allocating decoder sees it.
    if bytes[..25] != header {
        return Err(ValidationError::new(
            ValidationErrorKind::ChangedDuringValidation,
            "HCOMPRESS header changed after resource admission",
        ));
    }
    let mut input = HcInput::new(&bytes, c.cancel);
    if wide {
        hdecode64(&mut input)?;
    } else {
        hdecode32(&mut input)?;
    }
    if input.nextchar != bytes.len() || (input.buffer2 & ((1 << input.bits_to_go) - 1)) != 0 {
        return Err(integrity("trailing HCOMPRESS bytes or nonzero padding"));
    }
    c.checkpoint()
}

/// Bit/byte reader for the HCOMPRESS stream (mirrors cfitsio's global
/// `nextchar` + `buffer2`/`bits_to_go` state machine, but as a struct).
struct HcInput<'a> {
    data: &'a [u8],
    nextchar: usize,
    buffer2: i32,
    bits_to_go: i32,
    cancel: Option<&'a AtomicBool>,
}

impl<'a> HcInput<'a> {
    fn new(data: &'a [u8], cancel: Option<&'a AtomicBool>) -> Self {
        HcInput {
            data,
            nextchar: 0,
            buffer2: 0,
            bits_to_go: 0,
            cancel,
        }
    }

    fn checkpoint(&self) -> Result<()> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            Err(ValidationError::new(
                ValidationErrorKind::Cancelled,
                "HCOMPRESS cancelled",
            ))
        } else {
            Ok(())
        }
    }
    #[inline]
    fn next_byte(&mut self) -> Result<i32> {
        if self.nextchar.is_multiple_of(4096) {
            self.checkpoint()?;
        }
        let b = *self
            .data
            .get(self.nextchar)
            .ok_or_else(|| integrity("HCOMPRESS: unexpected end of stream"))?;
        self.nextchar += 1;
        Ok(b as i32)
    }

    /// Read `n` raw bytes (no bit buffering); cfitsio `qread`.
    fn qread(&mut self, n: usize) -> Result<&[u8]> {
        let start = self.nextchar;
        let end = start + n;
        if end > self.data.len() {
            return Err(integrity("HCOMPRESS: unexpected end of stream (qread)"));
        }
        self.nextchar = end;
        Ok(&self.data[start..end])
    }

    /// Read a big-endian 4-byte int (cfitsio `readint`).
    fn readint(&mut self) -> Result<i32> {
        let b = self.qread(4)?;
        let mut a = b[0] as i32;
        for &x in &b[1..4] {
            a = (a << 8) + x as i32;
        }
        Ok(a)
    }

    /// Read a big-endian 8-byte long long (cfitsio `readlonglong`).
    fn readlonglong(&mut self) -> Result<i64> {
        let b = self.qread(8)?;
        let mut a = b[0] as i64;
        for &x in &b[1..8] {
            a = (a << 8) + x as i64;
        }
        Ok(a)
    }

    fn start_inputing_bits(&mut self) {
        self.bits_to_go = 0;
    }

    fn input_bit(&mut self) -> Result<i32> {
        if self.bits_to_go == 0 {
            self.buffer2 = self.next_byte()?;
            self.bits_to_go = 8;
        }
        self.bits_to_go -= 1;
        Ok((self.buffer2 >> self.bits_to_go) & 1)
    }

    fn input_nbits(&mut self, n: i32) -> Result<i32> {
        if self.bits_to_go < n {
            self.buffer2 = (self.buffer2 << 8) | self.next_byte()?;
            self.bits_to_go += 8;
        }
        self.bits_to_go -= n;
        Ok((self.buffer2 >> self.bits_to_go) & ((1 << n) - 1))
    }

    #[inline]
    fn input_nybble(&mut self) -> Result<i32> {
        self.input_nbits(4)
    }

    /// Read `n` 4-bit nybbles into `array` (cfitsio `input_nnybble`).
    fn input_nnybble(&mut self, n: usize, array: &mut [u8]) -> Result<()> {
        if n == 1 {
            array[0] = self.input_nybble()? as u8;
            return Ok(());
        }
        if self.bits_to_go == 8 {
            // Backspace to reuse the last char (cfitsio quirk).
            self.nextchar -= 1;
            self.bits_to_go = 0;
        }
        let shift1 = self.bits_to_go + 4;
        let shift2 = self.bits_to_go;
        let mut kk = 0usize;
        let mut ii = 0usize;
        if self.bits_to_go == 0 {
            while ii < n / 2 {
                self.buffer2 = (self.buffer2 << 8) | self.next_byte()?;
                array[kk] = ((self.buffer2 >> 4) & 15) as u8;
                array[kk + 1] = (self.buffer2 & 15) as u8;
                kk += 2;
                ii += 1;
            }
        } else {
            while ii < n / 2 {
                self.buffer2 = (self.buffer2 << 8) | self.next_byte()?;
                array[kk] = ((self.buffer2 >> shift1) & 15) as u8;
                array[kk + 1] = ((self.buffer2 >> shift2) & 15) as u8;
                kk += 2;
                ii += 1;
            }
        }
        if ii * 2 != n {
            array[n - 1] = self.input_nybble()? as u8;
        }
        Ok(())
    }

    /// Huffman decode of a 4-bit code (cfitsio `input_huffman`).
    fn input_huffman(&mut self) -> Result<i32> {
        let mut c = self.input_nbits(3)?;
        if c < 4 {
            return Ok(1 << c);
        }
        c = self.input_bit()? | (c << 1);
        if c < 13 {
            match c {
                8 => return Ok(3),
                9 => return Ok(5),
                10 => return Ok(10),
                11 => return Ok(12),
                12 => return Ok(15),
                _ => {}
            }
        }
        c = self.input_bit()? | (c << 1);
        if c < 31 {
            match c {
                26 => return Ok(6),
                27 => return Ok(7),
                28 => return Ok(9),
                29 => return Ok(11),
                30 => return Ok(13),
                _ => {}
            }
        }
        c = self.input_bit()? | (c << 1);
        if c == 62 {
            Ok(0)
        } else {
            Ok(14)
        }
    }
}

/// Expand 4-bit quadtree values from `a[(nx+1)/2,(ny+1)/2]` into `b[nx,ny]`
/// (2x2 per value); cfitsio `qtree_copy`. `a` and `b` are the same buffer here, so
/// we operate in place exactly as the C does (iterating from the end first).
fn qtree_copy(buf: &mut [u8], nx: usize, ny: usize, n: usize, input: &HcInput<'_>) -> Result<()> {
    if nx == 0 || ny == 0 {
        return Ok(());
    }
    let nx2 = nx.div_ceil(2);
    let ny2 = ny.div_ceil(2);
    // Copy 4-bit values to b, from the end (a,b same array).
    // k is index of a[i,j]; s00 is index of b[2*i,2*j].
    let mut k = (ny2 * (nx2 - 1) + ny2 - 1) as isize;
    for i in (0..nx2).rev() {
        input.checkpoint()?;
        let mut s00 = (2 * (n * i + ny2 - 1)) as isize;
        for j in (0..ny2).rev() {
            if j.is_multiple_of(4096) {
                input.checkpoint()?;
            }
            buf[s00 as usize] = buf[k as usize];
            k -= 1;
            s00 -= 2;
        }
    }
    // Expand each 2x2 block. Mapping: bit3->b[s00], bit2->b[s00+1],
    // bit1->b[s10], bit0->b[s10+1] where s10 = s00+n.
    let mut i = 0usize;
    while i + 1 < nx {
        input.checkpoint()?;
        let mut s00 = n * i;
        let s10base = s00 + n;
        let mut s10 = s10base;
        let mut j = 0usize;
        while j + 1 < ny {
            if j.is_multiple_of(4096) {
                input.checkpoint()?;
            }
            let v = buf[s00];
            buf[s10 + 1] = v & 1;
            buf[s10] = (v >> 1) & 1;
            buf[s00 + 1] = (v >> 2) & 1;
            buf[s00] = (v >> 3) & 1;
            s00 += 2;
            s10 += 2;
            j += 2;
        }
        if j < ny {
            // odd row length
            let v = buf[s00];
            buf[s10] = (v >> 1) & 1;
            buf[s00] = (v >> 3) & 1;
        }
        i += 2;
    }
    if i < nx {
        // odd column length: last row, s10 off edge
        let mut s00 = n * i;
        let mut j = 0usize;
        while j + 1 < ny {
            if j.is_multiple_of(4096) {
                input.checkpoint()?;
            }
            let v = buf[s00];
            buf[s00 + 1] = (v >> 2) & 1;
            buf[s00] = (v >> 3) & 1;
            s00 += 2;
            j += 2;
        }
        if j < ny {
            let v = buf[s00];
            buf[s00] = (v >> 3) & 1;
        }
    }
    Ok(())
}

/// One quadtree expansion step (cfitsio `qtree_expand`): copy+expand then read a
/// fresh Huffman code into every non-zero element (scanning from the end).
fn qtree_expand(input: &mut HcInput, buf: &mut [u8], nx: usize, ny: usize) -> Result<()> {
    qtree_copy(buf, nx, ny, ny, input)?;
    for i in (0..nx * ny).rev() {
        if i.is_multiple_of(4096) {
            input.checkpoint()?;
        }
        if buf[i] != 0 {
            buf[i] = input.input_huffman()? as u8;
        }
    }
    Ok(())
}

macro_rules! impl_hdecompress {
    ($name:ident, $t:ty, $bitins:ident, $read_bdirect:ident, $qtree_decode:ident,
     $dodecode:ident, $hinv:ident, $undigitize:ident, $unshuffle:ident) => {
        /// Distribute even/odd interleaved coefficients (cfitsio `unshuffle`).
        /// `offset` is the base index into `a`; pointer arithmetic is done in
        /// `isize` so the trailing (unused) decrements can go negative as in C.
        fn $unshuffle(
            a: &mut [$t],
            offset: usize,
            n: usize,
            n2: usize,
            tmp: &mut [$t],
            input: &HcInput<'_>,
        ) -> Result<()> {
            let base = offset as isize;
            let n2i = n2 as isize;
            let nhalf = (n + 1) >> 1;
            // copy 2nd half of array to tmp
            let mut p1 = base + n2i * nhalf as isize;
            for (index, slot) in tmp.iter_mut().take(n - nhalf).enumerate() {
                if index.is_multiple_of(4096) {
                    input.checkpoint()?;
                }
                *slot = a[p1 as usize];
                p1 += n2i;
            }
            // distribute 1st half to even elements (descending)
            let mut p2 = base + n2i * (nhalf as isize - 1);
            let mut p1e = base + ((n2i * (nhalf as isize - 1)) << 1);
            let mut i = nhalf as isize - 1;
            while i >= 0 {
                if i % 4096 == 0 {
                    input.checkpoint()?;
                }
                a[p1e as usize] = a[p2 as usize];
                p2 -= n2i;
                p1e -= n2i + n2i;
                i -= 1;
            }
            // distribute 2nd half (tmp) to odd elements
            let mut p1o = base + n2i;
            let mut pt = 0usize;
            let mut i = 1usize;
            while i < n {
                if i.is_multiple_of(4096) {
                    input.checkpoint()?;
                }
                a[p1o as usize] = tmp[pt];
                p1o += n2i + n2i;
                pt += 1;
                i += 2;
            }
            Ok(())
        }

        /// Insert expanded 4-bit codes from `aa[(nx+1)/2,(ny+1)/2]` into bitplane
        /// `bit` of `b[nx,ny]` (cfitsio `qtree_bitins`).
        fn $bitins(
            aa: &[u8],
            nx: usize,
            ny: usize,
            b: &mut [$t],
            n: usize,
            bit: i32,
            input: &HcInput<'_>,
        ) -> Result<()> {
            if nx == 0 || ny == 0 {
                return Ok(());
            }
            let plane_val: $t = (1 as $t) << bit;
            let mut k = 0usize;
            let mut i = 0usize;
            while i + 1 < nx {
                input.checkpoint()?;
                let s00 = n * i;
                let mut s00 = s00;
                let mut j = 0usize;
                while j + 1 < ny {
                    if j.is_multiple_of(4096) {
                        input.checkpoint()?;
                    }
                    let v = aa[k];
                    if v & 1 != 0 {
                        b[s00 + n + 1] |= plane_val;
                    }
                    if v & 2 != 0 {
                        b[s00 + n] |= plane_val;
                    }
                    if v & 4 != 0 {
                        b[s00 + 1] |= plane_val;
                    }
                    if v & 8 != 0 {
                        b[s00] |= plane_val;
                    }
                    s00 += 2;
                    k += 1;
                    j += 2;
                }
                if j < ny {
                    // odd row: s00+1, s10+1 off edge -> only bits 1 (s10) and 3 (s00)
                    let v = aa[k];
                    if v & 2 != 0 {
                        b[s00 + n] |= plane_val;
                    }
                    if v & 8 != 0 {
                        b[s00] |= plane_val;
                    }
                    k += 1;
                }
                i += 2;
            }
            if i < nx {
                // odd column: last row, s10 off edge -> bits 2 (s00+1) and 3 (s00)
                let mut s00 = n * i;
                let mut j = 0usize;
                while j + 1 < ny {
                    if j.is_multiple_of(4096) {
                        input.checkpoint()?;
                    }
                    let v = aa[k];
                    if v & 4 != 0 {
                        b[s00 + 1] |= plane_val;
                    }
                    if v & 8 != 0 {
                        b[s00] |= plane_val;
                    }
                    s00 += 2;
                    k += 1;
                    j += 2;
                }
                if j < ny {
                    // corner: only bit 3 (s00)
                    let v = aa[k];
                    if v & 8 != 0 {
                        b[s00] |= plane_val;
                    }
                    k += 1;
                }
            }
            let _ = k;
            Ok(())
        }

        /// Read a directly-stored bit plane and insert it (cfitsio `read_bdirect`).
        fn $read_bdirect(
            input: &mut HcInput,
            a: &mut [$t],
            n: usize,
            nqx: usize,
            nqy: usize,
            scratch: &mut [u8],
            bit: i32,
        ) -> Result<()> {
            let cnt = nqx.div_ceil(2) * nqy.div_ceil(2);
            input.input_nnybble(cnt, scratch)?;
            $bitins(scratch, nqx, nqy, a, n, bit, input)?;
            Ok(())
        }

        /// Decode the quadtree-coded bit planes of one quadrant (cfitsio
        /// `qtree_decode`).
        fn $qtree_decode(
            input: &mut HcInput,
            a: &mut [$t],
            a_off: usize,
            n: usize,
            nqx: usize,
            nqy: usize,
            nbitplanes: i32,
        ) -> Result<()> {
            let nqmax = nqx.max(nqy);
            let log2n = (usize::BITS - nqmax.max(1).saturating_sub(1).leading_zeros()) as i32;
            let nqx2 = nqx.div_ceil(2);
            let nqy2 = nqy.div_ceil(2);
            let mut scratch = allocate(nqx2 * nqy2 + 4, input)?;

            let asl = if nqx == 0 || nqy == 0 {
                &mut a[..0]
            } else {
                &mut a[a_off..]
            };

            let mut bit = nbitplanes - 1;
            while bit >= 0 {
                input.checkpoint()?;
                let b = input.input_nybble()?;
                if b == 0 {
                    $read_bdirect(input, asl, n, nqx, nqy, &mut scratch, bit)?;
                } else if b != 0xf {
                    return Err(integrity("qtree_decode: bad format code"));
                } else {
                    scratch[0] = input.input_huffman()? as u8;
                    let mut nx = 1usize;
                    let mut ny = 1usize;
                    let mut nfx = nqx;
                    let mut nfy = nqy;
                    let mut c = 1usize << log2n;
                    let mut k = 1;
                    while k < log2n {
                        c >>= 1;
                        nx <<= 1;
                        ny <<= 1;
                        if nfx <= c {
                            nx -= 1;
                        } else {
                            nfx -= c;
                        }
                        if nfy <= c {
                            ny -= 1;
                        } else {
                            nfy -= c;
                        }
                        qtree_expand(input, &mut scratch, nx, ny)?;
                        k += 1;
                    }
                    $bitins(&scratch, nqx, nqy, asl, n, bit, input)?;
                }
                bit -= 1;
            }
            Ok(())
        }

        /// Decode the four quadrants into coefficient array `a` (cfitsio
        /// `dodecode`).
        fn $dodecode(
            input: &mut HcInput,
            a: &mut [$t],
            nx: usize,
            ny: usize,
            nbitplanes: [u8; 3],
        ) -> Result<()> {
            let nel = nx * ny;
            let nx2 = nx.div_ceil(2);
            let ny2 = ny.div_ceil(2);
            // The admitted allocation is already zeroed in cancellable chunks.
            input.start_inputing_bits();
            $qtree_decode(input, a, 0, ny, nx2, ny2, nbitplanes[0] as i32)?;
            $qtree_decode(input, a, ny2, ny, nx2, ny / 2, nbitplanes[1] as i32)?;
            $qtree_decode(input, a, ny * nx2, ny, nx / 2, ny2, nbitplanes[1] as i32)?;
            $qtree_decode(
                input,
                a,
                ny * nx2 + ny2,
                ny,
                nx / 2,
                ny / 2,
                nbitplanes[2] as i32,
            )?;
            if input.input_nybble()? != 0 {
                return Err(integrity("dodecode: bad bit plane values (missing EOF)"));
            }
            if input.buffer2 & ((1 << input.bits_to_go) - 1) != 0 {
                return Err(integrity("nonzero HCOMPRESS bitplane padding"));
            }
            // sign bits
            input.start_inputing_bits();
            for (index, v) in a.iter_mut().take(nel).enumerate() {
                if index.is_multiple_of(4096) {
                    input.checkpoint()?;
                }
                if *v != 0 as $t && input.input_bit()? != 0 {
                    *v = (0 as $t).wrapping_sub(*v);
                }
            }
            Ok(())
        }

        fn $undigitize(a: &mut [$t], nel: usize, scale: i32, input: &HcInput<'_>) -> Result<()> {
            if scale <= 1 {
                return Ok(());
            }
            let s = scale as $t;
            for (index, v) in a.iter_mut().take(nel).enumerate() {
                if index.is_multiple_of(4096) {
                    input.checkpoint()?;
                }
                *v = (*v).wrapping_mul(s);
            }
            Ok(())
        }

        /// Inverse H-transform (cfitsio `hinv`). `smooth` is unsupported (the
        /// fixtures use SMOOTH=0); a non-zero value is rejected by the caller.
        fn $hinv(a: &mut [$t], nx: usize, ny: usize, input: &HcInput<'_>) -> Result<()> {
            let nmax = nx.max(ny);
            if nmax == 1 {
                return Ok(());
            }
            let log2n = (usize::BITS - (nmax - 1).leading_zeros()) as i32;
            let nmax_i = nmax;
            let mut tmp = allocate(nmax_i.div_ceil(2) + 1, input)?;

            let mut shift: i32 = 1;
            let mut bit0: $t = (1 as $t) << (log2n - 1);
            let mut bit1: $t = bit0 << 1;
            let mut bit2: $t = bit0 << 2;
            let mut mask0: $t = (0 as $t).wrapping_sub(bit0);
            let mut mask1: $t = mask0 << 1;
            let mask2: $t = mask0 << 2;
            let mut prnd0: $t = bit0 >> 1;
            let mut prnd1: $t = bit1 >> 1;
            let prnd2: $t = bit2 >> 1;
            let mut nrnd0: $t = prnd0 - 1;
            let mut nrnd1: $t = prnd1 - 1;
            let nrnd2: $t = prnd2 - 1;

            // round h0 to multiple of bit2
            a[0] = (a[0].wrapping_add(if a[0] >= 0 as $t { prnd2 } else { nrnd2 })) & mask2;

            let ny_i = ny as isize;
            let mut nxtop = 1usize;
            let mut nytop = 1usize;
            let mut nxf = nx;
            let mut nyf = ny;
            let mut c = 1usize << log2n;
            let mut k = log2n - 1;
            while k >= 0 {
                c >>= 1;
                nxtop <<= 1;
                nytop <<= 1;
                if nxf <= c {
                    nxtop -= 1;
                } else {
                    nxf -= c;
                }
                if nyf <= c {
                    nytop -= 1;
                } else {
                    nyf -= c;
                }
                if k == 0 {
                    nrnd0 = 0 as $t;
                    shift = 2;
                }
                // unshuffle in each dimension
                for i in 0..nxtop {
                    input.checkpoint()?;
                    $unshuffle(a, ny * i, nytop, 1, &mut tmp, input)?;
                }
                for j in 0..nytop {
                    input.checkpoint()?;
                    $unshuffle(a, j, nxtop, ny, &mut tmp, input)?;
                }
                let oddx = nxtop % 2;
                let oddy = nytop % 2;
                let mut i = 0usize;
                while i + oddx < nxtop {
                    // i steps by 2 over 0..nxtop-oddx
                    let mut s00 = (ny * i) as isize;
                    let mut s10 = s00 + ny_i;
                    let mut j = 0usize;
                    while j + oddy < nytop {
                        if j.is_multiple_of(4096) {
                            input.checkpoint()?;
                        }
                        let mut h0 = a[s00 as usize];
                        let mut hx = a[s10 as usize];
                        let mut hy = a[(s00 + 1) as usize];
                        let mut hc = a[(s10 + 1) as usize];
                        hx = (hx.wrapping_add(if hx >= 0 as $t { prnd1 } else { nrnd1 })) & mask1;
                        hy = (hy.wrapping_add(if hy >= 0 as $t { prnd1 } else { nrnd1 })) & mask1;
                        hc = (hc.wrapping_add(if hc >= 0 as $t { prnd0 } else { nrnd0 })) & mask0;
                        let lowbit0 = hc & bit0;
                        hx = if hx >= 0 as $t {
                            hx.wrapping_sub(lowbit0)
                        } else {
                            hx.wrapping_add(lowbit0)
                        };
                        hy = if hy >= 0 as $t {
                            hy.wrapping_sub(lowbit0)
                        } else {
                            hy.wrapping_add(lowbit0)
                        };
                        let lowbit1 = (hc ^ hx ^ hy) & bit1;
                        h0 = if h0 >= 0 as $t {
                            h0.wrapping_add(lowbit0).wrapping_sub(lowbit1)
                        } else {
                            h0.wrapping_add(if lowbit0 == 0 as $t {
                                lowbit1
                            } else {
                                lowbit0.wrapping_sub(lowbit1)
                            })
                        };
                        a[(s10 + 1) as usize] =
                            (h0.wrapping_add(hx).wrapping_add(hy).wrapping_add(hc)) >> shift;
                        a[s10 as usize] =
                            (h0.wrapping_add(hx).wrapping_sub(hy).wrapping_sub(hc)) >> shift;
                        a[(s00 + 1) as usize] =
                            (h0.wrapping_sub(hx).wrapping_add(hy).wrapping_sub(hc)) >> shift;
                        a[s00 as usize] =
                            (h0.wrapping_sub(hx).wrapping_sub(hy).wrapping_add(hc)) >> shift;
                        s00 += 2;
                        s10 += 2;
                        j += 2;
                    }
                    if oddy != 0 {
                        let mut h0 = a[s00 as usize];
                        let mut hx = a[s10 as usize];
                        hx = (hx.wrapping_add(if hx >= 0 as $t { prnd1 } else { nrnd1 })) & mask1;
                        let lowbit1 = hx & bit1;
                        h0 = if h0 >= 0 as $t {
                            h0.wrapping_sub(lowbit1)
                        } else {
                            h0.wrapping_add(lowbit1)
                        };
                        a[s10 as usize] = (h0.wrapping_add(hx)) >> shift;
                        a[s00 as usize] = (h0.wrapping_sub(hx)) >> shift;
                    }
                    i += 2;
                }
                if oddx != 0 {
                    let mut s00 = (ny * i) as isize;
                    let mut j = 0usize;
                    while j + oddy < nytop {
                        if j.is_multiple_of(4096) {
                            input.checkpoint()?;
                        }
                        let mut h0 = a[s00 as usize];
                        let mut hy = a[(s00 + 1) as usize];
                        hy = (hy.wrapping_add(if hy >= 0 as $t { prnd1 } else { nrnd1 })) & mask1;
                        let lowbit1 = hy & bit1;
                        h0 = if h0 >= 0 as $t {
                            h0.wrapping_sub(lowbit1)
                        } else {
                            h0.wrapping_add(lowbit1)
                        };
                        a[(s00 + 1) as usize] = (h0.wrapping_add(hy)) >> shift;
                        a[s00 as usize] = (h0.wrapping_sub(hy)) >> shift;
                        s00 += 2;
                        j += 2;
                    }
                    if oddy != 0 {
                        let h0 = a[s00 as usize];
                        a[s00 as usize] = h0 >> shift;
                    }
                }
                // divide masks/rounding by 2
                bit2 = bit1;
                bit1 = bit0;
                bit0 >>= 1;
                mask1 = mask0;
                mask0 >>= 1;
                prnd1 = prnd0;
                prnd0 >>= 1;
                nrnd1 = nrnd0;
                nrnd0 = prnd0 - 1;
                k -= 1;
            }
            let _ = (bit2, mask1, prnd1, nrnd1);
            Ok(())
        }

        /// Full HCOMPRESS decode for one quadrant-int width. Returns the pixel
        /// array (axis-1 fastest) along with `(nx_slow, ny_fast)`.
        fn $name(input: &mut HcInput) -> Result<(Vec<$t>, usize, usize)> {
            // magic code
            let magic = input.qread(2)?;
            if magic != [0xDDu8, 0x99u8] {
                return Err(integrity("HCOMPRESS: bad magic code"));
            }
            let nx = input.readint()? as usize; // slow axis
            let ny = input.readint()? as usize; // fast axis
            let scale = input.readint()?;
            let nel = nx
                .checked_mul(ny)
                .ok_or_else(|| integrity("HCOMPRESS: dimension overflow"))?;
            let sumall = input.readlonglong()?;
            let nbp = input.qread(3)?;
            let nbitplanes = [nbp[0], nbp[1], nbp[2]];

            let mut a = allocate(nel, input)?;
            $dodecode(input, &mut a, nx, ny, nbitplanes)?;
            // put sum of all pixels back into pixel 0
            a[0] = sumall as $t;

            $undigitize(&mut a, nel, scale, input)?;
            $hinv(&mut a, nx, ny, input)?;
            Ok((a, nx, ny))
        }
    };
}

impl_hdecompress!(
    hdecode32,
    i32,
    qtree_bitins32,
    read_bdirect32,
    qtree_decode32,
    dodecode32,
    hinv32,
    undigitize32,
    unshuffle32
);
impl_hdecompress!(
    hdecode64,
    i64,
    qtree_bitins64,
    read_bdirect64,
    qtree_decode64,
    dodecode64,
    hinv64,
    undigitize64,
    unshuffle64
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_hcompress_roundtrips_signed_samples_and_odd_geometry() {
        crate::fits::backend::with_cfitsio(|| {
            for (width, height) in [(4, 4), (5, 7), (33, 17), (1, 10), (10, 1)] {
                let original: Vec<i32> = (0..width * height)
                    .map(|i| ((i * 7919 + 31) % 65536) - 32768)
                    .collect();
                for wide in [false, true] {
                    let mut narrow = original.clone();
                    let mut broad: Vec<i64> = original.iter().map(|&v| v as i64).collect();
                    let mut compressed = vec![0u8; original.len() * 32 + 1024];
                    let mut length = compressed.len() as std::os::raw::c_long;
                    let mut status = 0;
                    // SAFETY: owned input arrays match the positive dimensions;
                    // output capacity is a conservative bound for these fixtures.
                    unsafe {
                        if wide {
                            fitsio::sys::fits_hcompress64(
                                broad.as_mut_ptr(),
                                width,
                                height,
                                0,
                                compressed.as_mut_ptr().cast(),
                                &mut length,
                                &mut status,
                            );
                        } else {
                            fitsio::sys::fits_hcompress(
                                narrow.as_mut_ptr(),
                                width,
                                height,
                                0,
                                compressed.as_mut_ptr().cast(),
                                &mut length,
                                &mut status,
                            );
                        }
                    }
                    assert_eq!(status, 0);
                    compressed.truncate(length as usize);
                    let mut input = HcInput::new(&compressed, None);
                    let (decoded, x, y) = if wide {
                        let (data, x, y) = hdecode64(&mut input).unwrap();
                        (data.into_iter().map(|v| v as i32).collect::<Vec<_>>(), x, y)
                    } else {
                        hdecode32(&mut input).unwrap()
                    };
                    assert_eq!((y, x), (width as usize, height as usize));
                    assert_eq!(decoded, original, "{width}x{height} wide={wide}");
                    assert_eq!(input.nextchar, compressed.len());
                    for end in 25..compressed.len() {
                        let mut input = HcInput::new(&compressed[..end], None);
                        let result = if wide {
                            hdecode64(&mut input).map(|_| ())
                        } else {
                            hdecode32(&mut input).map(|_| ())
                        };
                        assert!(result.is_err(), "truncation {end} of {}", compressed.len());
                    }
                }
            }
        });
    }

    #[test]
    fn lossy_transform_matches_native_and_observes_cancellation() {
        crate::fits::backend::with_cfitsio(|| {
            let mut source: Vec<i32> = (0..64).map(|i| (i * 1237) % 16384 - 8192).collect();
            let mut compressed = vec![0u8; 4096];
            let mut length = compressed.len() as std::os::raw::c_long;
            let mut status = 0;
            let mut expected = vec![0i32; 64];
            let (mut width, mut height, mut scale) = (0, 0, 0);
            // SAFETY: small owned encoder inputs/output; the native decoder sees
            // only its own valid encoded stream and a correctly sized output.
            unsafe {
                fitsio::sys::fits_hcompress(
                    source.as_mut_ptr(),
                    8,
                    8,
                    3,
                    compressed.as_mut_ptr().cast(),
                    &mut length,
                    &mut status,
                );
                assert_eq!(status, 0);
                fitsio::sys::fits_hdecompress(
                    compressed.as_mut_ptr(),
                    0,
                    expected.as_mut_ptr(),
                    &mut width,
                    &mut height,
                    &mut scale,
                    &mut status,
                );
            }
            assert_eq!(status, 0);
            assert_eq!((width, height, scale), (8, 8, 3));
            compressed.truncate(length as usize);
            let mut input = HcInput::new(&compressed, None);
            assert_eq!(hdecode32(&mut input).unwrap().0, expected);
            let cancel = AtomicBool::new(true);
            let mut input = HcInput::new(&compressed, Some(&cancel));
            assert_eq!(
                hdecode32(&mut input).unwrap_err().kind(),
                ValidationErrorKind::Cancelled
            );
            assert_eq!(
                hinv32(&mut expected, 8, 8, &input).unwrap_err().kind(),
                ValidationErrorKind::Cancelled
            );
        });
    }
}
