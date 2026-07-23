//! Economy config + type definitions: tunable thresholds, the `economy.toml`
//! schema (`EconomyFile`/`EconomyConfig`/`ResourceModelConfig`), the resource
//! vectors (`ResourceVector`/`StrategicOutput`), and the serialized output DTOs
//! (`EconomyReport`/`WorldEconomy`/`SystemEconomy`/`RouteEconomy`/edges + the
//! status enums).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ── Resource categories ────────────────────────────────────────────────────────

pub const RESOURCE_KEYS: &[&str] = &[
    "ore",
    "promethium",
    "foodstuffs",
    "manufactured",
    "archeotech",
    "recruits",
];

pub const STRATEGIC_RESOURCE_KEYS: &[&str] = &[
    "food",
    "ore",
    "manufacturing",
    "arms",
    "ships",
    "pilgrimage",
    "psyker_tithe",
    "manpower",
    "knowledge",
    "xenos_value",
];

// ── Calibration thresholds (§B12: named magic numbers) ──────────────────────────

/// A system/world producing at least this much of a needed strategic resource is
/// self-sufficient in it (skips the dependency-edge / supply-risk path).
pub(super) const SELF_SUFFICIENCY_OUTPUT: f32 = 30.0;
/// A world whose supply-resilience reaches this level has its supply risk lowered
/// by one tier.
pub(super) const SUPPLY_RESILIENCE_SAFE: f32 = 30.0;
/// Route-friction calibration: each hazard score is divided by its divisor, then
/// clamped into the per-hazard malus (piracy/interdiction) or bonus (patrol)
/// applied to the route weight.
pub(super) const ROUTE_PIRACY_DIVISOR: f32 = 200.0;
pub(super) const ROUTE_PIRACY_MAX_MALUS: f32 = 0.5;
pub(super) const ROUTE_INTERDICTION_DIVISOR: f32 = 200.0;
pub(super) const ROUTE_INTERDICTION_MAX_MALUS: f32 = 0.6;
pub(super) const ROUTE_PATROL_DIVISOR: f32 = 400.0;
pub(super) const ROUTE_PATROL_MAX_BONUS: f32 = 0.25;

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EconomyFile {
    #[serde(default)]
    pub economy: EconomyConfig,
    /// §4 NEW2.md: optional top-level `[resources]` block, matching the
    /// design-doc example (`[resources.world_type.AgriWorld]`).
    #[serde(default)]
    pub resources: ResourceModelConfig,
}

