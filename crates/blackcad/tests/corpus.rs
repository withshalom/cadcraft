//! What BlackCAD's engine keeps of real AutoCAD files, opened and saved without CADCraft's model.
//!
//! Runs only when `BLACKCAD_CORPUS` names a folder of DWG and DXF files. In CI that is BlackCAD's
//! MI-201 corpus (`corpus/mosaiciron-mi201` in withshalom/CAD: one part saved by AutoCAD from R12
//! to 2018). The files are AutoCAD's, so they are never committed here.
//!
//! Each file is opened, saved as `.cad` and as DWG, and both are opened again. It fails when a
//! `.cad` round trip loses an object (by handle) or a DWG round trip loses a model-space entity;
//! files the engine doesn't open are reported, not failed (2007 DWG isn't read yet).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;

use cadcraft_blackcad::cad_core::model::Space;
use cadcraft_blackcad::{Drawing, Format, open, save};

fn model_entities(d: &Drawing) -> usize {
    d.objects
        .values()
        .filter(|o| o.entity.as_ref().is_some_and(|e| e.space == Space::Model))
        .filter(|o| o.type_name != "BLOCK" && o.type_name != "ENDBLK")
        .count()
}

fn kept_as_bytes(d: &Drawing) -> usize {
    d.objects.values().filter(|o| o.raw.is_some()).count()
}

#[test]
fn corpus_round_trips_through_the_engine() {
    let Some(dir) = std::env::var_os("BLACKCAD_CORPUS") else {
        eprintln!("BLACKCAD_CORPUS not set: no corpus to check");
        return;
    };
    let mut files: Vec<std::path::PathBuf> =
        std::fs::read_dir(&dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    files.retain(|p| p.extension().and_then(|e| e.to_str()).and_then(Format::from_extension).is_some_and(|f| f != Format::Cad));
    files.sort();
    assert!(!files.is_empty(), "no DWG or DXF files in {}", std::path::Path::new(&dir).display());

    let mut report = String::new();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap();
        let opened = match open(&bytes) {
            Ok(o) => o,
            Err(e) => {
                let _ = writeln!(report, "{name}: doesn't open: {e}");
                continue;
            }
        };
        let d = &opened.drawing;
        let _ = write!(
            report,
            "{name}: {} objects ({} kept as DWG bytes), {} model-space entities",
            d.objects.len(),
            kept_as_bytes(d),
            model_entities(d)
        );

        // .cad: every object comes back under its handle.
        match save(d, Format::Cad).and_then(|s| open(&s.bytes)) {
            Ok(again) => {
                let lost: Vec<String> = d.objects.keys().filter(|h| !again.drawing.objects.contains_key(h)).map(|h| format!("{:X}", h.0)).collect();
                if lost.is_empty() {
                    let _ = write!(report, "; .cad keeps all");
                } else {
                    let _ = write!(report, "; .cad loses {}", lost.len());
                    failures.push(format!("{name}: .cad lost objects {}", lost.join(" ")));
                }
            }
            Err(e) => failures.push(format!("{name}: .cad round trip: {e}")),
        }

        // DWG 2018: the model-space entities come back, and what the writer left out is said.
        match save(d, Format::Dwg) {
            Ok(s) => match open(&s.bytes) {
                Ok(again) => {
                    let (a, b) = (model_entities(d), model_entities(&again.drawing));
                    let _ = write!(report, "; DWG {} objects, {a} → {b} entities", again.drawing.objects.len());
                    if !s.notes.is_empty() {
                        let _ =
                            write!(report, " ({} writer notes: {})", s.notes.len(), s.notes.iter().take(3).cloned().collect::<Vec<_>>().join(" | "));
                    }
                    if b < a {
                        failures.push(format!("{name}: DWG round trip kept {b} of {a} model-space entities"));
                    }
                }
                Err(e) => failures.push(format!("{name}: the saved DWG doesn't open: {e}")),
            },
            Err(e) => {
                let _ = write!(report, "; DWG doesn't save: {e}");
            }
        }
        let _ = writeln!(report);
    }
    println!("{report}");
    if let Some(out) = std::env::var_os("BLACKCAD_CORPUS_REPORT") {
        let _ = std::fs::write(out, &report);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
