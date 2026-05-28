//! Trade, tithe, and strategic resource layer (§12 NEW.md + §4 NEW2.md).
//!
//! Pure read-only derivation over the finished sector: each world declares a
//! production/consumption vector (keyed by `world_type` × `tech_level` ×
//! population scale, mapped from the existing `tags`), routes carry derived
//! trade volume = function of endpoint surplus/deficit gradient × distance
//! falloff × `RouteStability` × per-faction `RouteControl` interference.
//! No new RNG draws — same sector ⇒ same numbers.
//!
//! Default production tables ship in this module; users may override or
//! extend them in `economy.toml` (referenced by `inputs.economy` in
//! `sectorforge.toml`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::sector_model::{GeneratedRoute, GeneratedSector, GeneratedWorld, RouteStability};

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
    fn scale(mut self, f: f32) -> Self {
        self.ore *= f;
        self.promethium *= f;
        self.foodstuffs *= f;
        self.manufactured *= f;
        self.archeotech *= f;
        self.recruits *= f;
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

    fn add_assign(&mut self, other: &Self) {
        self.food += other.food;
        self.ore += other.ore;
        self.manufacturing += other.manufacturing;
        self.arms += other.arms;
        self.ships += other.ships;
        self.pilgrimage += other.pilgrimage;
        self.psyker_tithe += other.psyker_tithe;
        self.manpower += other.manpower;
        self.knowledge += other.knowledge;
        self.xenos_value += other.xenos_value;
    }

    fn scale(mut self, f: f32) -> Self {
        self.food *= f;
        self.ore *= f;
        self.manufacturing *= f;
        self.arms *= f;
        self.ships *= f;
        self.pilgrimage *= f;
        self.psyker_tithe *= f;
        self.manpower *= f;
        self.knowledge *= f;
        self.xenos_value *= f;
        self
    }

    fn clamp_scores(mut self) -> Self {
        self.food = self.food.clamp(0.0, 100.0);
        self.ore = self.ore.clamp(0.0, 100.0);
        self.manufacturing = self.manufacturing.clamp(0.0, 100.0);
        self.arms = self.arms.clamp(0.0, 100.0);
        self.ships = self.ships.clamp(0.0, 100.0);
        self.pilgrimage = self.pilgrimage.clamp(0.0, 100.0);
        self.psyker_tithe = self.psyker_tithe.clamp(0.0, 100.0);
        self.manpower = self.manpower.clamp(0.0, 100.0);
        self.knowledge = self.knowledge.clamp(0.0, 100.0);
        self.xenos_value = self.xenos_value.clamp(0.0, 100.0);
        self
    }

    #[must_use]
    pub fn weighted_priority_score(&self) -> f32 {
        self.xenos_value.mul_add(
            0.90,
            self.knowledge.mul_add(
                1.00,
                self.manpower.mul_add(
                    0.80,
                    self.psyker_tithe.mul_add(
                        1.10,
                        self.pilgrimage.mul_add(
                            0.55,
                            self.ships.mul_add(
                                1.20,
                                self.arms.mul_add(
                                    1.00,
                                    self.manufacturing
                                        .mul_add(0.85, self.food.mul_add(0.70, self.ore * 0.70)),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        )
    }
}

// ── Built-in vectors (positive = surplus, negative = deficit) ──────────────────

fn default_world_type_vector(world_type: &str) -> ResourceVector {
    match world_type {
        "HiveWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -40.0,
            manufactured: 40.0,
            archeotech: 0.0,
            recruits: 60.0,
        },
        "ForgeWorld" => ResourceVector {
            ore: -20.0,
            promethium: 0.0,
            foodstuffs: -20.0,
            manufactured: 80.0,
            archeotech: 10.0,
            recruits: 5.0,
        },
        "AgriWorld" | "AgriculturalWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 80.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 10.0,
        },
        "MiningWorld" => ResourceVector {
            ore: 70.0,
            promethium: 20.0,
            foodstuffs: -10.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 5.0,
        },
        "DeathWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 5.0,
            recruits: 40.0,
        },
        "KnightWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 10.0,
            manufactured: 10.0,
            archeotech: 0.0,
            recruits: 30.0,
        },
        "CivilisedWorld" | "CivilizedWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 10.0,
            manufactured: 15.0,
            archeotech: 0.0,
            recruits: 15.0,
        },
        "FeudalWorld" | "FeralWorld" => ResourceVector {
            ore: 5.0,
            promethium: 0.0,
            foodstuffs: 20.0,
            manufactured: -20.0,
            archeotech: 0.0,
            recruits: 25.0,
        },
        "FortressWorld" => ResourceVector {
            ore: 0.0,
            promethium: 5.0,
            foodstuffs: -10.0,
            manufactured: 5.0,
            archeotech: 0.0,
            recruits: 30.0,
        },
        "Shrine" | "ShrineWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 20.0,
        },
        "PleasureWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 5.0,
        },
        "DeadWorld" | "WarpLostWorld" | "QuarantinedWorld" | "Uninhabited" => {
            ResourceVector::default()
        }
        _ => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 0.0,
        },
    }
}

fn default_tech_multiplier(tech: &str) -> f32 {
    match tech {
        "STC" | "Archeotech" => 1.5,
        "Imperial" => 1.0,
        "Mechanicus" => 1.2,
        "PreImperial" | "Industrial" => 0.7,
        "Renaissance" | "Medieval" | "Iron" => 0.4,
        "Stone" | "Primitive" => 0.2,
        _ => 1.0,
    }
}

