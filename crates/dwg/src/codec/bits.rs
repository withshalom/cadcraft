//! Reading DWG's bit-level value forms (ODA specification §2).
//!
//! Object and header data are bit streams: values start at any bit, most significant bit of each
//! byte first, and many values use a two-bit code to pick a compact form. [`BitReader`] reads every
//! form the specification defines; each read checks that the data holds it.

use super::{DecodeError, Result, invalid};

/// A reader of a DWG bit stream over a byte slice.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Position in bits from the start of `data`.
    pos: usize,
    /// One past the last bit that may be read.
    end: usize,
}

/// A handle reference: its code (ownership or offset kind, §2.13) and value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleRef {
    pub code: u8,
    pub value: u64,
}

impl<'a> BitReader<'a> {
    /// A reader over all of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, end: data.len().saturating_mul(8) }
    }

    /// A reader over `data` that stops at bit `end_bit` (clamped to the data).
    pub fn with_end(data: &'a [u8], end_bit: usize) -> Self {
        BitReader { data, pos: 0, end: end_bit.min(data.len().saturating_mul(8)) }
    }

    /// Current position in bits.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Moves to bit `pos`, which must lie within the stream.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.end {
            return Err(DecodeError::Truncated("a seek target"));
        }
        self.pos = pos;
        Ok(())
    }

    /// Bits left to read.
    pub fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// One bit (B).
    pub fn bit(&mut self) -> Result<bool> {
        if self.pos >= self.end {
            return Err(DecodeError::Truncated("a bit"));
        }
        let byte = self.data.get(self.pos / 8).copied().ok_or(DecodeError::Truncated("a bit"))?;
        let set = byte & (0x80 >> (self.pos % 8)) != 0;
        self.pos += 1;
        Ok(set)
    }

    /// `n` bits (at most 64), most significant first.
    pub fn bits(&mut self, n: u32) -> Result<u64> {
        if n > 64 {
            return Err(invalid("bit count", format!("{n} bits in one read")));
        }
        if self.remaining() < n as usize {
            return Err(DecodeError::Truncated("a bit field"));
        }
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | u64::from(self.bit()?);
        }
        Ok(v)
    }

    /// A raw byte (RC), at any bit position.
    pub fn rc(&mut self) -> Result<u8> {
        Ok(self.bits(8)? as u8)
    }

    /// `N` raw bytes.
    fn raw<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.remaining() < N * 8 {
            return Err(DecodeError::Truncated("raw bytes"));
        }
        let mut out = [0u8; N];
        for b in &mut out {
            *b = self.rc()?;
        }
        Ok(out)
    }

    /// `n` raw bytes into a vector (capped by what is left in the stream).
    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if self.remaining() / 8 < n {
            return Err(DecodeError::Truncated("a byte run"));
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.rc()?);
        }
        Ok(out)
    }

    /// A raw little-endian short (RS).
    pub fn rs(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.raw()?))
    }

    /// A raw little-endian long (RL).
    pub fn rl(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.raw()?))
    }

    /// A raw little-endian double (RD).
    pub fn rd(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.raw()?))
    }

    /// A two-bit code (BB).
    pub fn bb(&mut self) -> Result<u8> {
        Ok(self.bits(2)? as u8)
    }

    /// One to three bits, stopping at the first 0 (3B, §2.1): 0–7.
    pub fn b3(&mut self) -> Result<u8> {
        let mut v = 0u8;
        for _ in 0..3 {
            let bit = self.bit()?;
            v = (v << 1) | u8::from(bit);
            if !bit {
                break;
            }
        }
        Ok(v)
    }

    /// A bitshort (BS, §2.2).
    pub fn bs(&mut self) -> Result<i16> {
        match self.bb()? {
            0 => self.rs(),
            1 => Ok(i16::from(self.rc()?)),
            2 => Ok(0),
            _ => Ok(256),
        }
    }

    /// A bitlong (BL, §2.3).
    pub fn bl(&mut self) -> Result<i32> {
        match self.bb()? {
            0 => self.rl(),
            1 => Ok(i32::from(self.rc()?)),
            2 => Ok(0),
            _ => Err(invalid("bitlong", "code 3 is not used")),
        }
    }

    /// A bitlonglong (BLL, §2.4): a 3B byte count, then that many bytes, least significant first.
    pub fn bll(&mut self) -> Result<u64> {
        let len = self.b3()?;
        let mut v = 0u64;
        for i in 0..u32::from(len) {
            v |= u64::from(self.rc()?) << (8 * i);
        }
        Ok(v)
    }

    /// A bitdouble (BD, §2.5).
    pub fn bd(&mut self) -> Result<f64> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(invalid("bitdouble", "code 3 is not used")),
        }
    }

    /// A 2D point of bitdoubles (2BD).
    pub fn bd2(&mut self) -> Result<[f64; 2]> {
        Ok([self.bd()?, self.bd()?])
    }

    /// A 3D point of bitdoubles (3BD).
    pub fn bd3(&mut self) -> Result<[f64; 3]> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    /// A 2D point of raw doubles (2RD).
    pub fn rd2(&mut self) -> Result<[f64; 2]> {
        Ok([self.rd()?, self.rd()?])
    }

    /// A 3D point of raw doubles (3RD).
    pub fn rd3(&mut self) -> Result<[f64; 3]> {
        Ok([self.rd()?, self.rd()?, self.rd()?])
    }

    /// A bitdouble with default (DD, §2.9): parts of `default` replaced by the stored bytes.
    pub fn dd(&mut self, default: f64) -> Result<f64> {
        let mut b = default.to_le_bytes();
        match self.bb()? {
            0 => {}
            1 => {
                for slot in b.iter_mut().take(4) {
                    *slot = self.rc()?;
                }
            }
            2 => {
                let hi: [u8; 2] = self.raw()?;
                let lo: [u8; 4] = self.raw()?;
                if let Some(s) = b.get_mut(4..6) {
                    s.copy_from_slice(&hi);
                }
                if let Some(s) = b.get_mut(0..4) {
                    s.copy_from_slice(&lo);
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(b))
    }

    /// A 3D point of bitdoubles with default (3DD), each coordinate against `default`'s.
    pub fn dd3(&mut self, default: [f64; 3]) -> Result<[f64; 3]> {
        Ok([self.dd(default[0])?, self.dd(default[1])?, self.dd(default[2])?])
    }

    /// A bit extrusion (BE, §2.8) for R2000+: one bit for (0,0,1), else a 3BD.
    pub fn be(&mut self) -> Result<[f64; 3]> {
        if self.bit()? { Ok([0.0, 0.0, 1.0]) } else { self.bd3() }
    }

    /// A bit thickness (BT, §2.10) for R2000+: one bit for 0.0, else a BD.
    pub fn bt(&mut self) -> Result<f64> {
        if self.bit()? { Ok(0.0) } else { self.bd() }
    }

    /// A handle reference (H, §2.13): a 4-bit code, a 4-bit byte count, then the value MSB first.
    pub fn handle(&mut self) -> Result<HandleRef> {
        let code = self.bits(4)? as u8;
        let count = self.bits(4)? as u32;
        if count > 8 {
            return Err(invalid("handle", format!("{count} value bytes (at most 8)")));
        }
        let mut value = 0u64;
        for _ in 0..count {
            value = (value << 8) | u64::from(self.rc()?);
        }
        Ok(HandleRef { code, value })
    }

    /// A handle reference resolved against `base` (the referencing object's handle) when its
    /// code says it is an offset (§2.13).
    pub fn handle_from(&mut self, base: u64) -> Result<u64> {
        let h = self.handle()?;
        Ok(match h.code {
            0x6 => base.wrapping_add(1),
            0x8 => base.wrapping_sub(1),
            0xA => base.wrapping_add(h.value),
            0xC => base.wrapping_sub(h.value),
            _ => h.value,
        })
    }

    /// The object type (OT, §2.12) of an R2010+ file.
    pub fn ot_r2010(&mut self) -> Result<u16> {
        match self.bb()? {
            0 => Ok(u16::from(self.rc()?)),
            1 => Ok(u16::from(self.rc()?) + 0x1F0),
            _ => Ok(u16::from_le_bytes(self.raw()?)),
        }
    }

    /// An 8-bit text (T) of R2004 and earlier: a BS length, then that many bytes in the drawing's
    /// code page (returned undecoded).
    pub fn text_bytes(&mut self) -> Result<Vec<u8>> {
        let len = self.bs()?;
        let len = usize::try_from(len).map_err(|_| invalid("text length", len.to_string()))?;
        self.bytes(len)
    }

    /// A Unicode text (TU) of R2007+: a BS character count, then UTF-16LE characters.
    pub fn text_unicode(&mut self) -> Result<String> {
        let len = self.bs()?;
        let len = usize::try_from(len).map_err(|_| invalid("text length", len.to_string()))?;
        if self.remaining() / 16 < len {
            return Err(DecodeError::Truncated("a Unicode text"));
        }
        let mut units = Vec::with_capacity(len);
        for _ in 0..len {
            units.push(u16::from_le_bytes(self.raw()?));
        }
        // A trailing NUL is part of the stored count in some writers.
        while units.last() == Some(&0) {
            units.pop();
        }
        Ok(String::from_utf16_lossy(&units))
    }
}

/// A byte-aligned cursor for the parts of the format that are not bit streams (the object map,
/// section maps and headers).
#[derive(Clone, Debug)]
pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        ByteReader { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated("a byte run"))?;
        let s = self.data.get(self.pos..end).ok_or(DecodeError::Truncated("a byte run"))?;
        self.pos = end;
        Ok(s)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16_le(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u16_be(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32_le(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64_le(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// A modular char (MC, §2.6) read as an unsigned value.
    pub fn mc_unsigned(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..10u32 {
            let b = self.u8()?;
            value |= u64::from(b & 0x7F).checked_shl(7 * i).unwrap_or(0);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(invalid("modular char", "longer than 10 bytes"))
    }

    /// A modular char (MC, §2.6) whose last byte's 0x40 bit is a sign.
    pub fn mc_signed(&mut self) -> Result<i64> {
        let mut value = 0i64;
        for i in 0..9u32 {
            let b = self.u8()?;
            if b & 0x80 != 0 {
                value |= i64::from(b & 0x7F) << (7 * i);
                continue;
            }
            let negative = b & 0x40 != 0;
            value |= i64::from(b & 0x3F) << (7 * i);
            return Ok(if negative { -value } else { value });
        }
        Err(invalid("modular char", "longer than 9 bytes"))
    }

    /// A modular short (MS, §2.7): 15-bit modules, each a little-endian short.
    pub fn ms(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..4u32 {
            let s = self.u16_le()?;
            value |= u64::from(s & 0x7FFF) << (15 * i);
            if s & 0x8000 == 0 {
                return Ok(value);
            }
        }
        Err(invalid("modular short", "longer than 4 shorts"))
    }
}
