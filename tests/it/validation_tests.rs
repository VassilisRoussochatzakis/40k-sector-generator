//! Validation behavior in adverse cases.

use camino::Utf8PathBuf;

#[test]
fn missing_world_data_fails_validation() {
    // Build a temp project that points at a nonexistent world-data dir.
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    std::fs::write(
        root.join("sectorforge.toml"),
        r#"
[project]
id = "x"
title = "x"

[inputs]
world_data_dir = "missing_dir"

[generation]
seed = "s"
sector_width = 4
sector_height = 4
system_count = 2
min_worlds_per_system = 1
max_worlds_per_system = 2

[outputs]
directory = "out"
formats = ["json"]
"#,
    )
    .unwrap();
    let result = sectorforge::load_project(&root);
    assert!(
        result.is_err(),
        "load should fail when world data dir is missing"
    );
}

#[test]
fn no_factions_is_ok() {
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();
    // §TF-P-1: catalogs live behind `Arc`; mutating a single field requires
    // either `Arc::make_mut` (clones-on-write) or rebuilding the `Arc`. The
    // test owns the sole reference at this point, so `make_mut` is cheap.
    std::sync::Arc::make_mut(&mut input.catalogs)
        .factions
        .clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
}

#[test]
fn majority_excluded_rows_is_severe_error() {
    // Regression guard: a column-truncated workbook (most rows missing fields)
    // must fail loudly, not silently generate from the surviving fraction.
    // See WB_EXCLUDED_ROWS_SEVERE in src/validate/validation.rs.
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();

    let cat = std::sync::Arc::make_mut(&mut input.catalogs);
    let valid = cat.world_rows[0].clone();
    cat.world_rows.clear();
    cat.world_rows.push(valid.clone()); // 1 usable row
    for _ in 0..4 {
        // Drop the first required field so the row is excluded by the pool
        // builder's `first_missing_field` check.
        let mut stub = valid.clone();
        stub.star_colour = None;
        cat.world_rows.push(stub);
    }

    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok, "4 excluded vs 1 usable must fail validation");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.code == "WB_EXCLUDED_ROWS_SEVERE"),
        "expected WB_EXCLUDED_ROWS_SEVERE, got {:?}",
        report.errors
    );
}

#[test]
fn oversized_sector_dims_fail_validation() {
    // Regression guard: a hand-edited `sectorforge.toml` with absurd dimensions
    // must be rejected before generation allocates a multi-gigabyte cell grid.
    // See GEN_SECTOR_TOO_LARGE / MAX_SECTOR_DIM in src/validate/validation.rs.
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();
    input.config.generation.sector_width = 2000;
    input.config.generation.sector_height = 2000;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok, "2000x2000 must fail validation");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.code == "GEN_SECTOR_TOO_LARGE"),
        "expected GEN_SECTOR_TOO_LARGE, got {:?}",
        report.errors
    );
}

fn manifest_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}

// ── Shared helpers for the per-code validation gaps below ───────────────────

/// Load a fresh, unmutated copy of the m42 example project. Baseline:
/// `report.ok == true`, no errors, no warnings.
fn m42() -> sectorforge::ProjectInput {
    let project = manifest_dir().join("examples/m42_project");
    sectorforge::load_project(project).unwrap()
}

fn has_err(r: &sectorforge::ValidationReport, code: &str) -> bool {
    r.errors.iter().any(|e| e.code == code)
}

fn has_warn(r: &sectorforge::ValidationReport, code: &str) -> bool {
    r.warnings.iter().any(|w| w.code == code)
}

/// First issue (error or warning) matching `code`, if any.
fn find<'a>(
    r: &'a sectorforge::ValidationReport,
    code: &str,
) -> Option<&'a sectorforge::ValidationIssue> {
    r.errors
        .iter()
        .chain(r.warnings.iter())
        .find(|e| e.code == code)
}

// ── G1: GEN_SECTOR_NOT_SQUARE ───────────────────────────────────────────────

#[test]
fn non_square_sector_is_error() {
    // NOTE: this is one of the two tests that deliberately violate the square
    // invariant — it exists precisely to exercise GEN_SECTOR_NOT_SQUARE.
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 6;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(has_err(&report, "GEN_SECTOR_NOT_SQUARE"));
    let issue = find(&report, "GEN_SECTOR_NOT_SQUARE").unwrap();
    assert_eq!(issue.severity, sectorforge::validation::Severity::Error);
    assert!(issue.message.contains('8'), "msg: {}", issue.message);
    assert!(issue.message.contains('6'), "msg: {}", issue.message);
    assert_eq!(
        issue.message,
        "sectors must be square: sector_width (8) must equal sector_height (6)"
    );
}

