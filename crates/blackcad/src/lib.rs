//! BlackCAD's engine under CADCraft's interface (the `blackcad-native` branch).
//!
//! The experiment: keep CADCraft's AutoCAD-style interface and replace what sits under it with
//! BlackCAD's Rust engine, `cad-core`. In that engine the drawing *is* a DWG database: objects
//! keyed by DWG handle, read from and written to DWG, DXF and BlackCAD's `.cad` without any
//! conversion in between, and objects it doesn't decode kept as their own DWG bytes.
//!
//! This crate is the seam between the two. It starts with files and display: opening any of
//! the three formats, saving them, and drawing model space. Commands, selection and editing come
//! next; the interface reaches the engine only through this crate, so CADCraft's own engine
//! stays untouched on this branch until the interface is moved over.
//!
//! Licensing: this crate's code is MIT OR Apache-2.0 like the rest of CADCraft. `cad-core` is
//! proprietary (`LicenseRef-Proprietary`, withshalom/CAD), so anything built with this crate
//! carries `cad-core`'s terms. See `NOTICE` and `docs/blackcad-native.md`.
//!
//! Never crash: every call into the engine runs under [`guard`], so a panic in `cad-core`
//! becomes an error instead of taking the drawing with it.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::panic::{AssertUnwindSafe, catch_unwind};

pub use cad_core;
pub use cad_core::model::{Drawing, Handle};

/// The engine's name and version, as it records them in the files it writes.
pub const ENGINE: &str = cad_core::PRODUCT_NAME;
pub const ENGINE_VERSION: &str = cad_core::VERSION;

/// Largest file [`open`] accepts (1 GiB).
pub const MAX_FILE_BYTES: usize = 1 << 30;

/// A file format the engine reads and writes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// BlackCAD's native `.cad`.
    Cad,
    /// DWG: read R13 to 2018 (2007 pending), written as AutoCAD 2018 (AC1032).
    Dwg,
    /// DXF, ASCII or binary; written as ASCII AC1032.
    Dxf,
}

impl Format {
    /// The usual file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Cad => "cad",
            Format::Dwg => "dwg",
            Format::Dxf => "dxf",
        }
    }

    /// The format a file extension names (any case), if the engine writes it.
    pub fn from_extension(ext: &str) -> Option<Format> {
        [Format::Cad, Format::Dwg, Format::Dxf].into_iter().find(|f| f.extension().eq_ignore_ascii_case(ext))
    }
}

/// The format of a file, from its contents.
pub fn detect(bytes: &[u8]) -> Option<Format> {
    if cad_core::native::is_native(bytes) {
        Some(Format::Cad)
    } else if bytes.starts_with(b"AC10") {
        Some(Format::Dwg)
    } else if cad_core::dxf::looks_like_dxf(bytes) {
        Some(Format::Dxf)
    } else {
        None
    }
}

/// Runs `f`, turning a panic in the engine into an error.
pub fn guard<T>(what: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(format!("{what}: the engine stopped unexpectedly")),
    }
}

/// An opened drawing and what the engine reported while reading it.
#[derive(Debug)]
pub struct Opened {
    pub drawing: Drawing,
    pub format: Format,
    /// Problems the reader recorded (damaged or undecoded objects), for people to read.
    pub notes: Vec<String>,
}

/// Opens a `.cad`, DWG or DXF file from its bytes.
pub fn open(bytes: &[u8]) -> Result<Opened, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!("the file is larger than {} MiB", MAX_FILE_BYTES >> 20));
    }
    let format = detect(bytes).ok_or("not a .cad, DWG or DXF file")?;
    guard("open", || {
        let drawing = match format {
            Format::Cad => cad_core::native::read_bytes(bytes),
            Format::Dwg => cad_core::dwg::read_bytes(bytes),
            Format::Dxf => cad_core::dxf::read_bytes(bytes),
        }
        .map_err(|e| e.to_string())?;
        let notes = drawing.diagnostics.iter().map(|d| d.message.clone()).collect();
        Ok(Opened { drawing, format, notes })
    })
}

/// A saved file and what the writer left out, if anything.
#[derive(Debug)]
pub struct Saved {
    pub bytes: Vec<u8>,
    /// What the format couldn't hold, for people to read. Empty when the file holds it all.
    pub notes: Vec<String>,
}

/// Saves a drawing in a format, as BlackCAD writes it.
pub fn save(d: &Drawing, format: Format) -> Result<Saved, String> {
    guard("save", || match format {
        Format::Cad => Ok(Saved { bytes: cad_core::native::to_bytes(d, &cad_core::native::WriteOptions::default()), notes: Vec::new() }),
        Format::Dwg => {
            let out = cad_core::dwg::write::to_bytes(d).map_err(|e| e.to_string())?;
            Ok(Saved { bytes: out.bytes, notes: out.notes })
        }
        Format::Dxf => {
            let out = cad_core::dxf::to_bytes(d, cad_core::dxf::DxfFormat::Ascii);
            Ok(Saved { bytes: out.bytes, notes: out.notes })
        }
    })
}

/// Model space as an SVG, drawn by the engine.
pub fn model_space_svg(d: &Drawing) -> Result<String, String> {
    guard("draw", || {
        let scene = cad_core::render::model_space(d);
        Ok(cad_core::export::svg::to_svg(&scene, &cad_core::export::svg::SvgOptions::default()))
    })
}

/// The engine's sample drawing (a flat plate with holes).
pub fn sample() -> Result<Drawing, String> {
    guard("sample", || Ok(cad_core::samples::plate()))
}

#[cfg(test)]
mod tests;
