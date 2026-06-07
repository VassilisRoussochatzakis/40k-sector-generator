//! Integration coverage for [`sectorforge::relations`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `RelationsReport` JSON across
//!    many random seeds (proptest).
//! 2. Structural invariants — every present-faction pair is in the matrix, each
//!    pair appears exactly once, canonical ordering holds.
//! 3. Golden markdown — stable headings + full-matrix header row for the
//!    bundled fixture.

use proptest::prelude::*;
use sectorforge::{
    generate_sector, load_project,
    relations::{self, load_relations_file, RelationAttitude, RelationsConfig, RelationsReport, Stance},
    GeneratedSector,
};

use crate::shared::{fixture_dir, fixture_sector};

/// G1: generate a fresh sector from `seed`, independent of the memoised
/// [`fixture_sector`], so the proptest genuinely exercises seed variance
/// rather than re-deriving on one cached sector.
fn gen_sector(seed: &str) -> GeneratedSector {
    let mut input = load_project(fixture_dir()).expect("load fixture");
    input.config.generation.seed = seed.to_string();
    generate_sector(input).expect("generate fixture")
}

fn build_report(sector: &GeneratedSector, cfg: &RelationsConfig) -> RelationsReport {
    RelationsReport {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        matrix: relations::derive_with(sector, cfg),
    }
}

#[test]
fn matrix_covers_every_present_faction_pair() {
    let sector = fixture_sector();
    let matrix = relations::derive(sector);

    let n = sector.factions.len();
    let expected = n * n.saturating_sub(1) / 2;
    assert_eq!(
        matrix.pairs.len(),
        expected,
        "pair count {} != C({n},2)={expected}",
        matrix.pairs.len()
    );

    // Canonical ordering: a < b for every pair, no duplicates.
    let mut seen = std::collections::BTreeSet::new();
    for p in &matrix.pairs {
        assert!(
            p.a.as_str() < p.b.as_str(),
            "pair not canonical: {} {}",
            p.a,
            p.b
        );
        let key = (p.a.to_string(), p.b.to_string());
        assert!(seen.insert(key), "duplicate pair {} ↔ {}", p.a, p.b);
    }
}

#[test]
fn stance_between_lookup_is_order_independent() {
    let matrix = relations::derive(fixture_sector());
    let Some(first) = matrix.pairs.first() else {
        return;
    };
    let fwd = matrix.stance_between(first.a.as_str(), first.b.as_str());
    let rev = matrix.stance_between(first.b.as_str(), first.a.as_str());
    assert_eq!(fwd, rev, "stance_between is direction-sensitive");
    assert_eq!(fwd, Some(first.stance));
}

#[test]
fn tension_and_metrics_are_within_valid_ranges() {
    let matrix = relations::derive(fixture_sector());
    for p in &matrix.pairs {
        assert!(
            p.tension.is_finite() && (0.0..=100.0).contains(&p.tension),
            "{} ↔ {} tension out of [0,100]: {}",
            p.a,
            p.b,
            p.tension
        );
        // Every metric is clamped to 0..=100 in derive.rs (clamp_score /
        // min(100)). Assert the documented bound (not the u8 0..=255 ceiling)
        // so future tuning that overflows 100 fails loudly; round-trip too (G9).
        for (label, m) in [
            ("pair", &p.metrics),
            ("a_to_b", &p.a_to_b.metrics),
            ("b_to_a", &p.b_to_a.metrics),
        ] {
            for (name, v) in [
                ("trust", m.trust),
                ("fear", m.fear),
                ("rivalry", m.rivalry),
                ("ideological_distance", m.ideological_distance),
                ("economic_dependency", m.economic_dependency),
                ("military_pressure", m.military_pressure),
                ("covert_activity", m.covert_activity),
            ] {
                assert!(v <= 100, "{} ↔ {} {label}.{name} > 100: {v}", p.a, p.b);
            }
            let _ = serde_json::to_string(m).expect("metric serializes");
        }
    }
}

