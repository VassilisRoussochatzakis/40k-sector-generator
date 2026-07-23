//! Native typed worlds configuration (TOML).
//!
//! Authoritative source of truth for worlds data. Edited inside the
//! application via typed widgets or hand-authored as plain TOML.
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

use crate::worlds::{GenerationRow, KeyTables, NotableFeature, StarColour, WorldType};

/// Default filename for the native worlds config inside a project's
/// `data/worlds/` directory.
pub const DEFAULT_FILENAME: &str = "worlds.toml";

/// Error returned by the worlds-data loader. Moved here from `worlds.rs`
/// together with the IO surface (`WorldsLoad` / [`load_worlds_data`]);
/// re-exported at `crate::worlds::WorldError` for path stability.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WorldError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
#[non_exhaustive]
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

/// Structured feature pool with per-world-type and per-star-colour
/// weighted overlays on top of a global pool.
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
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<Self, WorldsTomlError> {
        toml::from_str(text).map_err(|e| WorldsTomlError::Parse(e.to_string()))
    }

    /// Emit pretty TOML.
    pub fn to_toml_string(&self) -> Result<String, WorldsTomlError> {
        toml::to_string_pretty(self).map_err(|e| WorldsTomlError::Emit(e.to_string()))
    }

    /// Convert into the existing loader-tuple shape so downstream code
    /// (`world_pool::build_pool`) is untouched. Always uses the
    /// enum-derived `KeyTables` since enums are the authoritative
    /// variant set.
    pub fn to_loader_inputs(&self) -> (KeyTables, Vec<GenerationRow>) {
        (KeyTables::from_enums(), self.generation.clone())
    }

    /// Resolved feature pool with parsed map keys. Returns the empty
    /// shape when no `[features]` block was authored.
    pub fn resolved_features(&self) -> Result<ResolvedFeaturePool, WorldsTomlError> {
        let mut by_world_type: BTreeMap<WorldType, Vec<WeightedFeatureEntry>> = BTreeMap::new();
        for (k, v) in &self.features.by_world_type {
            let wt = crate::taxonomy::parse_world_type_variant(k).ok_or_else(|| {
                WorldsTomlError::BadVariant {
                    kind: "WorldType",
                    value: k.clone(),
                }
            })?;
            by_world_type.insert(wt, v.clone());
        }
        let mut by_star_colour: BTreeMap<StarColour, Vec<WeightedFeatureEntry>> = BTreeMap::new();
        for (k, v) in &self.features.by_star_colour {
            let sc = crate::taxonomy::parse_star_colour_variant(k).ok_or_else(|| {
                WorldsTomlError::BadVariant {
                    kind: "StarColour",
                    value: k.clone(),
                }
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

/// Full worlds-data load result. Includes the authored structured
/// feature pool (`§45 WD3`) when the TOML path is in use.
pub struct WorldsLoad {
    pub tables: KeyTables,
    pub rows: Vec<GenerationRow>,
    pub authored_features: Option<crate::worlds_toml::ResolvedFeaturePool>,
}

/// Load both row data and (when available) the authored feature pool.
///
/// Reads `<data_dir>/worlds.toml` (the only supported format).
/// Callers building a `WorldCandidatePool` should pass the returned
/// `authored_features` to `world_pool::apply_authored_features` so the
/// structured pool overlays the row-derived one.
pub fn load_worlds_data(data_dir: impl AsRef<Path>) -> Result<WorldsLoad, WorldError> {
    let dir = data_dir.as_ref();
    let toml_path = dir.join(crate::worlds_toml::DEFAULT_FILENAME);
    let cfg = crate::worlds_toml::WorldsConfig::from_path(&toml_path)
        .map_err(|e| WorldError::Invalid(format!("worlds.toml: {e}")))?;
    let (tables, rows) = cfg.to_loader_inputs();
    let features = cfg
        .resolved_features()
        .map_err(|e| WorldError::Invalid(format!("worlds.toml features: {e}")))?;
    let authored_features = if features.is_empty() {
        None
    } else {
        Some(features)
    };
    Ok(WorldsLoad {
        tables,
        rows,
        authored_features,
    })
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

    proptest::proptest! {
        /// TF-T-9: any `WorldsConfig` value (within our bounded shape) must
        /// round-trip through TOML emission/parsing. Catches escape bugs and
        /// missing `#[serde(default)]` annotations on optional fields.
        #[test]
        fn worlds_config_toml_roundtrips(
            seed in 0u64..1024,
            n_rows in 0usize..6,
            n_global in 0usize..6,
        ) {
            // Deterministic but proptest-driven instance: pick variants by seed
            // so each shrink converges on a small failing case.
            let pick_world = [
                WorldType::HiveWorld,
                WorldType::FeudalWorld,
                WorldType::AgriWorld,
                WorldType::ForgeWorld,
            ];
            let pick_star = [
                StarColour::Yellow,
                StarColour::RedDwarf,
                StarColour::White,
                StarColour::BlueWhite,
            ];
            let pick_feature = [
                NotableFeature::PowerfulNobles,
                NotableFeature::WarpPhenomena,
                NotableFeature::PowerfulCriminals,
                NotableFeature::ImportantShrine,
            ];

            let mut rows = Vec::new();
            for i in 0..n_rows {
                let s = (seed as usize + i) % pick_world.len();
                rows.push(GenerationRow {
                    star_colour: Some(pick_star[s % pick_star.len()]),
                    world_type: Some(pick_world[s].clone()),
                    atmosphere: None,
                    temperature: None,
                    biosphere: None,
                    population: None,
                    tech: None,
                    government: None,
                    notable_feature: Some(pick_feature[s % pick_feature.len()].clone()),
                    counter: None,
                    weight: Some(1.0 + (s as f64) * 0.5),
                });
            }
            let mut global = Vec::new();
            for i in 0..n_global {
                global.push(WeightedFeatureEntry {
                    feature: pick_feature[(seed as usize + i) % pick_feature.len()].clone(),
                    weight: 1.0 + (i as f64) * 0.25,
                });
            }
            let cfg = WorldsConfig {
                generation: rows,
                features: FeaturePoolConfig {
                    global,
                    ..Default::default()
                },
            };

            let s = cfg.to_toml_string().expect("emit");
            let back = WorldsConfig::from_str(&s).expect("parse");

            // Round-trip on the toml::Value layer keeps the comparison tolerant
            // to harmless cosmetic differences (key ordering, integer vs float
            // for whole-number weights) while still catching real corruption.
            let lhs: toml::Value = toml::from_str(&s).unwrap();
            let rhs: toml::Value = toml::from_str(&back.to_toml_string().unwrap()).unwrap();
            proptest::prop_assert_eq!(lhs, rhs);
        }
    }
}