#[test]
fn square_sector_has_no_not_square_error() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 8;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(!has_err(&report, "GEN_SECTOR_NOT_SQUARE"));
}

#[test]
fn non_square_sector_other_ordering_is_error() {
    // 10×8: both dims individually valid; only the inequality matters.
    let mut input = m42();
    input.config.generation.sector_width = 10;
    input.config.generation.sector_height = 8;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "GEN_SECTOR_NOT_SQUARE"));
}

// ── G2: GEN_SYSTEM_COUNT_OVERFLOW + allow_empty_hexes ───────────────────────

#[test]
fn system_count_overflow_when_empties_disallowed() {
    let mut input = m42();
    input.config.generation.sector_width = 4;
    input.config.generation.sector_height = 4; // 16 cells, square
    input.config.generation.system_count = 100;
    input.config.generation.allow_empty_hexes = false;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "GEN_SYSTEM_COUNT_OVERFLOW"));
    let issue = find(&report, "GEN_SYSTEM_COUNT_OVERFLOW").unwrap();
    assert_eq!(
        issue.message,
        "system_count 100 exceeds grid cells 16 and allow_empty_hexes is false"
    );
}

#[test]
fn system_count_overflow_suppressed_when_empties_allowed() {
    let mut input = m42();
    input.config.generation.sector_width = 4;
    input.config.generation.sector_height = 4;
    input.config.generation.system_count = 100;
    input.config.generation.allow_empty_hexes = true;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "GEN_SYSTEM_COUNT_OVERFLOW"));
}

#[test]
fn system_count_equal_to_cells_is_not_overflow() {
    // Strict `>`: 16 == 16 cells is allowed even with empties disallowed.
    let mut input = m42();
    input.config.generation.sector_width = 4;
    input.config.generation.sector_height = 4;
    input.config.generation.system_count = 16;
    input.config.generation.allow_empty_hexes = false;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "GEN_SYSTEM_COUNT_OVERFLOW"));
}

// ── G3: KEY_TABLE_EMPTY ─────────────────────────────────────────────────────

#[test]
fn empty_key_table_is_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.world_tables.world_types.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "KEY_TABLE_EMPTY"));
    let issue = find(&report, "KEY_TABLE_EMPTY").unwrap();
    assert!(
        issue.message.contains("world_types"),
        "msg: {}",
        issue.message
    );
    assert_eq!(issue.message, "Key table 'world_types' has no entries");
}

#[test]
fn intact_key_tables_have_no_empty_error() {
    let input = m42();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "KEY_TABLE_EMPTY"));
}

// ── G4: WB_NO_ROWS ──────────────────────────────────────────────────────────

#[test]
fn no_workbook_rows_fires_no_rows_and_no_usable() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .world_rows
        .clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(has_err(&report, "WB_NO_ROWS"));
    assert!(has_err(&report, "WB_NO_USABLE_ROWS"));
    // Nothing to exclude → no SEVERE.
    assert!(!has_err(&report, "WB_EXCLUDED_ROWS_SEVERE"));
}

// ── G5: WB_NO_USABLE_ROWS ───────────────────────────────────────────────────

#[test]
fn all_rows_excluded_fires_no_usable_and_severe() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let valid = c.world_rows[0].clone();
    c.world_rows.clear();
    for _ in 0..3 {
        let mut s = valid.clone();
        s.star_colour = None;
        c.world_rows.push(s);
    }
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(has_err(&report, "WB_NO_USABLE_ROWS"));
    assert!(has_err(&report, "WB_EXCLUDED_ROWS_SEVERE"));
    // Rows are non-empty (3 present), so WB_NO_ROWS must NOT fire.
    assert!(!has_err(&report, "WB_NO_ROWS"));
    assert_eq!(report.world_workbook.excluded_row_count, 3);
    assert_eq!(
        report
            .world_workbook
            .exclusion_reasons
            .get("missing field: star_colour"),
        Some(&3)
    );
}

// ── G6: WB_EXCLUDED_ROWS warning branch ─────────────────────────────────────

#[test]
fn minority_excluded_rows_is_warning() {
    // 3 valid + 2 invalid: excluded (2) <= usable (3) → warning, ok stays true.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let valid = c.world_rows[0].clone();
    c.world_rows.clear();
    for _ in 0..3 {
        c.world_rows.push(valid.clone());
    }
    for _ in 0..2 {
        let mut s = valid.clone();
        s.star_colour = None;
        c.world_rows.push(s);
    }
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "WB_EXCLUDED_ROWS"));
    assert!(!has_err(&report, "WB_EXCLUDED_ROWS_SEVERE"));
}