fn default_population_multiplier(pop_tag: &str) -> f32 {
    // tag form: "population:massive" etc.
    match pop_tag {
        "population:massive" => 1.5,
        "population:large" | "population:huge" => 1.3,
        "population:standard" => 1.0,
        "population:sole_settlement" | "population:lightly_populated" => 0.5,
        "population:minimal" => 0.25,
        "population:uninhabited" => 0.0,
        _ => 1.0,
    }
}

fn default_strategic_world_type(world_type: &str) -> StrategicOutput {
    match world_type {
        "AgriWorld" => StrategicOutput {
            food: 90.0,
            manpower: 20.0,
            manufacturing: 5.0,
            ..Default::default()
        },
        "ForgeWorld" => StrategicOutput {
            ore: 5.0,
            manufacturing: 85.0,
            arms: 75.0,
            ships: 45.0,
            knowledge: 35.0,
            manpower: 15.0,
            ..Default::default()
        },
        "IndustrialWorld" => StrategicOutput {
            manufacturing: 70.0,
            arms: 45.0,
            ships: 20.0,
            manpower: 35.0,
            ..Default::default()
        },
        "ExtractiveColony" | "Asteroid" => StrategicOutput {
            ore: 80.0,
            manufacturing: 10.0,
            manpower: 10.0,
            ..Default::default()
        },
        "HiveWorld" => StrategicOutput {
            food: 5.0,
            manufacturing: 45.0,
            arms: 25.0,
            manpower: 85.0,
            psyker_tithe: 30.0,
            knowledge: 20.0,
            ..Default::default()
        },
        "ShrineWorld" => StrategicOutput {
            food: 10.0,
            pilgrimage: 85.0,
            manpower: 25.0,
            knowledge: 20.0,
            ..Default::default()
        },
        "ResearchStation" => StrategicOutput {
            knowledge: 80.0,
            xenos_value: 20.0,
            manufacturing: 10.0,
            ..Default::default()
        },
        "BastionWorld" => StrategicOutput {
            arms: 55.0,
            ships: 25.0,
            manpower: 60.0,
            manufacturing: 25.0,
            ..Default::default()
        },
        "PenalWorld" => StrategicOutput {
            manpower: 45.0,
            ore: 25.0,
            ..Default::default()
        },
        "DeathWorld" | "FeralWorld" | "FeudalWorld" => StrategicOutput {
            food: 25.0,
            manpower: 45.0,
            ore: 10.0,
            ..Default::default()
        },
        "PleasureWorld" => StrategicOutput {
            pilgrimage: 45.0,
            knowledge: 10.0,
            ..Default::default()
        },
        "TombWorld" => StrategicOutput {
            knowledge: 25.0,
            xenos_value: 90.0,
            ore: 20.0,
            ..Default::default()
        },
        "XenosWorld" | "Worldship" => StrategicOutput {
            xenos_value: 75.0,
            knowledge: 30.0,
            ships: 20.0,
            ..Default::default()
        },
        "DeadWorld" | "WarpLostWorld" | "PlanetaryDump" => StrategicOutput::default(),
        _ => StrategicOutput {
            food: 20.0,
            ore: 15.0,
            manufacturing: 15.0,
            manpower: 20.0,
            knowledge: 10.0,
            ..Default::default()
        },
    }
}

fn strategic_tech_multiplier(tech: &str) -> f32 {
    match tech {
        "Primitive" => 0.35,
        "Low" => 0.60,
        "Standard" => 1.00,
        "High" => 1.20,
        "XenoHybrid" => 1.10,
        "Archaeotech" => 1.45,
        _ => default_tech_multiplier(tech),
    }
}

fn strategic_population_multiplier(pop_tag: &str, pop_label: &str) -> f32 {
    match pop_label {
        "Uninhabited" => 0.0,
        "Minimal" => 0.25,
        "LightlyPopulated" => 0.50,
        "SoleSettlement" => 0.55,
        "DenselyPopulated" => 1.00,
        "ExtremelyDense" => 1.35,
        _ => default_population_multiplier(pop_tag),
    }
}

fn default_feature_rule(feature: &str) -> Option<StrategicOutputRule> {
    let mut r = StrategicOutputRule::default();
    match feature {
        "HeavyMining" | "FreakGeology" | "GoldRush" => r.ore = Some(25.0),
        "HeavyIndustry" | "GreatWork" | "LocalTech" => r.manufacturing = Some(20.0),
        "MajorSpaceyard" => {
            r.ships = Some(35.0);
            r.manufacturing = Some(15.0);
            r.supply_resilience = Some(10.0);
        }
        "VastFortresses" | "MartialLaw" | "ImperialKnights" => {
            r.arms = Some(20.0);
            r.manpower = Some(15.0);
        }
        "ImportantShrine" | "PilgrimageSite" | "Missionaries" | "SororitasConvent" => {
            r.pilgrimage = Some(25.0);
        }
        "PsykerAcademy" | "PsykerCult" => r.psyker_tithe = Some(30.0),
        "AncientArchive" | "ArchaeotechRuins" | "ForbiddenTech" => {
            r.knowledge = Some(25.0);
            r.xenos_value = Some(10.0);
        }
        "XenoRuins" | "AncientTombs" | "SealedMenace" => r.xenos_value = Some(35.0),
        "TradeHub" | "Freeport" | "AdministrativeHub" | "SubsectorHegemon" => {
            r.trade_multiplier = Some(1.25);
            r.supply_resilience = Some(15.0);
        }
        "VerdantEcology" | "OceanWorld" | "JungleWorld" => r.food = Some(20.0),
        "WarZone" | "NavalBlockade" | "CivilWar" | "Pandemic" | "Quarantined" => {
            r.supply_resilience = Some(-15.0);
        }
        _ => return None,
    }
    Some(r)
}

