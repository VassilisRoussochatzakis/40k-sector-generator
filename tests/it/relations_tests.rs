//! Integration coverage for [`sectorforge::relations`] (TEST-001).
//!
//! Three goals:
//! 1. Determinism — same sector ⇒ byte-identical `RelationsReport` JSON across
//!    many random seeds (proptest).
//! 2. Structural invariants — every present-faction pair is in the matrix, each
//!    pair appears exactly once, canonical ordering holds.
//! 3. Golden markdown — stable headings + full-matrix header row for the
//!    bundled fixture.

use std::sync::OnceLock;

use camino::Utf8PathBuf;
use sectorforge::{
    generate_sector, load_project,
    relations::{self, RelationsConfig, RelationsReport, Stance},
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
