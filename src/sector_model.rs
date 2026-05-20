//! Output DTOs for a generated sector. Separate from worlds.rs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{FactionId, RouteId, SystemId, WorldId};

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
    /// Continuous area layers (§9 NEXT, §9.3) — Voronoi-style influence
    /// polygons + soft territory bands. Empty by default.
    #[serde(default)]
    pub influence_field: crate::influence_field::InfluenceField,
    /// Per-faction route-graph projection map (§4 NEXT, §7.2). Empty
    /// when no factions or routes exist.
    #[serde(default)]
    pub power_projection: crate::power_projection::PowerProjectionMap,
    /// §4 NEW.md: inter-faction diplomacy / relationship matrix. Empty when
    /// fewer than two factions exist or relations derivation is skipped.
    #[serde(default)]
    pub relations: crate::relations::RelationsMatrix,
    /// §5 NEW.md: regional warp phenomena overlays. Empty by default.
    #[serde(default)]
    pub regions: Vec<crate::regions::WarpRegion>,
    /// §12 NEW.md: derived per-world / per-system / sector economy snapshot.
    /// Default = no derivation run.
    #[serde(default)]
    pub economy: crate::economy::EconomyReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSystem {
    pub id: SystemId,
    pub index: usize,
    pub name: String,
    pub coord: HexCoord,
    pub star: GeneratedStar,
    pub worlds: Vec<GeneratedWorld>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_factions: Vec<FactionId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Aggregated multi-winner control summary across this system's worlds.
    /// See `faction_sector_control_and_power_design.md` §6.4.
    #[serde(default, skip_serializing_if = "SystemControlSummary::is_default")]
    pub control: SystemControlSummary,
    /// Static stability snapshot averaged across worlds + bumped by
    /// `control.state` (§11.1).
    #[serde(
        default,
        skip_serializing_if = "crate::stability::StabilityState::is_default"
    )]
    pub stability: crate::stability::StabilityState,
    /// Discrete orbital assets (§2 NEXT) — stations, shipyards, defense
    /// platforms, blockade fleets — derived from per-faction dimensions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orbital_assets: Vec<crate::orbital_assets::OrbitalAsset>,
    /// Blockade snapshot when dominant ≠ orbital_controller + blockade
    /// fleet present (§2 NEXT, §6.3).
    #[serde(
        default,
        skip_serializing_if = "crate::orbital_assets::BlockadeReport::is_default"
    )]
    pub blockade: crate::orbital_assets::BlockadeReport,
    /// Per-system conflict state (§5 NEXT, §11). Empty by default.
    #[serde(
        default,
        skip_serializing_if = "crate::conflict::ConflictState::is_default"
    )]
    pub conflict: crate::conflict::ConflictState,
    /// Intel / fog-of-war record for the system, keyed by observer faction
    /// id (§7 NEXT, §12). Empty when full omniscient view is in effect.
    #[serde(default, skip_serializing_if = "crate::intel::SystemIntel::is_empty")]
    pub intel: crate::intel::SystemIntel,
    /// Archetype-specific narrative state (§11 NEXT, §16). Default = no
    /// archetype rules fired for this system.
    #[serde(
        default,
        skip_serializing_if = "crate::archetypes::ArchetypeState::is_default"
    )]
    pub archetype: crate::archetypes::ArchetypeState,
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
    pub id: WorldId,
    pub index: usize,
    pub name: String,
    pub orbit: u8,
    pub source_row_index: usize,
    pub world: WorldDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub factions: Vec<WorldFactionPresence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Per-world political claims (border outlines per design §3.3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<FactionClaim>,
    /// Multi-winner control summary (§5.3).
    #[serde(default, skip_serializing_if = "WorldControlSummary::is_default")]
    pub control: WorldControlSummary,
    /// Static stability snapshot (§11.1). Derived from tags, world type, and
    /// factions present — no sim ticks.
    #[serde(
        default,
        skip_serializing_if = "crate::stability::StabilityState::is_default"
    )]
    pub stability: crate::stability::StabilityState,
    /// Named surface regions (§1 NEXT, §6.1) — capital, hive, underhive, etc.
    /// Empty when the world's type/population doesn't warrant a split.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<crate::surface_region::SurfaceRegion>,
    /// Per-world conflict state (§5 NEXT). Default = pristine.
    #[serde(
        default,
        skip_serializing_if = "crate::conflict::ConflictState::is_default"
    )]
    pub conflict: crate::conflict::ConflictState,
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
    pub id: RouteId,
    pub from_system_id: SystemId,
    pub to_system_id: SystemId,
    pub distance: u32,
    pub route_type: RouteType,
    pub stability: RouteStability,
    pub tags: Vec<String>,
    /// Per-faction control profile along this route (§3). Empty when no
    /// faction has meaningful endpoint presence.
    #[serde(default)]
    pub controls: Vec<crate::route_control::RouteControl>,
}

