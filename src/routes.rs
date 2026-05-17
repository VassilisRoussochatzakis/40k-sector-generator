//! Route rules loaded from `route_rules.toml`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRulesFile {
    pub routes: RouteRules,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRules {
    #[serde(default = "default_weight")]
    pub default_weight: f64,
    #[serde(default = "default_max_distance")]
    pub max_distance: u32,
    #[serde(default)]
    pub prefer_populated_worlds: bool,
    #[serde(default)]
    pub prefer_trade_hubs: bool,
    #[serde(default)]
    pub avoid_warp_phenomena: bool,
    #[serde(default)]
    pub modifiers: Vec<RouteModifier>,
}

impl Default for RouteRules {
    fn default() -> Self {
        Self {
            default_weight: default_weight(),
            max_distance: default_max_distance(),
            prefer_populated_worlds: true,
            prefer_trade_hubs: true,
            avoid_warp_phenomena: true,
            modifiers: Vec::new(),
        }
    }
}

fn default_weight() -> f64 {
    1.0
}

fn default_max_distance() -> u32 {
    4
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteModifier {
    pub when: RouteCondition,
    pub multiplier: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RouteCondition {
    #[serde(default)]
    pub notable_feature: Option<String>,
    #[serde(default)]
    pub world_type: Option<String>,
    #[serde(default)]
    pub government: Option<String>,
}
