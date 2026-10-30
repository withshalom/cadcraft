//! The class list (`AcDb:Classes`, ODA specification §10): what object types 500 and up are.
//!
//! An object whose type is 500 or more is an instance of the class with that number; the class
//! gives its DXF name (`MLEADERSTYLE`, `SCALE`, ...) and whether it is an entity.

use super::bits::{BitReader, ByteReader};
use super::container::Version;
use super::object::string_stream;
use super::{DecodeError, Result, invalid};

/// The 16-byte marker that starts the section.
const START: [u8; 16] = [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];
/// More classes than this in one file is treated as damage.
const MAX_CLASSES: usize = 10_000;

/// One class definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    pub number: u16,
    pub proxy_flags: u16,
    pub app_name: String,
    pub cpp_name: String,
    pub dxf_name: String,
    pub was_zombie: bool,
    /// 0x1F2 for entities, 0x1F3 for other objects.
    pub item_class_id: u16,
    pub instance_count: i32,
    pub dwg_version: i32,
    pub maintenance_version: i32,
    pub unknown: (i32, i32),
}

impl Class {
    pub fn is_entity(&self) -> bool {
        self.item_class_id == 0x1F2
    }
}

/// The decoded class list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Classes {
    pub max_number: i16,
    pub list: Vec<Class>,
}

impl Classes {
    /// The class with `number`.
    pub fn get(&self, number: u16) -> Option<&Class> {
        self.list.iter().find(|c| c.number == number)
    }
}

/// Reads texts either from the data stream (2004: 8-bit, code page) or from the string stream
/// (2010+: UTF-16).
enum Texts<'a> {
    Inline,
    Stream(BitReader<'a>),
}

impl Texts<'_> {
    fn read(&mut self, data: &mut BitReader<'_>) -> Result<String> {
        match self {
            // Class names are ASCII in practice; other bytes are kept as Latin-1.
            Texts::Inline => Ok(data.text_bytes()?.into_iter().take_while(|&b| b != 0).map(char::from).collect()),
            Texts::Stream(s) => s.text_unicode(),
        }
    }
}

/// Decodes the class section. `maintenance` is the file header's maintenance release version.
pub fn read(section: &[u8], version: Version, maintenance: u8) -> Result<Classes> {
    let mut b = ByteReader::new(section);
    if b.take(16)? != START.as_slice() {
        return Err(invalid("class section", "its start marker is missing"));
    }
    let size = b.u32_le()?;
    // A high size word follows in 2010 and 2013 files of maintenance version 4 and up, and in
    // every 2018 file (§10.2, §8).
    if version == Version::R2018 || (version >= Version::R2010 && maintenance > 3) {
        let _high = b.u32_le()?;
    }
    let body = b.take(usize::try_from(size).map_err(|_| invalid("class section size", size.to_string()))?)?;
    let mut r = BitReader::new(body);
    let (end, mut texts) = if version >= Version::R2010 {
        let total = r.rl()?;
        let total = usize::try_from(total).map_err(|_| invalid("class section bit size", total.to_string()))?;
        let data_start = r.position();
        let strings = string_stream(body, total)?;
        let mut stream = BitReader::with_end(body, total);
        let end = match strings {
            Some(s) => {
                stream.seek(s)?;
                s
            }
            None => total,
        };
        r.seek(data_start)?;
        (end, Texts::Stream(stream))
    } else {
        (body.len() * 8, Texts::Inline)
    };
    let max_number = r.bs()?;
    r.rc()?;
    r.rc()?;
    r.bit()?;
    let mut list = Vec::new();
    while r.position() < end {
        let number = r.bs()?;
        if number < 500 || number > max_number {
            break;
        }
        if list.len() >= MAX_CLASSES {
            return Err(DecodeError::TooLarge { what: "the class list", size: list.len() as u64, limit: MAX_CLASSES as u64 });
        }
        list.push(Class {
            number: number as u16,
            proxy_flags: r.bs()? as u16,
            app_name: texts.read(&mut r)?,
            cpp_name: texts.read(&mut r)?,
            dxf_name: texts.read(&mut r)?,
            was_zombie: r.bit()?,
            item_class_id: r.bs()? as u16,
            instance_count: r.bl()?,
            // §10.2 lists these as BS; AutoCAD's files store bitlongs (a BS and a BL coincide
            // for small values, so only later DWG versions show the difference).
            dwg_version: r.bl()?,
            maintenance_version: r.bl()?,
            unknown: (r.bl()?, r.bl()?),
        });
    }
    Ok(Classes { max_number, list })
}