fn apply_world_type_rule(mut base: StrategicOutput, rule: &StrategicOutputRule) -> StrategicOutput {
    if let Some(v) = rule.food {
        base.food = v;
    }
    if let Some(v) = rule.ore {
        base.ore = v;
    }
    if let Some(v) = rule.manufacturing {
        base.manufacturing = v;
    }
    if let Some(v) = rule.arms {
        base.arms = v;
    }
    if let Some(v) = rule.ships {
        base.ships = v;
    }
    if let Some(v) = rule.pilgrimage {
        base.pilgrimage = v;
    }
    if let Some(v) = rule.psyker_tithe {
        base.psyker_tithe = v;
    }
    if let Some(v) = rule.manpower {
        base.manpower = v;
    }
    if let Some(v) = rule.knowledge {
        base.knowledge = v;
    }
    if let Some(v) = rule.xenos_value {
        base.xenos_value = v;
    }
    base
}

fn apply_feature_rule(out: &mut StrategicOutput, rule: &StrategicOutputRule) {
    let mut add = StrategicOutput::default();
    if let Some(v) = rule.food {
        add.food = v;
    }
    if let Some(v) = rule.ore {
        add.ore = v;
    }
    if let Some(v) = rule.manufacturing {
        add.manufacturing = v;
    }
    if let Some(v) = rule.arms {
        add.arms = v;
    }
    if let Some(v) = rule.ships {
        add.ships = v;
    }
    if let Some(v) = rule.pilgrimage {
        add.pilgrimage = v;
    }
    if let Some(v) = rule.psyker_tithe {
        add.psyker_tithe = v;
    }
    if let Some(v) = rule.manpower {
        add.manpower = v;
    }
    if let Some(v) = rule.knowledge {
        add.knowledge = v;
    }
    if let Some(v) = rule.xenos_value {
        add.xenos_value = v;
    }
    out.add_assign(&add);
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

impl TitheStatus {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::Surplus => "surplus",
            Self::Adequate => "adequate",
            Self::Strained => "strained",
            Self::Delinquent => "delinquent",
            Self::Failed => "failed",
            Self::Falsified => "falsified",
        }
    }
}

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

impl SupplyRisk {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Vulnerable => "vulnerable",
            Self::Disrupted => "disrupted",
            Self::Collapsing => "collapsing",
        }
    }
}

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

impl StrategicPriority {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Local => "local",
            Self::Subsector => "subsector",
            Self::Sector => "sector",
            Self::CrusadeLevel => "crusade_level",
        }
    }
}

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

// ── Loader ─────────────────────────────────────────────────────────────────────

