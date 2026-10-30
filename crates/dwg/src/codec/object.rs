//! The envelope every object shares (ODA specification §20.1 and §20.2): its size, type, handle,
//! extended data, and where its data, string and handle streams lie.
//!
//! An object record in `AcDb:AcDbObjects` is: a modular-short size, (2010+) a modular-char size of
//! the handle stream in bits, the object's bit stream, and a CRC. [`Envelope::read`] decodes the
//! parts every object has and keeps the record's exact bytes, so a save can write back an object
//! CADCraft didn't change unchanged.

use super::bits::{BitReader, ByteReader};
use super::checksum::crc16;
use super::container::Version;
use super::{DecodeError, Result, invalid};

/// Largest object record accepted (the format's own ceiling is about 1 GB, §2.7).
const MAX_OBJECT_BYTES: u64 = 256 << 20;
/// Largest total of extended data in one object.
const MAX_EED_BYTES: usize = 16 << 20;
/// The seed of an object record's CRC.
const OBJECT_CRC_SEED: u16 = 0xC0C1;

/// One application's extended data on an object (§28): the APPID handle and the raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eed {
    pub app: u64,
    pub data: Vec<u8>,
}

/// Where an object's streams lie, in bits from the start of its bit stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Streams {
    /// Where the type-specific data starts (after the handle and the extended data).
    pub data: usize,
    /// One past the last bit of the data stream (the "endbit" before strings and handles).
    pub data_end: usize,
    /// Where the string stream starts (2007+ files with strings), else `None`.
    pub strings: Option<usize>,
    /// Where the handle stream starts.
    pub handles: usize,
    /// One past the last bit of the handle stream.
    pub end: usize,
}

/// The common part of one object record.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope<'a> {
    /// The record's exact bytes: size fields, bit stream and CRC.
    pub record: &'a [u8],
    /// The object's bit stream (without the size fields and the CRC).
    pub body: &'a [u8],
    pub type_code: u16,
    pub handle: u64,
    pub eed: Vec<Eed>,
    pub streams: Streams,
    pub crc_ok: bool,
}

impl<'a> Envelope<'a> {
    /// Decodes the record that starts at byte `offset` of the objects section.
    pub fn read(objects: &'a [u8], offset: usize, version: Version) -> Result<Envelope<'a>> {
        let rest = objects.get(offset..).ok_or(DecodeError::Truncated("an object record"))?;
        let mut head = ByteReader::new(rest);
        let size = head.ms()?;
        if size > MAX_OBJECT_BYTES {
            return Err(DecodeError::TooLarge { what: "an object", size, limit: MAX_OBJECT_BYTES });
        }
        let size = size as usize;
        let handle_bits = if version >= Version::R2010 { Some(head.mc_unsigned()?) } else { None };
        let body_start = head.position();
        let body_end = body_start.checked_add(size).ok_or(DecodeError::Truncated("an object record"))?;
        let body = rest.get(body_start..body_end).ok_or(DecodeError::Truncated("an object record"))?;
        let crc_stored = rest.get(body_end..body_end + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(DecodeError::Truncated("an object CRC"))?;
        let record = rest.get(..body_end + 2).ok_or(DecodeError::Truncated("an object record"))?;
        let crc_ok = rest.get(..body_end).is_some_and(|b| crc16(OBJECT_CRC_SEED, b) == crc_stored);
        let mut r = BitReader::new(body);
        let (type_code, data_end, handles) = match handle_bits {
            Some(bits) => {
                let total = size * 8;
                let bits = usize::try_from(bits).map_err(|_| invalid("handle stream size", bits.to_string()))?;
                let start = total.checked_sub(bits).ok_or(invalid("handle stream size", format!("{bits} bits in a {size}-byte object")))?;
                (r.ot_r2010()?, start, start)
            }
            None => {
                let type_code = r.bs()? as u16;
                let end = r.rl()?;
                let end = usize::try_from(end).map_err(|_| invalid("object data size", end.to_string()))?;
                if end > size * 8 {
                    return Err(invalid("object data size", format!("{end} bits in a {size}-byte object")));
                }
                (type_code, end, end)
            }
        };
        let handle = r.handle()?.value;
        let eed = read_eed(&mut r)?;
        let strings = if version >= Version::R2010 { string_stream(body, data_end)? } else { None };
        let streams = Streams { data: r.position(), data_end: strings.unwrap_or(data_end), strings, handles, end: size * 8 };
        Ok(Envelope { record, body, type_code, handle, eed, streams, crc_ok })
    }

    /// A reader over the type-specific data, positioned at its start.
    pub fn data_reader(&self) -> Result<BitReader<'a>> {
        let mut r = BitReader::with_end(self.body, self.streams.data_end);
        r.seek(self.streams.data)?;
        Ok(r)
    }

    /// A reader over the string stream, if the object has one.
    pub fn string_reader(&self) -> Result<Option<BitReader<'a>>> {
        match self.streams.strings {
            Some(start) => {
                // The string stream ends where its size field begins; reading past the strings
                // runs into that field, which the text reader treats as data, so the caller reads
                // only as many strings as the object has.
                let mut r = BitReader::with_end(self.body, self.streams.handles);
                r.seek(start)?;
                Ok(Some(r))
            }
            None => Ok(None),
        }
    }

    /// A reader over the handle stream.
    pub fn handle_reader(&self) -> Result<BitReader<'a>> {
        let mut r = BitReader::with_end(self.body, self.streams.end);
        r.seek(self.streams.handles)?;
        Ok(r)
    }
}

/// Extended data blocks (§28): a BS size, and while it is non-zero an APPID handle and that many
/// bytes.
fn read_eed(r: &mut BitReader<'_>) -> Result<Vec<Eed>> {
    let mut out = Vec::new();
    let mut total = 0usize;
    loop {
        let size = r.bs()?;
        if size == 0 {
            return Ok(out);
        }
        let size = usize::try_from(size).map_err(|_| invalid("extended data size", size.to_string()))?;
        total = total.saturating_add(size);
        if total > MAX_EED_BYTES {
            return Err(DecodeError::TooLarge { what: "an object's extended data", size: total as u64, limit: MAX_EED_BYTES as u64 });
        }
        let app = r.handle()?.value;
        out.push(Eed { app, data: r.bytes(size)? });
    }
}

/// Where the string stream starts (§20.1, 2007+): a flag in the last bit before `end_bit`, then
/// a size stored just below it (one short, or two when the first has its 0x8000 bit set).
pub fn string_stream(body: &[u8], end_bit: usize) -> Result<Option<usize>> {
    if end_bit == 0 {
        return Ok(None);
    }
    let mut r = BitReader::with_end(body, end_bit);
    let mut pos = end_bit - 1;
    r.seek(pos)?;
    if !r.bit()? {
        return Ok(None);
    }
    pos = pos.checked_sub(16).ok_or(invalid("string stream", "no room for its size"))?;
    r.seek(pos)?;
    let mut size = usize::from(r.rs()? as u16);
    if size & 0x8000 != 0 {
        pos = pos.checked_sub(16).ok_or(invalid("string stream", "no room for its size"))?;
        r.seek(pos)?;
        let hi = usize::from(r.rs()? as u16);
        size = (size & 0x7FFF) | (hi << 15);
    }
    pos.checked_sub(size).map(Some).ok_or_else(|| invalid("string stream", format!("{size} bits before bit {pos}")))
}
