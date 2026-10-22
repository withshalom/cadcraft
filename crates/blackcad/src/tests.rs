#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

/// Drawn entities in model space. Writing DWG and DXF adds graphics blocks for dimensions, so
/// entities inside blocks can grow; and the DXF reader marks model space's own BLOCK and ENDBLK
/// records as in model space, where the DWG reader doesn't, so those are left out.
fn entity_count(d: &Drawing) -> usize {
    d.objects
        .values()
        .filter(|o| o.entity.as_ref().is_some_and(|e| e.space == cad_core::model::Space::Model))
        .filter(|o| o.type_name != "BLOCK" && o.type_name != "ENDBLK")
        .count()
}

#[test]
fn the_sample_saves_and_reopens_in_every_format() {
    let d = sample().unwrap();
    let before = entity_count(&d);
    assert!(before > 0);
    for format in [Format::Cad, Format::Dwg, Format::Dxf] {
        let saved = save(&d, format).unwrap();
        assert_eq!(detect(&saved.bytes), Some(format), "{format:?} isn't recognised");
        let again = open(&saved.bytes).unwrap();
        assert_eq!(again.format, format);
        assert_eq!(entity_count(&again.drawing), before, "{format:?} round trip changed the entities");
    }
}

#[test]
fn cad_keeps_every_object_by_handle() {
    let d = sample().unwrap();
    let saved = save(&d, Format::Cad).unwrap();
    let again = open(&saved.bytes).unwrap().drawing;
    let handles = |d: &Drawing| d.objects.keys().copied().collect::<Vec<Handle>>();
    assert_eq!(handles(&again), handles(&d));
}

#[test]
fn model_space_draws() {
    let svg = model_space_svg(&sample().unwrap()).unwrap();
    assert!(svg.starts_with("<svg") || svg.starts_with("<?xml"), "{}", &svg[..svg.len().min(80)]);
    assert!(svg.len() > 200);
}

#[test]
fn formats_by_extension() {
    assert_eq!(Format::from_extension("DWG"), Some(Format::Dwg));
    assert_eq!(Format::from_extension("cad"), Some(Format::Cad));
    assert_eq!(Format::from_extension("svg"), None);
    for f in [Format::Cad, Format::Dwg, Format::Dxf] {
        assert_eq!(Format::from_extension(f.extension()), Some(f));
    }
}

#[test]
fn hostile_files_are_errors_not_crashes() {
    let good = save(&sample().unwrap(), Format::Dwg).unwrap().bytes;
    let mut inputs: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"AC1032".to_vec(),
        b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nNaN\n".to_vec(),
        b"\x89CAD\r\n\x1a\n".to_vec(),
        vec![0xFF; 4096],
    ];
    // The real file cut short, and with bytes flipped through it.
    for cut in [16, 128, 1024, good.len() / 2] {
        inputs.push(good[..cut.min(good.len())].to_vec());
    }
    for step in [7usize, 61, 509] {
        let mut b = good.clone();
        for i in (step..b.len()).step_by(step) {
            b[i] ^= 0x5A;
        }
        inputs.push(b);
    }
    for bytes in inputs {
        // Ok or Err are both fine; returning at all is the test.
        let _ = open(&bytes);
    }
    assert!(open(b"not a drawing").is_err());
}

#[test]
fn a_panic_in_the_engine_becomes_an_error() {
    let r: Result<(), String> = guard("test", || panic!("boom"));
    assert_eq!(r.unwrap_err(), "test: the engine stopped unexpectedly");
}