#[test]
fn equal_excluded_and_usable_is_warning_not_severe() {
    // Boundary: 2 valid + 2 invalid. n (2) > usable (2) is false → warning.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let valid = c.world_rows[0].clone();
    c.world_rows.clear();
    for _ in 0..2 {
        c.world_rows.push(valid.clone());
    }
    for _ in 0..2 {
        let mut s = valid.clone();
        s.star_colour = None;
        c.world_rows.push(s);
    }
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "WB_EXCLUDED_ROWS"));
    assert!(!has_err(&report, "WB_EXCLUDED_ROWS_SEVERE"));
}

// ── G7: FACTION_DUPLICATE_ID ────────────────────────────────────────────────

#[test]
fn duplicate_faction_id_is_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    // Capture the count BEFORE pushing — the pushed dup lands at index `n`.
    let n = c.factions.len();
    let dup = c.factions[0].clone();
    c.factions.push(dup);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(has_err(&report, "FACTION_DUPLICATE_ID"));
    let issue = find(&report, "FACTION_DUPLICATE_ID").unwrap();
    assert_eq!(issue.path, Some(format!("factions[{n}]")));
    assert_eq!(
        issue.message,
        "duplicate faction id 'imperial_administration'"
    );
}

// ── G8: FACTION_BAD_WEIGHT ──────────────────────────────────────────────────

#[test]
fn faction_bad_weight_is_error() {
    // Rule is `!(weight.is_finite() && weight > 0.0)`: 0.0, negatives, NaN, inf
    // all bad.
    for w in [0.0_f64, -1.0, f64::NAN, f64::INFINITY] {
        let mut input = m42();
        std::sync::Arc::make_mut(&mut input.catalogs).factions[0].weight = w;
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            has_err(&report, "FACTION_BAD_WEIGHT"),
            "weight {w} should be flagged"
        );
        let issue = find(&report, "FACTION_BAD_WEIGHT").unwrap();
        assert_eq!(issue.path, Some("factions[0]".to_string()));
    }
}

#[test]
fn faction_valid_weight_has_no_error() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs).factions[0].weight = 1.0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "FACTION_BAD_WEIGHT"));
}

// ── G9: RELATIONS_{PAIR,OVERRIDE}_UNKNOWN_FACTION ───────────────────────────

#[test]
fn relations_pair_unknown_faction_is_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.pair_overrides[0].a = "ghost".to_string();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(has_err(&report, "RELATIONS_PAIR_UNKNOWN_FACTION"));
    let issue = find(&report, "RELATIONS_PAIR_UNKNOWN_FACTION").unwrap();
    assert_eq!(
        issue.path,
        Some("relations.pair_overrides[0].a".to_string())
    );
    // Slot `b` is a known id; it must NOT be flagged.
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.path.as_deref() == Some("relations.pair_overrides[0].b")),
        "slot b should not be flagged"
    );
}

#[test]
fn relations_override_unknown_faction_is_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.overrides[0].a = "ghost".to_string();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "RELATIONS_OVERRIDE_UNKNOWN_FACTION"));
    let issue = find(&report, "RELATIONS_OVERRIDE_UNKNOWN_FACTION").unwrap();
    assert_eq!(issue.path, Some("relations.overrides[0].a".to_string()));
}

#[test]
fn relations_unknown_faction_suppressed_when_no_factions() {
    // Early-return in validate_relations when factions empty → neither code.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.pair_overrides[0].a = "ghost".to_string();
    c.relations.overrides[0].a = "ghost".to_string();
    c.factions.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "RELATIONS_PAIR_UNKNOWN_FACTION"));
    assert!(!has_err(&report, "RELATIONS_OVERRIDE_UNKNOWN_FACTION"));
}

#[test]
fn relations_pair_accepts_faction_kind_id() {
    // known_ids includes id, kind, top_faction_id, subfaction_id. The kind
    // "imperial" (factions[0].kind) is accepted, so no unknown-faction error.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.pair_overrides[0].a = "imperial".to_string();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.path.as_deref() == Some("relations.pair_overrides[0].a")),
        "a faction kind id must be accepted"
    );
}

// ── G10: ROUTE_BAD_DEFAULT_WEIGHT ───────────────────────────────────────────

#[test]
fn route_bad_default_weight_is_error() {
    for w in [0.0_f64, -1.0, f64::NAN] {
        let mut input = m42();
        std::sync::Arc::make_mut(&mut input.catalogs)
            .route_rules
            .default_weight = w;
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            has_err(&report, "ROUTE_BAD_DEFAULT_WEIGHT"),
            "default_weight {w} should be flagged"
        );
    }
}

#[test]
fn route_bad_default_weight_suppressed_when_routes_disabled() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .route_rules
        .default_weight = 0.0;
    input.config.generation.routes.enabled = false;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "ROUTE_BAD_DEFAULT_WEIGHT"));
}

