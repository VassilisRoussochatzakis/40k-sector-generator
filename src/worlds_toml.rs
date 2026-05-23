//! Native typed worlds configuration (TOML).
//!
//! Replaces the twin-CSV (`key.csv` + `generator.csv`) shape as the
//! authoritative source of truth for worlds data when the project is
//! authored inside the application. CSV remains an optional
//! import/export adapter for spreadsheet workflows.
//!
//! Layout:
//!
//! ```toml
//! [[generation]]
//! star_colour = "Yellow"
//! world_type  = "HiveWorld"
//! atmosphere  = "Breathable"
//! temperature = "Temperate"
//! biosphere   = "Thriving"
//! population  = "ExtremelyDense"
//! tech_level  = "High"
//! government  = "MilitaryGovernor"
//! notable_feature = "PowerfulNobles"
//! weight = 4.0
//!
//! [features]
//! global = [
//!   { feature = "WarpPhenomena", weight = 1.0 },
//! ]
//!
//! [features.by_world_type]
//! HiveWorld = [
//!   { feature = "PowerfulNobles", weight = 2.0 },
//!   { feature = "MartialLaw",     weight = 1.5 },
//! ]
//!
//! [features.by_star_colour]
//! O = [
//!   { feature = "CelestialPhenomena", weight = 3.0 },
//! ]
//! ```
//!
//! Map keys for `by_world_type` / `by_star_colour` use the Rust variant
//! names (e.g. `HiveWorld`, not `"Hive World"`) so they parse cleanly
//! as bare TOML keys.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::worlds::{GenerationRow, KeyTables, NotableFeature, StarColour, WorldError, WorldType};

/// Default filename for the native worlds config inside a project's
/// `data/worlds/` directory.
pub const DEFAULT_FILENAME: &str = "worlds.toml";

#[derive(Debug, Error)]
pub enum WorldsTomlError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(String),
    #[error("TOML emit error: {0}")]
    Emit(String),
    #[error("invalid variant name: {kind} = {value}")]
    BadVariant { kind: &'static str, value: String },
    #[error("world data: {0}")]
    World(#[from] WorldError),
}

/// Top-level native worlds config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldsConfig {
    #[serde(default)]
    pub generation: Vec<GenerationRow>,
    #[serde(default)]
    pub features: FeaturePoolConfig,
}

/// Structured feature pool. Replaces the implicit single-column
/// `notable_feature` flattening that CSV forced on the generator.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeaturePoolConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub global: Vec<WeightedFeatureEntry>,
    /// Map key: `WorldType` Rust variant name (e.g. `"HiveWorld"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub by_world_type: BTreeMap<String, Vec<WeightedFeatureEntry>>,
    /// Map key: `StarColour` Rust variant name (e.g. `"Yellow"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub by_star_colour: BTreeMap<String, Vec<WeightedFeatureEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedFeatureEntry {
    pub feature: NotableFeature,
    pub weight: f64,
}

impl WorldsConfig {
    /// Read a `worlds.toml` from disk.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, WorldsTomlError> {
        let text = fs::read_to_string(path.as_ref())?;
        Self::from_str(&text)
    }

    /// Parse from a TOML string.
    pub fn from_str(text: &str) -> Result<Self, WorldsTomlError> {
        toml::from_str(text).map_err(|e| WorldsTomlError::Parse(e.to_string()))
    }

    /// Emit pretty TOML.
    pub fn to_toml_string(&self) -> Result<String, WorldsTomlError> {
        toml::to_string_pretty(self).map_err(|e| WorldsTomlError::Emit(e.to_string()))
    }

    /// Convert into the existing loader-tuple shape so downstream code
    /// (`world_pool::build_pool`) is untouched. Always uses the
    /// enum-derived `KeyTables` since `worlds.toml` is by definition
    /// the native, enum-aware source.
    pub fn to_loader_inputs(&self) -> (KeyTables, Vec<GenerationRow>) {
        (KeyTables::from_enums(), self.generation.clone())
    }

    /// Resolved feature pool with parsed map keys. Returns the empty
    /// shape when no `[features]` block was authored.
    pub fn resolved_features(&self) -> Result<ResolvedFeaturePool, WorldsTomlError> {
        let mut by_world_type: BTreeMap<WorldType, Vec<WeightedFeatureEntry>> = BTreeMap::new();
        for (k, v) in &self.features.by_world_type {
            let wt = parse_world_type_variant(k).ok_or_else(|| WorldsTomlError::BadVariant {
                kind: "WorldType",
                value: k.clone(),
            })?;
            by_world_type.insert(wt, v.clone());
        }
        let mut by_star_colour: BTreeMap<StarColour, Vec<WeightedFeatureEntry>> = BTreeMap::new();
        for (k, v) in &self.features.by_star_colour {
            let sc = parse_star_colour_variant(k).ok_or_else(|| WorldsTomlError::BadVariant {
                kind: "StarColour",
                value: k.clone(),
            })?;
            by_star_colour.insert(sc, v.clone());
        }
        Ok(ResolvedFeaturePool {
            global: self.features.global.clone(),
            by_world_type,
            by_star_colour,
        })
    }
}

