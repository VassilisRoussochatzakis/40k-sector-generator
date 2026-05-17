//! Output DTOs for a generated sector. Separate from worlds.rs.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedSector {
    pub id: String,
    pub title: String,
    pub seed: String,
    pub generator_name: String,
    pub generator_version: String,
    pub width: u32,
    pub height: u32,
    pub systems: Vec<GeneratedSystem>,
    pub routes: Vec<GeneratedRoute>,
    pub factions: Vec<GeneratedFaction>,
    pub manifest: GenerationManifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedSystem {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub coord: HexCoord,
    pub star: GeneratedStar,
    pub worlds: Vec<GeneratedWorld>,
    pub primary_factions: Vec<String>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedStar {
    pub colour_code: String,
    pub colour_name: String,
    pub spectral_type: Option<String>,
    pub source_row_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedWorld {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub orbit: u8,
    pub source_row_index: usize,
    pub world: WorldDto,
    pub factions: Vec<WorldFactionPresence>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

/// Serializable view over crate::worlds::World. Variant names are stable
/// because worlds.rs Display impls use Debug (e.g. "HiveWorld").
#[derive(Debug, Clone, Serialize)]
pub struct WorldDto {
    pub star_colour: String,
    pub star_colour_code: String,
    pub world_type: String,
    pub atmosphere: String,
    pub temperature: String,
    pub biosphere: String,
    pub population: String,
    pub tech_level: String,
    pub government: String,
    pub notable_features: Vec<String>,
}

impl From<&crate::worlds::World> for WorldDto {
    fn from(w: &crate::worlds::World) -> Self {
        Self {
            star_colour: w.star_colour.short_name().to_string(),
            star_colour_code: w.star_colour.code().to_string(),
            world_type: format!("{}", w.world_type),
            atmosphere: format!("{}", w.atmosphere),
            temperature: format!("{}", w.temperature),
            biosphere: format!("{}", w.biosphere),
            population: format!("{}", w.population),
            tech_level: format!("{}", w.tech_level),
            government: format!("{}", w.government),
            notable_features: w
                .notable_features
                .iter()
                .map(|f| format!("{}", f))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedRoute {
    pub id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub distance: u32,
    pub route_type: RouteType,
    pub stability: RouteStability,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteType {
    StableWarpLane,
    ChartedPassage,
    DangerousPassage,
    SecretPassage,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteStability {
    Stable,
    Unstable,
    Hazardous,
    Lost,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedFaction {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub disposition: String,
    pub system_presence: Vec<String>,
    pub world_presence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldFactionPresence {
    pub faction_id: String,
    pub influence: FactionInfluence,
    pub relationship_to_government: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FactionInfluence {
    Hidden,
    Minor,
    Significant,
    Dominant,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationManifest {
    pub project_id: String,
    pub generated_at_policy: String,
    pub generator_name: String,
    pub generator_version: String,
    pub seed: String,
    pub seed_hash: String,
    pub profile: Option<String>,
    pub input_digests: BTreeMap<String, String>,
    pub settings_digest: String,
    pub system_count: usize,
    pub world_count: usize,
    pub route_count: usize,
}

/// Axial hex distance.
pub fn hex_distance(a: HexCoord, b: HexCoord) -> u32 {
    let dq = (a.q - b.q).abs();
    let dr = (a.r - b.r).abs();
    let ds = ((-a.q - a.r) - (-b.q - b.r)).abs();
    dq.max(dr).max(ds) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_distance_known_examples() {
        assert_eq!(hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 0, r: 0 }), 0);
        assert_eq!(hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 3, r: 0 }), 3);
        assert_eq!(hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 0, r: 3 }), 3);
        assert_eq!(hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 2 }), 4);
        assert_eq!(hex_distance(HexCoord { q: 1, r: -2 }, HexCoord { q: 3, r: -3 }), 2);
    }
}
