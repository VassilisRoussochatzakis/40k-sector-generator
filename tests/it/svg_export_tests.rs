//! Smoke test for the SVG exporter against the bundled m42 fixture.

use std::path::PathBuf;

use camino::Utf8PathBuf;

use crate::shared::fixture_sector;

const PIN_ENV: &str = "UPDATE_GOLDEN_SVG";

fn pinned_hash_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/svg_m42_default.blake3")
}

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

// G10: pin the SVG bytes with a committed blake3 golden, mirroring
// `golden_png.rs`. `RenderOptions::default()` has `heatmap: Off`, so the
// heat/heat_tints maps are lookup-only and the writer iterates in deterministic
// grid order — capturing the current bytes is safe. Pass `UPDATE_GOLDEN_SVG=1`
// to (re)bless the pin.
#[test]
fn svg_export_matches_pinned_blake3_hash() {
    let sector = fixture_sector();
    let opts = sectorforge::bitmap::RenderOptions::default();
    let svg = sectorforge::svg_export::render_sector_svg(sector, None, &opts);
    let hash = blake3::hash(svg.as_bytes()).to_hex().to_string();
    let pin = pinned_hash_path();
    if std::env::var_os(PIN_ENV).is_some() {
        std::fs::create_dir_all(pin.parent().unwrap()).unwrap();
        std::fs::write(&pin, format!("{hash}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&pin).unwrap_or_else(|_| {
        panic!("missing pinned hash; run `{PIN_ENV}=1 cargo test --test it -- svg_export` to bless")
    });
    assert_eq!(
        expected.trim(),
        hash,
        "SVG bytes drifted from pinned hash; if intentional, rerun with `{PIN_ENV}=1` to refresh"
    );
}