/// Feature pool with parsed map keys (post-validation).
#[derive(Debug, Clone, Default)]
pub struct ResolvedFeaturePool {
    pub global: Vec<WeightedFeatureEntry>,
    pub by_world_type: BTreeMap<WorldType, Vec<WeightedFeatureEntry>>,
    pub by_star_colour: BTreeMap<StarColour, Vec<WeightedFeatureEntry>>,
}

impl ResolvedFeaturePool {
    pub fn is_empty(&self) -> bool {
        self.global.is_empty() && self.by_world_type.is_empty() && self.by_star_colour.is_empty()
    }
}

/// Match Rust variant names for `WorldType` (e.g. `"HiveWorld"`).
fn parse_world_type_variant(s: &str) -> Option<WorldType> {
    WorldType::VARIANTS
        .iter()
        .find(|v| format!("{v:?}") == s)
        .cloned()
}

/// Match Rust variant names for `StarColour` (e.g. `"Yellow"`).
fn parse_star_colour_variant(s: &str) -> Option<StarColour> {
    StarColour::VARIANTS
        .iter()
        .copied()
        .find(|v| format!("{v:?}") == s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worlds::{Atmosphere, Biosphere, Government, Population, TechLevel, Temperature};

    fn sample() -> WorldsConfig {
        WorldsConfig {
            generation: vec![GenerationRow {
                star_colour: Some(StarColour::Yellow),
                world_type: Some(WorldType::HiveWorld),
                atmosphere: Some(Atmosphere::Breathable),
                temperature: Some(Temperature::Temperate),
                biosphere: Some(Biosphere::Thriving),
                population: Some(Population::ExtremelyDense),
                tech: Some(TechLevel::High),
                government: Some(Government::MilitaryGovernor),
                notable_feature: Some(NotableFeature::PowerfulNobles),
                counter: None,
                weight: Some(4.0),
            }],
            features: FeaturePoolConfig {
                global: vec![WeightedFeatureEntry {
                    feature: NotableFeature::WarpPhenomena,
                    weight: 1.0,
                }],
                by_world_type: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "HiveWorld".to_string(),
                        vec![WeightedFeatureEntry {
                            feature: NotableFeature::PowerfulNobles,
                            weight: 2.0,
                        }],
                    );
                    m
                },
                by_star_colour: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "BlueHypergiant".to_string(),
                        vec![WeightedFeatureEntry {
                            feature: NotableFeature::CelestialPhenomena,
                            weight: 3.0,
                        }],
                    );
                    m
                },
            },
        }
    }

    #[test]
    fn roundtrips_through_toml() {
        let cfg = sample();
        let text = cfg.to_toml_string().expect("emit");
        let parsed = WorldsConfig::from_str(&text).expect("parse");
        assert_eq!(parsed.generation.len(), 1);
        assert_eq!(parsed.generation[0].world_type, Some(WorldType::HiveWorld));
        assert_eq!(parsed.features.global.len(), 1);
        assert_eq!(parsed.features.by_world_type.len(), 1);
        assert_eq!(parsed.features.by_star_colour.len(), 1);
    }

    #[test]
    fn resolved_features_parses_map_keys() {
        let cfg = sample();
        let r = cfg.resolved_features().expect("resolve");
        assert!(r.by_world_type.contains_key(&WorldType::HiveWorld));
        assert!(r.by_star_colour.contains_key(&StarColour::BlueHypergiant));
    }

    #[test]
    fn resolved_features_rejects_unknown_variant() {
        let mut cfg = sample();
        cfg.features
            .by_world_type
            .insert("NotARealWorldType".to_string(), vec![]);
        let err = cfg.resolved_features().unwrap_err();
        assert!(matches!(err, WorldsTomlError::BadVariant { .. }));
    }

    #[test]
    fn to_loader_inputs_uses_enum_derived_keytables() {
        let cfg = sample();
        let (tables, rows) = cfg.to_loader_inputs();
        assert_eq!(rows.len(), 1);
        assert!(tables.world_types.contains_key("Hive World"));
        assert!(tables.star_colours.contains_key("G"));
    }
}