/// Load `economy.toml`. Missing file → defaults (disabled).
///
/// # Errors
///
/// Returns [`SectorError::ConfigParse`] on malformed TOML and
/// [`SectorError::Io`] on read failure.
pub fn load_economy_file(path: &Utf8Path) -> Result<EconomyConfig, SectorError> {
    if !path.exists() {
        return Ok(EconomyConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: EconomyFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.into_config())
}

// ── Derivation ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> EconomyReport {
    derive_with(
        sector,
        &EconomyConfig {
            enabled: true,
            ..Default::default()
        },
    )
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &EconomyConfig) -> EconomyReport {
    if !cfg.enabled {
        return EconomyReport::default();
    }
    let world_upper: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    let mut worlds: Vec<WorldEconomy> = Vec::with_capacity(world_upper);
    let mut systems: Vec<SystemEconomy> = Vec::with_capacity(sector.systems.len());

    for sys in &sector.systems {
        let mut sys_vec = ResourceVector::default();
        let mut sys_strategic = StrategicOutput::default();
        for w in &sys.worlds {
            let pop_tag = w
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let base = cfg
                .by_world_type
                .get(w.world.world_type.as_ref())
                .cloned()
                .unwrap_or_else(|| default_world_type_vector(&w.world.world_type));
            let tech = cfg
                .by_tech_level
                .get(w.world.tech_level.as_ref())
                .copied()
                .unwrap_or_else(|| default_tech_multiplier(&w.world.tech_level));
            let pop = cfg
                .by_population
                .get(pop_tag.as_ref())
                .copied()
                .unwrap_or_else(|| default_population_multiplier(&pop_tag));
            let vector = base.scale(tech * pop);
            sys_vec = add(&sys_vec, &vector);
            let (strategic_output, supply_resilience) =
                derive_world_strategic_output(w, cfg, &pop_tag);
            sys_strategic.add_assign(&strategic_output);

            let shortages: Vec<String> = RESOURCE_KEYS
                .iter()
                .filter(|k| vector.get(k) <= -20.0)
                .map(|k| (*k).to_string())
                .collect();

            worlds.push(WorldEconomy {
                system_id: sys.id.clone(),
                world_id: w.id.clone(),
                vector,
                strategic_output,
                tithe_status: TitheStatus::Adequate,
                supply_risk: SupplyRisk::Stable,
                strategic_priority: priority_for(&strategic_output),
                supply_resilience,
                stranded: false, // computed below once routes are processed
                shortages,
            });
        }
        let surplus_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) >= 20.0)
            .map(|k| (*k).to_string())
            .collect();
        let shortage_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) <= -20.0)
            .map(|k| (*k).to_string())
            .collect();
        systems.push(SystemEconomy {
            system_id: sys.id.clone(),
            vector: sys_vec,
            strategic_output: sys_strategic,
            tithe_status: TitheStatus::Adequate,
            supply_risk: SupplyRisk::Stable,
            strategic_priority: priority_for(&sys_strategic),
            surplus_resources,
            shortage_resources,
        });
    }

    // Routes: trade volume = endpoint gradient × friction.
    let by_sys: BTreeMap<&str, &SystemEconomy> =
        systems.iter().map(|s| (s.system_id.as_str(), s)).collect();
    let routes: Vec<RouteEconomy> = sector
        .routes
        .iter()
        .map(|r| route_economy(r, &by_sys))
        .collect();

    // TF-P-6: hoisted once and shared with both `derive_dependency_edges` and
    // the stranded check below. Previously each rebuilt their own copy.
    let mut valid_routes_by_sys: BTreeMap<&str, Vec<&crate::sector_model::GeneratedRoute>> =
        BTreeMap::new();
    for r in &sector.routes {
        if r.stability != RouteStability::Perilous {
            valid_routes_by_sys
                .entry(r.from_system_id.as_str())
                .or_default()
                .push(r);
            valid_routes_by_sys
                .entry(r.to_system_id.as_str())
                .or_default()
                .push(r);
        }
    }
    let dependency_edges =
        derive_dependency_edges(sector, &systems, &routes, &by_sys, &valid_routes_by_sys);

    // Stranded check: a world is stranded if it has any deficit ≥ 20 and the
    // system also nets a deficit there *and* no inbound route from a surplus
    // system on that resource exists.
    let mut stranded_world_idx: Vec<usize> = Vec::with_capacity(worlds.len());
    for (idx, we) in worlds.iter().enumerate() {
        let Some(sys) = by_sys.get(we.system_id.as_str()).copied() else {
            continue;
        };
        // Resources where the system itself is in deficit.
        let resource_deficits: Vec<&str> = RESOURCE_KEYS
            .iter()
            .copied()
            .filter(|k| sys.vector.get(k) <= -20.0 && we.vector.get(k) <= -20.0)
            .collect();
        if resource_deficits.is_empty() {
            continue;
        }
        let mut fix = false;
        if let Some(sys_routes) = valid_routes_by_sys.get(sys.system_id.as_str()) {
            for &r in sys_routes {
                let other = if r.from_system_id == sys.system_id {
                    r.to_system_id.as_str()
                } else {
                    r.from_system_id.as_str()
                };
                if let Some(other_sys) = by_sys.get(other) {
                    if resource_deficits
                        .iter()
                        .any(|k| other_sys.vector.get(k) >= 20.0)
                    {
                        fix = true;
                        break;
                    }
                }
            }
        }
        if !fix {
            stranded_world_idx.push(idx);
        }
    }
    for i in stranded_world_idx {
        worlds[i].stranded = true;
    }

    let world_refs: BTreeMap<&str, &GeneratedWorld> = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter().map(|w| (w.id.as_str(), w)))
        .collect();
    let world_counts: BTreeMap<&str, usize> = sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.worlds.len().max(1)))
        .collect();
    let system_refs: BTreeMap<&str, &crate::sector_model::GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    for sy in systems.iter_mut() {
        let count = *world_counts.get(sy.system_id.as_str()).unwrap_or(&1);
        let sys_ref = system_refs.get(sy.system_id.as_str()).copied();
        sy.supply_risk = system_supply_risk(sy, sys_ref, &dependency_edges);
        sy.strategic_priority = priority_for(&sy.strategic_output);
        sy.tithe_status = system_tithe_status(sy, count, sys_ref);
    }
    let sys_risk: BTreeMap<&str, SupplyRisk> = systems
        .iter()
        .map(|s| (s.system_id.as_str(), s.supply_risk))
        .collect();
    for we in worlds.iter_mut() {
        let system_risk = sys_risk
            .get(we.system_id.as_str())
            .copied()
            .unwrap_or(SupplyRisk::Stable);
        we.supply_risk = world_supply_risk(we, system_risk);
        we.strategic_priority = priority_for(&we.strategic_output);
        if let Some(w) = world_refs.get(we.world_id.as_str()).copied() {
            we.tithe_status = world_tithe_status(w, we);
        }
    }

    // Sector totals.
    let mut sector_balance = ResourceVector::default();
    let mut strategic_output = StrategicOutput::default();
    for sy in &systems {
        sector_balance = add(&sector_balance, &sy.vector);
        strategic_output.add_assign(&sy.strategic_output);
    }

    EconomyReport {
        enabled: true,
        worlds,
        systems,
        routes,
        sector_balance,
        strategic_output,
        dependency_edges,
    }
}

