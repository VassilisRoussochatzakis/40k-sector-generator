//! Integration coverage for [`sectorforge::personae`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `PersonaeReport` JSON across
//!    many random seeds (proptest).
//! 2. Structural invariants — every persona references a valid faction and a
//!    valid system/world anchor; ids unique; per-world and per-system caps
//!    honoured by the public config knobs.
//! 3. Golden markdown — stable headings and faction grouping for the fixture.

use proptest::prelude::*;
use sectorforge::personae::{self, PersonaAnchor, PersonaeConfig};

use crate::shared::{
    assert_derive_deterministic, derivation_proptest_config, fixture_sector, gen_sector, world_keys,
};

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

    let world_keys = world_keys(sector);

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
            _ => {}
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
            _ => {}
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
    assert_derive_deterministic(|| personae::derive(fixture_sector()));
}

proptest! {
    #![proptest_config(derivation_proptest_config())]

    /// G1: vary the generation seed and confirm the personae derivation is a
    /// pure function of the resulting sector — two independent generations from
    /// the same seed yield byte-identical report JSON, with unique persona names.
    #[test]
    fn personae_derive_deterministic_across_seeds(seed in "[a-z0-9]{4,12}") {
        let s1 = gen_sector(&seed);
        let s2 = gen_sector(&seed);
        let a = personae::derive(&s1);
        let b = personae::derive(&s2);
        prop_assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        let mut seen = std::collections::BTreeSet::new();
        for p in &a.personae {
            prop_assert!(seen.insert(p.name.clone()), "duplicate persona name: {}", p.name);
        }
    }
}
