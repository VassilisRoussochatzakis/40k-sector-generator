//! Integration coverage for [`sectorforge::economy`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `EconomyReport` JSON, across
//!    many random seeds (proptest).
//! 2. Structural invariants — per-world entries, per-system entries, sector
//!    balance keys, strategic outputs.
//! 3. Golden markdown — stable headings + table rows for a fixed fixture seed.

use std::sync::OnceLock;

use camino::Utf8PathBuf;
use proptest::prelude::*;
use sectorforge::{
    economy::{self, EconomyConfig, EconomyReport, RESOURCE_KEYS, STRATEGIC_RESOURCE_KEYS},
    generate_sector, load_project, GeneratedSector,
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

fn enabled_cfg() -> EconomyConfig {
    EconomyConfig {
        enabled: true,
        ..Default::default()
    }
}

fn sector_with_seed(seed: &str) -> GeneratedSector {
    let mut input = load_project(fixture_dir()).expect("load fixture");
    input.config.generation.seed = seed.to_string();
    generate_sector(input).expect("generate sector")
}

#[test]
fn disabled_config_returns_empty_report() {
    let sector = fixture_sector();
    let report = economy::derive_with(sector, &EconomyConfig::default());
    assert!(!report.enabled);
    assert!(report.worlds.is_empty());
    assert!(report.systems.is_empty());
    assert!(report.routes.is_empty());
}

#[test]
fn enabled_derive_populates_per_world_and_per_system_entries() {
    let sector = fixture_sector();
    let report = economy::derive_with(sector, &enabled_cfg());

    assert!(report.enabled);
    assert_eq!(report.systems.len(), sector.systems.len());

    let world_count: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    assert_eq!(report.worlds.len(), world_count);

    // Every route in the sector is mirrored 1:1 in the economy report.
    assert_eq!(report.routes.len(), sector.routes.len());

    // Every per-world entry references a real system+world id from the sector.
    let world_ids: std::collections::BTreeSet<(String, String)> = sector
        .systems
        .iter()
        .flat_map(|s| {
            s.worlds
                .iter()
                .map(move |w| (s.id.to_string(), w.id.to_string()))
        })
        .collect();
    for w in &report.worlds {
        assert!(
            world_ids.contains(&(w.system_id.to_string(), w.world_id.to_string())),
            "report references unknown world {}/{}",
            w.system_id,
            w.world_id,
        );
    }
}

#[test]
fn route_volumes_and_friction_are_finite_and_clamped() {
    let report = economy::derive_with(fixture_sector(), &enabled_cfg());
    for r in &report.routes {
        assert!(
            r.volume.is_finite(),
            "route {} volume non-finite",
            r.route_id
        );
        // `friction_for` clamps to [0.0, 1.5] (stable + patrolled routes can
        // exceed the baseline of 1.0).
        assert!(
            r.friction.is_finite() && (0.0..=1.5).contains(&r.friction),
            "route {} friction outside [0,1.5]: {}",
            r.route_id,
            r.friction
        );
    }
}

#[test]
fn strategic_outputs_are_finite_and_clamped() {
    let report = economy::derive_with(fixture_sector(), &enabled_cfg());
    for k in STRATEGIC_RESOURCE_KEYS {
        let v = report.strategic_output.get(k);
        assert!(v.is_finite(), "{k} strategic-output non-finite");
    }
    for sys in &report.systems {
        for k in STRATEGIC_RESOURCE_KEYS {
            let v = sys.strategic_output.get(k);
            assert!(v.is_finite(), "{} {} non-finite", sys.system_id, k);
            // Per-world clamps to [0,100] in derive; sum across worlds may
            // exceed 100 but must stay non-negative.
            assert!(v >= 0.0, "{} {} negative: {}", sys.system_id, k, v);
        }
    }
}

#[test]
fn render_markdown_includes_stable_anchors() {
    let sector = fixture_sector();
    let report = economy::derive_with(sector, &enabled_cfg());
    let md = economy::render_markdown(&sector.id, &report);

    assert!(md.starts_with("# Economy — "));
    assert!(md.contains("## Sector balance"));
    assert!(md.contains("## Strategic output"));
    assert!(md.contains("## Systems"));
    for k in RESOURCE_KEYS {
        assert!(
            md.contains(&format!("| {k} |")),
            "missing resource row: {k}"
        );
    }
    for k in STRATEGIC_RESOURCE_KEYS {
        assert!(
            md.contains(&format!("| {k} |")),
            "missing strategic row: {k}"
        );
    }
}

#[test]
fn render_markdown_disabled_message_when_no_derivation() {
    let sector = fixture_sector();
    let md = economy::render_markdown(&sector.id, &EconomyReport::default());
    assert!(md.contains("Economy derivation disabled"));
}

#[test]
fn derive_is_deterministic_for_fixture() {
    let a = economy::derive_with(fixture_sector(), &enabled_cfg());
    let b = economy::derive_with(fixture_sector(), &enabled_cfg());
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb, "economy report not deterministic for fixture");
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,
        max_shrink_iters: 16,
        .. ProptestConfig::default()
    })]

    /// Same seed ⇒ same `EconomyReport` JSON. Random seed across a broad
    /// alphabet to surface RNG leaks in derivation.
    #[test]
    fn determinism_holds_across_random_seeds(seed in "[a-z0-9-]{4,12}") {
        let sector_a = sector_with_seed(&seed);
        let sector_b = sector_with_seed(&seed);
        let report_a = economy::derive_with(&sector_a, &enabled_cfg());
        let report_b = economy::derive_with(&sector_b, &enabled_cfg());
        let ja = serde_json::to_string(&report_a).unwrap();
        let jb = serde_json::to_string(&report_b).unwrap();
        prop_assert_eq!(ja, jb, "economy report non-deterministic for seed={}", seed);
    }
}
