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
        kind: SystemKind::Star,
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
fn renders_well_formed_svg() {
    let sector = sample_sector();
    let svg = render_sector_svg(&sector, None, &RenderOptions::default());
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(svg.contains("polygon"));
}

/// Starless systems must render their kind-specific glyph (matching the live
/// egui renderer), not the retired grey square.
#[test]
fn starless_kinds_use_distinct_glyphs_not_grey_square() {
    let mut sector = sample_sector();
    sector.width = 8;
    sector.height = 4;
    let base = sector.systems[0].clone();
    sector.systems.clear();
    for (i, kind) in [
        SystemKind::SpecialLocation,
        SystemKind::BlackHole,
        SystemKind::WarpAnomaly,
        SystemKind::SpaceStation,
    ]
    .into_iter()
    .enumerate()
    {
        let mut sys = base.clone();
        sys.id = format!("s{i}").into();
        sys.kind = kind;
        sys.star = None;
        sys.coord = HexCoord { q: i as i32, r: 0 };
        sector.systems.push(sys);
    }
    let svg = render_sector_svg(&sector, None, &RenderOptions::default());

    // The black hole draws a solid black disk; nothing else in a star-free,
    // subsector-free map emits pure black.
    assert!(svg.contains("#000000"), "black-hole disk missing from SVG");
    // The retired grey-square marker (rgb 140,140,150) must be gone.
    assert!(
        !svg.contains("8c8c96"),
        "old grey-square glyph still emitted for starless systems"
    );
}
