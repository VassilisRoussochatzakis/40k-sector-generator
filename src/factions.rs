//! Faction definitions loaded from factions.toml.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionsFile {
    #[serde(default)]
    pub factions: Vec<FactionDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionDef {
    pub id: crate::ids::FactionId,
    pub name: String,
    pub kind: String,
    pub weight: f64,
    #[serde(default)]
    pub default_disposition: String,
    #[serde(default)]
    pub preferred_world_types: Vec<String>,
    #[serde(default)]
    pub preferred_governments: Vec<String>,
    #[serde(default)]
    pub preferred_notable_features: Vec<String>,
}
