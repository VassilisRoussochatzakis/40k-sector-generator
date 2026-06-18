//! §2 NEW.md + §10 NEW.md — seed search and sector diff integration tests.

use camino::Utf8PathBuf;
use sectorforge::diff::DiffConfig;
use sectorforge::search::{Constraint, SearchConfig, WishesFile};

use crate::shared::{fixture_dir as fixture_project, fixture_sector};

/// Build a `WishesFile` from the repeated `search: SearchConfig { .. }` skeleton.
/// Each test supplies its own distinct `base_seed`, `budget`, `report_top`, and
/// `constraints`; only the boilerplate is factored here.
fn wishes(base_seed: &str, budget: u32, report_top: u32, constraints: Vec<Constraint>) -> WishesFile {
    WishesFile {
        search: SearchConfig {
            base_seed: Some(base_seed.into()),
            budget,
            report_top,
        },
        constraints,
    }
}

// ── §2 SEARCH ──────────────────────────────────────────────────────────────────

#[test]
fn search_trivial_wishes_wins_immediately() {
    // Empty constraint list: every candidate satisfies all constraints (zero
    // of them). The winner is the n=0 candidate = the base seed verbatim.
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = wishes("trivial", 4, 5, Vec::new());
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
    let wishes = wishes(
        "preflight",
        1,
        1,
        vec![Constraint::FactionShareMin {
            faction_id: "no_such_faction".into(),
            min: 0.10,
        }],
    );
    let outcome = sectorforge::run_seed_search(&input, &wishes).unwrap();
    assert!(outcome.winning.is_none());
    assert!(!outcome.preflight_errors.is_empty());
}

#[test]
fn search_is_deterministic_for_same_inputs() {
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let wishes = wishes("repeat", 4, 5, vec![Constraint::RouteGraphConnected]);
    let a = sectorforge::run_seed_search(&input, &wishes).unwrap();
    let b = sectorforge::run_seed_search(&input, &wishes).unwrap();
    let aj = serde_json::to_string_pretty(&a).unwrap();
    let bj = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(aj, bj);
}

// #17: `run_search` enumerates candidate seeds with rayon's `into_par_iter`
// (FIX.txt §13). The order-preserving `collect` is supposed to keep the whole
// serialized `SearchOutcome` — crucially the `near_misses` set, which is filled
// concurrently across workers and then sorted — byte-identical regardless of how
// many threads ran. This pins that: the same inputs produce the same serialized
// output whether the search runs on a forced SINGLE-thread rayon pool or on the
// default multi-threaded global pool. The constraint is deliberately impossible
// to satisfy so every candidate becomes a near-miss and the `near_misses`
// codepath is actually exercised (asserted non-empty below).
#[test]
fn search_near_misses_are_byte_identical_single_vs_multi_thread() {
    let project = fixture_project();
    // A faction share of >= 0.99 for a single faction is unreachable on the m42
    // fixture, so no candidate wins and every evaluated candidate lands in
    // `near_misses` (capped at `report_top`).
    let wishes = wishes(
        "near-miss-determinism",
        6,
        5,
        vec![Constraint::FactionShareMin {
            faction_id: "imperial_administration".into(),
            min: 0.99,
        }],
    );

    let input = sectorforge::load_project(&project).unwrap();

    // (a) Forced single-thread: a 1-thread rayon pool. `into_par_iter` inside
    // `install` runs on this pool, so the parallel enumeration is serialised.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let single = pool.install(|| sectorforge::run_seed_search(&input, &wishes).unwrap());

    // (b) Default parallel: the global rayon pool (multi-threaded on CI/dev).
    let multi = sectorforge::run_seed_search(&input, &wishes).unwrap();

    // The search must have produced near-misses (no winner for an impossible
    // constraint) — otherwise this test would be vacuous.
    assert!(
        single.winning.is_none(),
        "impossible constraint must not win"
    );
    assert!(
        !single.near_misses.is_empty(),
        "an impossible constraint must populate near_misses"
    );

    // Whole-outcome byte equality (serialized), which includes `near_misses`
    // and their order.
    let single_json = serde_json::to_string(&single).unwrap();
    let multi_json = serde_json::to_string(&multi).unwrap();
    assert_eq!(
        single_json, multi_json,
        "run_search output (incl. near_misses) must be byte-identical \
         single-thread vs parallel"
    );

    // Belt-and-braces: a second default-pool run is also identical, guarding the
    // run-to-run determinism the parallel collect promises.
    let multi2 = sectorforge::run_seed_search(&input, &wishes).unwrap();
    assert_eq!(
        multi_json,
        serde_json::to_string(&multi2).unwrap(),
        "two default-pool runs must be byte-identical too"
    );
}