fn derive_world_strategic_output(
    w: &GeneratedWorld,
    cfg: &EconomyConfig,
    pop_tag: &str,
) -> (StrategicOutput, f32) {
    let mut out = default_strategic_world_type(&w.world.world_type);
    if let Some(rule) = cfg.resources.world_type.get(w.world.world_type.as_ref()) {
        out = apply_world_type_rule(out, rule);
    }

    let mut multiplier = 1.0_f32;
    let mut resilience = base_resilience(&w.world.world_type);
    for feature in &w.world.notable_features {
        if let Some(rule) = default_feature_rule(feature) {
            apply_feature_rule(&mut out, &rule);
            multiplier *= rule.trade_multiplier.unwrap_or(1.0);
            resilience += rule.supply_resilience.unwrap_or(0.0);
        }
        if let Some(rule) = cfg.resources.notable_feature.get(feature.as_ref()) {
            apply_feature_rule(&mut out, rule);
            multiplier *= rule.trade_multiplier.unwrap_or(1.0);
            resilience += rule.supply_resilience.unwrap_or(0.0);
        }
    }

    let tech = strategic_tech_multiplier(&w.world.tech_level);
    let pop = strategic_population_multiplier(pop_tag, &w.world.population);
    let instability = w
        .stability
        .famine_or_resource_stress
        .max(w.stability.rebellion_risk * 0.75)
        .max(w.stability.corruption * 0.50)
        .max(w.stability.warp_instability * 0.40)
        .max(w.conflict.intensity as f32 * 0.80);
    let stability_factor = (1.0 - instability / 180.0).clamp(0.35, 1.0);
    let control_factor = if w.control.contested {
        0.82
    } else {
        (0.75 + w.control.control_score / 400.0).clamp(0.75, 1.0)
    };

    let output = out
        .scale(multiplier * tech * pop * stability_factor * control_factor)
        .clamp_scores();
    (output, resilience.clamp(0.0, 100.0))
}

fn base_resilience(world_type: &str) -> f32 {
    match world_type {
        "HiveWorld" | "ForgeWorld" | "IndustrialWorld" => 10.0,
        "AgriWorld" | "ExtractiveColony" => 15.0,
        "BastionWorld" => 25.0,
        "FrontierWorld" | "DeathWorld" => 5.0,
        _ => 0.0,
    }
}

fn priority_for(output: &StrategicOutput) -> StrategicPriority {
    match output.weighted_priority_score() {
        s if s < 35.0 => StrategicPriority::Low,
        s if s < 80.0 => StrategicPriority::Local,
        s if s < 150.0 => StrategicPriority::Subsector,
        s if s < 240.0 => StrategicPriority::Sector,
        _ => StrategicPriority::CrusadeLevel,
    }
}

fn strategic_needs_for_world(world_type: &str) -> &'static [&'static str] {
    match world_type {
        "HiveWorld" => &["food"],
        "ForgeWorld" => &["food", "ore", "manpower"],
        "IndustrialWorld" => &["food", "ore", "manpower"],
        "BastionWorld" => &["food", "arms", "manpower"],
        "ShrineWorld" | "PleasureWorld" => &["food"],
        "ResearchStation" => &["food", "manufacturing"],
        "Orbital" | "Worldship" => &["food", "ore"],
        _ => &[],
    }
}

fn derive_dependency_edges<'a>(
    sector: &'a GeneratedSector,
    systems: &'a [SystemEconomy],
    routes: &'a [RouteEconomy],
    by_sys: &BTreeMap<&'a str, &'a SystemEconomy>,
    valid_routes_by_sys: &BTreeMap<&'a str, Vec<&'a crate::sector_model::GeneratedRoute>>,
) -> Vec<DependencyEdge> {
    let by_route: BTreeMap<&str, &RouteEconomy> =
        routes.iter().map(|r| (r.route_id.as_str(), r)).collect();

    let system_refs: BTreeMap<&str, &crate::sector_model::GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut out = Vec::with_capacity(systems.len() * 4);

    for consumer in systems {
        let Some(consumer_sys_ref) = system_refs.get(consumer.system_id.as_str()) else {
            continue;
        };
        let needs: BTreeSet<&str> = consumer_sys_ref
            .worlds
            .iter()
            .flat_map(|w| {
                strategic_needs_for_world(&w.world.world_type)
                    .iter()
                    .copied()
            })
            .collect();

        for resource in needs {
            if consumer.strategic_output.get(resource) >= 30.0 {
                continue;
            }
            let mut best: Option<DependencyEdge> = None;
            if let Some(consumer_routes) = valid_routes_by_sys.get(consumer.system_id.as_str()) {
                for &r in consumer_routes {
                    let supplier_id = if r.from_system_id == consumer.system_id {
                        r.to_system_id.as_str()
                    } else {
                        r.from_system_id.as_str()
                    };
                    let Some(supplier) = by_sys.get(supplier_id).copied() else {
                        continue;
                    };
                    let supply = supplier.strategic_output.get(resource);
                    if supply < 35.0 {
                        continue;
                    }
                    let friction = by_route
                        .get(r.id.as_str())
                        .map(|e| e.friction)
                        .unwrap_or_else(|| friction_for(r));
                    let score = supply * friction / (r.distance.max(1) as f32).sqrt();
                    let edge = DependencyEdge {
                        from_system_id: supplier.system_id.clone(),
                        to_system_id: consumer.system_id.clone(),
                        resource: resource.to_string(),
                        route_id: Some(r.id.clone()),
                        score,
                        risk: import_risk(r, friction, score),
                    };
                    if best.as_ref().map(|b| edge.score > b.score).unwrap_or(true) {
                        best = Some(edge);
                    }
                }
            }
            if let Some(edge) = best {
                out.push(edge);
            }
        }
    }
    out.sort_by(|a, b| {
        a.to_system_id
            .cmp(&b.to_system_id)
            .then(a.resource.cmp(&b.resource))
            .then(a.from_system_id.cmp(&b.from_system_id))
    });
    out
}

