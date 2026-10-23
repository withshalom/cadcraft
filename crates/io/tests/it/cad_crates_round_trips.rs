//! What CADCraft's own fixes keep when the open engine writes the file (dwg-fidelity branch): checks of
//! upstream fixes that DXF tests alone cover, run through DWG.

use cadcraft_doc::{Drawing, Layout, PageSetup};
use cadcraft_io::{Version, read, write_dwg_noted};

fn windowed() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut l = Layout::new("WIN", 3);
    l.page = PageSetup {
        width_mm: 297.0,
        height_mm: 420.0,
        margins_mm: [0.0; 4],
        plot_area: "window".into(),
        plot_window: [0.0, -297.0, 420.0, 0.0],
        ..PageSetup::default()
    };
    d.layouts.push(l);
    d
}

fn window_of(d: &Drawing) -> (String, [f64; 4]) {
    let page = &d.layouts.iter().find(|l| l.name == "WIN").unwrap().page;
    (page.plot_area.clone(), page.plot_window)
}

/// Upstream #1667 (plot window corners kept in DXF), through a 2018 DWG that the open engine writes.
#[test]
fn plot_window_corners_round_trip_through_cad_crates_dwg() {
    let w = write_dwg_noted(&windowed(), Version::R2018).unwrap();
    assert_eq!(w.engine, cadcraft_dwg::Engine::CadCrates, "{:?}", w.notes);
    let back = read(&w.dwg, "win.dwg").unwrap();
    assert_eq!(window_of(&back), ("window".into(), [0.0, -297.0, 420.0, 0.0]));
}
