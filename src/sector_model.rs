//! Output DTOs for a generated sector. Separate from worlds.rs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Aggregated multi-winner control summary across this system's worlds.
    /// See `faction_sector_control_and_power_design.md` §6.4.
    #[serde(default)]
    pub control: SystemControlSummary,
    /// Static stability snapshot averaged across worlds + bumped by
    /// `control.state` (§11.1).
    #[serde(default)]
    pub stability: crate::stability::StabilityState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedStar {
    pub colour_code: String,
    pub colour_name: String,
    pub spectral_type: Option<String>,
    pub source_row_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Per-world political claims (border outlines per design §3.3).
    #[serde(default)]
    pub claims: Vec<FactionClaim>,
    /// Multi-winner control summary (§5.3).
    #[serde(default)]
    pub control: WorldControlSummary,
    /// Static stability snapshot (§11.1). Derived from tags, world type, and
    /// factions present — no sim ticks.
    #[serde(default)]
    pub stability: crate::stability::StabilityState,
}

/// Serializable view over `crate::worlds::World`. Variant names are stable
/// because worlds.rs Display impls use Debug (e.g. "`HiveWorld`").
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            notable_features: w.notable_features.iter().map(|f| format!("{f}")).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedRoute {
    pub id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub distance: u32,
    pub route_type: RouteType,
    pub stability: RouteStability,
    pub tags: Vec<String>,
    /// Per-faction control profile along this route (§3). Empty when no
    /// faction has meaningful endpoint presence.
    #[serde(default)]
    pub controls: Vec<crate::route_control::RouteControl>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteType {
    StableWarpLane,
    ChartedPassage,
    DangerousPassage,
    SecretPassage,
}

impl RouteType {
    pub fn pattern(self) -> RoutePattern {
        match self {
            RouteType::StableWarpLane => RoutePattern::Solid,
            RouteType::ChartedPassage => RoutePattern::Dashed,
            RouteType::DangerousPassage => RoutePattern::DotDash,
            RouteType::SecretPassage => RoutePattern::Dotted,
        }
    }
}

/// Visual line pattern used to encode a `RouteType` on maps and legends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePattern {
    Solid,
    Dashed,
    DotDash,
    Dotted,
}