#[test]
fn route_valid_default_weight_has_no_error() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .route_rules
        .default_weight = 1.0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "ROUTE_BAD_DEFAULT_WEIGHT"));
}

// ── G11: ROUTE_BAD_MULTIPLIER ───────────────────────────────────────────────

#[test]
fn route_bad_multiplier_is_error() {
    for m in [0.0_f64, -1.0, f64::NAN] {
        let mut input = m42();
        let c = std::sync::Arc::make_mut(&mut input.catalogs);
        c.route_rules.modifiers.clear();
        c.route_rules
            .modifiers
            .push(sectorforge::routes::RouteModifier {
                when: sectorforge::routes::RouteCondition::default(),
                multiplier: m,
            });
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            has_err(&report, "ROUTE_BAD_MULTIPLIER"),
            "multiplier {m} should be flagged"
        );
        let issue = find(&report, "ROUTE_BAD_MULTIPLIER").unwrap();
        assert_eq!(issue.path, Some("routes.modifiers[0]".to_string()));
    }
}

// ── G12: REGIONS_COUNT_OVERFLOW ─────────────────────────────────────────────

#[test]
fn regions_count_overflow_is_error() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 8; // 64 cells → half = 32
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.count = 33;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "REGIONS_COUNT_OVERFLOW"));
    let issue = find(&report, "REGIONS_COUNT_OVERFLOW").unwrap();
    assert_eq!(
        issue.message,
        "regions.count 33 exceeds half the grid (32); shrink count or grow the sector"
    );
}

#[test]
fn regions_count_equal_to_half_is_not_overflow() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 8;
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.count = 32; // == cells/2, not `>`
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "REGIONS_COUNT_OVERFLOW"));
}

#[test]
fn regions_count_overflow_suppressed_when_disabled() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 8;
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = false;
    c.regions.count = 33;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "REGIONS_COUNT_OVERFLOW"));
}

// ── G13: ECONOMY_{TECH,POP}_MULTIPLIER_BAD ──────────────────────────────────

#[test]
fn economy_tech_multiplier_bad_is_error() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .economy
        .by_tech_level
        .insert("T5".to_string(), -1.0);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "ECONOMY_TECH_MULTIPLIER_BAD"));
    let issue = find(&report, "ECONOMY_TECH_MULTIPLIER_BAD").unwrap();
    assert_eq!(issue.path, Some("economy.by_tech_level.T5".to_string()));
    assert_eq!(
        issue.message,
        "economy.by_tech_level[T5] = -1 is not a finite non-negative number"
    );
}

#[test]
fn economy_pop_multiplier_bad_is_error() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .economy
        .by_population
        .insert("P9".to_string(), f32::NAN);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "ECONOMY_POP_MULTIPLIER_BAD"));
    let issue = find(&report, "ECONOMY_POP_MULTIPLIER_BAD").unwrap();
    assert_eq!(issue.path, Some("economy.by_population.P9".to_string()));
    assert_eq!(
        issue.message,
        "economy.by_population[P9] = NaN is not a finite non-negative number"
    );
}

#[test]
fn economy_multiplier_bad_suppressed_when_disabled() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.economy.enabled = false;
    c.economy.by_tech_level.insert("T5".to_string(), -1.0);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "ECONOMY_TECH_MULTIPLIER_BAD"));
}

#[test]
fn economy_multiplier_zero_is_allowed() {
    // Rule is `!v.is_finite() || v < 0.0`, so 0.0 passes.
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .economy
        .by_tech_level
        .insert("T0".to_string(), 0.0);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "ECONOMY_TECH_MULTIPLIER_BAD"));
}

// ── G14: RESOURCE_SCORE_BAD ─────────────────────────────────────────────────

#[test]
fn resource_score_bad_world_type_flags_each_field() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let rule = sectorforge::economy::StrategicOutputRule {
        food: Some(150.0),
        ore: Some(-1.0),
        ..Default::default()
    };
    c.economy
        .resources
        .world_type
        .insert("hive_world".to_string(), rule);
    let report = sectorforge::validate_project(&input).unwrap();
    let bad: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "RESOURCE_SCORE_BAD")
        .collect();
    assert_eq!(bad.len(), 2, "expected two bad scores, got {bad:?}");
    assert!(
        bad.iter()
            .any(|e| e.path.as_deref() == Some("resources.world_type.hive_world.food")),
        "missing food path in {bad:?}"
    );
    assert!(
        bad.iter()
            .any(|e| e.path.as_deref() == Some("resources.world_type.hive_world.ore")),
        "missing ore path in {bad:?}"
    );
}

