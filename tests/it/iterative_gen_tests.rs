//! Phase E (engine seam) tests for `generate_prefix` / `Stage` / `RerollNonces`
//! (`ITERATIVE_GENERATION.md` §4 Phase T 1–3).
//!
//! The linchpin guarantee (§2.2): every stage seeds a *fresh* `ChaCha8` from
//! `blake3(root_seed, stage, disc)`; stages share no stream; therefore a prefix
//! run is byte-for-byte the corresponding prefix of a full run for the same
//! `(config, seed, nonces)`, and the nonce-0 path is byte-identical to the
//! legacy one-shot orchestrator (so the golden suite stays green).

use std::sync::Arc;

use sectorforge::regions::RegionsConfig;
use sectorforge::{generate_prefix, RerollNonces, Stage};

use super::shared::{fixture_dir, fixture_sector};

fn fixture_input() -> sectorforge::ProjectInput {
    sectorforge::load_project(fixture_dir()).expect("load fixture project")
}

/// Clone `fixture_input` but substitute a different `RegionsConfig` for the
/// regions catalog — exactly what the builder's transient session override does
/// at `ProjectInput`-assembly time (`synthesize_project_input_with(Some(cfg))`),
/// without ever touching `data_catalogs`. `regions` is the only field changed.
fn fixture_input_with_regions(regions: RegionsConfig) -> sectorforge::ProjectInput {
    let mut input = fixture_input();
    let mut catalogs = (*input.catalogs).clone();
    catalogs.regions = regions;
    input.catalogs = Arc::new(catalogs);
    input
}

/// Equivalence: `generate_prefix(.., Stage::LAST, &default, ..)` is byte-for-byte
/// the legacy one-shot `generate_sector` (which the wrapper now delegates to).
#[test]
fn prefix_last_default_equals_one_shot() {
    let prefix = generate_prefix(
        fixture_input(),
        Stage::LAST,
        &RerollNonces::default(),
        |_| {},
        || false,
    )
    .expect("generate_prefix LAST");

    let one_shot_json = serde_json::to_string(fixture_sector()).expect("serialize one-shot");
    let prefix_json = serde_json::to_string(&prefix).expect("serialize prefix");
    assert_eq!(
        one_shot_json, prefix_json,
        "generate_prefix(Stage::LAST, default nonces) must equal the one-shot output byte-for-byte",
    );
}

/// Prefix monotonicity. Two properties hold at the *engine* level:
///
/// 1. **Determinism** — running `generate_prefix` twice at the same cutoff with
///    the same `(config, seed, nonces)` is byte-identical. This, combined with
///    the fresh-per-stage RNG (§2.2) and the passing `prefix_last_default_*`
///    equivalence test, is the operational form of "prefix-run ≡ full-run
///    prefix" (the cutoff cannot perturb earlier stages).
/// 2. **Structural stability** — the *count* of structural entities (systems,
///    worlds, routes) at a cutoff that has built them matches the full run.
///    Later stages only *decorate* those entities (faction presence, derived
///    system-state, archetypes) in place; they never add or drop systems/routes.
///    So per-field byte-equality with the full run is NOT expected (the full
///    run's `systems` carry downstream-derived fields), but counts are stable.
#[test]
fn prefix_stages_match_full_run_prefix() {
    let full = fixture_sector();
    let full_world_count: usize = full.systems.iter().map(|s| s.worlds.len()).sum();

    for cutoff in [
        Stage::Systems,
        Stage::Factions,
        Stage::RouteControls,
        Stage::LAST,
    ] {
        let run_a = generate_prefix(
            fixture_input(),
            cutoff,
            &RerollNonces::default(),
            |_| {},
            || false,
        )
        .expect("prefix run a");
        let run_b = generate_prefix(
            fixture_input(),
            cutoff,
            &RerollNonces::default(),
            |_| {},
            || false,
        )
        .expect("prefix run b");
        assert_eq!(
            serde_json::to_string(&run_a).unwrap(),
            serde_json::to_string(&run_b).unwrap(),
            "two prefix runs at cutoff {cutoff:?} must be byte-identical",
        );

        // Systems + worlds are built by Stage::Systems; their structure is stable
        // from that cutoff onward (only decorated later).
        assert_eq!(
            run_a.systems.len(),
            full.systems.len(),
            "system count at cutoff {cutoff:?} must match the full run",
        );
        let world_count: usize = run_a.systems.iter().map(|s| s.worlds.len()).sum();
        assert_eq!(
            world_count, full_world_count,
            "world count at cutoff {cutoff:?} must match the full run",
        );

        // Routes are built by Stage::Routes / finalised by RouteControls.
        if cutoff >= Stage::RouteControls {
            assert_eq!(
                run_a.routes.len(),
                full.routes.len(),
                "route count at cutoff {cutoff:?} must match the full run",
            );
        }

        // Overlays strictly after the cutoff stay empty/default.
        if cutoff < Stage::Routes {
            assert!(
                run_a.routes.is_empty(),
                "routes leaked before Routes cutoff"
            );
        }
        if cutoff < Stage::Relations {
            assert!(
                run_a.relations.pairs.is_empty(),
                "relations leaked before Relations cutoff",
            );
        }
        if cutoff < Stage::Chronicle {
            assert!(
                run_a.chronicle.events.is_empty(),
                "chronicle leaked before Chronicle cutoff",
            );
        }
    }
}