fn import_risk(r: &GeneratedRoute, friction: f32, score: f32) -> SupplyRisk {
    if score < 10.0 || r.stability == RouteStability::Perilous {
        SupplyRisk::Collapsing
    } else if score < 20.0 || friction < 0.35 || r.stability == RouteStability::Hazardous {
        SupplyRisk::Disrupted
    } else if score < 35.0 || friction < 0.70 || r.stability == RouteStability::Unstable {
        SupplyRisk::Vulnerable
    } else {
        SupplyRisk::Stable
    }
}

fn system_supply_risk(
    sy: &SystemEconomy,
    sys_ref: Option<&crate::sector_model::GeneratedSystem>,
    deps: &[DependencyEdge],
) -> SupplyRisk {
    let mut risk = if sy.shortage_resources.len() >= 2 {
        SupplyRisk::Disrupted
    } else if sy.shortage_resources.is_empty() {
        SupplyRisk::Stable
    } else {
        SupplyRisk::Vulnerable
    };
    if sys_ref.map(|s| s.blockade.under_blockade).unwrap_or(false) {
        risk = risk.max(SupplyRisk::Disrupted);
    }
    if let Some(sys) = sys_ref {
        for world in &sys.worlds {
            for resource in strategic_needs_for_world(&world.world.world_type) {
                if sy.strategic_output.get(resource) >= 30.0 {
                    continue;
                }
                let incoming: Vec<&DependencyEdge> = deps
                    .iter()
                    .filter(|e| e.to_system_id == sy.system_id && e.resource == *resource)
                    .collect();
                if incoming.is_empty() {
                    risk = risk.max(if *resource == "food" {
                        SupplyRisk::Collapsing
                    } else {
                        SupplyRisk::Disrupted
                    });
                } else if let Some(best) = incoming.iter().map(|e| e.risk).min() {
                    risk = risk.max(best);
                }
            }
        }
    }
    risk
}

fn world_supply_risk(we: &WorldEconomy, system_risk: SupplyRisk) -> SupplyRisk {
    let base = if we.stranded || we.shortages.len() >= 2 {
        SupplyRisk::Collapsing
    } else if !we.shortages.is_empty() {
        SupplyRisk::Disrupted
    } else {
        SupplyRisk::Stable
    };
    let mut risk = base.max(system_risk);
    if we.supply_resilience >= 30.0 {
        risk = lower_risk(risk);
    }
    risk
}

fn lower_risk(risk: SupplyRisk) -> SupplyRisk {
    match risk {
        SupplyRisk::Collapsing => SupplyRisk::Disrupted,
        SupplyRisk::Disrupted => SupplyRisk::Vulnerable,
        SupplyRisk::Vulnerable => SupplyRisk::Stable,
        SupplyRisk::Stable => SupplyRisk::Stable,
    }
}

fn world_tithe_status(w: &GeneratedWorld, we: &WorldEconomy) -> TitheStatus {
    let stress = (w.conflict.intensity as f32).mul_add(
        0.55,
        w.stability.corruption.mul_add(
            0.25,
            w.stability
                .famine_or_resource_stress
                .mul_add(0.5, w.stability.rebellion_risk * 0.35),
        ),
    ) + if w.control.contested { 20.0 } else { 0.0 }
        + if we.stranded { 30.0 } else { 0.0 };
    let reliability = we.strategic_output.weighted_priority_score() - stress;
    if w.control.hidden_master.is_some() && reliability >= 25.0 {
        return TitheStatus::Falsified;
    }
    tithe_from_reliability(reliability)
}

fn system_tithe_status(
    sy: &SystemEconomy,
    world_count: usize,
    sys_ref: Option<&crate::sector_model::GeneratedSystem>,
) -> TitheStatus {
    let avg_score = sy.strategic_output.weighted_priority_score() / world_count.max(1) as f32;
    let stress = sys_ref
        .map(|s| {
            (s.conflict.intensity as f32).mul_add(
                0.5,
                s.stability
                    .famine_or_resource_stress
                    .mul_add(0.4, s.stability.rebellion_risk * 0.25),
            ) + if s.blockade.under_blockade { 25.0 } else { 0.0 }
        })
        .unwrap_or(0.0);
    tithe_from_reliability(avg_score - stress)
}

fn tithe_from_reliability(reliability: f32) -> TitheStatus {
    match reliability {
        r if r >= 160.0 => TitheStatus::Surplus,
        r if r >= 75.0 => TitheStatus::Adequate,
        r if r >= 40.0 => TitheStatus::Strained,
        r if r >= 15.0 => TitheStatus::Delinquent,
        _ => TitheStatus::Failed,
    }
}