#[test]
fn resource_score_bad_notable_feature_scope() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let rule = sectorforge::economy::StrategicOutputRule {
        food: Some(-1.0),
        ..Default::default()
    };
    c.economy
        .resources
        .notable_feature
        .insert("warp_beacon".to_string(), rule);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "RESOURCE_SCORE_BAD"));
    let issue = find(&report, "RESOURCE_SCORE_BAD").unwrap();
    assert_eq!(
        issue.path,
        Some("resources.notable_feature.warp_beacon.food".to_string())
    );
}

#[test]
fn resource_score_bounds_inclusive() {
    // 0.0 and 100.0 are both in range (0.0..=100.0 inclusive) → no error.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    let rule = sectorforge::economy::StrategicOutputRule {
        food: Some(0.0),
        ore: Some(100.0),
        ..Default::default()
    };
    c.economy
        .resources
        .world_type
        .insert("hive_world".to_string(), rule);
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "RESOURCE_SCORE_BAD"));
}

// ── G15: OUT_BITMAP_SCALE_RANGE ─────────────────────────────────────────────

#[test]
fn bitmap_scale_out_of_range_fires_per_field() {
    let mut input = m42();
    input.config.outputs.bitmap.sector_scale = 0;
    input.config.outputs.bitmap.system_scale = 9;
    let report = sectorforge::validate_project(&input).unwrap();
    let bad: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "OUT_BITMAP_SCALE_RANGE")
        .collect();
    assert_eq!(bad.len(), 2, "expected two scale errors, got {bad:?}");
    assert!(
        bad.iter()
            .any(|e| e.message == "outputs.bitmap.sector_scale = 0; must be in 1..=8"),
        "missing sector_scale msg in {bad:?}"
    );
    assert!(
        bad.iter()
            .any(|e| e.message == "outputs.bitmap.system_scale = 9; must be in 1..=8"),
        "missing system_scale msg in {bad:?}"
    );
}

#[test]
fn bitmap_scale_bounds_inclusive() {
    let mut input = m42();
    input.config.outputs.bitmap.sector_scale = 1;
    input.config.outputs.bitmap.system_scale = 8;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "OUT_BITMAP_SCALE_RANGE"));
}

// ── G16: OUT_BITMAP_THEME_INVALID ───────────────────────────────────────────

#[test]
fn bitmap_invalid_theme_is_error() {
    let mut input = m42();
    input.config.outputs.bitmap.theme.name = Some("no_such".to_string());
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "OUT_BITMAP_THEME_INVALID"));
}

// ── G17: GEN_GRID_EMPTY ─────────────────────────────────────────────────────

#[test]
fn empty_grid_is_error_but_not_non_square() {
    // 0×0 is square, so only GEN_GRID_EMPTY fires.
    let mut input = m42();
    input.config.generation.sector_width = 0;
    input.config.generation.sector_height = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "GEN_GRID_EMPTY"));
    assert!(!has_err(&report, "GEN_SECTOR_NOT_SQUARE"));
}

#[test]
fn non_empty_grid_has_no_empty_error() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 8;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "GEN_GRID_EMPTY"));
}

// ── G18: GEN_WORLD_COUNT_RANGE ──────────────────────────────────────────────

#[test]
fn world_count_range_inverted_is_error() {
    let mut input = m42();
    input.config.generation.min_worlds_per_system = 5;
    input.config.generation.max_worlds_per_system = 2;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "GEN_WORLD_COUNT_RANGE"));
}

#[test]
fn world_count_range_equal_is_ok() {
    let mut input = m42();
    input.config.generation.min_worlds_per_system = 2;
    input.config.generation.max_worlds_per_system = 2;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "GEN_WORLD_COUNT_RANGE"));
}

// ── G19: GEN_MIN_WORLDS_ZERO ────────────────────────────────────────────────

#[test]
fn min_worlds_zero_is_warning_not_error() {
    let mut input = m42();
    input.config.generation.min_worlds_per_system = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok, "a warning must not flip ok");
    assert!(has_warn(&report, "GEN_MIN_WORLDS_ZERO"));
    assert!(!has_err(&report, "GEN_MIN_WORLDS_ZERO"));
}

#[test]
fn min_worlds_one_has_no_warning() {
    let mut input = m42();
    input.config.generation.min_worlds_per_system = 1;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "GEN_MIN_WORLDS_ZERO"));
}

// ── G20: GEN_FEATURE_POOL_SMALL ─────────────────────────────────────────────

#[test]
fn large_feature_count_warns_pool_small() {
    let mut input = m42();
    input.config.generation.world_feature_count = 10_000;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_warn(&report, "GEN_FEATURE_POOL_SMALL"));
}

#[test]
fn baseline_feature_count_has_no_pool_small_warning() {
    let input = m42();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "GEN_FEATURE_POOL_SMALL"));
}

