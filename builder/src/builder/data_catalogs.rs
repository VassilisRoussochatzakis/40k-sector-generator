//! In-memory mirrors of the configuration files that feed the generator.
//! The builder edits these (TOML editors per §37) and serialises them back
//! to disk on save.

use sectorforge::economy::EconomyConfig;
use sectorforge::factions::FactionsFile;
use sectorforge::history::HistoryConfig;
use sectorforge::names::NameTables;
use sectorforge::regions::RegionsConfig;
use sectorforge::relations::RelationsConfig;
use sectorforge::routes::RouteRules;
use sectorforge::worlds_toml::WorldsConfig;

#[derive(Debug, Clone, Default)]
pub struct DataCatalogs {
    pub worlds: Option<WorldsConfig>,
    pub names: Option<NameTables>,
    pub factions: Option<FactionsFile>,
    pub relations: Option<RelationsConfig>,
    pub route_rules: Option<RouteRules>,
    pub regions: Option<RegionsConfig>,
    pub economy: Option<EconomyConfig>,
    pub history: Option<HistoryConfig>,
}

impl DataCatalogs {
    pub fn new() -> Self {
        Self::default()
    }
}
