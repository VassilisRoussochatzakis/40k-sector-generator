use super::*;
use crate::sector_model::*;
use std::collections::BTreeMap;

fn empty_manifest() -> GenerationManifest {
    GenerationManifest {
        project_id: "p".into(),
        generated_at_policy: "x".into(),
        generator_name: "sectorforge".into(),
        generator_version: "0.1.0".into(),
        seed: "abc".into(),
        seed_hash: "h".into(),
        base_seed: None,
        candidate_index: None,
        constraints_digest: None,
        profile: None,
        input_digests: BTreeMap::new(),
        settings_digest: "s".into(),
        system_count: 0,
        world_count: 0,
        route_count: 0,
    }
}

fn sample_sector() -> GeneratedSector {
    let sys = GeneratedSystem {
        id: "s1".into(),
        index: 0,
        name: "Test".into(),
        coord: HexCoord { q: 1, r: 1 },
        kind: crate::sector_model::SystemKind::Star,
        star: Some(GeneratedStar {
            colour_code: "O".into(),
            colour_name: "orange dwarf".into(),
            spectral_type: None,
            source_row_index: None,
        }),
        worlds: vec![],
        primary_factions: vec![],
        tags: vec![],
        notes: vec![],
        control: Default::default(),
        stability: Default::default(),
        orbital_assets: Vec::new(),
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    };
    GeneratedSector {
        id: "demo".into(),
        title: "Demo".into(),
        seed: "abc".into(),
        generator_name: "sectorforge".into(),
        generator_version: "0.1.0".into(),
        width: 4,
        height: 4,
        systems: vec![sys],
        routes: vec![],
        factions: vec![],
        manifest: empty_manifest(),
        influence_field: Default::default(),
        power_projection: Default::default(),
        relations: Default::default(),
        regions: Vec::new().into(),
        economy: Default::default(),
        chronicle: Default::default(),
        id_history: BTreeMap::new(),
    }
}

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
