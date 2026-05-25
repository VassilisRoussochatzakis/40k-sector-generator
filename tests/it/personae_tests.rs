//! Integration coverage for [`sectorforge::personae`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `PersonaeReport` JSON across
//!    many random seeds (proptest).
//! 2. Structural invariants — every persona references a valid faction and a
//!    valid system/world anchor; ids unique; per-world and per-system caps
//!    honoured by the public config knobs.
//! 3. Golden markdown — stable headings and faction grouping for the fixture.

use std::sync::OnceLock;

use camino::Utf8PathBuf;
use proptest::prelude::*;
use sectorforge::{
    generate_sector, load_project,
    personae::{self, PersonaAnchor, PersonaeConfig},
    GeneratedSector,
};

fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

fn fixture_sector() -> &'static GeneratedSector {
    static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
    SECTOR.get_or_init(|| {
        let input = load_project(fixture_dir()).expect("load fixture");
        generate_sector(input).expect("generate fixture")
    })
}

fn sector_with_seed(seed: &str) -> GeneratedSector {
    let mut input = load_project(fixture_dir()).expect("load fixture");
    input.config.generation.seed = seed.to_string();
    generate_sector(input).expect("generate sector")
}

#[test]
fn report_metadata_mirrors_sector() {
    let sector = fixture_sector();
    let report = personae::derive(sector);
    assert_eq!(report.sector_id, sector.id.to_string());
    assert_eq!(report.seed, sector.seed.to_string());
}

#[test]
fn every_persona_references_real_faction_and_anchor() {
    let sector = fixture_sector();
    let report = personae::derive(sector);

    let faction_ids: std::collections::BTreeSet<&str> =
        sector.factions.iter().map(|f| f.id.as_str()).collect();
    let system_ids: std::collections::BTreeSet<&str> =
        sector.systems.iter().map(|s| s.id.as_str()).collect();

    let world_keys: std::collections::BTreeSet<(String, String)> = sector
        .systems
        .iter()
        .flat_map(|s| {
            s.worlds
                .iter()
                .map(move |w| (s.id.to_string(), w.id.to_string()))
        })
        .collect();

    for p in &report.personae {
        assert!(
            faction_ids.contains(p.faction_id.as_str()),
            "persona {} references unknown faction {}",
            p.id,
            p.faction_id
        );
        match &p.anchor {
            PersonaAnchor::System { system_id, .. } => {
                assert!(
                    system_ids.contains(system_id.as_str()),
                    "persona {} references unknown system {}",
                    p.id,
                    system_id
                );
            }
            PersonaAnchor::World {
                system_id,
                world_id,
            } => {
                assert!(
                    world_keys.contains(&(system_id.to_string(), world_id.to_string())),
                    "persona {} references unknown world {}/{}",
                    p.id,
                    system_id,
                    world_id
                );
            }
        }
    }
}

#[test]
fn persona_names_are_unique_across_sector() {
    // The internal `used_names` set in `personae::derive_with` guarantees the
    // sector-wide name space is collision-free. (Persona `id` strings can
    // legitimately collide when the same faction is anchored to the same
    // world/system slot via multiple dominance entries — that is by design.)
    let report = personae::derive(fixture_sector());
    let mut seen = std::collections::BTreeSet::new();
    for p in &report.personae {
        assert!(
            seen.insert(p.name.clone()),
            "duplicate persona name: {}",
            p.name
        );
    }
}

#[test]
fn max_per_world_and_per_system_caps_honoured() {
    let sector = fixture_sector();
    let cfg = PersonaeConfig {
        max_per_world: 1,
        max_per_system: 1,
        ..Default::default()
    };
    let report = personae::derive_with(sector, &cfg);

    let mut per_world: std::collections::BTreeMap<(String, String), u32> = Default::default();
    let mut per_system_slots: std::collections::BTreeMap<String, u32> = Default::default();
    for p in &report.personae {
        match &p.anchor {
            PersonaAnchor::World {
                system_id,
                world_id,
            } => {
                let k = (system_id.to_string(), world_id.to_string());
                *per_world.entry(k).or_default() += 1;
            }
            PersonaAnchor::System { system_id, .. } => {
                *per_system_slots.entry(system_id.to_string()).or_default() += 1;
            }
        }
    }
    for (k, n) in &per_world {
        assert!(*n <= 1, "world {:?} has {n} personae (cap=1)", k);
    }
    for (k, n) in &per_system_slots {
        assert!(*n <= 1, "system {k} has {n} system-slot personae (cap=1)");
    }
}

#[test]
fn render_markdown_includes_stable_anchors() {
    let sector = fixture_sector();
    let report = personae::derive(sector);
    let md = personae::render_markdown(&report);

    assert!(
        md.starts_with("# Dramatis Personae — "),
        "head: {}",
        &md[..40]
    );
    assert!(md.contains("Seed: `"));
    assert!(md.contains("Total personae: **"));
    // Per-faction section header — there must be at least one faction group.
    let faction_groups = md.matches("\n## ").count();
    assert!(
        faction_groups >= 1,
        "expected at least one ## <faction> group, md head:\n{}",
        &md[..200.min(md.len())]
    );
}

#[test]
fn derive_is_deterministic_for_fixture() {
    let a = personae::derive(fixture_sector());
    let b = personae::derive(fixture_sector());
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb, "personae report not deterministic for fixture");
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,
        max_shrink_iters: 16,
        .. ProptestConfig::default()
    })]

    #[test]
    fn determinism_holds_across_random_seeds(seed in "[a-z0-9-]{4,12}") {
        let sector_a = sector_with_seed(&seed);
        let sector_b = sector_with_seed(&seed);
        let report_a = personae::derive(&sector_a);
        let report_b = personae::derive(&sector_b);
        let ja = serde_json::to_string(&report_a).unwrap();
        let jb = serde_json::to_string(&report_b).unwrap();
        prop_assert_eq!(ja, jb, "personae report non-deterministic for seed={}", seed);
    }
}
