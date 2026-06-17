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
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
    Info,
}

enum_slug!(Severity {
    Error => "error",
    Warning => "warning",
    Info => "info",
});

impl core::fmt::Display for Severity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldWorkbookValidation {
    pub row_count: usize,
    pub usable_candidate_count: usize,
    pub excluded_row_count: usize,
    pub exclusion_reasons: BTreeMap<String, usize>,
    pub key_table_counts: BTreeMap<String, usize>,
}

/// Hard upper bound on a sector's side length. Far above any real authored
/// sector (the largest shipped example, `examples/SEC100`, is 100×100) — its
/// only job is to reject a hand-edited or malformed `sectorforge.toml` whose
/// absurd dimensions would otherwise allocate a multi-gigabyte cell grid and
/// OOM-abort the process (`panic = "abort"` in release). Distinct from
/// `MAX_CUSTOM_DIM` (80) in `crate::gen::random_sector`, which is the stricter
/// ergonomic cap on the `random` subcommand's curated presets.
pub const MAX_SECTOR_DIM: u32 = 1024;

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
    // Sectors must be square (width == height). This is the canonical
    // enforcement point — it catches hand-edited `sectorforge.toml`, loaded
    // sectors, and any UI that slips a non-square grid through.
    if g.sector_width != g.sector_height {
        errors.push(issue(
            "GEN_SECTOR_NOT_SQUARE",
            &format!(
                "sectors must be square: sector_width ({}) must equal sector_height ({})",
                g.sector_width, g.sector_height
            ),
            Severity::Error,
        ));
    }
    // Reject absurd grid dimensions before generation allocates the cell grid.
    // A malformed or hand-edited `sectorforge.toml` with a huge width/height
    // would otherwise build a multi-gigabyte `Vec<HexCoord>` and OOM-abort.
    if g.sector_width > MAX_SECTOR_DIM || g.sector_height > MAX_SECTOR_DIM {
        errors.push(issue(
            "GEN_SECTOR_TOO_LARGE",
            &format!(
                "sector dimensions ({} × {}) exceed the maximum supported {} per side",
                g.sector_width, g.sector_height, MAX_SECTOR_DIM
            ),
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
        &input.catalogs.world_rows,
        &input.catalogs.world_tables,
        &input.config.generation.world_selection,
    );
    if let Some(features) = &input.catalogs.authored_features {
        world_pool::apply_authored_features(&mut pool, features);
    }

    let t = &input.catalogs.world_tables;
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

    if input.catalogs.world_rows.is_empty() {
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
        let usable = pool.candidates.len();
        let breakdown = exclusion_reasons
            .iter()
            .map(|(r, c)| format!("{r}: {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        // A few malformed rows are a warning. But when *more rows are dropped
        // than kept*, the sector would silently generate from a fraction of its
        // intended templates — collapsing world and feature variety (e.g. a
        // column-truncated workbook). Escalate so it can't pass unnoticed.
        if n > usable {
            errors.push(issue(
                "WB_EXCLUDED_ROWS_SEVERE",
                &format!(
                    "{n} of {} workbook row(s) were excluded — more than half; \
                     only {usable} usable template(s) remain ({breakdown})",
                    n + usable
                ),
                Severity::Error,
            ));
        } else {
            warnings.push(issue(
                "WB_EXCLUDED_ROWS",
                &format!("{n} workbook row(s) were excluded ({breakdown})"),
                Severity::Warning,
            ));
        }
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
    for (idx, f) in input.catalogs.factions.iter().enumerate() {
        if !faction_ids.insert(f.id.clone()) {
            errors.push(ValidationIssue {
                code: ValidationCode::FactionDuplicateId.as_slug().to_string(),
                message: format!("duplicate faction id '{}'", f.id),
                path: Some(format!("factions[{idx}]")),
                row: None,
                severity: Severity::Error,
            });
        }
        if !(f.weight.is_finite() && f.weight > 0.0) {
            errors.push(ValidationIssue {
                code: ValidationCode::FactionBadWeight.as_slug().to_string(),
                message: format!("faction '{}' has non-positive or non-finite weight", f.id),
                path: Some(format!("factions[{idx}]")),
                row: None,
                severity: Severity::Error,
            });
        }
        for s in &f.preferred_world_types {
            if taxonomy::parse_world_type_variant(s).is_none() {
                warnings.push(ValidationIssue {
                    code: ValidationCode::FactionUnknownWorldType
                        .as_slug()
                        .to_string(),
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
                    code: ValidationCode::FactionUnknownGovernment
                        .as_slug()
                        .to_string(),
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
                    code: ValidationCode::FactionUnknownFeature.as_slug().to_string(),
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
        let r = &input.catalogs.route_rules;
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
                    code: ValidationCode::RouteBadMultiplier.as_slug().to_string(),
                    message: "route modifier multiplier must be positive and finite".to_string(),
                    path: Some(format!("routes.modifiers[{i}]")),
                    row: None,
                    severity: Severity::Error,
                });
            }
            // The `when` condition fields (notable_feature / world_type /
            // government / route_type) are typed enums (P10), so an unknown
            // value is already a hard deserialize error when the config loads —
            // there is nothing left to warn about here.
        }
    }

    // ── Name pools ──────────────────────────────────────────────────────────
    let n = &input.catalogs.names.system_names;
    if n.prefixes.is_empty() && n.suffixes.is_empty() && n.single_names.is_empty() {
        warnings.push(issue(
            "NAME_POOL_EMPTY",
            "system name pool is empty; fallback names like 'System N' will be used",
            Severity::Warning,
        ));
    }

    validate_relations(
        &input.catalogs.relations,
        &input.catalogs.factions,
        &mut errors,
        &mut warnings,
    );
    validate_regions(
        &input.catalogs.regions,
        &input.config.generation,
        &mut errors,
        &mut warnings,
    );
    validate_economy(&input.catalogs.economy, &mut errors, &mut warnings);

    ValidationReport {
        ok: errors.is_empty(),
        errors,
        warnings,
        world_workbook: WorldWorkbookValidation {
            row_count: input.catalogs.world_rows.len(),
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
                    code: ValidationCode::RelationsKindRuleEmpty.as_slug().to_string(),
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
                    code: ValidationCode::RelationsPairUnknownFaction
                        .as_slug()
                        .to_string(),
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
                    code: ValidationCode::RelationsOverrideUnknownFaction
                        .as_slug()
                        .to_string(),
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
                code: ValidationCode::RelationsKindRuleEmpty.as_slug().to_string(),
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
                code: ValidationCode::RegionsConditionBadWeight
                    .as_slug()
                    .to_string(),
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
                code: ValidationCode::EconomyTechMultiplierBad
                    .as_slug()
                    .to_string(),
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
                code: ValidationCode::EconomyPopMultiplierBad
                    .as_slug()
                    .to_string(),
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
                    code: ValidationCode::ResourceScoreBad.as_slug().to_string(),
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
                code: ValidationCode::ResourceTradeMultiplierBad
                    .as_slug()
                    .to_string(),
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
                code: ValidationCode::ResourceSupplyResilienceBad
                    .as_slug()
                    .to_string(),
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

/// Enum-backed subset of the validation `code` strings. Each variant maps to a
/// stable SCREAMING_SNAKE_CASE slug via [`Self::as_slug`], which is what
/// surfaces in the report JSON and the markdown render.
///
/// **This enum is *not* the complete code registry.** Today only the
/// config-driven `resources`/`relations`/`economy`/`regions`/`route`/`faction`
/// validators route through it; the structural checks (`GEN_*`, `OUT_*`,
/// `WB_*`, `KEY_TABLE_EMPTY`, `NAME_POOL_EMPTY`, …) still emit their codes as
/// bare string literals via the [`issue`] helper. Both forms produce the same
/// `ValidationIssue { code: String, .. }`; this enum simply gives a portion of
/// them compile-time-checked, single-site definitions. Preferring
/// `ValidationCode::Foo.as_slug()` over an inline literal — where a variant
/// exists — buys:
///
/// 1. Typos caught at compile time.
/// 2. Central enumerability for that subset (e.g. documentation, or a future
///    `--list-codes` CLI flag).
/// 3. Renaming a code as a single-site edit.
///
/// New codes do not *have* to be added here, but doing so is encouraged. The
/// `validation_code_slugs_are_unique_and_screaming_snake_case` test guards the
/// casing and uniqueness of the slugs this enum does own.
///
/// `#[non_exhaustive]` so adding a new code is not a SemVer break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ValidationCode {
    FactionDuplicateId,
    FactionBadWeight,
    FactionUnknownWorldType,
    FactionUnknownGovernment,
    FactionUnknownFeature,
    RouteBadMultiplier,
    RelationsKindRuleEmpty,
    RelationsPairUnknownFaction,
    RelationsOverrideUnknownFaction,
    RegionsConditionBadWeight,
    EconomyTechMultiplierBad,
    EconomyPopMultiplierBad,
    ResourceScoreBad,
    ResourceTradeMultiplierBad,
    ResourceSupplyResilienceBad,
}

enum_slug!(const ValidationCode {
    FactionDuplicateId => "FACTION_DUPLICATE_ID",
    FactionBadWeight => "FACTION_BAD_WEIGHT",
    FactionUnknownWorldType => "FACTION_UNKNOWN_WORLD_TYPE",
    FactionUnknownGovernment => "FACTION_UNKNOWN_GOVERNMENT",
    FactionUnknownFeature => "FACTION_UNKNOWN_FEATURE",
    RouteBadMultiplier => "ROUTE_BAD_MULTIPLIER",
    RelationsKindRuleEmpty => "RELATIONS_KIND_RULE_EMPTY",
    RelationsPairUnknownFaction => "RELATIONS_PAIR_UNKNOWN_FACTION",
    RelationsOverrideUnknownFaction => "RELATIONS_OVERRIDE_UNKNOWN_FACTION",
    RegionsConditionBadWeight => "REGIONS_CONDITION_BAD_WEIGHT",
    EconomyTechMultiplierBad => "ECONOMY_TECH_MULTIPLIER_BAD",
    EconomyPopMultiplierBad => "ECONOMY_POP_MULTIPLIER_BAD",
    ResourceScoreBad => "RESOURCE_SCORE_BAD",
    ResourceTradeMultiplierBad => "RESOURCE_TRADE_MULTIPLIER_BAD",
    ResourceSupplyResilienceBad => "RESOURCE_SUPPLY_RESILIENCE_BAD",
});

#[cfg(test)]
mod code_registry_tests {
    use super::ValidationCode;

    /// Every [`ValidationCode`] variant, enumerated for the guard test below.
    /// Kept in lock-step with the enum by [`assert_listed`], which `match`es
    /// exhaustively (no `_` arm) — so adding a new variant is a compile error
    /// until it is also added here, and no code can silently escape the guard.
    const ALL: &[ValidationCode] = &[
        ValidationCode::FactionDuplicateId,
        ValidationCode::FactionBadWeight,
        ValidationCode::FactionUnknownWorldType,
        ValidationCode::FactionUnknownGovernment,
        ValidationCode::FactionUnknownFeature,
        ValidationCode::RouteBadMultiplier,
        ValidationCode::RelationsKindRuleEmpty,
        ValidationCode::RelationsPairUnknownFaction,
        ValidationCode::RelationsOverrideUnknownFaction,
        ValidationCode::RegionsConditionBadWeight,
        ValidationCode::EconomyTechMultiplierBad,
        ValidationCode::EconomyPopMultiplierBad,
        ValidationCode::ResourceScoreBad,
        ValidationCode::ResourceTradeMultiplierBad,
        ValidationCode::ResourceSupplyResilienceBad,
    ];

    /// Compile-time anchor: this exhaustive `match` (no `_` arm) fails to compile
    /// if a `ValidationCode` variant is added without also being added to `ALL`.
    /// Called from the test below so it is never dead code.
    fn assert_listed(code: ValidationCode) {
        match code {
            ValidationCode::FactionDuplicateId
            | ValidationCode::FactionBadWeight
            | ValidationCode::FactionUnknownWorldType
            | ValidationCode::FactionUnknownGovernment
            | ValidationCode::FactionUnknownFeature
            | ValidationCode::RouteBadMultiplier
            | ValidationCode::RelationsKindRuleEmpty
            | ValidationCode::RelationsPairUnknownFaction
            | ValidationCode::RelationsOverrideUnknownFaction
            | ValidationCode::RegionsConditionBadWeight
            | ValidationCode::EconomyTechMultiplierBad
            | ValidationCode::EconomyPopMultiplierBad
            | ValidationCode::ResourceScoreBad
            | ValidationCode::ResourceTradeMultiplierBad
            | ValidationCode::ResourceSupplyResilienceBad => {}
        }
    }

    /// Every enum-backed slug is SCREAMING_SNAKE_CASE and unique. Guards the
    /// "split-brain" code registry (Finding #39): the enum owns only a subset of
    /// the codes, but the slugs it does own must stay well-formed and collision
    /// free.
    #[test]
    fn validation_code_slugs_are_unique_and_screaming_snake_case() {
        use std::collections::BTreeSet;

        let mut seen: BTreeSet<&'static str> = BTreeSet::new();
        for &code in ALL {
            assert_listed(code);
            let slug = code.as_slug();

            // SCREAMING_SNAKE_CASE: starts with an uppercase letter, then only
            // uppercase letters, digits, or underscores. No leading digit, no
            // lowercase, no other punctuation, never empty.
            assert!(
                matches!(slug.chars().next(), Some('A'..='Z')),
                "slug {slug:?} must start with an uppercase ASCII letter"
            );
            assert!(
                slug.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                "slug {slug:?} must be SCREAMING_SNAKE_CASE (^[A-Z][A-Z0-9_]*$)"
            );

            assert!(
                seen.insert(slug),
                "duplicate validation code slug: {slug:?}"
            );
        }

        assert_eq!(
            seen.len(),
            ALL.len(),
            "every ValidationCode variant must contribute a distinct slug"
        );
    }
}