impl GeneratedRoute {
    /// Deterministic visual rhythm for this route. `salt` should be a sector
    /// seed/id so same local route ids in different sectors do not all draw
    /// with the same pattern.
    #[must_use]
    pub fn pattern_with_salt(&self, salt: &str) -> RoutePattern {
        let key = format!(
            "{}\0{}\0{}\0{}\0{}\0{}",
            salt,
            self.id,
            self.from_system_id,
            self.to_system_id,
            self.distance,
            self.stability.pattern_key()
        );
        self.route_type.pattern_for_key(&key)
    }

    #[must_use]
    pub fn pattern(&self) -> RoutePattern {
        self.pattern_with_salt("")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteType {
    StableWarpLane,
    ChartedPassage,
    DangerousPassage,
    SecretPassage,
    /// Aeldari webway thread — invisible to non-Aeldari observers (§3 NEXT,
    /// §16.10).
    Webway,
    /// Inquisition black-ship convoy lane — Inquisition-only transit, opaque
    /// to outside intel (§3 NEXT, §16.1 governance stack).
    BlackShip,
    /// Hidden criminal / Drukhari raid lane — high secrecy + piracy
    /// (§3 NEXT, §16.10).
    SmugglingLane,
}

impl RouteType {
    /// Canonical legend/default pattern for this route type.
    #[must_use]
    pub fn pattern(self) -> RoutePattern {
        self.patterns()[0]
    }

    /// Full pattern family for this route type. Families are disjoint and cover
    /// every `RoutePattern`, so generated routes spread across the new styles
    /// without making two route types share the same default glyph.
    #[must_use]
    pub fn patterns(self) -> &'static [RoutePattern] {
        match self {
            RouteType::StableWarpLane => &[
                RoutePattern::Solid,
                RoutePattern::Railroad,
                RoutePattern::March,
            ],
            RouteType::ChartedPassage => &[
                RoutePattern::Dashed,
                RoutePattern::Bridge,
                RoutePattern::Twin,
            ],
            RouteType::DangerousPassage => &[
                RoutePattern::DotDash,
                RoutePattern::Cracked,
                RoutePattern::Staccato,
            ],
            RouteType::SecretPassage => &[
                RoutePattern::Dotted,
                RoutePattern::Tick,
                RoutePattern::Whisper,
            ],
            RouteType::Webway => &[
                RoutePattern::Burst,
                RoutePattern::Tripod,
                RoutePattern::Patter,
            ],
            RouteType::BlackShip => &[RoutePattern::Quartet, RoutePattern::DoubleTap],
            RouteType::SmugglingLane => &[
                RoutePattern::Gravel,
                RoutePattern::Pebble,
                RoutePattern::Ghost,
            ],
        }
    }

    #[must_use]
    pub fn pattern_for_key(self, key: &str) -> RoutePattern {
        let pool = self.patterns();
        pool[(stable_pattern_hash(self, key) as usize) % pool.len()]
    }

    /// True for the three hidden classes introduced by §3 (NEXT.md). Hidden
    /// routes are only visible to the faction that owns them and their
    /// directly-allied factions; the PNG legend renders them in a separate
    /// HIDDEN ROUTES block.
    #[must_use]
    pub fn is_hidden(self) -> bool {
        matches!(
            self,
            RouteType::Webway | RouteType::BlackShip | RouteType::SmugglingLane
        )
    }
}

fn stable_pattern_hash(route_type: RouteType, key: &str) -> u32 {
    fn feed(hash: &mut u32, bytes: &[u8]) {
        for b in bytes {
            *hash ^= u32::from(*b);
            *hash = hash.wrapping_mul(16_777_619);
        }
    }

    let mut hash = 2_166_136_261_u32;
    feed(&mut hash, b"sectorforge:route-pattern:v1");
    feed(&mut hash, &[0]);
    feed(&mut hash, route_type.pattern_key().as_bytes());
    feed(&mut hash, &[0]);
    feed(&mut hash, key.as_bytes());
    hash
}

impl RouteType {
    fn pattern_key(self) -> &'static str {
        match self {
            RouteType::StableWarpLane => "stable_warp_lane",
            RouteType::ChartedPassage => "charted_passage",
            RouteType::DangerousPassage => "dangerous_passage",
            RouteType::SecretPassage => "secret_passage",
            RouteType::Webway => "webway",
            RouteType::BlackShip => "black_ship",
            RouteType::SmugglingLane => "smuggling_lane",
        }
    }
}

