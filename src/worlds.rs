/// World parameter types. The enum set in this module is the authoritative
/// taxonomy; project data is loaded from `worlds.toml` via [`load_worlds_data`].
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Worlds-data loading (`WorldError`, `WorldsLoad`, `load_worlds_data`) lives in
// the `worlds_toml` IO module; re-exported here so `crate::worlds::*` paths and
// this module's `[load_worlds_data]` doc-link stay stable.
pub use crate::worlds_toml::{load_worlds_data, WorldError, WorldsLoad};

// ── Key-tab enum types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StarColour {
    BlueHypergiant, // O
    BlueWhite,      // B
    White,          // A
    YellowWhite,    // F
    Yellow,         // G
    OrangeDwarf,    // K
    RedDwarf,       // M
}

impl StarColour {
    pub fn code(self) -> &'static str {
        match self {
            Self::BlueHypergiant => "O",
            Self::BlueWhite => "B",
            Self::White => "A",
            Self::YellowWhite => "F",
            Self::Yellow => "G",
            Self::OrangeDwarf => "K",
            Self::RedDwarf => "M",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::BlueHypergiant => "blue hypergiant",
            Self::BlueWhite => "blue-white",
            Self::White => "white",
            Self::YellowWhite => "yellow-white",
            Self::Yellow => "yellow",
            Self::OrangeDwarf => "orange dwarf",
            Self::RedDwarf => "red dwarf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WorldType {
    AgriWorld,
    Asteroid,
    BastionWorld,
    DeathWorld,
    DeadWorld,
    ExtractiveColony,
    FeralWorld,
    FeudalWorld,
    ForgeWorld,
    FrontierWorld,
    HiveWorld,
    ImperialWorld,
    IndustrialWorld,
    Orbital,
    PenalWorld,
    PlanetaryDump,
    PlanetaryMonument,
    PleasureWorld,
    ResearchStation,
    ShrineWorld,
    TombWorld,
    WarpLostWorld,
    Worldship,
    XenosWorld,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Atmosphere {
    Airless,
    Breathable,
    Corrosive,
    Exotic,
    Thin,
    Tainted,
    Toxic,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Temperature {
    Freezing,
    Cold,
    Temperate,
    Hot,
    Boiling,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Biosphere {
    Nonexistent,
    Minimal,
    Thriving,
    Poisoned,
    XenoHybrid,
    XenoDominance,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Population {
    Uninhabited,
    Minimal,
    LightlyPopulated,
    SoleSettlement,
    DenselyPopulated,
    ExtremelyDense,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TechLevel {
    Primitive,
    Low,
    Standard,
    High,
    XenoHybrid,
    Archaeotech,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Government {
    BalkanizedLocalFactions,
    ChaosCult,
    ClansTribes,
    Communards,
    CorruptAristocrats,
    Demagogue,
    EcclesiarchicalAppointee,
    ElitistTyrant,
    ExploratorAuthority,
    GuildsCombine,
    Hereteks,
    HereticalImperialCult,
    InfractionistGang,
    LocalReligiousAuthorities,
    LoyalistMassMovement,
    MagistrateCouncil,
    MechanicusForgeLord,
    Megacorporations,
    MilitaryGovernor,
    None,
    PopulistTyrant,
    PuppetGovernment,
    RevolutionaryJunta,
    RogueTraderDynasty,
    ShadowyPsykerCabal,
    TraditionalOligarchy,
    TraditionalistAristocracy,
    Warlords,
    WarriorAristocracy,
    XenosOverlords,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NotableFeature {
    Abhumans,
    AlteredHumans,
    AncientArchive,
    AncientTombs,
    ArchaeotechRuins,
    BlindingMists,
    CelestialPhenomena,
    ChaosCultists,
    CivilWar,
    ColdWar,
    CrumblingArcologies,
    DaemonicCorruption,
    DangerousWildlife,
    DesertWorld,
    DeviantReligion,
    EugenicCult,
    ExtremeEnvironment,
    FactionalFragmentation,
    FailedParadise,
    FlyingCities,
    ForbiddenTech,
    ForeignControl,
    FreakGeology,
    FreakWeather,
    Freeport,
    FriendlyXenos,
    FrozenWorld,
    GoldRush,
    GreatWork,
    HeavyIndustry,
    HeavyMining,
    Hereteks,
    HolyWar,
    HostileBiosphere,
    HostileXenos,
    ImpendingDoom,
    ImperialKnights,
    ImportantShrine,
    InquisitionOutpost,
    JungleWorld,
    Libertines,
    LocalSpecialty,
    LocalTech,
    MajorSpaceyard,
    MartialLaw,
    MassPanic,
    MinimalContact,
    Missionaries,
    MutantHordes,
    NavalBlockade,
    NavalOutpost,
    NavigatorHouse,
    NomadicCities,
    NotableLocal,
    OceanWorld,
    OutOfContact,
    Pandemic,
    PilgrimageSite,
    PocketEmpire,
    PoliceState,
    PopularUprising,
    PowerfulCriminals,
    PowerfulNobles,
    PrimitiveXenos,
    Prosperous,
    PsykerAcademy,
    PsykerCult,
    Quarantined,
    Radioactive,
    RecentlyRediscovered,
    ScholaProgenium,
    SeagoingCities,
    SealedMenace,
    SecretMasters,
    Sectarians,
    SeismicInstability,
    Separatists,
    SilicaAnimus,
    SoleSuppliers,
    SororitasConvent,
    SpaceHulks,
    StrangeCustoms,
    StrangeHatred,
    SubsectorHegemon,
    TechPriestCult,
    TestSite,
    TheSilentTrade,
    TradeHub,
    AdministrativeHub,
    UnmappedWastes,
    VastFortresses,
    VerdantEcology,
    WarZone,
    WarpPhenomena,
    WitchHunt,
    XenoRuins,
    Xenophiles,
    Xenophobes,
    XenosInfiltrators,
    Zombies,
}

// ── Generator-template row types ─────────────────────────────────────────────

/// A single generation row from the Generator Template sheet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationRow {
    /// Star colour (A)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star_colour: Option<StarColour>,
    /// World type (B)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_type: Option<WorldType>,
    /// Atmosphere (C)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atmosphere: Option<Atmosphere>,
    /// Temperature (D)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,
    /// Biosphere (E)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biosphere: Option<Biosphere>,
    /// Population (F)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub population: Option<Population>,
    /// Tech level (G)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "tech_level"
    )]
    pub tech: Option<TechLevel>,
    /// Government (H)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub government: Option<Government>,
    /// Notable feature (I)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notable_feature: Option<NotableFeature>,
    /// Counter count from COUNTIF formula (col J, stored as integer in the sheet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<usize>,
    /// Weight formula result — numeric weight for random selection (col K).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

/// A fully resolved world ready for generation.
#[derive(Debug, Clone)]
pub struct World {
    pub star_colour: StarColour,
    pub world_type: WorldType,
    pub atmosphere: Atmosphere,
    pub temperature: Temperature,
    pub biosphere: Biosphere,
    pub population: Population,
    pub tech_level: TechLevel,
    pub government: Government,
    pub notable_features: Vec<NotableFeature>,
}

/// A single world within a solar system.
#[derive(Debug, Clone)]
pub struct WorldEntry {
    /// System identifier / seed (col L in template).
    pub system_seed: Option<f64>,
    /// Star spectral type (col M).
    pub star_type: Option<String>,
    /// Human-readable location name (col N).
    pub location_name: Option<String>,
    /// The resolved world parameters.
    pub world: World,
    /// Three notable features assigned to this world.
    pub features: [Option<NotableFeature>; 3],
}

/// A solar system containing multiple worlds and a star designation.
#[derive(Debug, Clone)]
pub struct System {
    /// System index (col K).
    pub system_index: Option<f64>,
    /// Star type from the template.
    pub star_type: Option<String>,
    /// Named location.
    pub location_name: Option<String>,
    /// Orbiting worlds.
    pub worlds: Vec<WorldEntry>,
}

// ── Key-tab parsing ──────────────────────────────────────────────────────────

/// Reference tables keyed by column index from the Key sheet (1-based).
#[derive(Debug, Default, Clone)]
pub struct KeyTables {
    /// Column A: star colour codes → enum variants.
    pub star_colours: HashMap<String, StarColour>,
    /// Column B: world type strings → enums.
    pub world_types: HashMap<String, WorldType>,
    /// Column C: atmosphere strings → enums.
    pub atmospheres: HashMap<String, Atmosphere>,
    /// Column D: temperature strings → enums.
    pub temperatures: HashMap<String, Temperature>,
    /// Column E: biosphere strings → enums.
    pub biospheres: HashMap<String, Biosphere>,
    /// Column F: population strings → enums.
    pub populations: HashMap<String, Population>,
    /// Column G: tech level strings → enums.
    pub tech_levels: HashMap<String, TechLevel>,
    /// Column H: government strings → enums.
    pub governments: HashMap<String, Government>,
    /// Column I: notable feature strings → enums (no mapping, just display names).
    pub notable_features: Vec<String>,
}

// ── FromStr implementations for Key-tab enums ────────────────────────────────
//
// Each parser accepts exactly the enum's canonical `display_name()` strings
// (for `StarColour` that is the spectral `code()`), found via `VARIANTS`.

impl std::str::FromStr for StarColour {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .copied()
            .ok_or(())
    }
}

impl std::str::FromStr for WorldType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for WorldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Atmosphere {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for Atmosphere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Temperature {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Biosphere {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for Biosphere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Population {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for Population {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for TechLevel {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for TechLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for Government {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl std::fmt::Display for Government {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::str::FromStr for NotableFeature {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .find(|v| v.display_name() == s)
            .cloned()
            .ok_or(())
    }
}

impl AsRef<str> for NotableFeature {
    fn as_ref(&self) -> &str {
        match self {
            Self::Abhumans => "Abhumans",
            Self::AlteredHumans => "AlteredHumans",
            Self::AncientArchive => "AncientArchive",
            Self::AncientTombs => "AncientTombs",
            Self::ArchaeotechRuins => "ArchaeotechRuins",
            Self::BlindingMists => "BlindingMists",
            Self::CelestialPhenomena => "CelestialPhenomena",
            Self::ChaosCultists => "ChaosCultists",
            Self::CivilWar => "CivilWar",
            Self::ColdWar => "ColdWar",
            Self::CrumblingArcologies => "CrumblingArcologies",
            Self::DaemonicCorruption => "DaemonicCorruption",
            Self::DangerousWildlife => "DangerousWildlife",
            Self::DesertWorld => "DesertWorld",
            Self::DeviantReligion => "DeviantReligion",
            Self::EugenicCult => "EugenicCult",
            Self::ExtremeEnvironment => "ExtremeEnvironment",
            Self::FactionalFragmentation => "FactionalFragmentation",
            Self::FailedParadise => "FailedParadise",
            Self::FlyingCities => "FlyingCities",
            Self::ForbiddenTech => "ForbiddenTech",
            Self::ForeignControl => "ForeignControl",
            Self::FreakGeology => "FreakGeology",
            Self::FreakWeather => "FreakWeather",
            Self::Freeport => "Freeport",
            Self::FriendlyXenos => "FriendlyXenos",
            Self::FrozenWorld => "FrozenWorld",
            Self::GoldRush => "GoldRush",
            Self::GreatWork => "GreatWork",
            Self::HeavyIndustry => "HeavyIndustry",
            Self::HeavyMining => "HeavyMining",
            Self::Hereteks => "Hereteks",
            Self::HolyWar => "HolyWar",
            Self::HostileBiosphere => "HostileBiosphere",
            Self::HostileXenos => "HostileXenos",
            Self::ImpendingDoom => "ImpendingDoom",
            Self::ImperialKnights => "ImperialKnights",
            Self::ImportantShrine => "ImportantShrine",
            Self::InquisitionOutpost => "InquisitionOutpost",
            Self::JungleWorld => "JungleWorld",
            Self::Libertines => "Libertines",
            Self::LocalSpecialty => "LocalSpecialty",
            Self::LocalTech => "LocalTech",
            Self::MajorSpaceyard => "MajorSpaceyard",
            Self::MartialLaw => "MartialLaw",
            Self::MassPanic => "MassPanic",
            Self::MinimalContact => "MinimalContact",
            Self::Missionaries => "Missionaries",
            Self::MutantHordes => "MutantHordes",
            Self::NavalBlockade => "NavalBlockade",
            Self::NavalOutpost => "NavalOutpost",
            Self::NavigatorHouse => "NavigatorHouse",
            Self::NomadicCities => "NomadicCities",
            Self::NotableLocal => "NotableLocal",
            Self::OceanWorld => "OceanWorld",
            Self::OutOfContact => "OutOfContact",
            Self::Pandemic => "Pandemic",
            Self::PilgrimageSite => "PilgrimageSite",
            Self::PocketEmpire => "PocketEmpire",
            Self::PoliceState => "PoliceState",
            Self::PopularUprising => "PopularUprising",
            Self::PowerfulCriminals => "PowerfulCriminals",
            Self::PowerfulNobles => "PowerfulNobles",
            Self::PrimitiveXenos => "PrimitiveXenos",
            Self::Prosperous => "Prosperous",
            Self::PsykerAcademy => "PsykerAcademy",
            Self::PsykerCult => "PsykerCult",
            Self::Quarantined => "Quarantined",
            Self::Radioactive => "Radioactive",
            Self::RecentlyRediscovered => "RecentlyRediscovered",
            Self::ScholaProgenium => "ScholaProgenium",
            Self::SeagoingCities => "SeagoingCities",
            Self::SealedMenace => "SealedMenace",
            Self::SecretMasters => "SecretMasters",
            Self::Sectarians => "Sectarians",
            Self::SeismicInstability => "SeismicInstability",
            Self::Separatists => "Separatists",
            Self::SilicaAnimus => "SilicaAnimus",
            Self::SoleSuppliers => "SoleSuppliers",
            Self::SororitasConvent => "SororitasConvent",
            Self::SpaceHulks => "SpaceHulks",
            Self::StrangeCustoms => "StrangeCustoms",
            Self::StrangeHatred => "StrangeHatred",
            Self::SubsectorHegemon => "SubsectorHegemon",
            Self::TechPriestCult => "TechPriestCult",
            Self::TestSite => "TestSite",
            Self::TheSilentTrade => "TheSilentTrade",
            Self::TradeHub => "TradeHub",
            Self::AdministrativeHub => "AdministrativeHub",
            Self::UnmappedWastes => "UnmappedWastes",
            Self::VastFortresses => "VastFortresses",
            Self::VerdantEcology => "VerdantEcology",
            Self::WarZone => "WarZone",
            Self::WarpPhenomena => "WarpPhenomena",
            Self::WitchHunt => "WitchHunt",
            Self::XenoRuins => "XenoRuins",
            Self::Xenophiles => "Xenophiles",
            Self::Xenophobes => "Xenophobes",
            Self::XenosInfiltrators => "XenosInfiltrators",
            Self::Zombies => "Zombies",
        }
    }
}

impl std::fmt::Display for NotableFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

// ── Enum-authoritative variants + canonical display names ────────────────────
//
// `VARIANTS` exposes the compile-time-complete variant set for every enum.
// `display_name` returns the canonical string form (matches `FromStr` keys,
// suitable for GUI dropdown labels).

impl StarColour {
    pub const VARIANTS: &'static [Self] = &[
        Self::BlueHypergiant,
        Self::BlueWhite,
        Self::White,
        Self::YellowWhite,
        Self::Yellow,
        Self::OrangeDwarf,
        Self::RedDwarf,
    ];
    pub fn display_name(self) -> &'static str {
        self.code()
    }
}

impl WorldType {
    pub const VARIANTS: &'static [Self] = &[
        Self::AgriWorld,
        Self::Asteroid,
        Self::BastionWorld,
        Self::DeathWorld,
        Self::DeadWorld,
        Self::ExtractiveColony,
        Self::FeralWorld,
        Self::FeudalWorld,
        Self::ForgeWorld,
        Self::FrontierWorld,
        Self::HiveWorld,
        Self::ImperialWorld,
        Self::IndustrialWorld,
        Self::Orbital,
        Self::PenalWorld,
        Self::PlanetaryDump,
        Self::PlanetaryMonument,
        Self::PleasureWorld,
        Self::ResearchStation,
        Self::ShrineWorld,
        Self::TombWorld,
        Self::WarpLostWorld,
        Self::Worldship,
        Self::XenosWorld,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AgriWorld => "Agri-World",
            Self::Asteroid => "Asteroid",
            Self::BastionWorld => "Bastion World",
            Self::DeathWorld => "Death World",
            Self::DeadWorld => "Dead World",
            Self::ExtractiveColony => "Extractive Colony",
            Self::FeralWorld => "Feral World",
            Self::FeudalWorld => "Feudal World",
            Self::ForgeWorld => "Forge World",
            Self::FrontierWorld => "Frontier World",
            Self::HiveWorld => "Hive World",
            Self::ImperialWorld => "Imperial World",
            Self::IndustrialWorld => "Industrial World",
            Self::Orbital => "Orbital",
            Self::PenalWorld => "Penal World",
            Self::PlanetaryDump => "Planetary Dump",
            Self::PlanetaryMonument => "Planetary Monument",
            Self::PleasureWorld => "Pleasure World",
            Self::ResearchStation => "Research Station",
            Self::ShrineWorld => "Shrine World",
            Self::TombWorld => "Tomb World",
            Self::WarpLostWorld => "Warp-Lost World",
            Self::Worldship => "Worldship",
            Self::XenosWorld => "Xenos World",
        }
    }
}

impl Atmosphere {
    pub const VARIANTS: &'static [Self] = &[
        Self::Airless,
        Self::Breathable,
        Self::Corrosive,
        Self::Exotic,
        Self::Thin,
        Self::Tainted,
        Self::Toxic,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Airless => "Airless",
            Self::Breathable => "Breathable",
            Self::Corrosive => "Corrosive",
            Self::Exotic => "Exotic",
            Self::Thin => "Thin",
            Self::Tainted => "Tainted",
            Self::Toxic => "Toxic",
        }
    }
}

impl Temperature {
    pub const VARIANTS: &'static [Self] = &[
        Self::Freezing,
        Self::Cold,
        Self::Temperate,
        Self::Hot,
        Self::Boiling,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Freezing => "Freezing",
            Self::Cold => "Cold",
            Self::Temperate => "Temperate",
            Self::Hot => "Hot",
            Self::Boiling => "Boiling",
        }
    }
}

impl Biosphere {
    pub const VARIANTS: &'static [Self] = &[
        Self::Nonexistent,
        Self::Minimal,
        Self::Thriving,
        Self::Poisoned,
        Self::XenoHybrid,
        Self::XenoDominance,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Nonexistent => "Nonexistent",
            Self::Minimal => "Minimal",
            Self::Thriving => "Thriving",
            Self::Poisoned => "Poisoned",
            Self::XenoHybrid => "Xeno Hybrid",
            Self::XenoDominance => "Xeno Dominance",
        }
    }
}

impl Population {
    pub const VARIANTS: &'static [Self] = &[
        Self::Uninhabited,
        Self::Minimal,
        Self::LightlyPopulated,
        Self::SoleSettlement,
        Self::DenselyPopulated,
        Self::ExtremelyDense,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Uninhabited => "Uninhabited",
            Self::Minimal => "Minimal",
            Self::LightlyPopulated => "Lightly Populated",
            Self::SoleSettlement => "Sole Settlement",
            Self::DenselyPopulated => "Densely Populated",
            Self::ExtremelyDense => "Extremely Dense",
        }
    }
}

