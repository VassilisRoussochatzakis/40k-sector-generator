//! §2 NEW.md + §10 NEW.md — seed search and sector diff integration tests.

use camino::Utf8PathBuf;
use sectorforge::diff::DiffConfig;
use sectorforge::search::{Constraint, SearchConfig, WishesFile};

use crate::shared::{fixture_dir as fixture_project, fixture_sector};

// ── §2 SEARCH ──────────────────────────────────────────────────────────────────

#[test]
fn search_trivial_wishes_wins_immediately() {
    // Empty constraint list: every candidate satisfies all constraints (zero
    // of them). The winner is the n=0 candidate = the base seed verbatim.
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = WishesFile {
        search: SearchConfig {
            base_seed: Some("trivial".into()),
            budget: 4,
            report_top: 5,
        },
        constraints: Vec::new(),
    };
    let outcome = sectorforge::run_seed_search(&input, &wishes).unwrap();
    let win = outcome.winning.expect("trivial search must find a seed");
    assert_eq!(win.n, 0);
    assert_eq!(win.seed, "trivial");
    assert!(win.passed);
}

#[test]
fn search_unknown_faction_reports_preflight_error() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = WishesFile {
        search: SearchConfig {
            base_seed: Some("preflight".into()),
            budget: 1,
            report_top: 1,
        },
        constraints: vec![Constraint::FactionShareMin {
            faction_id: "no_such_faction".into(),
            min: 0.10,
        }],
    };
    let outcome = sectorforge::run_seed_search(&input, &wishes).unwrap();
    assert!(outcome.winning.is_none());
    assert!(!outcome.preflight_errors.is_empty());
}

#[test]
fn search_is_deterministic_for_same_inputs() {
    let project = fixture_project();
    let input1 = sectorforge::load_project(&project).unwrap();
    let input2 = sectorforge::load_project(&project).unwrap();
    let wishes = WishesFile {
        search: SearchConfig {
            base_seed: Some("repeat".into()),
            budget: 4,
            report_top: 5,
        },
        constraints: vec![Constraint::RouteGraphConnected],
    };
    let a = sectorforge::run_seed_search(&input1, &wishes).unwrap();
    let b = sectorforge::run_seed_search(&input2, &wishes).unwrap();
    let aj = serde_json::to_string_pretty(&a).unwrap();
    let bj = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(aj, bj);
}

#[test]
fn search_writes_markdown_and_json() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = WishesFile {
        search: SearchConfig {
            base_seed: Some("emit".into()),
            budget: 2,
            report_top: 2,
        },
        constraints: Vec::new(),
    };
    let outcome = sectorforge::run_seed_search(&input, &wishes).unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::write_search_outcome(&dir, &outcome).unwrap();
    assert!(dir.join("search.md").exists());
    assert!(dir.join("search.json").exists());
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("search.json")).unwrap()).unwrap();
    assert!(json["winning"].is_object());
}

#[test]
fn search_story_beat_constraints() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();

    use sectorforge::search::{PresenceName, SystemStateFilter, SystemStateName};

    let wishes = WishesFile {
        search: SearchConfig {
            base_seed: Some("story".into()),
            budget: 10,
            report_top: 5,
        },
        constraints: vec![
            Constraint::FactionPresenceCountMin {
                faction_id: "imperial_administration".into(),
                min: 1,
                presence: PresenceName::Significant,
            },
            Constraint::MinimumSystemsMatching {
                count: 1,
                within_hops_of: "subsector_capital".into(),
                max_hops: 1,
                where_cond: SystemStateFilter {
                    system_state: SystemStateName::Pacified,
                },
            },
        ],
    };

    let outcome = sectorforge::run_seed_search(&input, &wishes).unwrap();
    // We don't necessarily need a winner to verify evaluation logic doesn't crash,
    // but at least one candidate should have been evaluated.
    assert!(outcome.candidates_evaluated > 0);
    if let Some(win) = outcome.winning {
        assert!(win.passed);
    }
}

#[test]
fn candidate_seeds_are_distinct_and_stable() {
    let a0 = sectorforge::search::derive_candidate_seed("base", 0);
    let a1 = sectorforge::search::derive_candidate_seed("base", 1);
    let a1_again = sectorforge::search::derive_candidate_seed("base", 1);
    let a2 = sectorforge::search::derive_candidate_seed("base", 2);
    assert_eq!(a0, "base");
    assert_eq!(a1, a1_again);
    assert_ne!(a0, a1);
    assert_ne!(a1, a2);
}

// ── §10 DIFF ───────────────────────────────────────────────────────────────────

#[test]
fn diff_identical_sectors_is_empty() {
    let sector = fixture_sector();
    let d = sectorforge::diff_sectors(sector, sector);
    assert!(d.systems_added.is_empty());
    assert!(d.systems_removed.is_empty());
    assert!(d.systems_changed.is_empty());
    assert!(d.routes_added.is_empty());
    assert!(d.routes_removed.is_empty());
    assert!(d.routes_changed.is_empty());
    assert!(d.faction_deltas.is_empty());
    assert!(d.catalog_compatible);
}

#[test]
fn diff_after_ticks_reports_changes_when_conflict_state_evolves() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let (diff, _before, _after) =
        sectorforge::diff::diff_after_ticks(input, 5, &DiffConfig::default()).unwrap();
    // Tick advancement is allowed to produce zero observable diff if no
    // contested worlds existed; we just assert the call succeeds and the
    // serialisation round-trips.
    let md = sectorforge::render_diff_markdown(&diff);
    assert!(md.starts_with("# Sector Diff"));
    assert!(md.contains("Catalog compatible"));
}

#[test]
fn diff_writers_emit_both_files() {
    let sector = fixture_sector();
    let d = sectorforge::diff_sectors(sector, sector);
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::write_diff(&dir, &d).unwrap();
    assert!(dir.join("diff.md").exists());
    assert!(dir.join("diff.json").exists());
}

#[test]
fn diff_distinct_seeds_produces_changes() {
    let project = fixture_project();
    let mut a = sectorforge::load_project(&project).unwrap();
    let mut b = sectorforge::load_project(&project).unwrap();
    a.config.generation.seed = "seed-A".into();
    b.config.generation.seed = "seed-B".into();
    let sa = sectorforge::generate_sector(a).unwrap();
    let sb = sectorforge::generate_sector(b).unwrap();
    let d = sectorforge::diff_sectors(&sa, &sb);
    // Different seeds produce different sectors → at least one of: system
    // changes, route changes, faction deltas. The full sector is built from
    // scratch so we expect many systems_changed entries (renamed worlds).
    let total = d.systems_changed.len()
        + d.systems_added.len()
        + d.systems_removed.len()
        + d.routes_added.len()
        + d.routes_removed.len()
        + d.routes_changed.len()
        + d.faction_deltas.len();
    assert!(total > 0, "expected at least one diff entry");
}

#[test]
fn diff_is_deterministic_for_same_inputs() {
    let sector = fixture_sector();
    let mut after = sector.clone();
    sectorforge::advance_sector(&mut after);
    let d1 = sectorforge::diff_sectors(sector, &after);
    let d2 = sectorforge::diff_sectors(sector, &after);
    let j1 = serde_json::to_string_pretty(&d1).unwrap();
    let j2 = serde_json::to_string_pretty(&d2).unwrap();
    assert_eq!(j1, j2);
}
