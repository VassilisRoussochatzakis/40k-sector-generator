//! Smoke test for the SVG exporter against the bundled m42 fixture.

use camino::Utf8PathBuf;

use crate::shared::fixture_sector;

#[test]
fn renders_m42_sector_as_well_formed_svg() {
    let sector = fixture_sector();
    let opts = sectorforge::bitmap::RenderOptions::default();
    let svg = sectorforge::svg_export::render_sector_svg(sector, None, &opts);

    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(
        svg.contains("<polygon"),
        "expected at least one hex polygon"
    );
    assert!(svg.contains("<circle"), "expected at least one star disc");
    assert!(svg.contains("<line"), "expected at least one route line");
    assert!(
        svg.contains("SECTOR:"),
        "expected legend title to be present"
    );
}

#[test]
fn writes_svg_file_to_disk() {
    let sector = fixture_sector();
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("sector.svg")).unwrap();
    sectorforge::svg_export::write_sector_svg_to(sector, &path, None).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.len() > 4096, "expected non-trivial SVG output");
}
