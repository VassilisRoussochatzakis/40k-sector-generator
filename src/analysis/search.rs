//! Constraint-directed seed search (§2 NEW.md).
//!
//! Lets a user declare what they want the generated sector to look like —
//! faction balance bands, presence of a particular world type under a
//! particular dominant faction, route-graph properties, system-state counts
//! — and finds a seed that satisfies the constraints by deterministic
//! enumeration over seeds derived from a base.
//!
//! The search is itself reproducible: same `base_seed` + same constraint
//! file + same project ⇒ same winning seed (or same "best-effort" near miss
//! when the budget is exhausted). The generator is not modified; the search
//! is a pure outer loop around `generate_sector`.
//!
//! Candidate seeds are derived from
//! `blake3("sectorforge:{base_seed}:search:{n}")`, mirroring the
//! per-stage RNG scheme. n=0 is always the base seed itself, so passing a
//! known-good seed as the base trivially finds it without enumeration.
//!
//! Constraints evaluate against the finished sector + its analytics report,
//! so they reuse the same fields the analytics dashboard exposes (§8).

use std::collections::HashMap;
use std::fmt::Write as _;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::analytics::{self, SectorAnalysis};
use crate::errors::SectorError;
use crate::input::ProjectInput;
use crate::sector_model::{GeneratedSector, SystemState};

// ── Public DTOs ────────────────────────────────────────────────────────────────

/// On-disk constraint file (`wishes.toml`) bundling a base seed, a search
/// budget, and the list of constraints to evaluate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WishesFile {
    /// Optional `[search]` block. When omitted, defaults are used and the
    /// base seed comes from the project's `sectorforge.toml`.
    #[serde(default)]
    pub search: SearchConfig,
    /// Constraints evaluated for every candidate. Empty ⇒ the first
    /// candidate wins.
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchConfig {
    /// Base seed for the search. Candidate seeds are derived from this. When
    /// `None`, the project's configured seed is used.
    #[serde(default)]
    pub base_seed: Option<String>,
    /// Maximum number of candidates evaluated, including n=0 (the base
    /// itself). Default: 256.
    #[serde(default = "default_budget")]
    pub budget: u32,
    /// How many near-miss candidates to keep in the report when no seed
    /// satisfies the constraints. Default: 5.
    #[serde(default = "default_report_top")]
    pub report_top: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            base_seed: None,
            budget: default_budget(),
            report_top: default_report_top(),
        }
    }
}

fn default_budget() -> u32 {
    256
}
fn default_report_top() -> u32 {
    5
}

/// One declarative constraint. The `kind` field selects the variant and the
/// remaining fields parameterise it. Unknown fields are ignored so future
/// variants don't break older configs.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Constraint {
    /// `share` is a fraction of total faction projection power (0.0..=1.0).
    FactionShareMin {
        faction_id: String,
        min: f32,
    },
    FactionShareMax {
        faction_id: String,
        max: f32,
    },
    FactionWorldCountMin {
        faction_id: String,
        min: u32,
    },
    FactionWorldCountMax {
        faction_id: String,
        max: u32,
    },
    FactionSystemCountMin {
        faction_id: String,
        min: u32,
    },
    FactionSystemCountMax {
        faction_id: String,
        max: u32,
    },
    /// At least `min_count` worlds whose `world_type` matches and (optionally)
    /// whose dominant faction is `dominant_faction_id`.
    WorldTypeExists {
        world_type: String,
        #[serde(default)]
        dominant_faction_id: Option<String>,
        #[serde(default = "default_one")]
        min_count: u32,
    },
    /// At least / at most `count` contested worlds.
    ContestedWorldMin {
        min: u32,
        #[serde(default)]
        n_way: Option<u32>,
    },
    ContestedWorldMax {
        max: u32,
    },
    /// At least `min` systems whose `SystemState` matches.
    SystemStateCountMin {
        state: SystemStateName,
        min: u32,
    },
    SystemStateCountMax {
        state: SystemStateName,
        max: u32,
    },
    /// §15 NEW2.md: At least `min` worlds where faction has a specific presence.
    FactionPresenceCountMin {
        faction_id: String,
        min: u32,
        presence: PresenceName,
    },
    /// §15 NEW2.md: At least `count` systems matching a state within `max_hops` of `within_hops_of` system tag.
    MinimumSystemsMatching {
        count: u32,
        within_hops_of: String,
        max_hops: u32,
        #[serde(rename = "where")]
        where_cond: SystemStateFilter,
    },
    /// Route graph has exactly one connected component.
    RouteGraphConnected,
    /// Route graph has zero articulation points.
    NoArticulationPoints,
    /// Route-graph diameter, in hops, no greater than `max_hops`.
    DiameterMax {
        max_hops: u32,
    },
    /// At most `max` systems with no incident routes.
    IsolatedSystemsMax {
        max: u32,
    },
    /// Sector-wide contested ratio (contested worlds / inhabited worlds) ≤ max.
    ContestedRatioMax {
        max: f32,
    },
    /// Sector-wide contested ratio ≥ min.
    ContestedRatioMin {
        min: f32,
    },
    /// §4 NEW.md: at least `min` faction pairs are at the given stance.
    StanceCountMin {
        stance: StanceName,
        min: u32,
    },
    /// §4 NEW.md: at most `max` faction pairs are at the given stance.
    StanceCountMax {
        stance: StanceName,
        max: u32,
    },
    /// §5 NEW.md: at least `min` regions of the named kind exist.
    RegionCountMin {
        region_kind: RegionKindName,
        min: u32,
    },
    /// §5 NEW.md: at most `max` regions of the named kind exist.
    RegionCountMax {
        region_kind: RegionKindName,
        max: u32,
    },
    /// §12 NEW.md: at most `max` stranded worlds.
    EconomyStrandedMax {
        max: u32,
    },
    /// §12 NEW.md: at least `min` net sector balance for the named resource.
    EconomyResourceMin {
        resource: String,
        min: f32,
    },
}

