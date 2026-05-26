//! In-memory mirrors of the configuration files that feed the generator.
//! The builder edits these (TOML editors per §37) and serialises them back
//! to disk on save.

use sectorforge::economy::EconomyConfig;
use sectorforge::factions::FactionsFile;
use sectorforge::history::HistoryConfig;
use sectorforge::hooks::HooksConfig;
use sectorforge::missions::MissionsConfig;
use sectorforge::names::NameTables;
use sectorforge::personae::PersonaeConfig;
use sectorforge::regions::RegionsConfig;
use sectorforge::relations::RelationsConfig;
use sectorforge::routes::RouteRules;
use sectorforge::sites::SitesConfig;
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
    /// §ST4: `SitesConfig` knobs + manual sites. Mirrors `data/sites.toml`
    /// on disk. The SITES tab edits this in-place; `recompute_sites` re-runs
    /// `sites::derive_with` against whatever lives here (falling back to
    /// defaults when `None`).
    pub sites: Option<SitesConfig>,
    /// §M1..§M5: `MissionsConfig` knobs + manual missions. Mirrors
    /// `data/missions.toml` on disk. The MISSIONS tab edits this in-place;
    /// `recompute_missions` re-runs `missions::derive_with` against whatever
    /// lives here (falling back to defaults when `None`).
    pub missions: Option<MissionsConfig>,
}

impl DataCatalogs {
    pub fn new() -> Self {
        Self::default()
    }
}