impl EconomyFile {
    #[must_use]
    pub fn into_config(mut self) -> EconomyConfig {
        if !self.resources.is_empty() {
            self.economy.resources = self.resources;
        }
        self.economy
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EconomyConfig {
    /// Whether the derivation runs. Defaults to `false` so legacy projects
    /// keep byte-identical sector JSON output.
    #[serde(default)]
    pub enabled: bool,
    /// Whether shortfall worlds nudge `stability.famine_or_resource_stress`
    /// upward. Read-only nudge; conflict tick is not affected.
    #[serde(default)]
    pub feed_stability: bool,
    /// Production/consumption table additions or overrides keyed by world_type.
    #[serde(default)]
    pub by_world_type: BTreeMap<String, ResourceVector>,
    /// Tech-level multipliers applied to the world-type vector.
    #[serde(default)]
    pub by_tech_level: BTreeMap<String, f32>,
    /// Population scale multipliers applied after tech.
    #[serde(default)]
    pub by_population: BTreeMap<String, f32>,
    /// §4 NEW2.md: strategic output rules. May be nested here or supplied as
    /// top-level `[resources]` and merged by `EconomyFile::into_config`.
    #[serde(default)]
    pub resources: ResourceModelConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ResourceVector {
    #[serde(default)]
    pub ore: f32,
    #[serde(default)]
    pub promethium: f32,
    #[serde(default)]
    pub foodstuffs: f32,
    #[serde(default)]
    pub manufactured: f32,
    #[serde(default)]
    pub archeotech: f32,
    #[serde(default)]
    pub recruits: f32,
}

impl ResourceVector {
    pub fn get(&self, key: &str) -> f32 {
        match key {
            "ore" => self.ore,
            "promethium" => self.promethium,
            "foodstuffs" => self.foodstuffs,
            "manufactured" => self.manufactured,
            "archeotech" => self.archeotech,
            "recruits" => self.recruits,
            _ => 0.0,
        }
    }
    /// Mutable refs to every resource field, in declaration order — single
    /// source of the field list for `scale` and per-field accumulation
    /// (builder `recompute_economy`, B5).
    pub fn fields_mut(&mut self) -> [&mut f32; 6] {
        [
            &mut self.ore,
            &mut self.promethium,
            &mut self.foodstuffs,
            &mut self.manufactured,
            &mut self.archeotech,
            &mut self.recruits,
        ]
    }

    pub fn fields(&self) -> [f32; 6] {
        [
            self.ore,
            self.promethium,
            self.foodstuffs,
            self.manufactured,
            self.archeotech,
            self.recruits,
        ]
    }

    pub(super) fn scale(mut self, factor: f32) -> Self {
        for f in self.fields_mut() {
            *f *= factor;
        }
        self
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ResourceModelConfig {
    #[serde(default)]
    pub world_type: BTreeMap<String, StrategicOutputRule>,
    #[serde(default)]
    pub notable_feature: BTreeMap<String, StrategicOutputRule>,
}

impl ResourceModelConfig {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.world_type.is_empty() && self.notable_feature.is_empty()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StrategicOutputRule {
    #[serde(default)]
    pub food: Option<f32>,
    #[serde(default)]
    pub ore: Option<f32>,
    #[serde(default)]
    pub manufacturing: Option<f32>,
    #[serde(default)]
    pub arms: Option<f32>,
    #[serde(default)]
    pub ships: Option<f32>,
    #[serde(default)]
    pub pilgrimage: Option<f32>,
    #[serde(default)]
    pub psyker_tithe: Option<f32>,
    #[serde(default)]
    pub manpower: Option<f32>,
    #[serde(default)]
    pub knowledge: Option<f32>,
    #[serde(default)]
    pub xenos_value: Option<f32>,
    #[serde(default)]
    pub trade_multiplier: Option<f32>,
    #[serde(default)]
    pub supply_resilience: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct StrategicOutput {
    #[serde(default)]
    pub food: f32,
    #[serde(default)]
    pub ore: f32,
    #[serde(default)]
    pub manufacturing: f32,
    #[serde(default)]
    pub arms: f32,
    #[serde(default)]
    pub ships: f32,
    #[serde(default)]
    pub pilgrimage: f32,
    #[serde(default)]
    pub psyker_tithe: f32,
    #[serde(default)]
    pub manpower: f32,
    #[serde(default)]
    pub knowledge: f32,
    #[serde(default)]
    pub xenos_value: f32,
}

impl StrategicOutput {
    #[must_use]
    pub fn get(&self, key: &str) -> f32 {
        match key {
            "food" => self.food,
            "ore" => self.ore,
            "manufacturing" => self.manufacturing,
            "arms" => self.arms,
            "ships" => self.ships,
            "pilgrimage" => self.pilgrimage,
            "psyker_tithe" => self.psyker_tithe,
            "manpower" => self.manpower,
            "knowledge" => self.knowledge,
            "xenos_value" => self.xenos_value,
            _ => 0.0,
        }
    }

    /// Mutable refs to every score field, in declaration order. Single source
    /// of the field list for the per-field arithmetic helpers (B5) — adding a
    /// field here flows it into `add_assign` / `scale` / `clamp_scores` at once.
    fn fields_mut(&mut self) -> [&mut f32; 10] {
        [
            &mut self.food,
            &mut self.ore,
            &mut self.manufacturing,
            &mut self.arms,
            &mut self.ships,
            &mut self.pilgrimage,
            &mut self.psyker_tithe,
            &mut self.manpower,
            &mut self.knowledge,
            &mut self.xenos_value,
        ]
    }

    fn fields(&self) -> [f32; 10] {
        [
            self.food,
            self.ore,
            self.manufacturing,
            self.arms,
            self.ships,
            self.pilgrimage,
            self.psyker_tithe,
            self.manpower,
            self.knowledge,
            self.xenos_value,
        ]
    }

    /// Sum `other`'s fields into `self`, in declaration order — used by the
    /// builder's `recompute_economy` to re-total per-system/sector rows.
    pub fn add_assign(&mut self, other: &Self) {
        let others = other.fields();
        for (f, o) in self.fields_mut().into_iter().zip(others) {
            *f += o;
        }
    }

    pub(super) fn scale(mut self, factor: f32) -> Self {
        for f in self.fields_mut() {
            *f *= factor;
        }
        self
    }

    pub(super) fn clamp_scores(mut self) -> Self {
        for f in self.fields_mut() {
            *f = f.clamp(0.0, 100.0);
        }
        self
    }

    #[must_use]
    pub fn weighted_priority_score(&self) -> f32 {
        // Per-field priority weights, indexed in `fields()` declaration order:
        // [food, ore, manufacturing, arms, ships, pilgrimage, psyker_tithe,
        //  manpower, knowledge, xenos_value].
        const WEIGHTS: [f32; 10] = [0.70, 0.70, 0.85, 1.00, 1.20, 0.55, 1.10, 0.80, 1.00, 0.90];
        let f = self.fields();
        // Reproduces the original nested `mul_add` chain BIT-FOR-BIT (golden-stable):
        // the innermost term is the plain multiply `ore * 0.70` (the seed accumulator),
        // then the remaining nine terms fold in source-nesting order — food first, then
        // manufacturing..xenos_value — each as one fused `field.mul_add(weight, acc)`.
        // The fold is data-dependent through `acc`, so no reassociation occurs; do NOT
        // rewrite as `iter().map(|(f, w)| f * w).sum()` (that changes FMA association).
        let mut acc = f[1] * WEIGHTS[1]; // ore * 0.70 — innermost seed, a plain multiply
        for i in [0usize, 2, 3, 4, 5, 6, 7, 8, 9] {
            acc = f[i].mul_add(WEIGHTS[i], acc);
        }
        acc
    }
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EconomyReport {
    /// True when the derivation ran (config.enabled or explicit call).
    #[serde(default)]
    pub enabled: bool,
    pub worlds: Vec<WorldEconomy>,
    pub systems: Vec<SystemEconomy>,
    pub routes: Vec<RouteEconomy>,
    pub sector_balance: ResourceVector,
    /// Sector-wide strategic-output totals after world type, features,
    /// population, tech, stability, and control modifiers.
    #[serde(default)]
    pub strategic_output: StrategicOutput,
    /// Material reliance edges: supplier system → dependent system by
    /// resource, optionally through a specific route.
    #[serde(default)]
    pub dependency_edges: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEconomy {
    pub system_id: crate::ids::SystemId,
    pub world_id: crate::ids::WorldId,
    pub vector: ResourceVector,
    #[serde(default)]
    pub strategic_output: StrategicOutput,
    #[serde(default)]
    pub tithe_status: TitheStatus,
    #[serde(default)]
    pub supply_risk: SupplyRisk,
    #[serde(default)]
    pub strategic_priority: StrategicPriority,
    #[serde(default)]
    pub supply_resilience: f32,
    /// True when net foodstuffs is negative *and* no inbound route can fix it.
    #[serde(default)]
    pub stranded: bool,
    /// Critical shortages by resource key (those with deficit >= 20).
    #[serde(default)]
    pub shortages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEconomy {
    pub system_id: crate::ids::SystemId,
    pub vector: ResourceVector,
    #[serde(default)]
    pub strategic_output: StrategicOutput,
    #[serde(default)]
    pub tithe_status: TitheStatus,
    #[serde(default)]
    pub supply_risk: SupplyRisk,
    #[serde(default)]
    pub strategic_priority: StrategicPriority,
    /// `surplus_resources`/`shortage_resources` for quick UI.
    #[serde(default)]
    pub surplus_resources: Vec<String>,
    #[serde(default)]
    pub shortage_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEconomy {
    pub route_id: crate::ids::RouteId,
    pub from_system_id: crate::ids::SystemId,
    pub to_system_id: crate::ids::SystemId,
    pub volume: f32,
    /// 0..=1 modifier from hazard tier × piracy/interdiction.
    #[serde(default = "default_one")]
    pub friction: f32,
}

fn default_one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TitheStatus {
    Surplus,
    #[default]
    Adequate,
    Strained,
    Delinquent,
    Failed,
    Falsified,
}

enum_slug!(TitheStatus {
    Surplus => "surplus",
    Adequate => "adequate",
    Strained => "strained",
    Delinquent => "delinquent",
    Failed => "failed",
    Falsified => "falsified",
});

impl core::fmt::Display for TitheStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SupplyRisk {
    #[default]
    Stable,
    Vulnerable,
    Disrupted,
    Collapsing,
}

enum_slug!(SupplyRisk {
    Stable => "stable",
    Vulnerable => "vulnerable",
    Disrupted => "disrupted",
    Collapsing => "collapsing",
});

impl core::fmt::Display for SupplyRisk {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StrategicPriority {
    #[default]
    Low,
    Local,
    Subsector,
    Sector,
    CrusadeLevel,
}

enum_slug!(StrategicPriority {
    Low => "low",
    Local => "local",
    Subsector => "subsector",
    Sector => "sector",
    CrusadeLevel => "crusade_level",
});

impl core::fmt::Display for StrategicPriority {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from_system_id: crate::ids::SystemId,
    pub to_system_id: crate::ids::SystemId,
    pub resource: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<crate::ids::RouteId>,
    /// Effective import score after route friction.
    pub score: f32,
    pub risk: SupplyRisk,
}
