//! Integration coverage for [`sectorforge::economy`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `EconomyReport` JSON, across
//!    many random seeds (proptest).
//! 2. Structural invariants — per-world entries, per-system entries, sector
//!    balance keys, strategic outputs.
//! 3. Golden markdown — stable headings + table rows for a fixed fixture seed.

use proptest::prelude::*;
use sectorforge::economy::{
    self, load_economy_file, EconomyConfig, EconomyFile, EconomyReport, ResourceModelConfig,
    StrategicOutputRule, RESOURCE_KEYS, STRATEGIC_RESOURCE_KEYS,
};

use crate::shared::{fixture_dir, fixture_sector, gen_sector};

fn enabled_cfg() -> EconomyConfig {
    EconomyConfig {
        enabled: true,
        ..Default::default()
    }
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

/// D2: `load_economy_file` parses the bundled `economy.toml`, the loaded config
/// is non-default (enabled + full tables), and its authored
/// `by_world_type`/`by_tech_level`/`resources` tables move numbers vs the
/// built-in defaults (`enabled_cfg()`), while a re-run stays byte-identical.
#[test]
fn load_economy_file_changes_derivation_deterministically() {
    let path = fixture_dir().join("data/worlds/economy.toml");
    let cfg = load_economy_file(&path).expect("load economy.toml");

    // 1. Loaded config is genuinely non-default — the file sets these knobs.
    assert!(cfg.enabled, "enabled not loaded from file");
    assert!(cfg.feed_stability, "feed_stability not loaded from file");
    assert!(!cfg.by_world_type.is_empty(), "by_world_type not loaded");
    assert!(!cfg.by_tech_level.is_empty(), "by_tech_level not loaded");
    assert!(!cfg.resources.is_empty(), "strategic resources not loaded");

    let sector = fixture_sector();

    // 2. The loaded tables differ from the built-in defaults, so the same
    //    sector derived with each produces different serialized output.
    //    `enabled_cfg()` is enabled but carries the BUILT-IN tables.
    let loaded = serde_json::to_string(&economy::derive_with(sector, &cfg)).unwrap();
    let builtin = serde_json::to_string(&economy::derive_with(sector, &enabled_cfg())).unwrap();
    assert_ne!(
        loaded, builtin,
        "loaded economy tables produced identical output to built-ins",
    );

    // 3. Re-running derive_with on the same loaded config is byte-identical.
    let loaded2 = serde_json::to_string(&economy::derive_with(sector, &cfg)).unwrap();
    assert_eq!(
        loaded, loaded2,
        "derive_with not deterministic for loaded config"
    );
}

/// Build a non-empty [`ResourceModelConfig`] with a single `world_type` entry
/// (food = 42.0) under `key`, so `is_empty()` is false.
fn rmc_with(key: &str) -> ResourceModelConfig {
    let mut world_type = std::collections::BTreeMap::new();
    world_type.insert(
        key.to_string(),
        StrategicOutputRule {
            food: Some(42.0),
            ..Default::default()
        },
    );
    ResourceModelConfig {
        world_type,
        notable_feature: std::collections::BTreeMap::new(),
    }
}

/// Gap 4: `EconomyFile::into_config` overwrites `economy.resources` with the
/// top-level `[resources]` ONLY when the top-level block is non-empty. When the
/// top-level is empty (false branch), a nested `economy.resources` survives
/// untouched.
#[test]
fn economy_file_into_config_keeps_nested_resources_when_toplevel_empty() {
    let mut f = EconomyFile::default();
    f.economy.resources = rmc_with("hive_world"); // nested populated
                                                  // f.resources stays default (empty) → the `if` is false → nested survives.
    let cfg = f.into_config();
    assert!(
        cfg.resources.world_type.contains_key("hive_world"),
        "nested resources were dropped despite empty top-level block",
    );
    assert_eq!(
        cfg.resources.world_type["hive_world"].food,
        Some(42.0),
        "nested resource value was not preserved",
    );
}

/// Gap 4: when BOTH the nested `economy.resources` and the top-level
/// `[resources]` are populated, the top-level block clobbers the nested one
/// (top-level wins).
#[test]
fn economy_file_into_config_toplevel_resources_clobber_nested() {
    let mut f = EconomyFile::default();
    f.economy.resources = rmc_with("hive_world"); // nested
    f.resources = rmc_with("forge_world"); // top-level (non-empty)
    let cfg = f.into_config();
    assert!(
        cfg.resources.world_type.contains_key("forge_world"),
        "top-level resources did not win",
    );
    assert!(
        !cfg.resources.world_type.contains_key("hive_world"),
        "nested resources were not clobbered by the top-level block",
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        max_shrink_iters: 32,
        ..ProptestConfig::default()
    })]

    /// G1: vary the generation seed and confirm the economy derivation is a pure
    /// function of the resulting sector — two independent generations from the
    /// same seed yield byte-identical report JSON, and friction stays clamped.
    #[test]
    fn economy_derive_deterministic_across_seeds(seed in "[a-z0-9]{4,12}") {
        let s1 = gen_sector(&seed);
        let s2 = gen_sector(&seed);
        let a = economy::derive_with(&s1, &enabled_cfg());
        let b = economy::derive_with(&s2, &enabled_cfg());
        prop_assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        for r in &a.routes {
            prop_assert!(r.friction.is_finite() && (0.0..=1.5).contains(&r.friction));
        }
    }
}
