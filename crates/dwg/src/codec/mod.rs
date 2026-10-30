//! CADCraft's own DWG codec: the file format itself, below the document model.
//!
//! Written for CADCraft from the Open Design Alliance's public *Open Design Specification for
//! .dwg files* (5.4.1). This module knows bits, checksums, compression, the file container and
//! the object map; it knows nothing about drawings. `cadcraft-io` maps what it reads to
//! `cadcraft-doc`.
//!
//! Every input byte is treated as hostile: reads are bounds-checked, arithmetic is checked, and
//! every size a file declares is capped before anything is allocated for it ([`Limits`]).

pub mod bits;
pub mod checksum;
pub mod classes;
pub mod container;
pub mod lz77;
pub mod object;
pub mod objmap;
pub mod types;

#[cfg(test)]
mod tests;

/// What went wrong while decoding a DWG file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The data ended before a value was complete (`what` names the value).
    Truncated(&'static str),
    /// A value the format doesn't allow (`what` names the value, `detail` says why).
    Invalid { what: &'static str, detail: String },
    /// The file declares more than [`Limits`] allows.
    TooLarge { what: &'static str, size: u64, limit: u64 },
    /// A DWG version this codec doesn't read (yet).
    Unsupported(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Truncated(what) => write!(f, "DWG: the file ends inside {what}"),
            DecodeError::Invalid { what, detail } => write!(f, "DWG: invalid {what}: {detail}"),
            DecodeError::TooLarge { what, size, limit } => write!(f, "DWG: {what} of {size} bytes is over the limit of {limit}"),
            DecodeError::Unsupported(v) => write!(f, "DWG: version {v} is not read by CADCraft's codec yet"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Result of a decoding step.
pub type Result<T> = std::result::Result<T, DecodeError>;

/// Shorthand for [`DecodeError::Invalid`].
pub(crate) fn invalid(what: &'static str, detail: impl Into<String>) -> DecodeError {
    DecodeError::Invalid { what, detail: detail.into() }
}

/// Caps on what a file may make the decoder allocate or loop over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest decompressed section (bytes).
    pub max_section_bytes: u64,
    /// Largest number of pages in the page map, and of pages in one section.
    pub max_pages: u32,
    /// Largest number of section descriptions in the section map.
    pub max_sections: u32,
    /// Largest number of entries in the object map.
    pub max_objects: u32,
}

impl Limits {
    /// Generous defaults: 512 MB per section, a million pages, 256 sections, 50 million objects.
    pub const DEFAULT: Limits = Limits { max_section_bytes: 512 << 20, max_pages: 1 << 20, max_sections: 256, max_objects: 50_000_000 };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}

/// Checks `size` against `limit`.
pub(crate) fn capped(what: &'static str, size: u64, limit: u64) -> Result<u64> {
    if size > limit { Err(DecodeError::TooLarge { what, size, limit }) } else { Ok(size) }
}
