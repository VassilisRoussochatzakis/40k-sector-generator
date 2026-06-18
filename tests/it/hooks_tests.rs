//! Integration coverage for [`sectorforge::hooks`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `HooksReport` JSON across many
//!    random seeds (proptest).
//! 2. Structural invariants — every hook anchors to a real system/world/route,
//!    ids are unique, weights are sorted descending, `gm_only` honoured by
//!    `hide_hidden_hooks`.
//! 3. Golden markdown — stable headings + per-hook attribute lines.

use proptest::prelude::*;
use sectorforge::hooks::{self, HookAnchor, HooksConfig};

use crate::shared::{
    assert_derive_deterministic, derivation_proptest_config, fixture_sector, gen_sector, world_keys,
};

#[test]
fn report_metadata_mirrors_sector() {
    let sector = fixture_sector();
    let report = hooks::derive(sector);
    assert_eq!(report.sector_id, sector.id.to_string());
    assert_eq!(report.seed, sector.seed.to_string());
}

#[test]
fn every_hook_anchors_to_a_real_target_and_id_is_unique() {
    let sector = fixture_sector();
    let report = hooks::derive(sector);

    let system_ids: std::collections::BTreeSet<&str> =
        sector.systems.iter().map(|s| s.id.as_str()).collect();
    let route_ids: std::collections::BTreeSet<&str> =
        sector.routes.iter().map(|r| r.id.as_str()).collect();
    let world_keys = world_keys(sector);

    let mut seen_ids = std::collections::BTreeSet::new();
    for h in &report.hooks {
        assert!(seen_ids.insert(h.id.clone()), "duplicate hook id: {}", h.id);
        match &h.anchor {
            HookAnchor::System { system_id } => {
                assert!(
                    system_ids.contains(system_id.as_str()),
                    "hook {} anchor on missing system {}",
                    h.id,
                    system_id
                );
            }
            HookAnchor::World {
                system_id,
                world_id,
            } => {
                assert!(
                    world_keys.contains(&(system_id.to_string(), world_id.to_string())),
                    "hook {} anchor on missing world {}/{}",
                    h.id,
                    system_id,
                    world_id
                );
            }
            HookAnchor::Route { route_id } => {
                assert!(
                    route_ids.contains(route_id.as_str()),
                    "hook {} anchor on missing route {}",
                    h.id,
                    route_id
                );
            }
            _ => {}
        }
    }
}

#[test]
fn hooks_are_sorted_by_descending_weight() {
    let report = hooks::derive(fixture_sector());
    let mut last: Option<u32> = None;
    for h in &report.hooks {
        if let Some(prev) = last {
            assert!(
                prev >= h.weight,
                "hook {} (w={}) appears after w={}",
                h.id,
                h.weight,
                prev
            );
        }
        last = Some(h.weight);
    }
}

#[test]
fn hide_hidden_hooks_filters_gm_only_entries() {
    let sector = fixture_sector();
    let gm_cfg = HooksConfig::default();
    let player_cfg = HooksConfig {
        hide_hidden_hooks: true,
        ..Default::default()
    };
    let gm_report = hooks::derive_with(sector, &gm_cfg);
    let player_report = hooks::derive_with(sector, &player_cfg);

    assert!(player_report.hooks.iter().all(|h| !h.gm_only));
    assert!(
        player_report.hooks.len() <= gm_report.hooks.len(),
        "player-edition cannot expose more hooks than GM edition"
    );
}

#[test]
fn render_markdown_includes_stable_anchors() {
    let sector = fixture_sector();
    let cfg = HooksConfig::default();
    let report = hooks::derive_with(sector, &cfg);
    let md = hooks::render_markdown(&report, &cfg);

    assert!(md.starts_with("# Plot Hooks — "), "head: {}", &md[..40]);
    assert!(md.contains("Seed: `"));
    assert!(md.contains("Total hooks: **"));
    assert!(md.contains("## Top hooks"));
    if !report.hooks.is_empty() {
        assert!(md.contains("- **Weight**:"));
        assert!(md.contains("- **Situation**:"));
        assert!(md.contains("- **Stakes**:"));
    }
}

#[test]
fn derive_is_deterministic_for_fixture() {
    assert_derive_deterministic(|| hooks::derive(fixture_sector()));
}

proptest! {
    #![proptest_config(derivation_proptest_config())]

    /// G1: vary the generation seed and confirm the hooks derivation is a pure
    /// function of the resulting sector — two independent generations from the
    /// same seed yield byte-identical report JSON, with weights non-increasing.
    #[test]
    fn hooks_derive_deterministic_across_seeds(seed in "[a-z0-9]{4,12}") {
        let s1 = gen_sector(&seed);
        let s2 = gen_sector(&seed);
        let a = hooks::derive(&s1);
        let b = hooks::derive(&s2);
        prop_assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        let mut last: Option<u32> = None;
        for h in &a.hooks {
            if let Some(prev) = last {
                prop_assert!(prev >= h.weight);
            }
            last = Some(h.weight);
        }
    }
}
