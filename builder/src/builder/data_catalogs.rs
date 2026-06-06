//! In-memory mirrors of the configuration files that feed the generator.
//! The builder edits these (TOML editors per §37) and serialises them back
//! to disk on save.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use sectorforge::economy::{
    EconomyConfig, ResourceVector, StrategicOutput, StrategicPriority, SupplyRisk, TitheStatus,
};
use sectorforge::factions::FactionsFile;
use sectorforge::history::HistoryConfig;
use sectorforge::hooks::HooksConfig;
use sectorforge::ids::{SystemId, WorldId};
use sectorforge::missions::MissionsConfig;
use sectorforge::names::NameTables;
use sectorforge::personae::PersonaeConfig;
use sectorforge::prose::ProseConfig;
use sectorforge::regions::RegionsConfig;
use sectorforge::relations::RelationsConfig;
use sectorforge::routes::RouteRules;
use sectorforge::sites::SitesConfig;
use sectorforge::worlds_toml::WorldsConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// §PR1..§PR4: `ProseConfig` (tone + include flags + overrides). Mirrors
    /// `data/prose.toml` on disk. The PROSE tab edits this in-place;
    /// `recompute_prose` re-runs `prose::derive_with` against whatever lives
    /// here (falling back to defaults when `None`). Overrides survive every
    /// "Regenerate prose" pass because they live inside the config itself.
    pub prose: Option<ProseConfig>,
}

impl DataCatalogs {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The full *catalog-edit* document slice that `BuilderCommand::EditCatalog`
/// snapshots so catalog edits are undoable (audit finding #8). Bundles the 13
/// in-memory config mirrors ([`DataCatalogs`]) **plus** the five §E1..§E3
/// per-world / per-system economy override side-tables, which live on
/// [`super::BuilderState`] rather than inside `DataCatalogs` but are edited from
/// the same ECONOMY panel and must round-trip together.
///
/// Bundling everything into one snapshot (the "extend the snapshot" option of
/// the feature spec, chosen over a sibling `EditEconomyOverride` command) keeps
/// a single coalescing path and a single undo entry per edit burst.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub catalogs: DataCatalogs,
    pub world_economy_overrides: BTreeMap<WorldId, ResourceVector>,
    pub world_strategic_overrides: BTreeMap<WorldId, StrategicOutput>,
    pub system_tithe_overrides: BTreeMap<SystemId, TitheStatus>,
    pub system_supply_overrides: BTreeMap<SystemId, SupplyRisk>,
    pub system_priority_overrides: BTreeMap<SystemId, StrategicPriority>,
}