// ── G21: FACTION_UNKNOWN_{WORLD_TYPE,GOVERNMENT,FEATURE} ─────────────────────

#[test]
fn faction_unknown_taxonomy_refs_are_warnings() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.factions[0].preferred_world_types = vec!["no_such_world".to_string()];
    c.factions[0].preferred_governments = vec!["no_such_gov".to_string()];
    c.factions[0].preferred_notable_features = vec!["no_such_feat".to_string()];
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok, "unknown-ref warnings must not flip ok");
    assert!(has_warn(&report, "FACTION_UNKNOWN_WORLD_TYPE"));
    assert!(has_warn(&report, "FACTION_UNKNOWN_GOVERNMENT"));
    assert!(has_warn(&report, "FACTION_UNKNOWN_FEATURE"));
    assert_eq!(
        find(&report, "FACTION_UNKNOWN_WORLD_TYPE").unwrap().path,
        Some("factions[0].preferred_world_types".to_string())
    );
    assert_eq!(
        find(&report, "FACTION_UNKNOWN_GOVERNMENT").unwrap().path,
        Some("factions[0].preferred_governments".to_string())
    );
    assert_eq!(
        find(&report, "FACTION_UNKNOWN_FEATURE").unwrap().path,
        Some("factions[0].preferred_notable_features".to_string())
    );
}

#[test]
fn baseline_factions_have_no_unknown_taxonomy_warnings() {
    let input = m42();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "FACTION_UNKNOWN_WORLD_TYPE"));
    assert!(!has_warn(&report, "FACTION_UNKNOWN_GOVERNMENT"));
    assert!(!has_warn(&report, "FACTION_UNKNOWN_FEATURE"));
}

// ── G22: ROUTE_MAX_DISTANCE_ZERO ────────────────────────────────────────────

#[test]
fn route_max_distance_zero_is_warning() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .route_rules
        .max_distance = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "ROUTE_MAX_DISTANCE_ZERO"));
}

#[test]
fn route_max_distance_zero_suppressed_when_routes_disabled() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .route_rules
        .max_distance = 0;
    input.config.generation.routes.enabled = false;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "ROUTE_MAX_DISTANCE_ZERO"));
}

#[test]
fn route_max_distance_nonzero_has_no_warning() {
    let mut input = m42();
    std::sync::Arc::make_mut(&mut input.catalogs)
        .route_rules
        .max_distance = 5;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "ROUTE_MAX_DISTANCE_ZERO"));
}

// ── G23: route-condition values are type-safe at load (P10) ──────────────────
//
// `RouteCondition`'s fields are typed enums, so a misspelled value is rejected
// when the config deserializes (the dedicated unit tests in `src/gen/routes.rs`
// cover that load-time error). There is therefore no `ROUTE_UNKNOWN_*` warning
// to raise here: an unknown value can never reach validation. This test pins
// that a fully-populated, valid typed condition validates cleanly.

#[test]
fn route_condition_valid_typed_fields_have_no_warning() {
    use sectorforge::sector_model::RouteType;
    use sectorforge::worlds::{Government, NotableFeature, WorldType};

    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.route_rules.modifiers.clear();
    let cond = sectorforge::routes::RouteCondition {
        notable_feature: Some(NotableFeature::TradeHub),
        world_type: Some(WorldType::ForgeWorld),
        government: Some(Government::MilitaryGovernor),
        route_type: Some(RouteType::Webway),
    };
    c.route_rules
        .modifiers
        .push(sectorforge::routes::RouteModifier {
            when: cond,
            multiplier: 1.0,
        });
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
}

// ── G24: NAME_POOL_EMPTY ────────────────────────────────────────────────────

#[test]
fn all_name_pools_empty_is_warning() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.names.system_names.prefixes.clear();
    c.names.system_names.suffixes.clear();
    c.names.system_names.single_names.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "NAME_POOL_EMPTY"));
}

#[test]
fn partial_name_pool_is_not_empty_warning() {
    // Rule is an AND-conjunction: leaving single_names non-empty suppresses it.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.names.system_names.prefixes.clear();
    c.names.system_names.suffixes.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "NAME_POOL_EMPTY"));
}

// ── G25: RELATIONS_KIND_RULE_EMPTY (both branches) ──────────────────────────

#[test]
fn relations_kind_rule_empty_with_factions_is_warning() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.kind_rules[0].a = String::new();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "RELATIONS_KIND_RULE_EMPTY"));
    assert_eq!(
        find(&report, "RELATIONS_KIND_RULE_EMPTY").unwrap().path,
        Some("relations.kind_rules[0]".to_string())
    );
}

