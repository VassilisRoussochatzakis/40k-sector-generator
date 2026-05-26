//! In-memory mirrors of the configuration files that feed the generator.
//! The builder edits these (TOML editors per §37) and serialises them back
//! to disk on save.

use sectorforge::economy::EconomyConfig;
use sectorforge::factions::FactionsFile;
use sectorforge::history::HistoryConfig;
use sectorforge::hooks::HooksConfig;
use sectorforge::names::NameTables;
use sectorforge::personae::PersonaeConfig;
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
    /// §PER1: per-faction-kind pools + dominance / per-anchor caps + manual
    /// personae. Mirrors `data/personae.toml` on disk. The PERSONAE tab edits
    /// this in-place; `recompute_personae` re-runs `personae::derive_with`
    /// using whatever lives here (falling back to defaults when `None`).
    pub personae: Option<PersonaeConfig>,
    /// §HK4: `HooksConfig` knobs + handcrafted hooks. Mirrors `data/hooks.toml`
    /// on disk. The HOOKS tab edits this in-place; `recompute_hooks` re-runs
    /// `hooks::derive_with` against whatever lives here (falling back to
    /// defaults when `None`).
    pub hooks: Option<HooksConfig>,
}

impl DataCatalogs {
    pub fn new() -> Self {
        Self::default()
    }
}