impl RoutePattern {
    /// Alternating on/off run-lengths in multiples of the stroke unit.
    /// An empty slice means a solid line.
    /// Runs whose length is `<= 1.5` units are rendered as a dot (filled disc)
    /// rather than a short rectangle, so dotted styles read clearly at low
    /// thickness.
    pub fn strides(self) -> &'static [f32] {
        match self {
            RoutePattern::Solid => &[],
            // Long bars: easy to read at a glance, period ~3x the dotted period.
            RoutePattern::Dashed => &[10.0, 5.0],
            // Dash + two dots: compound shape so it can't be confused with
            // a plain dash or a plain dot trail.
            RoutePattern::DotDash => &[5.0, 2.0, 1.0, 2.0, 1.0, 4.0],
            // Tight fine stippling.
            RoutePattern::Dotted => &[1.0, 2.0],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteStability {
    Stable,
    Unstable,
    Hazardous,
    Perilous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFaction {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub disposition: String,
    pub system_presence: Vec<String>,
    pub world_presence: Vec<String>,
    /// Aggregated multi-dimensional power across all controlled assets (§4.3).
    #[serde(default)]
    pub power: PowerProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFactionPresence {
    pub faction_id: String,
    pub influence: FactionInfluence,
    pub relationship_to_government: String,
    /// Multi-dimensional presence scores (§4.5). All fields in 0..=100.
    #[serde(default)]
    pub dimensions: PresenceDimensions,
    /// Computed dominance bucket from the weighted control score (§5.2).
    #[serde(default)]
    pub dominance: DominanceState,
    /// Intel-layer confidence 0..=100 (§12).
    #[serde(default = "default_intel_confidence")]
    pub intel_confidence: u8,
}

fn default_intel_confidence() -> u8 {
    100
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FactionInfluence {
    Hidden,
    Minor,
    Significant,
    Dominant,
}

impl FactionInfluence {
    /// Spec §10.9 scoring weight for primary-faction derivation.
    pub fn weight(self) -> f64 {
        match self {
            FactionInfluence::Dominant => 3.0,
            FactionInfluence::Significant => 2.0,
            FactionInfluence::Minor => 1.0,
            FactionInfluence::Hidden => 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Hex distance for pointy-top odd-r offset coordinates.
///
/// HexCoord (q, r) stores the offset column/row of a rectangular grid.
/// Converts to cube coordinates first and then takes the standard
/// max(|dx|, |dy|, |dz|) cube distance.
pub fn hex_distance(a: HexCoord, b: HexCoord) -> u32 {
    let (ax, az) = offset_r_to_cube(a);
    let ay = -ax - az;
    let (bx, bz) = offset_r_to_cube(b);
    let by = -bx - bz;
    let dx = (ax - bx).abs();
    let dy = (ay - by).abs();
    let dz = (az - bz).abs();
    dx.max(dy).max(dz) as u32
}

/// Pointy-top odd-r offset → cube (x, z). Odd rows are shifted right.
fn offset_r_to_cube(c: HexCoord) -> (i32, i32) {
    let x = c.q - (c.r - (c.r & 1)) / 2;
    let z = c.r;
    (x, z)
}

/// Pointy-top odd-r neighbor offsets for the given row parity.
/// Edge index → (dq, dr) matches vertex i / i+1 in `hex_vertices`:
/// 0:E, 1:SE, 2:SW, 3:W, 4:NW, 5:NE.
pub fn offset_r_neighbors(r: i32) -> [(i32, i32); 6] {
    if r & 1 == 0 {
        [(1, 0), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1)]
    } else {
        [(1, 0), (1, 1), (0, 1), (-1, 0), (0, -1), (1, -1)]
    }
}

// ── Multi-dimensional presence + control model ────────────────────────────────
//
// See `faction_sector_control_and_power_design.md` §4–§7. All scalar fields are
// in 0..=100 unless documented otherwise. Defaults zero so JSON files written
// before these fields existed still deserialize.

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct PresenceDimensions {
    pub admin: f32,
    pub military: f32,
    pub orbital: f32,
    pub economic: f32,
    /// Manufacturing / forge / industrial output (§4.3, §17). Distinct from
    /// `economic`, which models trade + commercial activity.
    #[serde(default)]
    pub industrial: f32,
    pub ideological: f32,
    pub covert: f32,
    pub logistics: f32,
    pub legitimacy: f32,
    /// 0..=100; how visible this presence is to the player (§4.5).
    pub visibility: f32,
}

impl PresenceDimensions {
    /// Spec §5.1: weighted local control score. Industrial output contributes
    /// at the same weight as economic since both encode commercial strength.
    #[must_use]
    pub fn local_control_score(&self) -> f32 {
        self.admin * 0.20
            + self.military * 0.18
            + self.orbital * 0.10
            + self.economic * 0.12
            + self.industrial * 0.06
            + self.ideological * 0.10
            + self.covert * 0.08
            + self.logistics * 0.10
            + self.legitimacy * 0.06
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DominanceState {
    #[default]
    Rumored,
    Presence,
    Influence,
    Contested,
    Controlled,
    Stronghold,
}

impl DominanceState {
    /// §5.2 buckets — pure mapping from control score.
    #[must_use]
    pub fn from_score(score: f32) -> Self {
        let s = score.round() as i32;
        match s {
            i if i < 10 => Self::Rumored,
            10..=24 => Self::Presence,
            25..=44 => Self::Influence,
            45..=59 => Self::Contested,
            60..=79 => Self::Controlled,
            _ => Self::Stronghold,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClaimType {
    LegalSovereignty,
    ImperialMandate,
    TreatyRight,
    ReligiousMandate,
    DynasticRight,
    CommercialCharter,
    MilitaryOccupation,
    AncientDomain,
    HuntingGround,
    CovertWrit,
    Rebellion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionClaim {
    pub faction_id: String,
    pub claim_type: ClaimType,
    /// 0..=100; populated from local control or fixed minima per claim kind.
    pub strength: u8,
}

/// Per-world multi-winner snapshot (§5.3 / §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldControlSummary {
    /// Highest local-control-score faction — the map fill (§9.1).
    pub dominant: Option<String>,
    /// Strongest recognized claimant; drives outer-ring border (§9.1).
    pub sovereign: Option<String>,
    /// Strongest military / orbital presence (§9.1 inner ring).
    pub occupier: Option<String>,
    /// Strongest economic presence.
    pub economic_hegemon: Option<String>,
    /// Strongest ideological/popular authority.
    pub popular_authority: Option<String>,
    /// Strongest covert presence — only reported when visibility is low.
    pub hidden_master: Option<String>,
    /// True when top two factions are within 15 points and top ≥ 35 (§14).
    pub contested: bool,
    /// Local control score of the dominant faction (0..=100).
    pub control_score: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemState {
    Pacified,
    Fragmented,
    Blockaded,
    Warzone,
    Infiltrated,
    Quarantined,
    Uncharted,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemControlSummary {
    pub state: Option<SystemState>,
    pub dominant: Option<String>,
    pub sovereign: Option<String>,
    pub orbital_controller: Option<String>,
    pub economic_hegemon: Option<String>,
    pub hidden_master: Option<String>,
    pub top_factions: Vec<ScoredFaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredFaction {
    pub faction_id: String,
    pub score: f32,
}

/// Aggregated faction projection budget (§4.3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct PowerProfile {
    pub administrative: f32,
    pub military: f32,
    pub naval: f32,
    pub economic: f32,
    pub industrial: f32,
    pub ideological: f32,
    pub covert: f32,
    pub logistical: f32,
    pub legitimacy: f32,
}

impl PowerProfile {
    /// Spec §4.3 default projection — weighted single-number total.
    #[must_use]
    pub fn total_projection(&self) -> f32 {
        self.administrative * 0.8
            + self.military * 1.1
            + self.naval * 1.2
            + self.economic * 0.9
            + self.industrial * 0.9
            + self.ideological * 0.7
            + self.covert * 0.7
            + self.logistical * 1.0
            + self.legitimacy * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_distance_known_examples() {
        assert_eq!(
            hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 0, r: 0 }),
            0
        );
        // Same row → straight column delta.
        assert_eq!(
            hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 3, r: 0 }),
            3
        );
        // Straight down 3 rows, same column → 3 hex steps (zigzag absorbs the
        // odd-row half-shift).
        assert_eq!(
            hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 0, r: 3 }),
            3
        );
        // Diagonal: (0,0) → (2,2) in odd-r offset.
        assert_eq!(
            hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 2 }),
            3
        );
    }

    #[test]
    fn neighbors_are_distance_one() {
        for r in 0..4 {
            for q in 0..4 {
                let here = HexCoord { q, r };
                for (dq, dr) in offset_r_neighbors(r) {
                    let there = HexCoord {
                        q: q + dq,
                        r: r + dr,
                    };
                    assert_eq!(hex_distance(here, there), 1, "neighbor of {:?}", here);
                }
            }
        }
    }
}