#[test]
fn relations_kind_rule_empty_no_factions_branch() {
    // The no-factions early-return branch emits the identical code + path.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.relations.kind_rules[0].a = String::new();
    c.factions.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_warn(&report, "RELATIONS_KIND_RULE_EMPTY"));
    assert_eq!(
        find(&report, "RELATIONS_KIND_RULE_EMPTY").unwrap().path,
        Some("relations.kind_rules[0]".to_string())
    );
}

// ── G26: REGIONS_COUNT_ZERO ─────────────────────────────────────────────────

#[test]
fn regions_count_zero_is_warning() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.count = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_warn(&report, "REGIONS_COUNT_ZERO"));
}

#[test]
fn regions_count_zero_suppressed_when_disabled() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = false;
    c.regions.count = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "REGIONS_COUNT_ZERO"));
}

#[test]
fn regions_count_nonzero_has_no_zero_warning() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.count = 3;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "REGIONS_COUNT_ZERO"));
}

// ── G27: REGIONS_MEAN_SIZE_ZERO ─────────────────────────────────────────────

#[test]
fn regions_mean_size_zero_is_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.mean_size = 0;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "REGIONS_MEAN_SIZE_ZERO"));
}

#[test]
fn regions_mean_size_one_has_no_error() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.mean_size = 1;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "REGIONS_MEAN_SIZE_ZERO"));
}

// ── G28: REGIONS_CONDITION_BAD_WEIGHT ───────────────────────────────────────

#[test]
fn regions_condition_bad_weight_only_negative_flagged() {
    // Asymmetry vs faction/route: rule is `!finite || weight < 0.0`, so the
    // weight==0.0 entry (index 0) passes; only the -1.0 entry (index 1) fails.
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.conditions.clear();
    c.regions
        .conditions
        .push(sectorforge::regions::ConditionEntry {
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            weight: 0.0,
            label: None,
        });
    c.regions
        .conditions
        .push(sectorforge::regions::ConditionEntry {
            kind: sectorforge::regions::RegionConditionKind::Turbulence,
            weight: -1.0,
            label: None,
        });
    let report = sectorforge::validate_project(&input).unwrap();
    let bad: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.code == "REGIONS_CONDITION_BAD_WEIGHT")
        .collect();
    assert_eq!(bad.len(), 1, "only the negative entry should fail: {bad:?}");
    assert_eq!(
        bad[0].path,
        Some("regions.conditions[1].weight".to_string())
    );
    assert_eq!(
        bad[0].message,
        "regions.conditions[1].weight -1 is not a finite non-negative number"
    );
}

#[test]
fn regions_condition_nan_weight_is_flagged() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    c.regions.enabled = true;
    c.regions.conditions.clear();
    c.regions
        .conditions
        .push(sectorforge::regions::ConditionEntry {
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            weight: f64::NAN,
            label: None,
        });
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "REGIONS_CONDITION_BAD_WEIGHT"));
}

// ── G29: RESOURCE_TRADE_MULTIPLIER_BAD ──────────────────────────────────────

#[test]
fn resource_trade_multiplier_bad_is_error() {
    for v in [-1.0_f32, f32::NAN] {
        let mut input = m42();
        let c = std::sync::Arc::make_mut(&mut input.catalogs);
        let rule = sectorforge::economy::StrategicOutputRule {
            trade_multiplier: Some(v),
            ..Default::default()
        };
        c.economy
            .resources
            .notable_feature
            .insert("warp_beacon".to_string(), rule);
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            has_err(&report, "RESOURCE_TRADE_MULTIPLIER_BAD"),
            "trade_multiplier {v} should be flagged"
        );
        let issue = find(&report, "RESOURCE_TRADE_MULTIPLIER_BAD").unwrap();
        assert_eq!(
            issue.path,
            Some("resources.notable_feature.warp_beacon.trade_multiplier".to_string())
        );
    }
}

#[test]
fn resource_trade_multiplier_zero_and_high_allowed() {
    // No upper cap; zero allowed (rule is `!finite || v < 0.0`).
    for v in [0.0_f32, 3.0] {
        let mut input = m42();
        let c = std::sync::Arc::make_mut(&mut input.catalogs);
        let rule = sectorforge::economy::StrategicOutputRule {
            trade_multiplier: Some(v),
            ..Default::default()
        };
        c.economy
            .resources
            .notable_feature
            .insert("warp_beacon".to_string(), rule);
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            !has_err(&report, "RESOURCE_TRADE_MULTIPLIER_BAD"),
            "trade_multiplier {v} should be allowed"
        );
    }
}

// ── G30: RESOURCE_SUPPLY_RESILIENCE_BAD ─────────────────────────────────────

