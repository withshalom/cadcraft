//! The object map (`AcDb:Handles`, ODA specification §23): each object's handle and where its data
//! starts in the `AcDb:AcDbObjects` section.
//!
//! The map is a run of blocks, each a big-endian size, pairs of modular chars (handle delta, then a
//! signed position delta) and a CRC. A block of size 2 (just its CRC) ends the map.

use super::bits::ByteReader;
use super::checksum::crc16;
use super::{DecodeError, Limits, Result, invalid};

/// One object's handle and its byte offset in the objects section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub handle: u64,
    pub offset: u64,
}

/// The decoded object map, in file order (handles ascend).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectMap {
    pub entries: Vec<MapEntry>,
    /// Blocks whose CRC didn't match (counted, not fatal).
    pub bad_crcs: u32,
}

impl ObjectMap {
    /// The offset of the object with `handle`.
    pub fn offset(&self, handle: u64) -> Option<u64> {
        self.entries.binary_search_by_key(&handle, |e| e.handle).ok().and_then(|i| self.entries.get(i)).map(|e| e.offset)
    }
}

/// Decodes the `AcDb:Handles` section.
pub fn read(data: &[u8], limits: Limits) -> Result<ObjectMap> {
    let mut map = ObjectMap::default();
    let mut r = ByteReader::new(data);
    let mut handle = 0u64;
    let mut offset = 0i64;
    loop {
        let block_start = r.position();
        let size = usize::from(r.u16_be()?);
        if size <= 2 {
            return Ok(map);
        }
        // The size counts the two size bytes and the data, not the CRC.
        let body_len = size - 2;
        let body = r.take(body_len)?;
        let crc_stored = r.u16_be()?;
        let block = data.get(block_start..block_start + size).ok_or(DecodeError::Truncated("an object map block"))?;
        if crc16(0xC0C1, block) != crc_stored {
            map.bad_crcs += 1;
        }
        let mut b = ByteReader::new(body);
        while b.remaining() > 0 {
            if map.entries.len() >= limits.max_objects as usize {
                return Err(DecodeError::TooLarge { what: "the object map", size: map.entries.len() as u64, limit: u64::from(limits.max_objects) });
            }
            handle = handle.checked_add(b.mc_unsigned()?).ok_or(invalid("object map", "handle overflows"))?;
            offset = offset.checked_add(b.mc_signed()?).ok_or(invalid("object map", "offset overflows"))?;
            let at = u64::try_from(offset).map_err(|_| invalid("object map", format!("negative offset {offset}")))?;
            map.entries.push(MapEntry { handle, offset: at });
        }
    }
}