#[test]
fn hot_stances_carry_nonempty_cause() {
    let matrix = relations::derive(fixture_sector());
    for p in &matrix.pairs {
        if matches!(p.stance, Stance::Hostile | Stance::AtWar) {
            assert!(
                !p.cause.trim().is_empty(),
                "hot pair {} ↔ {} has empty cause",
                p.a,
                p.b
            );
        }
    }
}

#[test]
fn render_markdown_includes_stable_anchors() {
    let sector = fixture_sector();
    let report = build_report(sector, &RelationsConfig::default());
    let md = relations::render_markdown(&report);

    assert!(md.starts_with("# Diplomacy — "), "md head: {}", &md[..40]);
    assert!(md.contains("Seed: `"));
    assert!(md.contains("Total pairs: **"));
    assert!(md.contains("## Full matrix"));
    assert!(md.contains("## Faction dossiers"));
    // Column header row exists with all the metrics columns.
    assert!(md.contains("| A | B | Public | Secret | Treaty | Trust | Fear | Rivalry"));
}

#[test]
fn derive_is_deterministic_for_fixture() {
    let a = relations::derive(fixture_sector());
    let b = relations::derive(fixture_sector());
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb, "relations matrix not deterministic for fixture");
}

/// D1: `load_relations_file` parses the bundled `relations.toml`, the loaded
/// config is non-default (knobs are set), and feeding it through
/// [`relations::derive_with`] moves numbers vs the bare [`relations::derive`] —
/// while a re-run on the same loaded config stays byte-identical (determinism).
#[test]
fn load_relations_file_changes_derivation_deterministically() {
    let path = fixture_dir().join("data/factions/relations.toml");
    let cfg = load_relations_file(&path).expect("load relations.toml");

    // 1. Loaded config is genuinely non-default — the file sets these knobs.
    assert!(cfg.feed_conflict, "feed_conflict not loaded from file");
    assert!(!cfg.kind_rules.is_empty(), "kind_rules not loaded");
    assert!(!cfg.overrides.is_empty(), "rich overrides not loaded");
    assert!(!cfg.pair_overrides.is_empty(), "pair_overrides not loaded");

    let sector = fixture_sector();

    // 2. derive (built-in defaults) differs from derive_with(loaded) — the
    //    authored kind_rules + overrides change stances/attitudes, so the
    //    serialized matrix is not byte-identical.
    let base = serde_json::to_string(&relations::derive(sector)).unwrap();
    let with = serde_json::to_string(&relations::derive_with(sector, &cfg)).unwrap();
    assert_ne!(base, with, "loaded config did not change the derived matrix");

    // 3. Re-running derive_with on the same loaded config is byte-identical.
    let with2 = serde_json::to_string(&relations::derive_with(sector, &cfg)).unwrap();
    assert_eq!(with, with2, "derive_with not deterministic for loaded config");

    // 4. (Conditional) The file's rich override pins ecclesiarchy×inquisition
    //    public_attitude = "allied". If those faction ids exist in the m42
    //    fixture, the derived pair must carry that attitude; canonical order is
    //    "ecclesiarchy" < "inquisition". Gated so a missing pair is a no-op.
    let matrix = relations::derive_with(sector, &cfg);
    if let Some(rel) = matrix
        .pairs
        .iter()
        .find(|p| p.a.as_str() == "ecclesiarchy" && p.b.as_str() == "inquisition")
    {
        assert_eq!(
            rel.public_attitude,
            RelationAttitude::Allied,
            "rich override did not pin ecclesiarchy×inquisition public_attitude",
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        max_shrink_iters: 32,
        ..ProptestConfig::default()
    })]

    /// G1: vary the generation seed and confirm the relations derivation is a
    /// pure function of the resulting sector — two independent generations from
    /// the same seed yield byte-identical report JSON, with canonical pair order.
    #[test]
    fn relations_derive_deterministic_across_seeds(seed in "[a-z0-9]{4,12}") {
        let s1 = gen_sector(&seed);
        let s2 = gen_sector(&seed);
        let a = build_report(&s1, &RelationsConfig::default());
        let b = build_report(&s2, &RelationsConfig::default());
        prop_assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        for p in &a.matrix.pairs {
            prop_assert!(p.a.as_str() < p.b.as_str(), "pair not canonical: {} {}", p.a, p.b);
        }
    }
}
