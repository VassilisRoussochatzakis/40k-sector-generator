//! Integration coverage for [`sectorforge::hooks`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `HooksReport` JSON across many
//!    random seeds (proptest).
//! 2. Structural invariants — every hook anchors to a real system/world/route,
//!    ids are unique, weights are sorted descending, `gm_only` honoured by
//!    `hide_hidden_hooks`.
//! 3. Golden markdown — stable headings + per-hook attribute lines.

use std::sync::OnceLock;

use camino::Utf8PathBuf;
use proptest::prelude::*;
use sectorforge::{
    generate_sector,
    hooks::{self, HookAnchor, HooksConfig},
    load_project, GeneratedSector,
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
    let world_keys: std::collections::BTreeSet<(String, String)> = sector
        .systems
        .iter()
        .flat_map(|s| {
            s.worlds
                .iter()
                .map(move |w| (s.id.to_string(), w.id.to_string()))
        })
        .collect();

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
    let a = hooks::derive(fixture_sector());
    let b = hooks::derive(fixture_sector());
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb, "hooks report not deterministic for fixture");
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
        let report_a = hooks::derive(&sector_a);
        let report_b = hooks::derive(&sector_b);
        let ja = serde_json::to_string(&report_a).unwrap();
        let jb = serde_json::to_string(&report_b).unwrap();
        prop_assert_eq!(ja, jb, "hooks report non-deterministic for seed={}", seed);
    }
}