fn route_economy(r: &GeneratedRoute, by_sys: &BTreeMap<&str, &SystemEconomy>) -> RouteEconomy {
    let a = by_sys.get(r.from_system_id.as_str()).copied();
    let b = by_sys.get(r.to_system_id.as_str()).copied();
    let gradient: f32 = match (a, b) {
        (Some(a), Some(b)) => {
            let legacy = RESOURCE_KEYS
                .iter()
                .map(|k| (a.vector.get(k) - b.vector.get(k)).abs())
                .sum::<f32>()
                / RESOURCE_KEYS.len() as f32;
            let strategic = STRATEGIC_RESOURCE_KEYS
                .iter()
                .map(|k| (a.strategic_output.get(k) - b.strategic_output.get(k)).abs())
                .sum::<f32>()
                / STRATEGIC_RESOURCE_KEYS.len() as f32;
            strategic.mul_add(0.5, legacy)
        }
        _ => 0.0,
    };
    let friction = friction_for(r);
    let distance_falloff = 1.0 / (r.distance.max(1) as f32);
    RouteEconomy {
        route_id: r.id.clone(),
        from_system_id: r.from_system_id.clone(),
        to_system_id: r.to_system_id.clone(),
        volume: (gradient * friction * distance_falloff).max(0.0),
        friction,
    }
}

fn friction_for(r: &GeneratedRoute) -> f32 {
    let mut f = match r.stability {
        RouteStability::Stable => 1.0,
        RouteStability::Unstable => 0.75,
        RouteStability::Hazardous => 0.45,
        RouteStability::Perilous => 0.10,
    };
    let max_piracy: f32 = r.controls.iter().map(|c| c.piracy).fold(0.0_f32, f32::max);
    let max_interdiction: f32 = r
        .controls
        .iter()
        .map(|c| c.interdiction)
        .fold(0.0_f32, f32::max);
    let max_patrol: f32 = r.controls.iter().map(|c| c.patrol).fold(0.0_f32, f32::max);
    f *= 1.0 - (max_piracy / 200.0).clamp(0.0, 0.5);
    f *= 1.0 - (max_interdiction / 200.0).clamp(0.0, 0.6);
    f *= 1.0 + (max_patrol / 400.0).clamp(0.0, 0.25);
    f.clamp(0.0, 1.5)
}

fn add(a: &ResourceVector, b: &ResourceVector) -> ResourceVector {
    ResourceVector {
        ore: a.ore + b.ore,
        promethium: a.promethium + b.promethium,
        foodstuffs: a.foodstuffs + b.foodstuffs,
        manufactured: a.manufactured + b.manufactured,
        archeotech: a.archeotech + b.archeotech,
        recruits: a.recruits + b.recruits,
    }
}

// ── Markdown ───────────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(sector_id: &str, report: &EconomyReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Economy — {sector_id}");
    if !report.enabled {
        let _ = writeln!(
            s,
            "\n_Economy derivation disabled. Enable in `economy.toml`._"
        );
        return s;
    }
    let _ = writeln!(s, "\n## Sector balance");
    let _ = writeln!(s, "| Resource | Net |");
    let _ = writeln!(s, "|----------|-----|");
    for k in RESOURCE_KEYS {
        let _ = writeln!(s, "| {k} | {:.1} |", report.sector_balance.get(k));
    }

    let _ = writeln!(s, "\n## Strategic output");
    let _ = writeln!(s, "| Output | Score |");
    let _ = writeln!(s, "|--------|------:|");
    for k in STRATEGIC_RESOURCE_KEYS {
        let _ = writeln!(s, "| {k} | {:.1} |", report.strategic_output.get(k));
    }

    let _ = writeln!(s, "\n## Systems");
    let _ = writeln!(
        s,
        "| System | Tithe | Supply | Priority | Surplus | Shortage |"
    );
    let _ = writeln!(
        s,
        "|--------|-------|--------|----------|---------|----------|"
    );
    for sy in &report.systems {
        let _ = writeln!(
            s,
            "| {} | {:?} | {:?} | {:?} | {} | {} |",
            sy.system_id,
            sy.tithe_status,
            sy.supply_risk,
            sy.strategic_priority,
            if sy.surplus_resources.is_empty() {
                "—".to_string()
            } else {
                sy.surplus_resources.join(", ")
            },
            if sy.shortage_resources.is_empty() {
                "—".to_string()
            } else {
                sy.shortage_resources.join(", ")
            },
        );
    }

    if !report.dependency_edges.is_empty() {
        let _ = writeln!(s, "\n## Dependency edges");
        let _ = writeln!(
            s,
            "| Supplier | Consumer | Resource | Route | Risk | Score |"
        );
        let _ = writeln!(
            s,
            "|----------|----------|----------|-------|------|------:|"
        );
        for e in &report.dependency_edges {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {:?} | {:.1} |",
                e.from_system_id,
                e.to_system_id,
                e.resource,
                e.route_id.as_deref().unwrap_or("—"),
                e.risk,
                e.score
            );
        }
    }

    let mut stranded_iter = report.worlds.iter().filter(|w| w.stranded).peekable();
    if stranded_iter.peek().is_some() {
        let _ = writeln!(s, "\n## Stranded worlds");
        for w in stranded_iter {
            let _ = writeln!(
                s,
                "- `{}` in `{}` — shortages: {}",
                w.world_id,
                w.system_id,
                if w.shortages.is_empty() {
                    "(systemic)".into()
                } else {
                    w.shortages.join(", ")
                }
            );
        }
    }

    // Top 10 routes by volume.
    let mut top: Vec<&RouteEconomy> = report.routes.iter().collect();
    top.sort_by(|a, b| {
        b.volume
            .partial_cmp(&a.volume)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let _ = writeln!(s, "\n## Top trade lanes");
    for r in top.iter().take(10) {
        let _ = writeln!(
            s,
            "- {} → {} — volume {:.1} (friction {:.2})",
            r.from_system_id, r.to_system_id, r.volume, r.friction
        );
    }
    s
}

/// Write `economy.md` + `economy.json` into a dir.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    sector_id: &str,
    report: &EconomyReport,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        "economy",
        &render_markdown(sector_id, report),
        report,
    )
}