/// Visual line pattern used to encode route type plus per-route variety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePattern {
    Solid,
    Dashed,
    DotDash,
    Dotted,
    /// Short bars on short gaps — busy texture, reads as "active"
    Cracked,
    /// Long dash / very long gap — barely-there, ghostly
    Ghost,
    /// Triple-burst pattern: three quick dashes then rest
    Burst,
    /// Zigzag rhythm: alternating long/short bars
    Staccato,
    /// Fine gravel: short dash / tight gap, denser than Cracked
    Gravel,
    /// Two thick bars separated by medium gap
    Twin,
    /// Long dash / tiny gap / tiny dot / tiny gap — "triple" motif
    Tripod,
    /// Short dash / long gap — sparse tick marks
    Tick,
    /// Medium dash / short gap / medium dash / short gap (symmetric)
    Bridge,
    /// Rapid dots: very short dash, tight gap
    Patter,
    /// Long bar / short gap / long bar / very long gap (4-part cycle)
    Quartet,
    /// Extra-long bars: heavy-duty look
    Railroad,
    /// Two short bars with medium gap — "double-tap" rhythm
    DoubleTap,
    /// Tiny dots very close together — near-solid at small scale
    Pebble,
    /// Very short dash / very long gap — barely-punctuated
    Whisper,
    /// Five equal dashes evenly spaced — ceremonial, regular
    March,
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
            // --- 16 new strides ---
            RoutePattern::Cracked => &[3.0, 2.0],
            RoutePattern::Ghost => &[12.0, 15.0],
            RoutePattern::Burst => &[1.5, 2.0, 1.5, 2.0, 1.5, 8.0],
            RoutePattern::Staccato => &[6.0, 3.0, 2.0, 3.0],
            RoutePattern::Gravel => &[2.0, 1.5],
            RoutePattern::Twin => &[4.0, 2.0, 4.0, 5.0],
            RoutePattern::Tripod => &[6.0, 1.0, 1.0, 1.0, 1.0, 1.0, 6.0],
            RoutePattern::Tick => &[2.0, 8.0],
            RoutePattern::Bridge => &[4.0, 2.0, 4.0, 2.0],
            RoutePattern::Patter => &[0.8, 1.2],
            RoutePattern::Quartet => &[5.0, 3.0, 3.0, 7.0],
            RoutePattern::Railroad => &[14.0, 6.0],
            RoutePattern::DoubleTap => &[2.5, 2.0, 2.5, 6.0],
            RoutePattern::Pebble => &[1.0, 1.0],
            RoutePattern::Whisper => &[1.0, 14.0],
            RoutePattern::March => &[3.0, 3.0, 3.0, 3.0, 3.0, 3.0],
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