#[test]
fn search_with_progress_matches_run_search_and_reports() {
    // §SR2: the progress-reporting variant must produce a byte-identical
    // outcome to `run_search`, and must invoke the callback at least once with
    // a `tried` count inside [1, budget].
    use std::sync::atomic::{AtomicU32, Ordering};

    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = wishes("progress", 4, 5, vec![Constraint::RouteGraphConnected]);

    let plain = sectorforge::run_seed_search(&input, &wishes).unwrap();

    let calls = AtomicU32::new(0);
    let max_tried = AtomicU32::new(0);
    let with_progress = sectorforge::search::run_search_with_progress(&input, &wishes, |p| {
        calls.fetch_add(1, Ordering::Relaxed);
        max_tried.fetch_max(p.tried, Ordering::Relaxed);
        assert_eq!(p.budget, 4);
        assert!(p.tried >= 1 && p.tried <= p.budget);
        assert!(p.passed <= p.tried);
    })
    .unwrap();

    assert_eq!(
        serde_json::to_string_pretty(&plain).unwrap(),
        serde_json::to_string_pretty(&with_progress).unwrap(),
    );
    assert!(calls.load(Ordering::Relaxed) >= 1);
    let mt = max_tried.load(Ordering::Relaxed);
    assert!((1..=4).contains(&mt));
}

#[test]
fn search_writes_markdown_and_json() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let wishes = wishes("emit", 2, 2, Vec::new());
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

    let wishes = wishes(
        "story",
        10,
        5,
        vec![
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
    );

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
    let (diff, before, after) =
        sectorforge::diff::diff_after_ticks(input, 5, &DiffConfig::default()).unwrap();
    // Markdown round-trip crash guard.
    let md = sectorforge::render_diff_markdown(&diff, &DiffConfig::default());
    assert!(md.starts_with("# Sector Diff"));
    assert!(md.contains("Catalog compatible"));
    // Meaningful content check: `advance_sector` must actually evolve conflict
    // state, so `after` differs from `before`. The diff markdown does not yet
    // surface conflict age/momentum/intensity, so assert on the world conflict
    // fields directly — this guards the "advance_sector silently became a
    // no-op" regression (G8). System ordering is stable across ticks.
    let mut conflict_evolved = 0usize;
    for (sb, sa) in before.systems.iter().zip(after.systems.iter()) {
        assert_eq!(sb.id, sa.id, "system ordering must be stable across ticks");
        for (wb, wa) in sb.worlds.iter().zip(sa.worlds.iter()) {
            if wb.conflict.age != wa.conflict.age
                || wb.conflict.momentum != wa.conflict.momentum
                || wb.conflict.intensity != wa.conflict.intensity
            {
                conflict_evolved += 1;
            }
        }
    }
    assert!(
        conflict_evolved > 0,
        "5 ticks of advance_sector must evolve at least one world's conflict state"
    );
}

#[test]
fn diff_writers_emit_both_files() {
    let sector = fixture_sector();
    let d = sectorforge::diff_sectors(sector, sector);
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::write_diff(&dir, &d, &DiffConfig::default()).unwrap();
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