/// §4 NEW.md mirror of [`crate::relations::Stance`].
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StanceName {
    Allied,
    Aligned,
    Neutral,
    Rival,
    Hostile,
    AtWar,
}

enum_slug!(StanceName {
    Allied => "allied",
    Aligned => "aligned",
    Neutral => "neutral",
    Rival => "rival",
    Hostile => "hostile",
    AtWar => "at_war",
});

impl core::fmt::Display for StanceName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl StanceName {
    /// Slug-for-slug mirror of [`crate::relations::Stance`], so equal slugs
    /// mean the same stance.
    fn matches(self, s: crate::relations::Stance) -> bool {
        self.as_slug() == s.as_slug()
    }
}

/// §5 NEW.md mirror of [`crate::regions::RegionConditionKind`].
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RegionKindName {
    WarpStorm,
    Turbulence,
    CalmCorridor,
    Blackout,
    Anomaly,
}

enum_slug!(RegionKindName {
    WarpStorm => "warp_storm",
    Turbulence => "turbulence",
    CalmCorridor => "calm_corridor",
    Blackout => "blackout",
    Anomaly => "anomaly",
});

impl core::fmt::Display for RegionKindName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl RegionKindName {
    /// Slug-for-slug mirror of [`crate::regions::RegionConditionKind`], so
    /// equal slugs mean the same kind (kinds this mirror lacks never match).
    fn matches(self, k: crate::regions::RegionConditionKind) -> bool {
        self.as_slug() == k.as_slug()
    }
}

fn default_one() -> u32 {
    1
}

/// Serializable mirror of [`SystemState`] for use inside constraint files.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SystemStateName {
    Pacified,
    Fragmented,
    Blockaded,
    Warzone,
    Infiltrated,
    Quarantined,
    Uncharted,
}

enum_slug!(SystemStateName {
    Pacified => "pacified",
    Fragmented => "fragmented",
    Blockaded => "blockaded",
    Warzone => "warzone",
    Infiltrated => "infiltrated",
    Quarantined => "quarantined",
    Uncharted => "uncharted",
});

impl core::fmt::Display for SystemStateName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SystemStateFilter {
    pub system_state: SystemStateName,
}