/// Early cutoffs still return a valid, self-consistent sector (partial
/// assembly), never an empty/garbage one.
#[test]
fn early_cutoff_returns_valid_sector() {
    let full = fixture_sector();

    let placement = generate_prefix(
        fixture_input(),
        Stage::Placement,
        &RerollNonces::default(),
        |_| {},
        || false,
    )
    .expect("prefix Placement");

    // Identity + dimensions are populated from config regardless of cutoff.
    assert_eq!(placement.seed.as_ref(), full.seed.as_ref());
    assert_eq!(placement.width, full.width);
    assert_eq!(placement.height, full.height);
    assert_eq!(placement.width, placement.height, "sectors stay square");
    // Manifest is always built (stage 12 is RNG-free and always runs).
    assert_eq!(placement.manifest.system_count, 0);
    // No structural stages past Placement ran, so systems are not yet built.
    assert!(placement.systems.is_empty());
    // The partial sector still passes post-gen invariant validation.
    let report = sectorforge::validate_sector(&placement);
    assert!(
        report.ok,
        "partial (placement-cutoff) sector must be valid: {:?}",
        report.violations
    );
}

/// Re-roll determinism: a non-zero nonce is reproducible run-to-run, differs
/// from the default, and (for a structural stage) actually changes the output.
#[test]
fn reroll_is_deterministic_and_distinct() {
    let mut bumped = RerollNonces::default();
    bumped.bump(Stage::Placement);

    let a = generate_prefix(fixture_input(), Stage::LAST, &bumped, |_| {}, || false)
        .expect("prefix bumped a");
    let b = generate_prefix(fixture_input(), Stage::LAST, &bumped, |_| {}, || false)
        .expect("prefix bumped b");
    let a_json = serde_json::to_string(&a).unwrap();
    let b_json = serde_json::to_string(&b).unwrap();
    assert_eq!(a_json, b_json, "same nonce must reproduce identical output");

    let default_json = serde_json::to_string(fixture_sector()).unwrap();
    assert_ne!(
        default_json, a_json,
        "bumping the Placement nonce must change the output (placement RNG is live)",
    );
}

/// A nonce on a late stage leaves earlier stages untouched: bumping the
/// `Chronicle` nonce changes the chronicle but not the systems/routes/factions.
#[test]
fn late_nonce_leaves_earlier_stages_unchanged() {
    let full = fixture_sector();

    let mut chron_bumped = RerollNonces::default();
    chron_bumped.bump(Stage::Chronicle);
    let rolled = generate_prefix(
        fixture_input(),
        Stage::LAST,
        &chron_bumped,
        |_| {},
        || false,
    )
    .expect("prefix chronicle-bumped");

    // Earlier structural stages are byte-identical.
    assert_eq!(
        serde_json::to_string(&full.systems).unwrap(),
        serde_json::to_string(&rolled.systems).unwrap(),
        "systems must not change when only the Chronicle nonce is bumped",
    );
    assert_eq!(
        serde_json::to_string(&full.routes).unwrap(),
        serde_json::to_string(&rolled.routes).unwrap(),
        "routes must not change when only the Chronicle nonce is bumped",
    );
    assert_eq!(
        serde_json::to_string(&full.factions).unwrap(),
        serde_json::to_string(&rolled.factions).unwrap(),
        "factions must not change when only the Chronicle nonce is bumped",
    );

    // The chronicle itself differs only if the fixture produced any events; if
    // it did, the bump must change them. (Guard against a no-event fixture
    // making this vacuous.)
    if !full.chronicle.events.is_empty() {
        assert_ne!(
            serde_json::to_string(&full.chronicle).unwrap(),
            serde_json::to_string(&rolled.chronicle).unwrap(),
            "the Chronicle nonce bump must change the chronicle",
        );
    }
}

