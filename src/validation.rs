//! Pre-generation validation. Pure — no I/O.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::input::ProjectInput;
use crate::taxonomy;
use crate::world_pool;

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
    pub world_workbook: WorldWorkbookValidation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub row: Option<usize>,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldWorkbookValidation {
    pub row_count: usize,
    pub usable_candidate_count: usize,
    pub excluded_row_count: usize,
    pub exclusion_reasons: BTreeMap<String, usize>,
    pub key_table_counts: BTreeMap<String, usize>,
}

pub fn validate(input: &ProjectInput) -> ValidationReport {
    let mut errors: Vec<ValidationIssue> = Vec::new();
    let mut warnings: Vec<ValidationIssue> = Vec::new();

    // ── Sector geometry / counts ────────────────────────────────────────────
    let g = &input.config.generation;
    let grid_cells = (g.sector_width as usize) * (g.sector_height as usize);
    if grid_cells == 0 {
        errors.push(issue(
            "GEN_GRID_EMPTY",
            "sector_width * sector_height must be > 0",
            Severity::Error,
        ));
    }
    if g.system_count > grid_cells && !g.allow_empty_hexes {
        errors.push(issue(
            "GEN_SYSTEM_COUNT_OVERFLOW",
            &format!(
                "system_count {} exceeds grid cells {} and allow_empty_hexes is false",
                g.system_count, grid_cells
            ),
            Severity::Error,
        ));
    }
    if g.min_worlds_per_system > g.max_worlds_per_system {
        errors.push(issue(
            "GEN_WORLD_COUNT_RANGE",
            "min_worlds_per_system must be <= max_worlds_per_system",
            Severity::Error,
        ));
    }
    if g.min_worlds_per_system == 0 {
        warnings.push(issue(
            "GEN_MIN_WORLDS_ZERO",
            "min_worlds_per_system is 0; systems may have no worlds",
            Severity::Warning,
        ));
    }

    // ── Output formats ──────────────────────────────────────────────────────
    if input.config.outputs.formats.is_empty() {
        warnings.push(issue(
            "OUT_NO_FORMATS",
            "outputs.formats is empty; nothing will be written",
            Severity::Warning,
        ));
    }

    let bm = &input.config.outputs.bitmap;
    for (name, v) in [
        ("sector_scale", bm.sector_scale),
        ("system_scale", bm.system_scale),
    ] {
        if v == 0 || v > 8 {
            errors.push(issue(
                "OUT_BITMAP_SCALE_RANGE",
                &format!("outputs.bitmap.{name} = {v}; must be in 1..=8"),
                Severity::Error,
            ));
        }
    }
    if let Err(message) = crate::map_theme::resolve_map_theme(&bm.theme) {
        errors.push(issue(
            "OUT_BITMAP_THEME_INVALID",
            &format!("outputs.bitmap.theme invalid: {message}"),
            Severity::Error,
        ));
    }

    // ── World workbook ──────────────────────────────────────────────────────
    let mut pool = world_pool::build_pool(
        &input.world_rows,
        &input.world_tables,
        &input.config.generation.world_selection,
    );
    if let Some(features) = &input.authored_features {
        world_pool::apply_authored_features(&mut pool, features);
    }

    let t = &input.world_tables;
    let key_counts: BTreeMap<String, usize> = [
        ("star_colours", t.star_colours.len()),
        ("world_types", t.world_types.len()),
        ("atmospheres", t.atmospheres.len()),
        ("temperatures", t.temperatures.len()),
        ("biospheres", t.biospheres.len()),
        ("populations", t.populations.len()),
        ("tech_levels", t.tech_levels.len()),
        ("governments", t.governments.len()),
        ("notable_features", t.notable_features.len()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    for (name, n) in &key_counts {
        if *n == 0 {
            errors.push(issue(
                "KEY_TABLE_EMPTY",
                &format!("Key table '{name}' has no entries"),
                Severity::Error,
            ));
        }
    }

    if input.world_rows.is_empty() {
        errors.push(issue(
            "WB_NO_ROWS",
            "Generator Template sheet had no rows",
            Severity::Error,
        ));
    }
    if pool.candidates.is_empty() {
        errors.push(issue(
            "WB_NO_USABLE_ROWS",
            "Workbook produced zero usable world candidates",
            Severity::Error,
        ));
    }
    let mut exclusion_reasons: BTreeMap<String, usize> = BTreeMap::new();
    for ex in &pool.excluded_rows {
        *exclusion_reasons.entry(ex.reason.to_string()).or_insert(0) += 1;
    }
    if !pool.excluded_rows.is_empty() {
        let n = pool.excluded_rows.len();
        let breakdown = exclusion_reasons
            .iter()
            .map(|(r, c)| format!("{r}: {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(issue(
            "WB_EXCLUDED_ROWS",
            &format!("{n} workbook row(s) were excluded ({breakdown})"),
            Severity::Warning,
        ));
    }

    // World feature count vs available pool
    let feature_pool_size =
        pool.feature_pool.global.len() + pool.feature_pool.key_table_features.len();
    if input.config.generation.world_feature_count > feature_pool_size.max(1) * 2 {
        warnings.push(issue(
            "GEN_FEATURE_POOL_SMALL",
            &format!(
                "Requested {} features per world but feature pool is small ({} unique)",
                input.config.generation.world_feature_count, feature_pool_size
            ),
            Severity::Warning,
        ));
    }

    // ── Factions ────────────────────────────────────────────────────────────
    let mut faction_ids: BTreeSet<crate::ids::FactionId> = BTreeSet::new();
    for (idx, f) in input.factions.iter().enumerate() {
        if !faction_ids.insert(f.id.clone()) {
            errors.push(ValidationIssue {
                code: "FACTION_DUPLICATE_ID".to_string(),
                message: format!("duplicate faction id '{}'", f.id),
                path: Some(format!("factions[{idx}]")),
                row: None,
                severity: Severity::Error,
            });
        }
        if !(f.weight.is_finite() && f.weight > 0.0) {
            errors.push(ValidationIssue {
                code: "FACTION_BAD_WEIGHT".to_string(),
                message: format!("faction '{}' has non-positive or non-finite weight", f.id),
                path: Some(format!("factions[{idx}]")),
                row: None,
                severity: Severity::Error,
            });
        }
        for s in &f.preferred_world_types {
            if taxonomy::parse_world_type_variant(s).is_none() {
                warnings.push(ValidationIssue {
                    code: "FACTION_UNKNOWN_WORLD_TYPE".to_string(),
                    message: format!("faction '{}' references unknown world type '{}'", f.id, s),
                    path: Some(format!("factions[{idx}].preferred_world_types")),
                    row: None,
                    severity: Severity::Warning,
                });
            }
        }
        for s in &f.preferred_governments {
            if taxonomy::parse_government_variant(s).is_none() {
                warnings.push(ValidationIssue {
                    code: "FACTION_UNKNOWN_GOVERNMENT".to_string(),
                    message: format!("faction '{}' references unknown government '{}'", f.id, s),
                    path: Some(format!("factions[{idx}].preferred_governments")),
                    row: None,
                    severity: Severity::Warning,
                });
            }
        }
        for s in &f.preferred_notable_features {
            if taxonomy::parse_notable_feature_variant(s).is_none() {
                warnings.push(ValidationIssue {
                    code: "FACTION_UNKNOWN_FEATURE".to_string(),
                    message: format!("faction '{}' references unknown feature '{}'", f.id, s),
                    path: Some(format!("factions[{idx}].preferred_notable_features")),
                    row: None,
                    severity: Severity::Warning,
                });
            }
        }
    }

    // ── Route rules ─────────────────────────────────────────────────────────
    if input.config.generation.routes.enabled {
        let r = &input.route_rules;
        if !(r.default_weight.is_finite() && r.default_weight > 0.0) {
            errors.push(issue(
                "ROUTE_BAD_DEFAULT_WEIGHT",
                "route default_weight must be positive and finite",
                Severity::Error,
            ));
        }
        if r.max_distance == 0 {
            warnings.push(issue(
                "ROUTE_MAX_DISTANCE_ZERO",
                "route max_distance is 0; no routes can be generated",
                Severity::Warning,
            ));
        }
        for (i, m) in r.modifiers.iter().enumerate() {
            if !(m.multiplier.is_finite() && m.multiplier > 0.0) {
                errors.push(ValidationIssue {
                    code: "ROUTE_BAD_MULTIPLIER".to_string(),
                    message: "route modifier multiplier must be positive and finite".to_string(),
                    path: Some(format!("routes.modifiers[{i}]")),
                    row: None,
                    severity: Severity::Error,
                });
            }
            if let Some(s) = &m.when.notable_feature {
                if taxonomy::parse_notable_feature_variant(s).is_none() {
                    warnings.push(ValidationIssue {
                        code: "ROUTE_UNKNOWN_FEATURE".to_string(),
                        message: format!("route condition references unknown feature '{s}'"),
                        path: Some(format!("routes.modifiers[{i}].when.notable_feature")),
                        row: None,
                        severity: Severity::Warning,
                    });
                }
            }
            if let Some(s) = &m.when.world_type {
                if taxonomy::parse_world_type_variant(s).is_none() {
                    warnings.push(ValidationIssue {
                        code: "ROUTE_UNKNOWN_WORLD_TYPE".to_string(),
                        message: format!("route condition references unknown world type '{s}'"),
                        path: Some(format!("routes.modifiers[{i}].when.world_type")),
                        row: None,
                        severity: Severity::Warning,
                    });
                }
            }
            if let Some(s) = &m.when.government {
                if taxonomy::parse_government_variant(s).is_none() {
                    warnings.push(ValidationIssue {
                        code: "ROUTE_UNKNOWN_GOVERNMENT".to_string(),
                        message: format!("route condition references unknown government '{s}'"),
                        path: Some(format!("routes.modifiers[{i}].when.government")),
                        row: None,
                        severity: Severity::Warning,
                    });
                }
            }
            if let Some(s) = &m.when.route_type {
                if crate::sector_model::RouteType::from_key(&taxonomy::to_snake_case(s)).is_none() {
                    warnings.push(ValidationIssue {
                        code: "ROUTE_UNKNOWN_ROUTE_TYPE".to_string(),
                        message: format!("route condition references unknown route type '{s}'"),
                        path: Some(format!("routes.modifiers[{i}].when.route_type")),
                        row: None,
                        severity: Severity::Warning,
                    });
                }
            }
        }
    }

    // ── Name pools ──────────────────────────────────────────────────────────
    let n = &input.names.system_names;
    if n.prefixes.is_empty() && n.suffixes.is_empty() && n.single_names.is_empty() {
        warnings.push(issue(
            "NAME_POOL_EMPTY",
            "system name pool is empty; fallback names like 'System N' will be used",
            Severity::Warning,
        ));
    }

    validate_relations(
        &input.relations,
        &input.factions,
        &mut errors,
        &mut warnings,
    );
    validate_regions(
        &input.regions,
        &input.config.generation,
        &mut errors,
        &mut warnings,
    );
    validate_economy(&input.economy, &mut errors, &mut warnings);

    ValidationReport {
        ok: errors.is_empty(),
        errors,
        warnings,
        world_workbook: WorldWorkbookValidation {
            row_count: input.world_rows.len(),
            usable_candidate_count: pool.candidates.len(),
            excluded_row_count: pool.excluded_rows.len(),
            exclusion_reasons,
            key_table_counts: key_counts,
        },
    }
}

/// §5 NEW2.md: preflight `relations.toml`. Surface unknown faction ids in
/// overrides and obvious typos in disposition rules.
fn validate_relations(
    cfg: &crate::relations::RelationsConfig,
    factions: &[crate::factions::FactionDef],
    errors: &mut Vec<ValidationIssue>,
    warnings: &mut Vec<ValidationIssue>,
) {
    // No factions → no meaningful id set to check overrides against; skip
    // those checks rather than flagging every override id as "unknown".
    if factions.is_empty() {
        for (i, r) in cfg.kind_rules.iter().enumerate() {
            if r.a.is_empty() || r.b.is_empty() {
                warnings.push(ValidationIssue {
                    code: "RELATIONS_KIND_RULE_EMPTY".into(),
                    message: format!("relations.kind_rules[{i}] has empty kind id"),
                    path: Some(format!("relations.kind_rules[{i}]")),
                    row: None,
                    severity: Severity::Warning,
                });
            }
        }
        return;
    }
    let known_ids: BTreeSet<String> = factions
        .iter()
        .flat_map(|f| {
            [
                f.id.to_string(),
                f.kind.clone(),
                f.top_faction_id().to_string(),
                f.subfaction_id().to_string(),
            ]
        })
        .collect();
    for (i, ov) in cfg.pair_overrides.iter().enumerate() {
        for (slot, id) in [("a", &ov.a), ("b", &ov.b)] {
            if !known_ids.contains(id.as_str()) {
                errors.push(ValidationIssue {
                    code: "RELATIONS_PAIR_UNKNOWN_FACTION".into(),
                    message: format!(
                        "relations.pair_overrides[{i}].{slot} = '{id}' is not a known faction id"
                    ),
                    path: Some(format!("relations.pair_overrides[{i}].{slot}")),
                    row: None,
                    severity: Severity::Error,
                });
            }
        }
    }
    for (i, ov) in cfg.overrides.iter().enumerate() {
        for (slot, id) in [("a", &ov.a), ("b", &ov.b)] {
            if !known_ids.contains(id.as_str()) {
                errors.push(ValidationIssue {
                    code: "RELATIONS_OVERRIDE_UNKNOWN_FACTION".into(),
                    message: format!(
                        "relations.overrides[{i}].{slot} = '{id}' is not a known faction id"
                    ),
                    path: Some(format!("relations.overrides[{i}].{slot}")),
                    row: None,
                    severity: Severity::Error,
                });
            }
        }
    }
    for (i, r) in cfg.kind_rules.iter().enumerate() {
        if r.a.is_empty() || r.b.is_empty() {
            warnings.push(ValidationIssue {
                code: "RELATIONS_KIND_RULE_EMPTY".into(),
                message: format!("relations.kind_rules[{i}] has empty kind id"),
                path: Some(format!("relations.kind_rules[{i}]")),
                row: None,
                severity: Severity::Warning,
            });
        }
    }
}

/// §5 NEW.md: preflight `regions.toml`. Catch nonsense counts / sizes.
fn validate_regions(
    cfg: &crate::regions::RegionsConfig,
    g: &crate::config::GenerationConfig,
    errors: &mut Vec<ValidationIssue>,
    warnings: &mut Vec<ValidationIssue>,
) {
    if !cfg.enabled {
        return;
    }
    if cfg.count == 0 {
        warnings.push(issue(
            "REGIONS_COUNT_ZERO",
            "regions.enabled is true but regions.count is 0; no regions will be generated",
            Severity::Warning,
        ));
    }
    let cells = (g.sector_width as u64) * (g.sector_height as u64);
    if (cfg.count as u64) > cells / 2 {
        errors.push(issue(
            "REGIONS_COUNT_OVERFLOW",
            &format!(
                "regions.count {} exceeds half the grid ({}); shrink count or grow the sector",
                cfg.count,
                cells / 2
            ),
            Severity::Error,
        ));
    }
    if cfg.mean_size == 0 {
        errors.push(issue(
            "REGIONS_MEAN_SIZE_ZERO",
            "regions.mean_size must be >= 1",
            Severity::Error,
        ));
    }
    for (i, c) in cfg.conditions.iter().enumerate() {
        if !c.weight.is_finite() || c.weight < 0.0 {
            errors.push(ValidationIssue {
                code: "REGIONS_CONDITION_BAD_WEIGHT".into(),
                message: format!(
                    "regions.conditions[{i}].weight {} is not a finite non-negative number",
                    c.weight
                ),
                path: Some(format!("regions.conditions[{i}].weight")),
                row: None,
                severity: Severity::Error,
            });
        }
    }
}

/// §12 NEW.md: preflight `economy.toml`. Catch non-finite multipliers.
fn validate_economy(
    cfg: &crate::economy::EconomyConfig,
    errors: &mut Vec<ValidationIssue>,
    _warnings: &mut Vec<ValidationIssue>,
) {
    if !cfg.enabled {
        return;
    }
    for (k, v) in &cfg.by_tech_level {
        if !v.is_finite() || *v < 0.0 {
            errors.push(ValidationIssue {
                code: "ECONOMY_TECH_MULTIPLIER_BAD".into(),
                message: format!(
                    "economy.by_tech_level[{k}] = {v} is not a finite non-negative number"
                ),
                path: Some(format!("economy.by_tech_level.{k}")),
                row: None,
                severity: Severity::Error,
            });
        }
    }
    for (k, v) in &cfg.by_population {
        if !v.is_finite() || *v < 0.0 {
            errors.push(ValidationIssue {
                code: "ECONOMY_POP_MULTIPLIER_BAD".into(),
                message: format!(
                    "economy.by_population[{k}] = {v} is not a finite non-negative number"
                ),
                path: Some(format!("economy.by_population.{k}")),
                row: None,
                severity: Severity::Error,
            });
        }
    }
    for (scope, rules) in [
        ("world_type", &cfg.resources.world_type),
        ("notable_feature", &cfg.resources.notable_feature),
    ] {
        for (name, rule) in rules {
            validate_resource_rule(scope, name, rule, errors);
        }
    }
}

fn validate_resource_rule(
    scope: &str,
    name: &str,
    rule: &crate::economy::StrategicOutputRule,
    errors: &mut Vec<ValidationIssue>,
) {
    let path = |field: &str| format!("resources.{scope}.{name}.{field}");
    let mut check_score = |field: &str, value: Option<f32>| {
        if let Some(v) = value {
            if !v.is_finite() || !(0.0..=100.0).contains(&v) {
                errors.push(ValidationIssue {
                    code: "RESOURCE_SCORE_BAD".into(),
                    message: format!("resources.{scope}.{name}.{field} = {v} is not 0..=100"),
                    path: Some(path(field)),
                    row: None,
                    severity: Severity::Error,
                });
            }
        }
    };
    check_score("food", rule.food);
    check_score("ore", rule.ore);
    check_score("manufacturing", rule.manufacturing);
    check_score("arms", rule.arms);
    check_score("ships", rule.ships);
    check_score("pilgrimage", rule.pilgrimage);
    check_score("psyker_tithe", rule.psyker_tithe);
    check_score("manpower", rule.manpower);
    check_score("knowledge", rule.knowledge);
    check_score("xenos_value", rule.xenos_value);
    if let Some(v) = rule.trade_multiplier {
        if !v.is_finite() || v < 0.0 {
            errors.push(ValidationIssue {
                code: "RESOURCE_TRADE_MULTIPLIER_BAD".into(),
                message: format!(
                    "resources.{scope}.{name}.trade_multiplier = {v} is not finite non-negative"
                ),
                path: Some(path("trade_multiplier")),
                row: None,
                severity: Severity::Error,
            });
        }
    }
    if let Some(v) = rule.supply_resilience {
        if !v.is_finite() || !(-100.0..=100.0).contains(&v) {
            errors.push(ValidationIssue {
                code: "RESOURCE_SUPPLY_RESILIENCE_BAD".into(),
                message: format!(
                    "resources.{scope}.{name}.supply_resilience = {v} is not -100..=100"
                ),
                path: Some(path("supply_resilience")),
                row: None,
                severity: Severity::Error,
            });
        }
    }
}

fn issue(code: &str, message: &str, severity: Severity) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        message: message.to_string(),
        path: None,
        row: None,
        severity,
    }
}
