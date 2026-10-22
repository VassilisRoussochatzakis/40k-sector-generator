//! Shared rendering primitives reused by both `bitmap` (PNG) and `svg_export`
//! (SVG) backends.
//!
//! REFACTOR_PLAN.md Task 3 Pass C/D: the [`canvas::Canvas`] trait is the
//! backend-neutral drawing surface, and per-shape modules ([`routes`],
//! [`grid`]) are generic over it. Bitmap quantises `f32` coordinates to
//! `i32` inside its impl; SVG passes through. See [`canvas`] for the
//! contract that keeps `golden_png` byte-identical.
//!
//! System, label, legend, and region geometry stays backend-specific.
//! Pure label predicates, legend metadata, and heat tints are shared here. The
//! star-disk pieces look identical on the surface, but the bitmap
//! `(r as i32) / 2` floor-division (e.g. in the Redacted capital marker)
//! does not round-trip through `f32 * 0.5` without changing PNG bytes, so
//! their `Canvas` rewrite is parked behind the same precision constraint
//! that already kept text rendering split. The plan's 800-line floor isn't
//! met by `routes` + `grid` alone (saved ≈ 750), but landing the safe slice
//! is preferable to "while I'm in there" precision changes that
//! `golden_png.rs` would catch.

pub(crate) mod canvas;
pub(crate) mod colors;
pub(crate) mod geom;
pub(crate) mod grid;
pub(crate) mod labels;
pub(crate) mod legend;
pub(crate) mod options;
pub(crate) mod routes;

pub use options::RenderOptions;

#[cfg(test)]
pub(crate) mod test_fixtures {
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

    pub(crate) fn sample_sector() -> GeneratedSector {
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
}
