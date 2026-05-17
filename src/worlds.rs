/// World parameter types loaded from two CSV files in a project's worlds dir:
///
///   key.csv       — reference lookup tables (one column per enum kind):
///     star_colour, world_type, atmosphere, temperature, biosphere,
///     population, tech_level, government, notable_feature
///
///   generator.csv — weighted generator rows:
///     star_colour, world_type, atmosphere, temperature, biosphere,
///     population, tech_level, government, notable_feature, counter, weight
///
/// Both files have a header row. Empty cells are treated as missing. The CSV
/// parser handles RFC 4180 quoted fields.
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ── Key-tab enum types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Atmosphere {
    Airless,
    Breathable,
    Corrosive,
    Exotic,
    Thin,
    Tainted,
    Toxic,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Temperature {
    Freezing,
    Cold,
    Temperate,
    Hot,
    Boiling,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Biosphere {
    Nonexistent,
    Minimal,
    Thriving,
    Poisoned,
    XenoHybrid,
    XenoDominance,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Population {
    Uninhabited,
    Minimal,
    LightlyPopulated,
    SoleSettlement,
    DenselyPopulated,
    ExtremelyDense,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TechLevel {
    Primitive,
    Low,
    Standard,
    High,
    XenoHybrid,
    Archaeotech,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
pub struct GenerationRow {
    /// Star colour (A)
    pub star_colour: Option<StarColour>,
    /// World type (B)
    pub world_type: Option<WorldType>,
    /// Atmosphere (C)
    pub atmosphere: Option<Atmosphere>,
    /// Temperature (D)
    pub temperature: Option<Temperature>,
    /// Biosphere (E)
    pub biosphere: Option<Biosphere>,
    /// Population (F)
    pub population: Option<Population>,
    /// Tech level (G)
    pub tech: Option<TechLevel>,
    /// Government (H)
    pub government: Option<Government>,
    /// Notable feature (I)
    pub notable_feature: Option<NotableFeature>,
    /// Counter count from COUNTIF formula (col J, stored as integer in the sheet).
    pub counter: Option<usize>,
    /// Weight formula result — numeric weight for random selection (col K).
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
#[derive(Debug, Default)]
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

/// Helper: trimmed non-empty string from a CSV cell.
fn cell_str(cell: &str) -> Option<&str> {
    let s = cell.trim();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Parse a CSV file into header + rows (RFC 4180: quoted fields, "" escapes ").
/// Returns (header, rows). Empty trailing lines skipped.
pub fn parse_csv(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let records = parse_csv_records(text)?;
    let mut iter = records.into_iter();
    let header = iter
        .next()
        .ok_or_else(|| "csv has no header row".to_string())?;
    Ok((header, iter.collect()))
}

fn parse_csv_records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    record.push(std::mem::take(&mut field));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    record.push(std::mem::take(&mut field));
                    if !is_empty_record(&record) {
                        records.push(std::mem::take(&mut record));
                    } else {
                        record.clear();
                    }
                }
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    if !is_empty_record(&record) {
                        records.push(std::mem::take(&mut record));
                    } else {
                        record.clear();
                    }
                }
                _ => field.push(c),
            }
        }
    }
    if in_quotes {
        return Err("csv ended inside a quoted field".to_string());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(std::mem::take(&mut field));
        if !is_empty_record(&record) {
            records.push(record);
        }
    }
    Ok(records)
}

fn is_empty_record(record: &[String]) -> bool {
    record.iter().all(|f| f.trim().is_empty())
}

impl KeyTables {
    /// Parse a `key.csv` file. Expected header (case-insensitive, order fixed):
    ///   star_colour,world_type,atmosphere,temperature,biosphere,
    ///   population,tech_level,government,notable_feature
    pub fn from_csv_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text =
            fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_csv_str(&text)
    }

    pub fn from_csv_str(text: &str) -> Result<Self, String> {
        let (_header, rows) = parse_csv(text)?;
        let mut tables = Self::default();
        for row in rows {
            let get = |idx: usize| row.get(idx).map(|s| s.as_str()).and_then(cell_str);

            if let Some(s) = get(0) {
                if let Ok(v) = s.parse::<StarColour>() {
                    tables.star_colours.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(1) {
                if let Ok(v) = s.parse::<WorldType>() {
                    tables.world_types.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(2) {
                if let Ok(v) = s.parse::<Atmosphere>() {
                    tables.atmospheres.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(3) {
                if let Ok(v) = s.parse::<Temperature>() {
                    tables.temperatures.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(4) {
                if let Ok(v) = s.parse::<Biosphere>() {
                    tables.biospheres.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(5) {
                if let Ok(v) = s.parse::<Population>() {
                    tables.populations.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(6) {
                if let Ok(v) = s.parse::<TechLevel>() {
                    tables.tech_levels.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(7) {
                if let Ok(v) = s.parse::<Government>() {
                    tables.governments.insert(s.to_owned(), v);
                }
            }
            if let Some(s) = get(8) {
                tables.notable_features.push(s.to_owned());
            }
        }
        Ok(tables)
    }
}

impl std::str::FromStr for StarColour {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "O" => Ok(Self::BlueHypergiant),
            "B" => Ok(Self::BlueWhite),
            "A" => Ok(Self::White),
            "F" => Ok(Self::YellowWhite),
            "G" => Ok(Self::Yellow),
            "K" => Ok(Self::OrangeDwarf),
            "M" => Ok(Self::RedDwarf),
            _ => Err(()),
        }
    }
}

/// Parse a single CSV record into a GenerationRow.
fn parse_generation_row(row: &[String]) -> GenerationRow {
    fn parse<T: std::str::FromStr>(row: &[String], idx: usize) -> Option<T> {
        row.get(idx)
            .map(|s| s.as_str())
            .and_then(cell_str)
            .and_then(|s| s.parse().ok())
    }

    GenerationRow {
        star_colour: parse(row, 0),
        world_type: parse(row, 1),
        atmosphere: parse(row, 2),
        temperature: parse(row, 3),
        biosphere: parse(row, 4),
        population: parse(row, 5),
        tech: parse(row, 6),
        government: parse(row, 7),
        notable_feature: parse(row, 8),
        counter: row
            .get(9)
            .map(|s| s.as_str())
            .and_then(cell_str)
            .and_then(|s| s.parse::<i64>().ok())
            .map(|n| n as usize),
        weight: row
            .get(10)
            .map(|s| s.as_str())
            .and_then(cell_str)
            .and_then(|s| s.parse::<f64>().ok()),
    }
}

/// Load a project's world data: `key.csv` + `generator.csv` from the given dir.
pub fn load_generation_rows(
    data_dir: impl AsRef<Path>,
) -> Result<(KeyTables, Vec<GenerationRow>), String> {
    let dir = data_dir.as_ref();
    let key_path = dir.join("key.csv");
    let gen_path = dir.join("generator.csv");

    let tables = KeyTables::from_csv_path(&key_path)?;

    let gen_text = fs::read_to_string(&gen_path)
        .map_err(|e| format!("read {}: {e}", gen_path.display()))?;
    let (_header, records) = parse_csv(&gen_text)?;
    let rows: Vec<GenerationRow> = records.iter().map(|r| parse_generation_row(r)).collect();

    Ok((tables, rows))
}

// ── FromStr implementations for Key-tab enums ────────────────────────────────

impl std::str::FromStr for WorldType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Agri-World" => Self::AgriWorld,
            "Asteroid" => Self::Asteroid,
            "Bastion World" => Self::BastionWorld,
            "Death World" => Self::DeathWorld,
            "Dead World" => Self::DeadWorld,
            "Extractive Colony" => Self::ExtractiveColony,
            "Feral World" => Self::FeralWorld,
            "Feudal World" => Self::FeudalWorld,
            "Forge World" => Self::ForgeWorld,
            "Frontier World" => Self::FrontierWorld,
            "Hive World" => Self::HiveWorld,
            "Industrial World" => Self::IndustrialWorld,
            "Orbital" => Self::Orbital,
            "Penal World" => Self::PenalWorld,
            "Planetary Dump" => Self::PlanetaryDump,
            "Planetary Monument" => Self::PlanetaryMonument,
            "Pleasure World" => Self::PleasureWorld,
            "Research Station" => Self::ResearchStation,
            "Shrine World" => Self::ShrineWorld,
            "Tomb World" => Self::TombWorld,
            "Warp-Lost World" => Self::WarpLostWorld,
            "Worldship" => Self::Worldship,
            "Xenos World" => Self::XenosWorld,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Airless" => Self::Airless,
            "Breathable" => Self::Breathable,
            "Corrosive" => Self::Corrosive,
            "Exotic" => Self::Exotic,
            "Thin" => Self::Thin,
            "Tainted" => Self::Tainted,
            "Toxic" => Self::Toxic,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Freezing" => Self::Freezing,
            "Cold" => Self::Cold,
            "Temperate" => Self::Temperate,
            "Hot" => Self::Hot,
            "Boiling" => Self::Boiling,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Nonexistent" => Self::Nonexistent,
            "Minimal" => Self::Minimal,
            "Thriving" => Self::Thriving,
            "Poisoned" => Self::Poisoned,
            "Xeno Hybrid" => Self::XenoHybrid,
            "Xeno Dominance" => Self::XenoDominance,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Uninhabited" => Self::Uninhabited,
            "Minimal" => Self::Minimal,
            "Lightly Populated" => Self::LightlyPopulated,
            "Sole Settlement" => Self::SoleSettlement,
            "Densely Populated" => Self::DenselyPopulated,
            "Extremely Dense" => Self::ExtremelyDense,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Primitive" => Self::Primitive,
            "Low" => Self::Low,
            "Standard" => Self::Standard,
            "High" => Self::High,
            "Xeno Hybrid" => Self::XenoHybrid,
            "Archaeotech" => Self::Archaeotech,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Balkanized Local Factions" => Self::BalkanizedLocalFactions,
            "Chaos Cult" => Self::ChaosCult,
            "Clans / Tribes" => Self::ClansTribes,
            "Communards" => Self::Communards,
            "Corrupt Aristocrats" => Self::CorruptAristocrats,
            "Demagogue" => Self::Demagogue,
            "Ecclesiarchical Appointee" => Self::EcclesiarchicalAppointee,
            "Elitist Tyrant" => Self::ElitistTyrant,
            "Explorator Authority" => Self::ExploratorAuthority,
            "Guilds / Combines" => Self::GuildsCombine,
            "Hereteks" => Self::Hereteks,
            "Heretical Imperial Cult" => Self::HereticalImperialCult,
            "Infractionist Gang" => Self::InfractionistGang,
            "Local Religious Authorities" => Self::LocalReligiousAuthorities,
            "Loyalist Mass Movement" => Self::LoyalistMassMovement,
            "Magistrate Council" => Self::MagistrateCouncil,
            "Mechanicus Forge-Lord" => Self::MechanicusForgeLord,
            "Megacorporations" => Self::Megacorporations,
            "Military Governor" => Self::MilitaryGovernor,
            "None" => Self::None,
            "Populist Tyrant" => Self::PopulistTyrant,
            "Puppet Government" => Self::PuppetGovernment,
            "Revolutionary Junta" => Self::RevolutionaryJunta,
            "Rogue Trader Dynasty" => Self::RogueTraderDynasty,
            "Shadowy Psyker Cabal" => Self::ShadowyPsykerCabal,
            "Traditional Oligarchy" => Self::TraditionalOligarchy,
            "Traditionalist Aristocracy" => Self::TraditionalistAristocracy,
            "Warlords" => Self::Warlords,
            "Warrior Aristocracy" => Self::WarriorAristocracy,
            "Xenos Overlords" => Self::XenosOverlords,
            _ => return Err(()),
        })
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
        Ok(match s {
            "Abhumans" => Self::Abhumans,
            "Altered Humans" => Self::AlteredHumans,
            "Ancient Archive" => Self::AncientArchive,
            "Ancient Tombs" => Self::AncientTombs,
            "Archaeotech Ruins" => Self::ArchaeotechRuins,
            "Blinding Mists" => Self::BlindingMists,
            "Celestial Phenomena" => Self::CelestialPhenomena,
            "Chaos Cultists" => Self::ChaosCultists,
            "Civil War" => Self::CivilWar,
            "Cold War" => Self::ColdWar,
            "Crumbling Arcologies" => Self::CrumblingArcologies,
            "Daemonic Corruption" => Self::DaemonicCorruption,
            "Dangerous Wildlife" => Self::DangerousWildlife,
            "Desert World" => Self::DesertWorld,
            "Deviant Religion" => Self::DeviantReligion,
            "Eugenic Cult" => Self::EugenicCult,
            "Extreme Environment" => Self::ExtremeEnvironment,
            "Factional Fragmentation" => Self::FactionalFragmentation,
            "Failed Paradise" => Self::FailedParadise,
            "Flying Cities" => Self::FlyingCities,
            "Forbidden Tech" => Self::ForbiddenTech,
            "Foreign Control" => Self::ForeignControl,
            "Freak Geology" => Self::FreakGeology,
            "Freak Weather" => Self::FreakWeather,
            "Freeport" => Self::Freeport,
            "Friendly Xenos" => Self::FriendlyXenos,
            "Frozen World" => Self::FrozenWorld,
            "Gold Rush" => Self::GoldRush,
            "Great Work" => Self::GreatWork,
            "Heavy Industry" => Self::HeavyIndustry,
            "Heavy Mining" => Self::HeavyMining,
            "Hereteks" => Self::Hereteks,
            "Holy War" => Self::HolyWar,
            "Hostile Biosphere" => Self::HostileBiosphere,
            "Hostile Xenos" => Self::HostileXenos,
            "Impending Doom" => Self::ImpendingDoom,
            "Imperial Knights" => Self::ImperialKnights,
            "Important Shrine" => Self::ImportantShrine,
            "Inquisition Outpost" => Self::InquisitionOutpost,
            "Jungle World" => Self::JungleWorld,
            "Libertines" => Self::Libertines,
            "Local Specialty" => Self::LocalSpecialty,
            "Local Tech" => Self::LocalTech,
            "Major Spaceyard" => Self::MajorSpaceyard,
            "Martial Law" => Self::MartialLaw,
            "Mass Panic" => Self::MassPanic,
            "Minimal Contact" => Self::MinimalContact,
            "Missionaries" => Self::Missionaries,
            "Mutant Hordes" => Self::MutantHordes,
            "Naval Blockade" => Self::NavalBlockade,
            "Naval Outpost" => Self::NavalOutpost,
            "Navigator House" => Self::NavigatorHouse,
            "Nomadic Cities" => Self::NomadicCities,
            "Notable Local" => Self::NotableLocal,
            "Ocean World" => Self::OceanWorld,
            "Out of Contact" => Self::OutOfContact,
            "Pandemic" => Self::Pandemic,
            "Pilgrimage Site" => Self::PilgrimageSite,
            "Pocket Empire" => Self::PocketEmpire,
            "Police State" => Self::PoliceState,
            "Popular Uprising" => Self::PopularUprising,
            "Powerful Criminals" => Self::PowerfulCriminals,
            "Powerful Nobles" => Self::PowerfulNobles,
            "Primitive Xenos" => Self::PrimitiveXenos,
            "Prosperous" => Self::Prosperous,
            "Psyker Academy" => Self::PsykerAcademy,
            "Psyker Cult" => Self::PsykerCult,
            "Quarantined" => Self::Quarantined,
            "Radioactive" => Self::Radioactive,
            "Recently Rediscovered" => Self::RecentlyRediscovered,
            "Schola Progenium" => Self::ScholaProgenium,
            "Seagoing Cities" => Self::SeagoingCities,
            "Sealed Menace" => Self::SealedMenace,
            "Secret Masters" => Self::SecretMasters,
            "Sectarians" => Self::Sectarians,
            "Seismic Instability" => Self::SeismicInstability,
            "Separatists" => Self::Separatists,
            "Silica Animus" => Self::SilicaAnimus,
            "Sole Suppliers" => Self::SoleSuppliers,
            "Sororitas Convent" => Self::SororitasConvent,
            "Space Hulks" => Self::SpaceHulks,
            "Strange Customs" => Self::StrangeCustoms,
            "Strange Hatred" => Self::StrangeHatred,
            "Subsector Hegemon" => Self::SubsectorHegemon,
            "Tech-Priest Cult" => Self::TechPriestCult,
            "Test Site" => Self::TestSite,
            "The Silent Trade" => Self::TheSilentTrade,
            "Trade Hub" => Self::TradeHub,
            "Administrative Hub" => Self::AdministrativeHub,
            "Unmapped Wastes" => Self::UnmappedWastes,
            "Vast Fortresses" => Self::VastFortresses,
            "Verdant Ecology" => Self::VerdantEcology,
            "War Zone" => Self::WarZone,
            "Warp Phenomena" => Self::WarpPhenomena,
            "Witch Hunt" => Self::WitchHunt,
            "Xeno Ruins" => Self::XenoRuins,
            "Xenophiles" => Self::Xenophiles,
            "Xenophobes" => Self::Xenophobes,
            "Xenos Infiltrators" => Self::XenosInfiltrators,
            "Zombies" => Self::Zombies,
            _ => return Err(()),
        })
    }
}

impl std::fmt::Display for NotableFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