impl SystemStateFilter {
    pub fn matches(&self, s: Option<SystemState>) -> bool {
        s == Some(SystemState::from(self.system_state))
    }
    pub fn debug_name(&self) -> String {
        format!("state={:?}", self.system_state)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
#[non_exhaustive]
pub enum PresenceName {
    Hidden,
    Minor,
    Significant,
    Dominant,
}

enum_slug!(PresenceName {
    Hidden => "Hidden",
    Minor => "Minor",
    Significant => "Significant",
    Dominant => "Dominant",
});

impl core::fmt::Display for PresenceName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl PresenceName {
    fn matches(self, inf: crate::sector_model::FactionInfluence) -> bool {
        use crate::sector_model::FactionInfluence as I;
        matches!(
            (self, inf),
            (PresenceName::Hidden, I::Hidden)
                | (PresenceName::Minor, I::Minor)
                | (PresenceName::Significant, I::Significant)
                | (PresenceName::Dominant, I::Dominant)
        )
    }
}

impl From<SystemStateName> for SystemState {
    fn from(s: SystemStateName) -> Self {
        match s {
            SystemStateName::Pacified => SystemState::Pacified,
            SystemStateName::Fragmented => SystemState::Fragmented,
            SystemStateName::Blockaded => SystemState::Blockaded,
            SystemStateName::Warzone => SystemState::Warzone,
            SystemStateName::Infiltrated => SystemState::Infiltrated,
            SystemStateName::Quarantined => SystemState::Quarantined,
            SystemStateName::Uncharted => SystemState::Uncharted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintReport {
    pub label: String,
    pub passed: bool,
    pub observed: String,
    pub required: String,
    /// 0.0 means "exactly satisfied". Positive numbers are the *miss
    /// distance* used for ranking — higher = worse violation.
    pub miss: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CandidateReport {
    pub n: u32,
    pub seed: String,
    pub passed: bool,
    pub total_miss: f32,
    pub constraints: Vec<ConstraintReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOutcome {
    pub base_seed: String,
    pub budget: u32,
    pub candidates_evaluated: u32,
    pub winning: Option<CandidateReport>,
    /// Sorted by ascending total_miss. Always non-empty when at least one
    /// candidate was evaluated.
    pub near_misses: Vec<CandidateReport>,
    /// Pre-flight validation errors against the project (e.g. unknown
    /// faction id). Search aborts early when this is non-empty.
    pub preflight_errors: Vec<String>,
}

/// §SR2: live progress snapshot for a running search. Reported after every
/// candidate the search actually generates + evaluates. Skipped candidates
/// (short-circuited once a lower-`n` winner exists) do not bump `tried`.
#[derive(Debug, Clone, Copy)]
pub struct SearchProgress {
    /// Candidates generated + evaluated so far.
    pub tried: u32,
    /// Candidates that satisfied every constraint so far.
    pub passed: u32,
    /// Lowest `total_miss` observed so far. `None` until the first candidate
    /// is evaluated.
    pub best_miss: Option<f32>,
    /// Total candidate budget (the denominator for a progress bar).
    pub budget: u32,
}

// ── Loading ────────────────────────────────────────────────────────────────────

/// Parse a wishes file from disk.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be read and
/// [`SectorError::ConfigParse`] if the TOML is malformed.
pub fn load_wishes(path: impl AsRef<Utf8Path>) -> Result<WishesFile, SectorError> {
    let p = path.as_ref();
    let text = std::fs::read_to_string(p).map_err(|e| SectorError::io(p.as_str(), e))?;
    toml::from_str(&text).map_err(|e| SectorError::config_parse(p.as_str(), e.to_string()))
}

// ── Seed derivation ────────────────────────────────────────────────────────────

/// Deterministic candidate seed derived from a base seed and ordinal.
/// `n == 0` returns the base seed unchanged (so a known-good seed wins the
/// search immediately without enumeration).
#[must_use]
pub fn derive_candidate_seed(base_seed: &str, n: u32) -> String {
    if n == 0 {
        return base_seed.to_string();
    }
    let bytes = crate::rng::derive_stage_seed(base_seed, "search", &n.to_string());
    // 16 hex chars = 64-bit identifier — short enough to type, large enough
    // to make collisions over a 256-candidate search vanishingly unlikely.
    crate::rng::hex(&bytes[..8])
}

// ── Pre-flight ─────────────────────────────────────────────────────────────────

fn preflight(project: &ProjectInput, constraints: &[Constraint]) -> Vec<String> {
    let mut errors = Vec::new();
    let known_factions: std::collections::BTreeSet<&str> = project
        .catalogs
        .factions
        .iter()
        .flat_map(|f| [f.id.as_str(), f.kind.as_str()])
        .collect();
    for c in constraints {
        if let Some(id) = referenced_faction(c) {
            if !known_factions.contains(id.as_str()) {
                errors.push(format!(
                    "constraint references unknown faction id `{id}`; known: [{}]",
                    known_factions
                        .iter()
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    errors
}

fn referenced_faction(c: &Constraint) -> Option<String> {
    match c {
        Constraint::FactionShareMin { faction_id, .. }
        | Constraint::FactionShareMax { faction_id, .. }
        | Constraint::FactionWorldCountMin { faction_id, .. }
        | Constraint::FactionWorldCountMax { faction_id, .. }
        | Constraint::FactionSystemCountMin { faction_id, .. }
        | Constraint::FactionSystemCountMax { faction_id, .. }
        | Constraint::FactionPresenceCountMin { faction_id, .. } => Some(faction_id.clone()),
        Constraint::WorldTypeExists {
            dominant_faction_id: Some(id),
            ..
        } => Some(id.clone()),
        _ => None,
    }
}

// ── Constraint evaluation ──────────────────────────────────────────────────────

type FactionIndex<'a> = HashMap<&'a str, &'a analytics::FactionShare>;

fn build_faction_index(analysis: &SectorAnalysis) -> FactionIndex<'_> {
    analysis
        .faction_balance
        .top_factions
        .iter()
        .map(|f| (f.faction_id.as_str(), f))
        .collect()
}

fn share_of(idx: &FactionIndex<'_>, faction_id: &str) -> f32 {
    idx.get(faction_id).map_or(0.0, |f| f.share)
}

fn world_count_of(idx: &FactionIndex<'_>, faction_id: &str) -> u32 {
    idx.get(faction_id).map_or(0, |f| f.world_presence_count)
}

fn system_count_of(idx: &FactionIndex<'_>, faction_id: &str) -> u32 {
    idx.get(faction_id).map_or(0, |f| f.system_presence_count)
}

fn count_world_type_dominant(
    sector: &GeneratedSector,
    world_type: &str,
    dominant_faction_id: Option<&str>,
) -> u32 {
    let mut n = 0u32;
    for sys in &sector.systems {
        for w in sys.worlds.iter() {
            if w.world.world_type.to_string().as_str() != world_type {
                continue;
            }
            if let Some(fid) = dominant_faction_id {
                if w.control.dominant.as_deref() != Some(fid) {
                    continue;
                }
            }
            n += 1;
        }
    }
    n
}

fn count_contested_worlds(sector: &GeneratedSector, n_way: Option<u32>) -> u32 {
    let mut n = 0u32;
    for sys in &sector.systems {
        for w in sys.worlds.iter() {
            if !w.control.contested {
                continue;
            }
            if let Some(k) = n_way {
                if (w.claims.len() as u32) < k {
                    continue;
                }
            }
            n += 1;
        }
    }
    n
}

fn count_system_state(sector: &GeneratedSector, target: SystemState) -> u32 {
    sector
        .systems
        .iter()
        .filter(|s| s.control.state == Some(target))
        .count() as u32
}

fn count_faction_presence(
    sector: &GeneratedSector,
    faction_id: &str,
    presence: PresenceName,
) -> u32 {
    let mut n = 0u32;
    for sys in &sector.systems {
        for w in &sys.worlds {
            for p in &w.factions {
                if p.faction_id.as_str() == faction_id && presence.matches(p.influence) {
                    n += 1;
                    break;
                }
            }
        }
    }
    n
}

fn count_systems_matching_distance(
    sector: &GeneratedSector,
    within_hops_of: &str,
    max_hops: u32,
    where_cond: &SystemStateFilter,
) -> u32 {
    let targets: Vec<crate::ids::SystemId> = sector
        .systems
        .iter()
        .filter(|s| {
            s.tags.iter().any(|t| t.as_ref() == within_hops_of) || s.id.as_str() == within_hops_of
        })
        .map(|s| s.id.clone())
        .collect();

    if targets.is_empty() {
        return 0;
    }

    // Build the undirected adjacency map once (O(R)), so the BFS below visits
    // each system's neighbours via a lookup instead of rescanning every route
    // on each dequeue. A `BTreeMap` keeps key/value order deterministic; the
    // resulting `reachable` set (and thus the count) is the union of the
    // `max_hops`-balls around the targets and is order-independent regardless.
    let mut adjacency: std::collections::BTreeMap<crate::ids::SystemId, Vec<crate::ids::SystemId>> =
        std::collections::BTreeMap::new();
    for r in &sector.routes {
        adjacency
            .entry(r.from_system_id.clone())
            .or_default()
            .push(r.to_system_id.clone());
        adjacency
            .entry(r.to_system_id.clone())
            .or_default()
            .push(r.from_system_id.clone());
    }

    let mut reachable = std::collections::BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    for t in targets {
        queue.push_back((t.clone(), 0));
        reachable.insert(t);
    }

    while let Some((curr, dist)) = queue.pop_front() {
        if dist >= max_hops {
            continue;
        }
        if let Some(neighbours) = adjacency.get(&curr) {
            for id in neighbours {
                if !reachable.contains(id) {
                    reachable.insert(id.clone());
                    queue.push_back((id.clone(), dist + 1));
                }
            }
        }
    }

    let mut n = 0u32;
    for id in reachable {
        if let Some(sys) = sector.get_system(&id) {
            if where_cond.matches(sys.control.state) {
                n += 1;
            }
        }
    }
    n
}

/// Evaluate a single constraint. Returns a per-constraint report including a
/// non-negative `miss` distance used for ranking near-misses.
fn evaluate(
    c: &Constraint,
    sector: &GeneratedSector,
    analysis: &SectorAnalysis,
    fac_idx: &FactionIndex<'_>,
) -> ConstraintReport {
    match c {
        Constraint::FactionShareMin { faction_id, min } => {
            let s = share_of(fac_idx, faction_id);
            let passed = s >= *min;
            let miss = (*min - s).max(0.0);
            ConstraintReport {
                label: format!("faction_share_min({faction_id})"),
                passed,
                observed: format!("{:.3}", s),
                required: format!(">= {:.3}", min),
                miss,
            }
        }
        Constraint::FactionShareMax { faction_id, max } => {
            let s = share_of(fac_idx, faction_id);
            let passed = s <= *max;
            let miss = (s - *max).max(0.0);
            ConstraintReport {
                label: format!("faction_share_max({faction_id})"),
                passed,
                observed: format!("{:.3}", s),
                required: format!("<= {:.3}", max),
                miss,
            }
        }
        Constraint::FactionWorldCountMin { faction_id, min } => {
            let n = world_count_of(fac_idx, faction_id);
            let passed = n >= *min;
            let miss = if passed {
                0.0
            } else {
                (*min as f32) - n as f32
            };
            ConstraintReport {
                label: format!("faction_world_count_min({faction_id})"),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::FactionWorldCountMax { faction_id, max } => {
            let n = world_count_of(fac_idx, faction_id);
            let passed = n <= *max;
            let miss = if passed { 0.0 } else { n as f32 - *max as f32 };
            ConstraintReport {
                label: format!("faction_world_count_max({faction_id})"),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::FactionSystemCountMin { faction_id, min } => {
            let n = system_count_of(fac_idx, faction_id);
            let passed = n >= *min;
            let miss = if passed {
                0.0
            } else {
                (*min as f32) - n as f32
            };
            ConstraintReport {
                label: format!("faction_system_count_min({faction_id})"),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::FactionSystemCountMax { faction_id, max } => {
            let n = system_count_of(fac_idx, faction_id);
            let passed = n <= *max;
            let miss = if passed { 0.0 } else { n as f32 - *max as f32 };
            ConstraintReport {
                label: format!("faction_system_count_max({faction_id})"),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::WorldTypeExists {
            world_type,
            dominant_faction_id,
            min_count,
        } => {
            let n = count_world_type_dominant(sector, world_type, dominant_faction_id.as_deref());
            let passed = n >= *min_count;
            let miss = if passed {
                0.0
            } else {
                (*min_count as f32) - n as f32
            };
            let label = match dominant_faction_id {
                Some(fid) => format!("world_type_exists({world_type} dominant={fid})"),
                None => format!("world_type_exists({world_type})"),
            };
            ConstraintReport {
                label,
                passed,
                observed: n.to_string(),
                required: format!(">= {min_count}"),
                miss,
            }
        }
        Constraint::ContestedWorldMin { min, n_way } => {
            let n = count_contested_worlds(sector, *n_way);
            let passed = n >= *min;
            let miss = if passed {
                0.0
            } else {
                (*min as f32) - n as f32
            };
            let label = match n_way {
                Some(k) => format!("contested_world_min(n_way={k})"),
                None => "contested_world_min".to_string(),
            };
            ConstraintReport {
                label,
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::ContestedWorldMax { max } => {
            let n = count_contested_worlds(sector, None);
            let passed = n <= *max;
            let miss = if passed { 0.0 } else { n as f32 - *max as f32 };
            ConstraintReport {
                label: "contested_world_max".to_string(),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::SystemStateCountMin { state, min } => {
            let n = count_system_state(sector, SystemState::from(*state));
            let passed = n >= *min;
            let miss = if passed {
                0.0
            } else {
                (*min as f32) - n as f32
            };
            ConstraintReport {
                label: format!("system_state_count_min({state:?})"),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::SystemStateCountMax { state, max } => {
            let n = count_system_state(sector, SystemState::from(*state));
            let passed = n <= *max;
            let miss = if passed { 0.0 } else { n as f32 - *max as f32 };
            ConstraintReport {
                label: format!("system_state_count_max({state:?})"),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::FactionPresenceCountMin {
            faction_id,
            min,
            presence,
        } => {
            let n = count_faction_presence(sector, faction_id, *presence);
            let passed = n >= *min;
            let miss = if passed {
                0.0
            } else {
                (*min as f32) - n as f32
            };
            ConstraintReport {
                label: format!(
                    "faction_presence_count_min({faction_id} presence={:?})",
                    presence
                ),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::MinimumSystemsMatching {
            count,
            within_hops_of,
            max_hops,
            where_cond,
        } => {
            let n = count_systems_matching_distance(sector, within_hops_of, *max_hops, where_cond);
            let passed = n >= *count;
            let miss = if passed {
                0.0
            } else {
                (*count as f32) - n as f32
            };
            ConstraintReport {
                label: format!(
                    "minimum_systems_matching({} within {max_hops} hops of {within_hops_of})",
                    where_cond.debug_name()
                ),
                passed,
                observed: n.to_string(),
                required: format!(">= {count}"),
                miss,
            }
        }
        Constraint::RouteGraphConnected => {
            let n = analysis.connectivity.component_count;
            let passed = n == 1;
            // Miss = extra components.
            let miss = if passed {
                0.0
            } else {
                n.saturating_sub(1) as f32
            };
            ConstraintReport {
                label: "route_graph_connected".to_string(),
                passed,
                observed: format!("components={n}"),
                required: "components == 1".to_string(),
                miss,
            }
        }
        Constraint::NoArticulationPoints => {
            let n = analysis.connectivity.articulation_point_ids.len() as u32;
            let passed = n == 0;
            let miss = n as f32;
            ConstraintReport {
                label: "no_articulation_points".to_string(),
                passed,
                observed: format!("articulation_points={n}"),
                required: "== 0".to_string(),
                miss,
            }
        }
        Constraint::DiameterMax { max_hops } => {
            let observed = analysis.connectivity.diameter_hops;
            let (passed, miss) = match observed {
                Some(d) if d <= *max_hops => (true, 0.0),
                Some(d) => (false, (d - *max_hops) as f32),
                // Disconnected → infinite diameter; treat as a hard miss.
                None => (false, f32::from(u16::MAX)),
            };
            ConstraintReport {
                label: "diameter_max".to_string(),
                passed,
                observed: match observed {
                    Some(d) => d.to_string(),
                    None => "disconnected".to_string(),
                },
                required: format!("<= {max_hops}"),
                miss,
            }
        }
        Constraint::IsolatedSystemsMax { max } => {
            let n = analysis.connectivity.isolated_system_ids.len() as u32;
            let passed = n <= *max;
            let miss = if passed { 0.0 } else { (n - *max) as f32 };
            ConstraintReport {
                label: "isolated_systems_max".to_string(),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::ContestedRatioMax { max } => {
            let r = analysis.contested_world_ratio;
            let passed = r <= *max;
            let miss = (r - *max).max(0.0);
            ConstraintReport {
                label: "contested_ratio_max".to_string(),
                passed,
                observed: format!("{:.3}", r),
                required: format!("<= {:.3}", max),
                miss,
            }
        }
        Constraint::ContestedRatioMin { min } => {
            let r = analysis.contested_world_ratio;
            let passed = r >= *min;
            let miss = (*min - r).max(0.0);
            ConstraintReport {
                label: "contested_ratio_min".to_string(),
                passed,
                observed: format!("{:.3}", r),
                required: format!(">= {:.3}", min),
                miss,
            }
        }
        Constraint::StanceCountMin { stance, min } => {
            let n = sector
                .relations
                .pairs
                .iter()
                .filter(|p| stance.matches(p.stance))
                .count() as u32;
            let passed = n >= *min;
            let miss = (*min as f32 - n as f32).max(0.0);
            ConstraintReport {
                label: format!("stance_count_min:{stance:?}"),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::StanceCountMax { stance, max } => {
            let n = sector
                .relations
                .pairs
                .iter()
                .filter(|p| stance.matches(p.stance))
                .count() as u32;
            let passed = n <= *max;
            let miss = (n as f32 - *max as f32).max(0.0);
            ConstraintReport {
                label: format!("stance_count_max:{stance:?}"),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::RegionCountMin { region_kind, min } => {
            let n = sector
                .regions
                .iter()
                .filter(|r| region_kind.matches(r.kind))
                .count() as u32;
            let passed = n >= *min;
            let miss = (*min as f32 - n as f32).max(0.0);
            ConstraintReport {
                label: format!("region_count_min:{region_kind:?}"),
                passed,
                observed: n.to_string(),
                required: format!(">= {min}"),
                miss,
            }
        }
        Constraint::RegionCountMax { region_kind, max } => {
            let n = sector
                .regions
                .iter()
                .filter(|r| region_kind.matches(r.kind))
                .count() as u32;
            let passed = n <= *max;
            let miss = (n as f32 - *max as f32).max(0.0);
            ConstraintReport {
                label: format!("region_count_max:{region_kind:?}"),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::EconomyStrandedMax { max } => {
            let n = sector.economy.worlds.iter().filter(|w| w.stranded).count() as u32;
            let passed = n <= *max;
            let miss = (n as f32 - *max as f32).max(0.0);
            ConstraintReport {
                label: "economy_stranded_max".to_string(),
                passed,
                observed: n.to_string(),
                required: format!("<= {max}"),
                miss,
            }
        }
        Constraint::EconomyResourceMin { resource, min } => {
            let v = match resource.as_str() {
                "ore" => sector.economy.sector_balance.ore,
                "promethium" => sector.economy.sector_balance.promethium,
                "foodstuffs" => sector.economy.sector_balance.foodstuffs,
                "manufactured" => sector.economy.sector_balance.manufactured,
                "archeotech" => sector.economy.sector_balance.archeotech,
                "recruits" => sector.economy.sector_balance.recruits,
                _ => 0.0,
            };
            let passed = v >= *min;
            let miss = (*min - v).max(0.0);
            ConstraintReport {
                label: format!("economy_resource_min:{resource}"),
                passed,
                observed: format!("{v:.1}"),
                required: format!(">= {min:.1}"),
                miss,
            }
        }
    }
}

fn evaluate_all(
    sector: &GeneratedSector,
    analysis: &SectorAnalysis,
    constraints: &[Constraint],
    n: u32,
    seed: &str,
) -> CandidateReport {
    let fac_idx = build_faction_index(analysis);
    let reports: Vec<ConstraintReport> = constraints
        .iter()
        .map(|c| evaluate(c, sector, analysis, &fac_idx))
        .collect();
    let passed = reports.iter().all(|r| r.passed);
    let total_miss: f32 = reports.iter().map(|r| r.miss).sum();
    CandidateReport {
        n,
        seed: seed.to_string(),
        passed,
        total_miss,
        constraints: reports,
    }
}

// ── Search driver ──────────────────────────────────────────────────────────────

/// Deterministic search over candidate seeds. Stops at the first satisfying
/// candidate; otherwise returns the best near-miss + a sorted list of the
/// top `report_top` near-misses.
///
/// # Errors
///
/// Propagates any [`SectorError`] from `generate_sector` for individual
/// candidates after the preflight passes. Preflight failures (unknown faction
/// ids etc.) are reported in `SearchOutcome::preflight_errors` rather than
/// raised as errors, so the CLI can print a friendly message.
pub fn run_search(
    project_template: &ProjectInput,
    wishes: &WishesFile,
) -> Result<SearchOutcome, SectorError> {
    run_search_with_progress(project_template, wishes, |_| {})
}

/// §SR2: same deterministic search as [`run_search`], but invokes `progress`
/// with a [`SearchProgress`] snapshot after every candidate is generated +
/// evaluated. The callback runs on rayon worker threads, so it must be
/// `Sync` and cheap (the builder forwards into a shared mutex + repaint). The
/// returned outcome is byte-for-byte identical to [`run_search`].
///
/// # Errors
///
/// Same as [`run_search`].
pub fn run_search_with_progress(
    project_template: &ProjectInput,
    wishes: &WishesFile,
    progress: impl Fn(SearchProgress) + Sync,
) -> Result<SearchOutcome, SectorError> {
    let base_seed = wishes
        .search
        .base_seed
        .clone()
        .unwrap_or_else(|| project_template.config.generation.seed.clone());
    let budget = wishes.search.budget.max(1);
    let report_top = wishes.search.report_top.max(1) as usize;

    let preflight_errors = preflight(project_template, &wishes.constraints);
    if !preflight_errors.is_empty() {
        return Ok(SearchOutcome {
            base_seed,
            budget,
            candidates_evaluated: 0,
            winning: None,
            near_misses: Vec::new(),
            preflight_errors,
        });
    }

    // FIX.txt §13 + TF-P-2: data-parallel candidate enumeration with a shared
    // "lowest winning n" guard. Once any worker has produced a passing
    // candidate at index `k`, every other worker skips work for `n > k` —
    // the final result only ever surfaces the lowest passing `n`, so higher
    // candidates can never be returned as `winning`. Determinism preserved:
    // the winner is still the lowest n in `(0..budget)` whose candidate
    // passes (since we always finish n=0).
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    enum Slot {
        Skipped,
        Report(Box<CandidateReport>),
    }

    let lowest_winner = AtomicU32::new(u32::MAX);

    // §SR2 live counters shared across the rayon workers. `best_bits` holds the
    // lowest `total_miss` seen so far as raw f32 bits; for the non-negative
    // finite misses this search produces, bitwise `fetch_min` matches numeric
    // min. The sentinel `f32::INFINITY` means "nothing evaluated yet".
    let tried = AtomicU32::new(0);
    let passed = AtomicU32::new(0);
    let best_bits = AtomicU32::new(f32::INFINITY.to_bits());
    let report_progress = |report: &CandidateReport| {
        let tried_now = tried.fetch_add(1, Ordering::Relaxed) + 1;
        if report.passed {
            passed.fetch_add(1, Ordering::Relaxed);
        }
        best_bits.fetch_min(report.total_miss.to_bits(), Ordering::Relaxed);
        let best = f32::from_bits(best_bits.load(Ordering::Relaxed));
        progress(SearchProgress {
            tried: tried_now,
            passed: passed.load(Ordering::Relaxed),
            best_miss: best.is_finite().then_some(best),
            budget,
        });
    };

    let slots: Vec<Slot> = (0..budget)
        .into_par_iter()
        .map(|n| {
            // Short-circuit: if a passing candidate at a lower n already exists,
            // this n can only contribute to `near_misses`. The cost of full
            // sector generation is two orders of magnitude above the
            // `near_misses` evaluation, so skipping here is the main win.
            if n >= lowest_winner.load(Ordering::Relaxed) {
                return Slot::Skipped;
            }

            let seed = derive_candidate_seed(&base_seed, n);
            let mut input = clone_project_with_seed(project_template, &seed);
            input.config.generation.seed = seed.clone();

            let pre = crate::validation::validate(&input);
            if !pre.ok {
                return Slot::Skipped;
            }

            let sector = match crate::generation::generate(input) {
                Ok(s) => s,
                Err(_) => return Slot::Skipped,
            };
            let analysis = analytics::analyze(&sector);
            let report = evaluate_all(&sector, &analysis, &wishes.constraints, n, &seed);
            if report.passed {
                lowest_winner.fetch_min(n, Ordering::Relaxed);
            }
            report_progress(&report);
            Slot::Report(Box::new(report))
        })
        .collect();

    let mut near_misses: Vec<CandidateReport> = Vec::new();
    let mut winning: Option<CandidateReport> = None;

    for slot in slots {
        match slot {
            Slot::Skipped => {}
            Slot::Report(r) => {
                if r.passed && winning.is_none() {
                    winning = Some(*r);
                } else if !r.passed {
                    insert_top_n(&mut near_misses, *r, report_top);
                }
            }
        }
    }

    // Match sequential semantics: serial code increments `evaluated` once per
    // iteration and breaks immediately after the first pass; in parallel we
    // ran the full budget, so report what the serial counter would have read.
    let evaluated: u32 = match &winning {
        Some(w) => w.n.saturating_add(1),
        None => budget,
    };

    near_misses.sort_by(|a, b| {
        crate::analysis::cmp_f32_asc(a.total_miss, b.total_miss).then_with(|| a.n.cmp(&b.n))
    });

    Ok(SearchOutcome {
        base_seed,
        budget,
        candidates_evaluated: evaluated,
        winning,
        near_misses,
        preflight_errors,
    })
}

fn insert_top_n(buf: &mut Vec<CandidateReport>, cand: CandidateReport, top: usize) {
    if buf.len() < top {
        buf.push(cand);
        return;
    }
    // Find current worst (largest total_miss). Replace if this one is better.
    let mut worst_idx = 0usize;
    for (i, r) in buf.iter().enumerate() {
        if r.total_miss > buf[worst_idx].total_miss {
            worst_idx = i;
        }
    }
    if cand.total_miss < buf[worst_idx].total_miss {
        buf[worst_idx] = cand;
    }
}

/// Clone a project input + override the configured seed. Catalogs and parsed
/// data files are reused (only the seed changes), so the search loop avoids
/// re-reading disk for every candidate.
///
/// §TF-P-1: catalogs are shared via `Arc::clone` (one refcount bump) instead of
/// 14 deep clones per candidate. The mutable `config` (seed is overwritten) and
/// the small `root_dir` / `input_digests` still clone normally.
fn clone_project_with_seed(template: &ProjectInput, seed: &str) -> ProjectInput {
    let mut config = template.config.clone();
    config.generation.seed = seed.to_string();
    ProjectInput {
        root_dir: template.root_dir.clone(),
        config,
        catalogs: std::sync::Arc::clone(&template.catalogs),
        input_digests: template.input_digests.clone(),
    }
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_outcome_markdown(outcome: &SearchOutcome) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Seed Search");
    let _ = writeln!(
        s,
        "\nBase seed: `{}`  ·  Budget: {}  ·  Evaluated: {}",
        outcome.base_seed, outcome.budget, outcome.candidates_evaluated
    );
    if !outcome.preflight_errors.is_empty() {
        let _ = writeln!(s, "\n## Preflight Errors");
        for e in &outcome.preflight_errors {
            let _ = writeln!(s, "- {e}");
        }
        return s;
    }
    if let Some(w) = &outcome.winning {
        let _ = writeln!(s, "\n## Winning Seed");
        let _ = writeln!(s, "\n**Seed:** `{}` (candidate #{})", w.seed, w.n);
        let _ = writeln!(s, "\n| Constraint | Required | Observed |");
        let _ = writeln!(s, "|---|---|---|");
        for c in &w.constraints {
            let _ = writeln!(
                s,
                "| {} | `{}` | {} |",
                c.label,
                c.required,
                if c.passed {
                    format!("✓ {}", c.observed)
                } else {
                    format!("✗ {}", c.observed)
                }
            );
        }
    } else {
        let _ = writeln!(
            s,
            "\n## No seed satisfied all constraints within the budget."
        );
        if let Some(best) = outcome.near_misses.first() {
            let _ = writeln!(
                s,
                "\nBest near-miss: candidate #{} (seed `{}`), total miss {:.3}.",
                best.n, best.seed, best.total_miss
            );
        }
    }
    if !outcome.near_misses.is_empty() {
        let _ = writeln!(s, "\n## Near-Miss Candidates");
        let _ = writeln!(s, "\n| # | Seed | Total miss | Failed constraints |");
        let _ = writeln!(s, "|---:|---|---:|---|");
        for cand in &outcome.near_misses {
            let failed: Vec<String> = cand
                .constraints
                .iter()
                .filter(|c| !c.passed)
                .map(|c| format!("{} (obs={}, req={})", c.label, c.observed, c.required))
                .collect();
            let _ = writeln!(
                s,
                "| {} | `{}` | {:.3} | {} |",
                cand.n,
                cand.seed,
                cand.total_miss,
                failed.join("; ")
            );
        }
    }
    s
}

// ── Output bundles ─────────────────────────────────────────────────────────────

/// Write `search.md` + `search.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written, and
/// [`SectorError::ExportFailed`] if the outcome cannot be serialised.
pub fn write_search_outcome(
    output_dir: &Utf8Path,
    outcome: &SearchOutcome,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        "search",
        &render_outcome_markdown(outcome),
        outcome,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_candidate_seed_is_deterministic_and_distinct() {
        let a = derive_candidate_seed("base", 0);
        assert_eq!(a, "base");
        let b = derive_candidate_seed("base", 1);
        let c = derive_candidate_seed("base", 1);
        assert_eq!(b, c);
        assert_ne!(a, b);
        let d = derive_candidate_seed("different-base", 1);
        assert_ne!(b, d);
    }

    #[test]
    fn insert_top_n_keeps_lowest_miss_only() {
        let mk = |miss: f32| CandidateReport {
            n: 0,
            seed: "x".into(),
            passed: false,
            total_miss: miss,
            constraints: Vec::new(),
        };
        let mut buf: Vec<CandidateReport> = Vec::new();
        for m in [3.0, 1.0, 4.0, 2.0, 5.0, 0.5] {
            insert_top_n(&mut buf, mk(m), 3);
        }
        let mut got: Vec<f32> = buf.iter().map(|x| x.total_miss).collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, vec![0.5, 1.0, 2.0]);
    }
}