impl TechLevel {
    pub const VARIANTS: &'static [Self] = &[
        Self::Primitive,
        Self::Low,
        Self::Standard,
        Self::High,
        Self::XenoHybrid,
        Self::Archaeotech,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Primitive => "Primitive",
            Self::Low => "Low",
            Self::Standard => "Standard",
            Self::High => "High",
            Self::XenoHybrid => "Xeno Hybrid",
            Self::Archaeotech => "Archaeotech",
        }
    }
}

impl Government {
    pub const VARIANTS: &'static [Self] = &[
        Self::BalkanizedLocalFactions,
        Self::ChaosCult,
        Self::ClansTribes,
        Self::Communards,
        Self::CorruptAristocrats,
        Self::Demagogue,
        Self::EcclesiarchicalAppointee,
        Self::ElitistTyrant,
        Self::ExploratorAuthority,
        Self::GuildsCombine,
        Self::Hereteks,
        Self::HereticalImperialCult,
        Self::InfractionistGang,
        Self::LocalReligiousAuthorities,
        Self::LoyalistMassMovement,
        Self::MagistrateCouncil,
        Self::MechanicusForgeLord,
        Self::Megacorporations,
        Self::MilitaryGovernor,
        Self::None,
        Self::PopulistTyrant,
        Self::PuppetGovernment,
        Self::RevolutionaryJunta,
        Self::RogueTraderDynasty,
        Self::ShadowyPsykerCabal,
        Self::TraditionalOligarchy,
        Self::TraditionalistAristocracy,
        Self::Warlords,
        Self::WarriorAristocracy,
        Self::XenosOverlords,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::BalkanizedLocalFactions => "Balkanized Local Factions",
            Self::ChaosCult => "Chaos Cult",
            Self::ClansTribes => "Clans / Tribes",
            Self::Communards => "Communards",
            Self::CorruptAristocrats => "Corrupt Aristocrats",
            Self::Demagogue => "Demagogue",
            Self::EcclesiarchicalAppointee => "Ecclesiarchical Appointee",
            Self::ElitistTyrant => "Elitist Tyrant",
            Self::ExploratorAuthority => "Explorator Authority",
            Self::GuildsCombine => "Guilds / Combines",
            Self::Hereteks => "Hereteks",
            Self::HereticalImperialCult => "Heretical Imperial Cult",
            Self::InfractionistGang => "Infractionist Gang",
            Self::LocalReligiousAuthorities => "Local Religious Authorities",
            Self::LoyalistMassMovement => "Loyalist Mass Movement",
            Self::MagistrateCouncil => "Magistrate Council",
            Self::MechanicusForgeLord => "Mechanicus Forge-Lord",
            Self::Megacorporations => "Megacorporations",
            Self::MilitaryGovernor => "Military Governor",
            Self::None => "None",
            Self::PopulistTyrant => "Populist Tyrant",
            Self::PuppetGovernment => "Puppet Government",
            Self::RevolutionaryJunta => "Revolutionary Junta",
            Self::RogueTraderDynasty => "Rogue Trader Dynasty",
            Self::ShadowyPsykerCabal => "Shadowy Psyker Cabal",
            Self::TraditionalOligarchy => "Traditional Oligarchy",
            Self::TraditionalistAristocracy => "Traditionalist Aristocracy",
            Self::Warlords => "Warlords",
            Self::WarriorAristocracy => "Warrior Aristocracy",
            Self::XenosOverlords => "Xenos Overlords",
        }
    }
}