#[test]
fn resource_supply_resilience_out_of_range_is_error() {
    for v in [-150.0_f32, 150.0, f32::NAN] {
        let mut input = m42();
        let c = std::sync::Arc::make_mut(&mut input.catalogs);
        let rule = sectorforge::economy::StrategicOutputRule {
            supply_resilience: Some(v),
            ..Default::default()
        };
        c.economy
            .resources
            .notable_feature
            .insert("warp_beacon".to_string(), rule);
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            has_err(&report, "RESOURCE_SUPPLY_RESILIENCE_BAD"),
            "supply_resilience {v} should be flagged"
        );
        let issue = find(&report, "RESOURCE_SUPPLY_RESILIENCE_BAD").unwrap();
        assert_eq!(
            issue.path,
            Some("resources.notable_feature.warp_beacon.supply_resilience".to_string())
        );
    }
}

#[test]
fn resource_supply_resilience_signed_bounds_allowed() {
    // Signed inclusive range -100.0..=100.0; negatives are valid here.
    for v in [-100.0_f32, 0.0, 100.0] {
        let mut input = m42();
        let c = std::sync::Arc::make_mut(&mut input.catalogs);
        let rule = sectorforge::economy::StrategicOutputRule {
            supply_resilience: Some(v),
            ..Default::default()
        };
        c.economy
            .resources
            .world_type
            .insert("hive_world".to_string(), rule);
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            !has_err(&report, "RESOURCE_SUPPLY_RESILIENCE_BAD"),
            "supply_resilience {v} should be allowed"
        );
    }
}

// ── G31: validate() ok-aggregation ──────────────────────────────────────────

#[test]
fn warnings_only_keeps_ok_true() {
    let mut input = m42();
    input.config.generation.min_worlds_per_system = 0; // warning only
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(report.errors.is_empty());
    assert_eq!(report.ok, report.errors.is_empty());
}

#[test]
fn errors_flip_ok_false() {
    let mut input = m42();
    input.config.generation.sector_width = 8;
    input.config.generation.sector_height = 6; // error
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok);
    assert!(!report.errors.is_empty());
    assert_eq!(report.ok, report.errors.is_empty());
}

// ── G32: MAX_SECTOR_DIM boundary ────────────────────────────────────────────

#[test]
fn sector_at_max_dim_is_not_too_large() {
    let mut input = m42();
    input.config.generation.sector_width = sectorforge::validation::MAX_SECTOR_DIM;
    input.config.generation.sector_height = sectorforge::validation::MAX_SECTOR_DIM;
    // Leave allow_empty_hexes (true) and system_count (24) at baseline so the
    // huge grid doesn't trip GEN_SYSTEM_COUNT_OVERFLOW.
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_err(&report, "GEN_SECTOR_TOO_LARGE"));
}

#[test]
fn sector_over_max_dim_is_too_large() {
    let mut input = m42();
    input.config.generation.sector_width = sectorforge::validation::MAX_SECTOR_DIM + 1;
    input.config.generation.sector_height = sectorforge::validation::MAX_SECTOR_DIM + 1;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(has_err(&report, "GEN_SECTOR_TOO_LARGE"));
    let issue = find(&report, "GEN_SECTOR_TOO_LARGE").unwrap();
    assert_eq!(
        issue.message,
        "sector dimensions (1025 × 1025) exceed the maximum supported 1024 per side"
    );
}

// ── G33: OUT_NO_FORMATS ─────────────────────────────────────────────────────

#[test]
fn empty_output_formats_is_warning() {
    let mut input = m42();
    input.config.outputs.formats = vec![];
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
    assert!(has_warn(&report, "OUT_NO_FORMATS"));
}

#[test]
fn nonempty_output_formats_has_no_warning() {
    let mut input = m42();
    input.config.outputs.formats = vec![sectorforge::config::OutputFormat::Json];
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!has_warn(&report, "OUT_NO_FORMATS"));
}

// ── G34: WorldWorkbookValidation summary fields ─────────────────────────────

#[test]
fn workbook_summary_counts_excluded_rows() {
    let mut input = m42();
    let c = std::sync::Arc::make_mut(&mut input.catalogs);
    // Capture the baseline row count; do NOT hardcode 143.
    let orig = c.world_rows.len();
    let valid = c.world_rows[0].clone();
    for _ in 0..3 {
        let mut s = valid.clone();
        s.star_colour = None;
        c.world_rows.push(s);
    }
    let report = sectorforge::validate_project(&input).unwrap();
    let wb = &report.world_workbook;
    assert_eq!(wb.excluded_row_count, 3);
    assert_eq!(wb.row_count, orig + 3);
    assert_eq!(
        wb.exclusion_reasons.get("missing field: star_colour"),
        Some(&3)
    );
    let total_excluded: usize = wb.exclusion_reasons.values().sum();
    assert_eq!(total_excluded, 3);
}