impl RouteStability {
    fn pattern_key(self) -> &'static str {
        match self {
            RouteStability::Stable => "stable",
            RouteStability::Unstable => "unstable",
            RouteStability::Hazardous => "hazardous",
            RouteStability::Perilous => "perilous",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFaction {
    pub id: FactionId,
    pub name: String,
    pub kind: String,
    pub disposition: String,
    /// Specific catalogue entries grouped under this overall faction kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subfactions: Vec<GeneratedSubfaction>,
    pub system_presence: Vec<SystemId>,
    pub world_presence: Vec<WorldId>,
    /// Aggregated multi-dimensional power across all controlled assets (§4.3).
    #[serde(default)]
    pub power: PowerProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSubfaction {
    pub id: FactionId,
    pub name: String,
    pub disposition: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub system_presence: Vec<SystemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_presence: Vec<WorldId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldFactionPresence {
    /// Overall faction id. Generated from `FactionDef.kind` for new sectors.
    pub faction_id: FactionId,
    /// Specific catalogue entry selected under the overall faction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subfaction_id: Option<FactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subfaction_name: Option<String>,
    pub influence: FactionInfluence,
    pub relationship_to_government: String,
    /// Multi-dimensional presence scores (§4.5). All fields in 0..=100.
    #[serde(default, skip_serializing_if = "PresenceDimensions::is_default")]
    pub dimensions: PresenceDimensions,
    /// Computed dominance bucket from the weighted control score (§5.2).
    #[serde(default, skip_serializing_if = "DominanceState::is_default")]
    pub dominance: DominanceState,
    /// Intel-layer confidence 0..=100 (§12).
    #[serde(
        default = "default_intel_confidence",
        skip_serializing_if = "is_default_intel_confidence"
    )]
    pub intel_confidence: u8,
}

fn default_intel_confidence() -> u8 {
    100
}

fn is_default_intel_confidence(v: &u8) -> bool {
    *v == 100
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
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
    #[must_use]
    pub fn is_default(&self) -> bool {
        matches!(self, DominanceState::Rumored)
    }
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
    pub faction_id: FactionId,
    pub claim_type: ClaimType,
    /// 0..=100; populated from local control or fixed minima per claim kind.
    pub strength: u8,
}

impl SystemControlSummary {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl WorldControlSummary {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Per-world multi-winner snapshot (§5.3 / §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorldControlSummary {
    /// Highest local-control-score faction — the map fill (§9.1).
    pub dominant: Option<FactionId>,
    /// Strongest recognized claimant; drives outer-ring border (§9.1).
    pub sovereign: Option<FactionId>,
    /// Strongest military / orbital presence (§9.1 inner ring).
    pub occupier: Option<FactionId>,
    /// Strongest economic presence.
    pub economic_hegemon: Option<FactionId>,
    /// Strongest ideological/popular authority.
    pub popular_authority: Option<FactionId>,
    /// Strongest covert presence — only reported when visibility is low.
    pub hidden_master: Option<FactionId>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SystemControlSummary {
    pub state: Option<SystemState>,
    pub dominant: Option<FactionId>,
    pub sovereign: Option<FactionId>,
    pub orbital_controller: Option<FactionId>,
    pub economic_hegemon: Option<FactionId>,
    pub hidden_master: Option<FactionId>,
    pub top_factions: Vec<ScoredFaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredFaction {
    pub faction_id: FactionId,
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

    #[test]
    fn route_type_default_patterns_are_unique() {
        let route_types = [
            RouteType::StableWarpLane,
            RouteType::ChartedPassage,
            RouteType::DangerousPassage,
            RouteType::SecretPassage,
            RouteType::Webway,
            RouteType::BlackShip,
            RouteType::SmugglingLane,
        ];
        let mut defaults = Vec::new();
        for route_type in route_types {
            let pattern = route_type.pattern();
            assert!(
                !defaults.contains(&pattern),
                "{route_type:?} duplicated default pattern {pattern:?}"
            );
            defaults.push(pattern);
        }
    }

    #[test]
    fn route_pattern_pools_cover_all_patterns_once() {
        let route_types = [
            RouteType::StableWarpLane,
            RouteType::ChartedPassage,
            RouteType::DangerousPassage,
            RouteType::SecretPassage,
            RouteType::Webway,
            RouteType::BlackShip,
            RouteType::SmugglingLane,
        ];
        let all_patterns = [
            RoutePattern::Solid,
            RoutePattern::Dashed,
            RoutePattern::DotDash,
            RoutePattern::Dotted,
            RoutePattern::Cracked,
            RoutePattern::Ghost,
            RoutePattern::Burst,
            RoutePattern::Staccato,
            RoutePattern::Gravel,
            RoutePattern::Twin,
            RoutePattern::Tripod,
            RoutePattern::Tick,
            RoutePattern::Bridge,
            RoutePattern::Patter,
            RoutePattern::Quartet,
            RoutePattern::Railroad,
            RoutePattern::DoubleTap,
            RoutePattern::Pebble,
            RoutePattern::Whisper,
            RoutePattern::March,
        ];
        let mut seen = Vec::new();
        for route_type in route_types {
            for pattern in route_type.patterns() {
                assert!(
                    !seen.contains(pattern),
                    "{pattern:?} appears in more than one route pattern pool"
                );
                seen.push(*pattern);
            }
        }
        assert_eq!(seen.len(), all_patterns.len());
        for pattern in all_patterns {
            assert!(seen.contains(&pattern), "{pattern:?} missing from pools");
        }
    }

    #[test]
    fn generated_route_pattern_comes_from_its_type_pool() {
        let route = GeneratedRoute {
            id: "route-0001-0002".into(),
            from_system_id: "sys-0001".into(),
            to_system_id: "sys-0002".into(),
            distance: 3,
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Stable,
            tags: Vec::new(),
            controls: Vec::new(),
        };
        let pattern = route.pattern_with_salt("sector-a");
        assert!(route.route_type.patterns().contains(&pattern));
    }
}