impl NotableFeature {
    pub const VARIANTS: &'static [Self] = &[
        Self::Abhumans,
        Self::AlteredHumans,
        Self::AncientArchive,
        Self::AncientTombs,
        Self::ArchaeotechRuins,
        Self::BlindingMists,
        Self::CelestialPhenomena,
        Self::ChaosCultists,
        Self::CivilWar,
        Self::ColdWar,
        Self::CrumblingArcologies,
        Self::DaemonicCorruption,
        Self::DangerousWildlife,
        Self::DesertWorld,
        Self::DeviantReligion,
        Self::EugenicCult,
        Self::ExtremeEnvironment,
        Self::FactionalFragmentation,
        Self::FailedParadise,
        Self::FlyingCities,
        Self::ForbiddenTech,
        Self::ForeignControl,
        Self::FreakGeology,
        Self::FreakWeather,
        Self::Freeport,
        Self::FriendlyXenos,
        Self::FrozenWorld,
        Self::GoldRush,
        Self::GreatWork,
        Self::HeavyIndustry,
        Self::HeavyMining,
        Self::Hereteks,
        Self::HolyWar,
        Self::HostileBiosphere,
        Self::HostileXenos,
        Self::ImpendingDoom,
        Self::ImperialKnights,
        Self::ImportantShrine,
        Self::InquisitionOutpost,
        Self::JungleWorld,
        Self::Libertines,
        Self::LocalSpecialty,
        Self::LocalTech,
        Self::MajorSpaceyard,
        Self::MartialLaw,
        Self::MassPanic,
        Self::MinimalContact,
        Self::Missionaries,
        Self::MutantHordes,
        Self::NavalBlockade,
        Self::NavalOutpost,
        Self::NavigatorHouse,
        Self::NomadicCities,
        Self::NotableLocal,
        Self::OceanWorld,
        Self::OutOfContact,
        Self::Pandemic,
        Self::PilgrimageSite,
        Self::PocketEmpire,
        Self::PoliceState,
        Self::PopularUprising,
        Self::PowerfulCriminals,
        Self::PowerfulNobles,
        Self::PrimitiveXenos,
        Self::Prosperous,
        Self::PsykerAcademy,
        Self::PsykerCult,
        Self::Quarantined,
        Self::Radioactive,
        Self::RecentlyRediscovered,
        Self::ScholaProgenium,
        Self::SeagoingCities,
        Self::SealedMenace,
        Self::SecretMasters,
        Self::Sectarians,
        Self::SeismicInstability,
        Self::Separatists,
        Self::SilicaAnimus,
        Self::SoleSuppliers,
        Self::SororitasConvent,
        Self::SpaceHulks,
        Self::StrangeCustoms,
        Self::StrangeHatred,
        Self::SubsectorHegemon,
        Self::TechPriestCult,
        Self::TestSite,
        Self::TheSilentTrade,
        Self::TradeHub,
        Self::AdministrativeHub,
        Self::UnmappedWastes,
        Self::VastFortresses,
        Self::VerdantEcology,
        Self::WarZone,
        Self::WarpPhenomena,
        Self::WitchHunt,
        Self::XenoRuins,
        Self::Xenophiles,
        Self::Xenophobes,
        Self::XenosInfiltrators,
        Self::Zombies,
    ];
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Abhumans => "Abhumans",
            Self::AlteredHumans => "Altered Humans",
            Self::AncientArchive => "Ancient Archive",
            Self::AncientTombs => "Ancient Tombs",
            Self::ArchaeotechRuins => "Archaeotech Ruins",
            Self::BlindingMists => "Blinding Mists",
            Self::CelestialPhenomena => "Celestial Phenomena",
            Self::ChaosCultists => "Chaos Cultists",
            Self::CivilWar => "Civil War",
            Self::ColdWar => "Cold War",
            Self::CrumblingArcologies => "Crumbling Arcologies",
            Self::DaemonicCorruption => "Daemonic Corruption",
            Self::DangerousWildlife => "Dangerous Wildlife",
            Self::DesertWorld => "Desert World",
            Self::DeviantReligion => "Deviant Religion",
            Self::EugenicCult => "Eugenic Cult",
            Self::ExtremeEnvironment => "Extreme Environment",
            Self::FactionalFragmentation => "Factional Fragmentation",
            Self::FailedParadise => "Failed Paradise",
            Self::FlyingCities => "Flying Cities",
            Self::ForbiddenTech => "Forbidden Tech",
            Self::ForeignControl => "Foreign Control",
            Self::FreakGeology => "Freak Geology",
            Self::FreakWeather => "Freak Weather",
            Self::Freeport => "Freeport",
            Self::FriendlyXenos => "Friendly Xenos",
            Self::FrozenWorld => "Frozen World",
            Self::GoldRush => "Gold Rush",
            Self::GreatWork => "Great Work",
            Self::HeavyIndustry => "Heavy Industry",
            Self::HeavyMining => "Heavy Mining",
            Self::Hereteks => "Hereteks",
            Self::HolyWar => "Holy War",
            Self::HostileBiosphere => "Hostile Biosphere",
            Self::HostileXenos => "Hostile Xenos",
            Self::ImpendingDoom => "Impending Doom",
            Self::ImperialKnights => "Imperial Knights",
            Self::ImportantShrine => "Important Shrine",
            Self::InquisitionOutpost => "Inquisition Outpost",
            Self::JungleWorld => "Jungle World",
            Self::Libertines => "Libertines",
            Self::LocalSpecialty => "Local Specialty",
            Self::LocalTech => "Local Tech",
            Self::MajorSpaceyard => "Major Spaceyard",
            Self::MartialLaw => "Martial Law",
            Self::MassPanic => "Mass Panic",
            Self::MinimalContact => "Minimal Contact",
            Self::Missionaries => "Missionaries",
            Self::MutantHordes => "Mutant Hordes",
            Self::NavalBlockade => "Naval Blockade",
            Self::NavalOutpost => "Naval Outpost",
            Self::NavigatorHouse => "Navigator House",
            Self::NomadicCities => "Nomadic Cities",
            Self::NotableLocal => "Notable Local",
            Self::OceanWorld => "Ocean World",
            Self::OutOfContact => "Out of Contact",
            Self::Pandemic => "Pandemic",
            Self::PilgrimageSite => "Pilgrimage Site",
            Self::PocketEmpire => "Pocket Empire",
            Self::PoliceState => "Police State",
            Self::PopularUprising => "Popular Uprising",
            Self::PowerfulCriminals => "Powerful Criminals",
            Self::PowerfulNobles => "Powerful Nobles",
            Self::PrimitiveXenos => "Primitive Xenos",
            Self::Prosperous => "Prosperous",
            Self::PsykerAcademy => "Psyker Academy",
            Self::PsykerCult => "Psyker Cult",
            Self::Quarantined => "Quarantined",
            Self::Radioactive => "Radioactive",
            Self::RecentlyRediscovered => "Recently Rediscovered",
            Self::ScholaProgenium => "Schola Progenium",
            Self::SeagoingCities => "Seagoing Cities",
            Self::SealedMenace => "Sealed Menace",
            Self::SecretMasters => "Secret Masters",
            Self::Sectarians => "Sectarians",
            Self::SeismicInstability => "Seismic Instability",
            Self::Separatists => "Separatists",
            Self::SilicaAnimus => "Silica Animus",
            Self::SoleSuppliers => "Sole Suppliers",
            Self::SororitasConvent => "Sororitas Convent",
            Self::SpaceHulks => "Space Hulks",
            Self::StrangeCustoms => "Strange Customs",
            Self::StrangeHatred => "Strange Hatred",
            Self::SubsectorHegemon => "Subsector Hegemon",
            Self::TechPriestCult => "Tech-Priest Cult",
            Self::TestSite => "Test Site",
            Self::TheSilentTrade => "The Silent Trade",
            Self::TradeHub => "Trade Hub",
            Self::AdministrativeHub => "Administrative Hub",
            Self::UnmappedWastes => "Unmapped Wastes",
            Self::VastFortresses => "Vast Fortresses",
            Self::VerdantEcology => "Verdant Ecology",
            Self::WarZone => "War Zone",
            Self::WarpPhenomena => "Warp Phenomena",
            Self::WitchHunt => "Witch Hunt",
            Self::XenoRuins => "Xeno Ruins",
            Self::Xenophiles => "Xenophiles",
            Self::Xenophobes => "Xenophobes",
            Self::XenosInfiltrators => "Xenos Infiltrators",
            Self::Zombies => "Zombies",
        }
    }
}