/// ITERATIVE_GENERATION.md §5 — the transient **regions override** (applied at
/// `ProjectInput` assembly) is deterministic and back-compatible:
///
/// 1. Substituting a *changed* `RegionsConfig` (here: a different count + a
///    single forced condition) changes the generated regions overlay — so the
///    builder's session patch reaches the engine and the preview reflects it.
/// 2. Substituting the project's catalog config **verbatim** reproduces the
///    legacy run byte-for-byte — the "override == None" path is byte-identical
///    to today (invariants #2 / #6), because the assembly substitution with the
///    unchanged value is a no-op.
///
/// Headless/pure: this exercises the engine seam (`generate_prefix` over an
/// assembled `ProjectInput`), which is exactly what the builder's
/// `synthesize_project_input_with` feeds it. The regions stage seeds anomaly
/// hexes that bias world selection and (with `apply_to_routes`) rewrite route
/// stability, so we run through `Stage::RouteControls` and compare the `regions`
/// overlay.
#[test]
fn regions_override_changes_preview_and_none_is_byte_identical() {
    let cutoff = Stage::RouteControls;
    let nonces = RerollNonces::default();

    // Baseline: the fixture's own catalog (m42's regions.toml is enabled).
    let legacy = generate_prefix(fixture_input(), cutoff, &nonces, |_| {}, || false)
        .expect("legacy prefix run");
    assert!(
        legacy.regions.iter().count() > 0,
        "fixture precondition: m42 regions.toml is enabled, so the baseline grows regions",
    );

    // (1) A *changed* override must change the regions overlay. Force a single
    //     deterministic condition + a different count so the difference is
    //     unambiguous and not just a re-label.
    let patch = RegionsConfig {
        enabled: true,
        count: 1,
        mean_size: 4,
        apply_to_routes: true,
        conditions: vec![sectorforge::regions::ConditionEntry {
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            weight: 1.0,
            label: Some("Override Storm".to_string()),
        }],
    };
    let patched = generate_prefix(
        fixture_input_with_regions(patch.clone()),
        cutoff,
        &nonces,
        |_| {},
        || false,
    )
    .expect("override prefix run");
    assert_ne!(
        serde_json::to_string(&legacy.regions).unwrap(),
        serde_json::to_string(&patched.regions).unwrap(),
        "a changed regions override must change the generated regions overlay",
    );
    // Determinism: the same override reproduces itself.
    let patched_again = generate_prefix(
        fixture_input_with_regions(patch),
        cutoff,
        &nonces,
        |_| {},
        || false,
    )
    .expect("override prefix run (repeat)");
    assert_eq!(
        serde_json::to_string(&patched.regions).unwrap(),
        serde_json::to_string(&patched_again.regions).unwrap(),
        "the same regions override must reproduce identical output",
    );

    // (2) Substituting the catalog config *verbatim* (the "override == None"
    //     case at the engine layer) reproduces the legacy run byte-for-byte.
    let verbatim = fixture_input().catalogs.regions.clone();
    let none_equiv = generate_prefix(
        fixture_input_with_regions(verbatim),
        cutoff,
        &nonces,
        |_| {},
        || false,
    )
    .expect("verbatim prefix run");
    assert_eq!(
        serde_json::to_string(&legacy).unwrap(),
        serde_json::to_string(&none_equiv).unwrap(),
        "override-absent (verbatim catalog) must be byte-identical to the legacy run \
         (invariants #2/#6)",
    );
}
