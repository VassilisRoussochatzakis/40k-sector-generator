//! Adapts `worlds::GenerationRow` rows into a weighted candidate pool.

use std::collections::BTreeMap;

use crate::config::WorldSelectionConfig;
use crate::errors::SectorError;
use crate::taxonomy;
use crate::worlds::{
    Atmosphere, Biosphere, GenerationRow, Government, KeyTables, NotableFeature, Population,
    StarColour, TechLevel, Temperature, World, WorldType,
};

#[derive(Debug, Clone)]
pub struct WorldCandidate {
    pub row_index: usize,
    pub star_colour: StarColour,
    pub world_type: WorldType,
    pub atmosphere: Atmosphere,
    pub temperature: Temperature,
    pub biosphere: Biosphere,
    pub population: Population,
    pub tech_level: TechLevel,
    pub government: Government,
    pub primary_feature: Option<NotableFeature>,
    pub counter: Option<usize>,
    pub weight: f64,
}

impl WorldCandidate {
    pub fn to_world(&self, features: Vec<NotableFeature>) -> World {
        World {
            star_colour: self.star_colour,
            world_type: self.world_type.clone(),
            atmosphere: self.atmosphere.clone(),
            temperature: self.temperature.clone(),
            biosphere: self.biosphere.clone(),
            population: self.population.clone(),
            tech_level: self.tech_level.clone(),
            government: self.government.clone(),
            notable_features: features,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorldCandidatePool {
    pub candidates: Vec<WorldCandidate>,
    pub excluded_rows: Vec<ExcludedRow>,
    pub feature_pool: FeaturePool,
}

#[derive(Debug, Clone)]
pub struct ExcludedRow {
    pub row_index: usize,
    pub reason: ExclusionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionReason {
    MissingWeight,
    NonPositiveWeight,
    NaNOrInfiniteWeight,
    MissingRequiredField(&'static str),
}

impl std::fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingWeight => write!(f, "missing weight"),
            Self::NonPositiveWeight => write!(f, "weight <= 0"),
            Self::NaNOrInfiniteWeight => write!(f, "weight is NaN or infinite"),
            Self::MissingRequiredField(name) => write!(f, "missing field: {name}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeaturePool {
    pub by_world_type: BTreeMap<String, Vec<WeightedFeature>>,
    pub by_star_colour: BTreeMap<String, Vec<WeightedFeature>>,
    pub global: Vec<WeightedFeature>,
    pub key_table_features: Vec<NotableFeature>,
}

#[derive(Debug, Clone)]
pub struct WeightedFeature {
    pub feature: NotableFeature,
    pub weight: f64,
}

/// Build a candidate pool from raw `GenerationRow` values.
///
/// `require_complete_rows = true` is the strict default: only fully resolved
/// rows with positive finite weights become candidates. Anything else is
/// reported via `excluded_rows`.
pub fn build_pool(
    rows: &[GenerationRow],
    tables: &KeyTables,
    cfg: &WorldSelectionConfig,
) -> WorldCandidatePool {
    let mut pool = WorldCandidatePool::default();

    for (row_index, row) in rows.iter().enumerate() {
        let weight = match row.weight {
            None => {
                pool.excluded_rows.push(ExcludedRow {
                    row_index,
                    reason: ExclusionReason::MissingWeight,
                });
                continue;
            }
            Some(w) if !w.is_finite() => {
                pool.excluded_rows.push(ExcludedRow {
                    row_index,
                    reason: ExclusionReason::NaNOrInfiniteWeight,
                });
                continue;
            }
            Some(w) if w <= 0.0 => {
                pool.excluded_rows.push(ExcludedRow {
                    row_index,
                    reason: ExclusionReason::NonPositiveWeight,
                });
                continue;
            }
            Some(w) => w,
        };

        if cfg.require_complete_rows {
            let missing = first_missing_field(row);
            if let Some(field) = missing {
                pool.excluded_rows.push(ExcludedRow {
                    row_index,
                    reason: ExclusionReason::MissingRequiredField(field),
                });
                continue;
            }
        } else if first_missing_field(row).is_some() {
            // Partial-row mode (allow_partial_rows) is not implemented in the
            // first cut; exclude and report.
            pool.excluded_rows.push(ExcludedRow {
                row_index,
                reason: ExclusionReason::MissingRequiredField("partial_rows_unsupported"),
            });
            continue;
        }

        // SAFETY: presence of each field already checked above.
        let cand = WorldCandidate {
            row_index,
            star_colour: row.star_colour.unwrap(),
            world_type: row.world_type.clone().unwrap(),
            atmosphere: row.atmosphere.clone().unwrap(),
            temperature: row.temperature.clone().unwrap(),
            biosphere: row.biosphere.clone().unwrap(),
            population: row.population.clone().unwrap(),
            tech_level: row.tech.clone().unwrap(),
            government: row.government.clone().unwrap(),
            primary_feature: row.notable_feature.clone(),
            counter: row.counter,
            weight,
        };

        if let Some(feature) = &cand.primary_feature {
            let wf = WeightedFeature {
                feature: feature.clone(),
                weight,
            };
            pool.feature_pool
                .by_world_type
                .entry(cand.world_type.to_string())
                .or_default()
                .push(wf.clone());
            pool.feature_pool
                .by_star_colour
                .entry(taxonomy::star_colour_variant_name(cand.star_colour).to_string())
                .or_default()
                .push(wf.clone());
            pool.feature_pool.global.push(wf);
        }

        pool.candidates.push(cand);
    }

    // Populate key_table_features from the Key sheet so partial pools can still
    // pull from a full notable-feature vocabulary.
    for name in &tables.notable_features {
        if let Some(f) = taxonomy::parse_notable_feature_variant(name)
            .or_else(|| name.parse::<NotableFeature>().ok())
        {
            pool.feature_pool.key_table_features.push(f);
        }
    }

    pool
}

fn first_missing_field(row: &GenerationRow) -> Option<&'static str> {
    if row.star_colour.is_none() {
        return Some("star_colour");
    }
    if row.world_type.is_none() {
        return Some("world_type");
    }
    if row.atmosphere.is_none() {
        return Some("atmosphere");
    }
    if row.temperature.is_none() {
        return Some("temperature");
    }
    if row.biosphere.is_none() {
        return Some("biosphere");
    }
    if row.population.is_none() {
        return Some("population");
    }
    if row.tech.is_none() {
        return Some("tech");
    }
    if row.government.is_none() {
        return Some("government");
    }
    None
}

// ── Workbook inspection helpers (used by `inspect-worlds` CLI) ──────────────

#[derive(Debug, Clone)]
pub struct WorkbookStats {
    pub data_dir: String,
    pub key_table_counts: BTreeMap<String, usize>,
    pub generator_rows: usize,
    pub usable_candidates: usize,
    pub excluded_rows: usize,
    pub top_star_colours: Vec<(String, f64)>,
    pub top_world_types: Vec<(String, f64)>,
    pub top_features: Vec<(String, usize)>,
}

pub fn inspect_workbook(path: &str) -> Result<WorkbookStats, SectorError> {
    let (tables, rows) = crate::worlds::load_generation_rows(std::path::Path::new(path))
        .map_err(|message| SectorError::WorldDataLoad {
            path: path.to_string(),
            message,
        })?;

    let cfg = WorldSelectionConfig::default();
    let pool = build_pool(&rows, &tables, &cfg);

    let key_counts: BTreeMap<String, usize> = [
        ("star_colours", tables.star_colours.len()),
        ("world_types", tables.world_types.len()),
        ("atmospheres", tables.atmospheres.len()),
        ("temperatures", tables.temperatures.len()),
        ("biospheres", tables.biospheres.len()),
        ("populations", tables.populations.len()),
        ("tech_levels", tables.tech_levels.len()),
        ("governments", tables.governments.len()),
        ("notable_features", tables.notable_features.len()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let mut star_totals: BTreeMap<String, f64> = BTreeMap::new();
    let mut wt_totals: BTreeMap<String, f64> = BTreeMap::new();
    let mut feat_counts: BTreeMap<String, usize> = BTreeMap::new();

    for c in &pool.candidates {
        let sc_name = taxonomy::star_colour_variant_name(c.star_colour).to_string();
        *star_totals.entry(sc_name).or_insert(0.0) += c.weight;
        *wt_totals.entry(c.world_type.to_string()).or_insert(0.0) += c.weight;
        if let Some(f) = &c.primary_feature {
            *feat_counts.entry(f.to_string()).or_insert(0) += 1;
        }
    }

    let top_star_colours = top_by_weight(star_totals, 10);
    let top_world_types = top_by_weight(wt_totals, 10);
    let top_features = top_by_count(feat_counts, 10);

    Ok(WorkbookStats {
        data_dir: path.to_string(),
        key_table_counts: key_counts,
        generator_rows: rows.len(),
        usable_candidates: pool.candidates.len(),
        excluded_rows: pool.excluded_rows.len(),
        top_star_colours,
        top_world_types,
        top_features,
    })
}

fn top_by_weight(map: BTreeMap<String, f64>, n: usize) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(n);
    v
}

fn top_by_count(map: BTreeMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(n);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(weight: Option<f64>) -> GenerationRow {
        GenerationRow {
            star_colour: Some(StarColour::RedDwarf),
            world_type: Some(WorldType::HiveWorld),
            atmosphere: Some(Atmosphere::Toxic),
            temperature: Some(Temperature::Temperate),
            biosphere: Some(Biosphere::Poisoned),
            population: Some(Population::ExtremelyDense),
            tech: Some(TechLevel::Standard),
            government: Some(Government::MilitaryGovernor),
            notable_feature: Some(NotableFeature::TradeHub),
            counter: Some(1),
            weight,
        }
    }

    fn missing_pop_row() -> GenerationRow {
        let mut r = row(Some(1.0));
        r.population = None;
        r
    }

    #[test]
    fn world_pool_excludes_rows_missing_weight() {
        let rows = vec![row(None), row(Some(1.0))];
        let tables = KeyTables::default();
        let pool = build_pool(&rows, &tables, &WorldSelectionConfig::default());
        assert_eq!(pool.candidates.len(), 1);
        assert_eq!(pool.excluded_rows.len(), 1);
        assert_eq!(pool.excluded_rows[0].reason, ExclusionReason::MissingWeight);
    }

    #[test]
    fn world_pool_excludes_rows_with_zero_or_negative_weight() {
        let rows = vec![row(Some(0.0)), row(Some(-1.5)), row(Some(2.0))];
        let pool = build_pool(
            &rows,
            &KeyTables::default(),
            &WorldSelectionConfig::default(),
        );
        assert_eq!(pool.candidates.len(), 1);
        assert_eq!(pool.excluded_rows.len(), 2);
    }

    #[test]
    fn world_pool_requires_complete_rows_when_strict() {
        let rows = vec![missing_pop_row(), row(Some(1.0))];
        let pool = build_pool(
            &rows,
            &KeyTables::default(),
            &WorldSelectionConfig::default(),
        );
        assert_eq!(pool.candidates.len(), 1);
        assert_eq!(pool.excluded_rows.len(), 1);
    }

    #[test]
    fn world_candidate_to_world_preserves_fields() {
        let rows = vec![row(Some(1.0))];
        let pool = build_pool(
            &rows,
            &KeyTables::default(),
            &WorldSelectionConfig::default(),
        );
        let cand = &pool.candidates[0];
        let features = vec![NotableFeature::TradeHub, NotableFeature::PoliceState];
        let w = cand.to_world(features.clone());
        assert_eq!(w.star_colour, cand.star_colour);
        assert_eq!(w.world_type, cand.world_type);
        assert_eq!(w.notable_features, features);
    }
}