impl KeyTables {
    /// Build a complete key table from the compiled enum variant set.
    ///
    /// Enums are authoritative, so the lookup map is always
    /// reconstructed from `VARIANTS`.
    pub fn from_enums() -> Self {
        let mut t = Self::default();
        for v in StarColour::VARIANTS {
            t.star_colours.insert(v.display_name().to_owned(), *v);
        }
        for v in WorldType::VARIANTS {
            t.world_types.insert(v.display_name().to_owned(), v.clone());
        }
        for v in Atmosphere::VARIANTS {
            t.atmospheres.insert(v.display_name().to_owned(), v.clone());
        }
        for v in Temperature::VARIANTS {
            t.temperatures
                .insert(v.display_name().to_owned(), v.clone());
        }
        for v in Biosphere::VARIANTS {
            t.biospheres.insert(v.display_name().to_owned(), v.clone());
        }
        for v in Population::VARIANTS {
            t.populations.insert(v.display_name().to_owned(), v.clone());
        }
        for v in TechLevel::VARIANTS {
            t.tech_levels.insert(v.display_name().to_owned(), v.clone());
        }
        for v in Government::VARIANTS {
            t.governments.insert(v.display_name().to_owned(), v.clone());
        }
        for v in NotableFeature::VARIANTS {
            t.notable_features.push(v.display_name().to_owned());
        }
        t
    }
}
