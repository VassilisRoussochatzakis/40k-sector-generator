/// World parameter types. The enum set in this module is the authoritative
/// taxonomy; project data is loaded from `worlds.toml` via [`load_worlds_data`].
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Worlds-data loading (`WorldError`, `WorldsLoad`, `load_worlds_data`) lives in
// the `worlds_toml` IO module; re-exported here so `crate::worlds::*` paths and
// this module's `[load_worlds_data]` doc-link stay stable.
pub use crate::worlds_toml::{load_worlds_data, WorldError, WorldsLoad};

// Keep enum order, picker order, and display labels in one declaration.
macro_rules! world_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        $(#[$meta])*
        pub enum $name { $($variant),+ }

        impl $name {
            pub const VARIANTS: &'static [Self] = &[$(Self::$variant),+];

            pub fn display_name(&self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }
    };
}

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

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
WorldType {
    AgriWorld => "Agri-World",
    Asteroid => "Asteroid",
    BastionWorld => "Bastion World",
    DeathWorld => "Death World",
    DeadWorld => "Dead World",
    ExtractiveColony => "Extractive Colony",
    FeralWorld => "Feral World",
    FeudalWorld => "Feudal World",
    ForgeWorld => "Forge World",
    FrontierWorld => "Frontier World",
    HiveWorld => "Hive World",
    ImperialWorld => "Imperial World",
    IndustrialWorld => "Industrial World",
    Orbital => "Orbital",
    PenalWorld => "Penal World",
    PlanetaryDump => "Planetary Dump",
    PlanetaryMonument => "Planetary Monument",
    PleasureWorld => "Pleasure World",
    ResearchStation => "Research Station",
    ShrineWorld => "Shrine World",
    TombWorld => "Tomb World",
    WarpLostWorld => "Warp-Lost World",
    Worldship => "Worldship",
    XenosWorld => "Xenos World",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
Atmosphere {
    Airless => "Airless",
    Breathable => "Breathable",
    Corrosive => "Corrosive",
    Exotic => "Exotic",
    Thin => "Thin",
    Tainted => "Tainted",
    Toxic => "Toxic",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
Temperature {
    Freezing => "Freezing",
    Cold => "Cold",
    Temperate => "Temperate",
    Hot => "Hot",
    Boiling => "Boiling",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
Biosphere {
    Nonexistent => "Nonexistent",
    Minimal => "Minimal",
    Thriving => "Thriving",
    Poisoned => "Poisoned",
    XenoHybrid => "Xeno Hybrid",
    XenoDominance => "Xeno Dominance",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
Population {
    Uninhabited => "Uninhabited",
    Minimal => "Minimal",
    LightlyPopulated => "Lightly Populated",
    SoleSettlement => "Sole Settlement",
    DenselyPopulated => "Densely Populated",
    ExtremelyDense => "Extremely Dense",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
TechLevel {
    Primitive => "Primitive",
    Low => "Low",
    Standard => "Standard",
    High => "High",
    XenoHybrid => "Xeno Hybrid",
    Archaeotech => "Archaeotech",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
Government {
    BalkanizedLocalFactions => "Balkanized Local Factions",
    ChaosCult => "Chaos Cult",
    ClansTribes => "Clans / Tribes",
    Communards => "Communards",
    CorruptAristocrats => "Corrupt Aristocrats",
    Demagogue => "Demagogue",
    EcclesiarchicalAppointee => "Ecclesiarchical Appointee",
    ElitistTyrant => "Elitist Tyrant",
    ExploratorAuthority => "Explorator Authority",
    GuildsCombine => "Guilds / Combines",
    Hereteks => "Hereteks",
    HereticalImperialCult => "Heretical Imperial Cult",
    InfractionistGang => "Infractionist Gang",
    LocalReligiousAuthorities => "Local Religious Authorities",
    LoyalistMassMovement => "Loyalist Mass Movement",
    MagistrateCouncil => "Magistrate Council",
    MechanicusForgeLord => "Mechanicus Forge-Lord",
    Megacorporations => "Megacorporations",
    MilitaryGovernor => "Military Governor",
    None => "None",
    PopulistTyrant => "Populist Tyrant",
    PuppetGovernment => "Puppet Government",
    RevolutionaryJunta => "Revolutionary Junta",
    RogueTraderDynasty => "Rogue Trader Dynasty",
    ShadowyPsykerCabal => "Shadowy Psyker Cabal",
    TraditionalOligarchy => "Traditional Oligarchy",
    TraditionalistAristocracy => "Traditionalist Aristocracy",
    Warlords => "Warlords",
    WarriorAristocracy => "Warrior Aristocracy",
    XenosOverlords => "Xenos Overlords",
}
}

world_enum! {
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
NotableFeature {
    Abhumans => "Abhumans",
    AlteredHumans => "Altered Humans",
    AncientArchive => "Ancient Archive",
    AncientTombs => "Ancient Tombs",
    ArchaeotechRuins => "Archaeotech Ruins",
    BlindingMists => "Blinding Mists",
    CelestialPhenomena => "Celestial Phenomena",
    ChaosCultists => "Chaos Cultists",
    CivilWar => "Civil War",
    ColdWar => "Cold War",
    CrumblingArcologies => "Crumbling Arcologies",
    DaemonicCorruption => "Daemonic Corruption",
    DangerousWildlife => "Dangerous Wildlife",
    DesertWorld => "Desert World",
    DeviantReligion => "Deviant Religion",
    EugenicCult => "Eugenic Cult",
    ExtremeEnvironment => "Extreme Environment",
    FactionalFragmentation => "Factional Fragmentation",
    FailedParadise => "Failed Paradise",
    FlyingCities => "Flying Cities",
    ForbiddenTech => "Forbidden Tech",
    ForeignControl => "Foreign Control",
    FreakGeology => "Freak Geology",
    FreakWeather => "Freak Weather",
    Freeport => "Freeport",
    FriendlyXenos => "Friendly Xenos",
    FrozenWorld => "Frozen World",
    GoldRush => "Gold Rush",
    GreatWork => "Great Work",
    HeavyIndustry => "Heavy Industry",
    HeavyMining => "Heavy Mining",
    Hereteks => "Hereteks",
    HolyWar => "Holy War",
    HostileBiosphere => "Hostile Biosphere",
    HostileXenos => "Hostile Xenos",
    ImpendingDoom => "Impending Doom",
    ImperialKnights => "Imperial Knights",
    ImportantShrine => "Important Shrine",
    InquisitionOutpost => "Inquisition Outpost",
    JungleWorld => "Jungle World",
    Libertines => "Libertines",
    LocalSpecialty => "Local Specialty",
    LocalTech => "Local Tech",
    MajorSpaceyard => "Major Spaceyard",
    MartialLaw => "Martial Law",
    MassPanic => "Mass Panic",
    MinimalContact => "Minimal Contact",
    Missionaries => "Missionaries",
    MutantHordes => "Mutant Hordes",
    NavalBlockade => "Naval Blockade",
    NavalOutpost => "Naval Outpost",
    NavigatorHouse => "Navigator House",
    NomadicCities => "Nomadic Cities",
    NotableLocal => "Notable Local",
    OceanWorld => "Ocean World",
    OutOfContact => "Out of Contact",
    Pandemic => "Pandemic",
    PilgrimageSite => "Pilgrimage Site",
    PocketEmpire => "Pocket Empire",
    PoliceState => "Police State",
    PopularUprising => "Popular Uprising",
    PowerfulCriminals => "Powerful Criminals",
    PowerfulNobles => "Powerful Nobles",
    PrimitiveXenos => "Primitive Xenos",
    Prosperous => "Prosperous",
    PsykerAcademy => "Psyker Academy",
    PsykerCult => "Psyker Cult",
    Quarantined => "Quarantined",
    Radioactive => "Radioactive",
    RecentlyRediscovered => "Recently Rediscovered",
    ScholaProgenium => "Schola Progenium",
    SeagoingCities => "Seagoing Cities",
    SealedMenace => "Sealed Menace",
    SecretMasters => "Secret Masters",
    Sectarians => "Sectarians",
    SeismicInstability => "Seismic Instability",
    Separatists => "Separatists",
    SilicaAnimus => "Silica Animus",
    SoleSuppliers => "Sole Suppliers",
    SororitasConvent => "Sororitas Convent",
    SpaceHulks => "Space Hulks",
    StrangeCustoms => "Strange Customs",
    StrangeHatred => "Strange Hatred",
    SubsectorHegemon => "Subsector Hegemon",
    TechPriestCult => "Tech-Priest Cult",
    TestSite => "Test Site",
    TheSilentTrade => "The Silent Trade",
    TradeHub => "Trade Hub",
    AdministrativeHub => "Administrative Hub",
    UnmappedWastes => "Unmapped Wastes",
    VastFortresses => "Vast Fortresses",
    VerdantEcology => "Verdant Ecology",
    WarZone => "War Zone",
    WarpPhenomena => "Warp Phenomena",
    WitchHunt => "Witch Hunt",
    XenoRuins => "Xeno Ruins",
    Xenophiles => "Xenophiles",
    Xenophobes => "Xenophobes",
    XenosInfiltrators => "Xenos Infiltrators",
    Zombies => "Zombies",
}
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
