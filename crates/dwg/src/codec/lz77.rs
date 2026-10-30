//! The LZ77 variant that compresses 2004+ section pages (ODA specification §4.7).
//!
//! A stream is a literal run followed by opcodes, each copying earlier output and then more
//! literals, until opcode 0x11. Every length and offset comes from the file, so each is checked
//! against the input and against the output size the caller expects.

use super::{DecodeError, Result, invalid};

/// The input cursor with the format's length codings.
struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8> {
        let b = self.data.get(self.pos).copied().ok_or(DecodeError::Truncated("compressed data"))?;
        self.pos += 1;
        Ok(b)
    }

    fn peek(&self) -> Result<u8> {
        self.data.get(self.pos).copied().ok_or(DecodeError::Truncated("compressed data"))
    }

    /// A Literal Length. A first byte with any of its high four bits set is the next opcode, not a
    /// length: the length is then 0 and the byte is left unread.
    fn literal_length(&mut self) -> Result<usize> {
        let first = self.peek()?;
        if first & 0xF0 != 0 {
            return Ok(0);
        }
        self.pos += 1;
        if first != 0 {
            return Ok(usize::from(first) + 3);
        }
        let mut total = 0x0Fusize;
        loop {
            let b = self.byte()?;
            if b == 0 {
                total = total.checked_add(0xFF).ok_or(invalid("literal length", "overflows"))?;
            } else {
                return total.checked_add(usize::from(b) + 3).ok_or(invalid("literal length", "overflows"));
            }
        }
    }

    /// A Long Compression Offset.
    fn long_count(&mut self) -> Result<usize> {
        let first = self.byte()?;
        if first != 0 {
            return Ok(usize::from(first));
        }
        let mut total = 0xFFusize;
        loop {
            let b = self.byte()?;
            if b == 0 {
                total = total.checked_add(0xFF).ok_or(invalid("copy length", "overflows"))?;
            } else {
                return total.checked_add(usize::from(b)).ok_or(invalid("copy length", "overflows"));
            }
        }
    }

    /// A Two Byte Offset: the offset and the literal count carried in its low two bits.
    fn two_byte_offset(&mut self) -> Result<(usize, usize)> {
        let first = self.byte()?;
        let second = self.byte()?;
        Ok(((usize::from(first) >> 2) | (usize::from(second) << 6), usize::from(first & 0x03)))
    }

    /// The literal count after a copy: the two-bit count if non-zero, else a Literal Length.
    fn literal_after(&mut self, count: usize) -> Result<usize> {
        if count == 0 { self.literal_length() } else { Ok(count) }
    }
}

/// One copy step: how many bytes, from how far back (0 = the previous byte), and how many literal
/// bytes follow.
struct Step {
    copy: usize,
    back: usize,
    literals: usize,
}

/// Decodes the copy opcode that starts with `op`.
fn step(input: &mut Input<'_>, op: u8) -> Result<Step> {
    let (copy, offset, count) = match op {
        0x10 => {
            let copy = input.long_count()?.saturating_add(9);
            let (o, c) = input.two_byte_offset()?;
            (copy, o + 0x3FFF, c)
        }
        0x12..=0x1F => {
            let (o, c) = input.two_byte_offset()?;
            (usize::from(op & 0x0F) + 2, o + 0x3FFF, c)
        }
        0x20 => {
            let copy = input.long_count()?.saturating_add(0x21);
            let (o, c) = input.two_byte_offset()?;
            (copy, o, c)
        }
        0x21..=0x3F => {
            let (o, c) = input.two_byte_offset()?;
            (usize::from(op) - 0x1E, o, c)
        }
        0x40..=0xFF => {
            let second = input.byte()?;
            let offset = (usize::from(second) << 2) | (usize::from(op & 0x0C) >> 2);
            ((usize::from(op) >> 4) - 1, offset, usize::from(op & 0x03))
        }
        _ => return Err(invalid("compression opcode", format!("{op:#04x}"))),
    };
    let literals = input.literal_after(count)?;
    Ok(Step { copy, back: offset, literals })
}

/// Appends `n` bytes copied from `back + 1` bytes before the end of `out` (the source may overlap
/// what is being written, which repeats a pattern).
fn copy_back(out: &mut Vec<u8>, back: usize, n: usize) -> Result<()> {
    let start = out.len().checked_sub(back + 1).ok_or(invalid("compression offset", format!("{back} reaches before the start")))?;
    for i in 0..n {
        let b = out.get(start + i).copied().ok_or(invalid("compression offset", "out of range"))?;
        out.push(b);
    }
    Ok(())
}

/// Appends `n` literal bytes from the input.
fn literals(out: &mut Vec<u8>, input: &mut Input<'_>, n: usize) -> Result<()> {
    let end = input.pos.checked_add(n).ok_or(DecodeError::Truncated("literal bytes"))?;
    let bytes = input.data.get(input.pos..end).ok_or(DecodeError::Truncated("literal bytes"))?;
    out.extend_from_slice(bytes);
    input.pos = end;
    Ok(())
}

/// Decompresses `data`, which must expand to at most `max_out` bytes.
pub fn decompress(data: &[u8], max_out: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(max_out.min(data.len().saturating_mul(8)).min(1 << 24));
    let mut input = Input { data, pos: 0 };
    let first = input.literal_length()?;
    check_room(&out, first, max_out)?;
    literals(&mut out, &mut input, first)?;
    loop {
        let op = input.byte()?;
        if op == 0x11 {
            return Ok(out);
        }
        let s = step(&mut input, op)?;
        check_room(&out, s.copy.saturating_add(s.literals), max_out)?;
        copy_back(&mut out, s.back, s.copy)?;
        literals(&mut out, &mut input, s.literals)?;
    }
}

fn check_room(out: &[u8], more: usize, max_out: usize) -> Result<()> {
    if out.len().saturating_add(more) > max_out {
        return Err(DecodeError::TooLarge { what: "a decompressed page", size: (out.len().saturating_add(more)) as u64, limit: max_out as u64 });
    }
    Ok(())
}
