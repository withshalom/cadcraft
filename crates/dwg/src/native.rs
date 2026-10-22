//! BlackCAD's native format, `.cad`, through its open `cad-native` crate (MIT OR Apache-2.0,
//! <https://github.com/withshalom/CAD>), on the same bridge as DWG: a `.cad` file is read into
//! BlackCAD's drawing model and handed to CADCraft as DXF bytes, and CADCraft's DXF is read into
//! that model and written as `.cad`. The format keeps what DWG and DXF can't: a change journal,
//! a SHA-256 hash chain, recovery data for damaged bytes, and DWG objects the model doesn't decode
//! yet, as their original bytes. Standard library only, so the web build has it too.

use std::panic::{AssertUnwindSafe, catch_unwind};

use cad_dxf::DxfFormat;

use crate::blackcad::{APP_NAME, APP_VERSION, left_out_note, warnings};
use crate::{Limits, mb};

/// File extension of `.cad` drawings.
pub const EXTENSION: &str = cad_native::EXTENSION;

/// True when the bytes are a `.cad` drawing: its magic, or (for a file whose start is damaged)
/// the copy of its header at the end.
pub fn is_cad(bytes: &[u8]) -> bool {
    cad_native::looks_native(bytes)
}

/// Read a `.cad` file into ASCII DXF (AutoCAD 2018 format), with a note for each thing the
/// conversion left out or the reader repaired.
pub fn cad_to_dxf(bytes: &[u8], limits: Limits) -> Result<(Vec<u8>, Vec<String>), String> {
    if bytes.len() > limits.max_dwg_bytes {
        return Err(format!("CAD: the file is {}, larger than the {} CADCraft opens", mb(bytes.len() as u64), mb(limits.max_dwg_bytes as u64)));
    }
    let d = catch_unwind(|| cad_native::read_bytes(bytes))
        .map_err(|_| "BlackCAD's .cad reader failed on this file".to_string())?
        .map_err(|e| format!("CAD: {e}"))?;
    let out = catch_unwind(AssertUnwindSafe(|| cad_dxf::to_bytes(&d, DxfFormat::Ascii)))
        .map_err(|_| "BlackCAD's .cad→DXF conversion failed on this file".to_string())?;
    if out.bytes.len() > limits.max_dxf_bytes {
        return Err(format!(
            "CAD: converting this {} file produced more than {} of drawing data (the limit)",
            mb(bytes.len() as u64),
            mb(limits.max_dxf_bytes as u64)
        ));
    }
    let mut notes = warnings(&d, "CAD");
    if let Some(note) = left_out_note(&out.left_out, "CAD", "couldn't be converted to DXF") {
        notes.push(note);
    }
    Ok((out.bytes, notes))
}

/// Write CADCraft's DXF as a `.cad` file, naming CADCraft as its writer. `preview` is an SVG of
/// the drawing that the file keeps for file browsers and Open dialogs. The notes say what the
/// DXF reader repaired on the way.
pub fn dxf_to_cad(dxf: &[u8], preview: Option<String>) -> Result<(Vec<u8>, Vec<String>), String> {
    let d = catch_unwind(|| cad_dxf::read_bytes(dxf))
        .map_err(|_| "BlackCAD's DXF reader failed on this drawing".to_string())?
        .map_err(|e| format!("DXF: {e}"))?;
    let opts = cad_native::WriteOptions {
        preview_svg: preview.map(cad_native::PreviewSvg::Drawn),
        writer: Some(format!("{APP_NAME} {APP_VERSION}")),
        ..Default::default()
    };
    let bytes = catch_unwind(AssertUnwindSafe(|| cad_native::to_bytes(&d, &opts)))
        .map_err(|_| "BlackCAD's .cad writer failed on this drawing".to_string())?;
    Ok((bytes, warnings(&d, "DXF")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dxf() -> String {
        crate::tests::simple_dxf()
    }

    #[test]
    fn a_drawing_goes_to_cad_and_back() {
        let (cad, notes) = dxf_to_cad(dxf().as_bytes(), Some("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into())).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(is_cad(&cad) && !crate::is_dwg(&cad));
        let info = cad_native::inspect(&cad).unwrap();
        assert_eq!(info.writer, format!("{APP_NAME} {APP_VERSION}"));
        assert!(cad_native::preview_svg(&cad).unwrap().is_some());
        let (back, notes) = cad_to_dxf(&cad, Limits::DEFAULT).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let text = String::from_utf8_lossy(&back);
        assert!(text.contains("LINE") && text.contains("CIRCLE"), "{text}");
    }

    #[test]
    fn damaged_and_oversized_files_are_errors() {
        let (cad, _) = dxf_to_cad(dxf().as_bytes(), None).unwrap();
        // A cut-off file is an error or what could be recovered, never a crash.
        let _ = cad_to_dxf(&cad[..cad.len() / 3], Limits::DEFAULT);
        let small = Limits { max_dwg_bytes: 16, ..Limits::DEFAULT };
        assert!(cad_to_dxf(&cad, small).unwrap_err().contains("larger than"));
        assert!(cad_to_dxf(b"not a drawing", Limits::DEFAULT).is_err());
    }
}
