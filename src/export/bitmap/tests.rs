use super::*;
use crate::export::render_core::test_fixtures::sample_sector;
use crate::sector_model::*;

#[test]
fn renders_without_panicking() {
    let s = sample_sector();
    let img = render(&s, 1, None, RenderOptions::default());
    assert!(img.width() > 0);
    assert!(img.height() > 0);
}

#[test]
fn scaled_render_is_larger() {
    let s = sample_sector();
    let small = render(&s, 1, None, RenderOptions::default());
    let big = render(&s, 4, None, RenderOptions::default());
    assert!(big.width() >= small.width() * 3);
    assert!(big.height() >= small.height() * 3);
}

#[test]
fn glyph_returns_blank_for_space() {
    assert_eq!(primitives::glyph(' '), [0; 7]);
}

// GAP 143: an EMPTY sector (no systems/routes/factions) must still rasterise to
// a non-degenerate image (the legend + empty hex grid force width/height > 0)
// and encode to non-empty PNG bytes. The SVG twin must likewise produce a
// well-formed document. `render` is private but reachable here via `super::*`.
#[test]
fn empty_sector_renders_bitmap_and_svg() {
    let s = GeneratedSector {
        width: 4,
        height: 4,
        ..Default::default()
    };

    // Bitmap path (private `render`, in scope via `super::*`).
    let img = render(&s, 1, None, RenderOptions::default());
    assert!(img.width() > 0, "empty-sector image width should be > 0");
    assert!(img.height() > 0, "empty-sector image height should be > 0");

    let png = encode_png_bytes(&img).unwrap();
    assert!(!png.is_empty(), "encoded PNG should be non-empty");

    // SVG twin (lives in the sibling `svg_export` module).
    let svg = crate::svg_export::render_sector_svg(&s, None, &RenderOptions::default());
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
}
