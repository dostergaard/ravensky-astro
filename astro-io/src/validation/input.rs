//! Bounded payload input shared by container decoders.
use super::*;
use std::io::BufRead;

enum Location<'a> {
    Attached(u64),
    Inline(&'a [u8]),
}

// Logical consumption excludes read-ahead, preserving exact frame boundaries.
pub(super) struct Input<'a, 'ctx> {
    context: &'a mut Context<'ctx>,
    location: Location<'a>,
    length: u64,
    pub(super) consumed: u64,
    buffer: Buffer,
    start: usize,
    end: usize,
}
impl<'a, 'ctx> Input<'a, 'ctx> {
    pub(super) fn attached(
        context: &'a mut Context<'ctx>,
        offset: u64,
        length: u64,
    ) -> Result<Self> {
        context.extent(offset, length)?;
        Self::new(context, Location::Attached(offset), length)
    }

    pub(super) fn inline(context: &'a mut Context<'ctx>, bytes: &'a [u8]) -> Result<Self> {
        Self::new(context, Location::Inline(bytes), bytes.len() as u64)
    }

    fn new(context: &'a mut Context<'ctx>, location: Location<'a>, length: u64) -> Result<Self> {
        let buffer = context.memory.buffer(length.min(65536) as usize)?;
        Ok(Self {
            context,
            location,
            length,
            consumed: 0,
            buffer,
            start: 0,
            end: 0,
        })
    }
}
impl BufRead for Input<'_, '_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.context.checkpoint().map_err(io::Error::other)?;
        if self.start == self.end && self.consumed < self.length {
            let n = (self.length - self.consumed).min(self.buffer.len() as u64) as usize;
            match self.location {
                Location::Attached(base) => self
                    .context
                    .read(base + self.consumed, &mut self.buffer[..n])
                    .map_err(io::Error::other)?,
                Location::Inline(bytes) => self.buffer[..n]
                    .copy_from_slice(&bytes[self.consumed as usize..self.consumed as usize + n]),
            }
            self.start = 0;
            self.end = n;
        }
        Ok(&self.buffer[self.start..self.end])
    }
    fn consume(&mut self, n: usize) {
        let n = n.min(self.end - self.start);
        self.start += n;
        self.consumed += n as u64;
    }
}
impl Read for Input<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self.fill_buf()?;
        let n = bytes.len().min(output.len());
        output[..n].copy_from_slice(&bytes[..n]);
        self.consume(n);
        Ok(n)
    }
}

pub(super) fn decode_error(error: io::Error) -> ValidationError {
    let message = error.to_string();
    if let Some(inner) = error.into_inner() {
        if let Ok(original) = inner.downcast::<ValidationError>() {
            return *original;
        }
    }
    integrity(format!("payload decode: {message}"))
}