/// §12 stability nudge: increase `famine_or_resource_stress` on every world
/// that is stranded on foodstuffs. Read-only and bounded; no other stability
/// fields are touched, so the conflict tick does not oscillate.
pub fn apply_stability_nudge(report: &EconomyReport, sector: &mut GeneratedSector) {
    if !report.enabled {
        return;
    }
    let stranded: BTreeMap<&str, bool> = report
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| (w.world_id.as_str(), true))
        .collect();
    for sys in sector.systems.iter_mut() {
        for w in sys.worlds.iter_mut() {
            if stranded.contains_key(w.id.as_str()) {
                let nudged = (w.stability.famine_or_resource_stress + 20.0).clamp(0.0, 100.0);
                w.stability.famine_or_resource_stress = nudged;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedRoute, GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest,
        HexCoord, RouteStability, RouteType, SystemControlSummary, WorldControlSummary, WorldDto,
    };
    use std::collections::BTreeMap as Map;

    fn world(id: &str, world_type: &str, tech: &str, pop_tag: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: id.into(),
            index: 1,
            name: id.into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "G".into(),
                star_colour_code: "G".into(),
                world_type: world_type.into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Standard".into(),
                population: "Standard".into(),
                tech_level: tech.into(),
                government: "ImperialCommander".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![pop_tag.into()],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
            intel: Default::default(),
        }
    }

    fn sys(id: &str, worlds: Vec<GeneratedWorld>) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            kind: crate::sector_model::SystemKind::Star,
            coord: HexCoord { q: 0, r: 0 },
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds,
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    fn sector(systems: Vec<GeneratedSystem>) -> GeneratedSector {
        GeneratedSector {
            id: "econ-test".into(),
            title: "Econ".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems,
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                base_seed: None,
                candidate_index: None,
                constraints_digest: None,
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: vec![].into(),
            economy: Default::default(),
            chronicle: Default::default(),
            id_history: Default::default(),
        }
    }

    fn route(id: &str, from: &str, to: &str) -> GeneratedRoute {
        GeneratedRoute {
            id: id.into(),
            from_system_id: from.into(),
            to_system_id: to.into(),
            distance: 1,
            route_type: RouteType::StableWarpLane,
            stability: RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        }
    }

    #[test]
    fn disabled_yields_empty() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                "HiveWorld",
                "Imperial",
                "population:massive",
            )],
        )]);
        let r = derive_with(&s, &EconomyConfig::default());
        assert!(!r.enabled);
        assert!(r.worlds.is_empty());
    }

    #[test]
    fn hive_world_food_deficit() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                "HiveWorld",
                "Imperial",
                "population:massive",
            )],
        )]);
        let r = derive(&s);
        assert!(r.enabled);
        assert!(r.worlds[0].vector.foodstuffs < 0.0);
    }

    #[test]
    fn deterministic() {
        let s = sector(vec![
            sys(
                "sys-0001",
                vec![world("a", "HiveWorld", "Imperial", "population:massive")],
            ),
            sys(
                "sys-0002",
                vec![world("b", "AgriWorld", "Imperial", "population:standard")],
            ),
        ]);
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn strategic_dependency_edges_link_food_supplier_to_hive() {
        let mut s = sector(vec![
            sys(
                "sys-0001",
                vec![world(
                    "agri",
                    "AgriWorld",
                    "Standard",
                    "population:standard",
                )],
            ),
            sys(
                "sys-0002",
                vec![world("hive", "HiveWorld", "Standard", "population:massive")],
            ),
        ]);
        s.routes = vec![route("route-1", "sys-0001", "sys-0002")];
        let r = derive(&s);
        assert!(r.strategic_output.food > 0.0);
        assert!(r.dependency_edges.iter().any(|e| {
            e.from_system_id == "sys-0001" && e.to_system_id == "sys-0002" && e.resource == "food"
        }));
        let hive = r.worlds.iter().find(|w| w.world_id == "hive").unwrap();
        assert!(hive.strategic_output.manpower > 0.0);
    }

    #[test]
    fn top_level_resources_block_merges_into_config() {
        let parsed: EconomyFile = toml::from_str(
            r#"
            [economy]
            enabled = true

            [resources.world_type.AgriWorld]
            food = 42
            manpower = 7
            "#,
        )
        .unwrap();
        let cfg = parsed.into_config();
        let rule = cfg.resources.world_type.get("AgriWorld").unwrap();
        assert_eq!(rule.food, Some(42.0));
        assert_eq!(rule.manpower, Some(7.0));
    }
}
