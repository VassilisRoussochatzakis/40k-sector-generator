//! Top-level builder state (§D5). One instance is owned by the eframe app
//! while the builder is in focus.

use std::collections::BTreeSet;

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{SystemId, WorldId};
use sectorforge::sector_model::GeneratedSector;
use sectorforge::{InvariantReport, ValidationReport};

use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::DerivationCache;
use super::errors::BuilderError;
use super::index::BuilderIndex;
use super::snapshot::Snapshot;

/// Modal dialogs the builder can have open. Only one at a time.
#[derive(Debug, Clone)]
pub enum ModalKind {
    NewProject {
        name: String,
        title: String,
        seed: String,
        width: u32,
        height: u32,
    },
    OpenProject {
        path: Option<Utf8PathBuf>,
    },
    SaveAs {
        path: Utf8PathBuf,
    },
    PlaceSystem {
        coord: sectorforge::sector_model::HexCoord,
        name: String,
    },
    ConfirmRevertSnapshot {
        snapshot_name: String,
    },
    Message(String),
}

/// Generic job handle the builder tracks for off-thread work (§47).
/// Type-erased so it can sit alongside other pending jobs in a vec.
pub struct JobHandle {
    pub id: String,
    pub description: String,
}

pub struct BuilderState {
    pub sector: GeneratedSector,
    pub project_path: Option<Utf8PathBuf>,
    pub config: AppConfig,
    pub data_catalogs: DataCatalogs,
    pub index: BuilderIndex,
    pub command_log: Vec<BuilderCommand>,
    /// Position of the cursor inside `command_log`. Used for redo: commands
    /// past `cursor` are redo candidates; commands before are undoable.
    pub command_cursor: usize,
    pub snapshots: Vec<Snapshot>,
    /// §Q1: pinned systems live in this side-table — never written to JSON.
    pub pinned_systems: BTreeSet<SystemId>,
    pub pinned_worlds: BTreeSet<WorldId>,
    pub derivation_cache: DerivationCache,
    pub dirty: bool,
    pub auto_save_path: Option<Utf8PathBuf>,
    pub validation_report: Option<ValidationReport>,
    pub invariant_report: Option<InvariantReport>,
    pub modal: Option<ModalKind>,
    pub pending_jobs: Vec<JobHandle>,
    /// §49: when true, structural renumbers prefer the stable mode that
    /// preserves existing IDs.
    pub stable_ids_on_rename: bool,
}

impl BuilderState {
    /// Construct a brand-new blank builder session sized `width x height`.
    pub fn new_blank(id: &str, title: &str, seed: &str, width: u32, height: u32) -> Self {
        let sector = GeneratedSector::empty(id, title, seed, width, height);
        let index = BuilderIndex::rebuild(&sector);
        Self {
            sector,
            project_path: None,
            config: default_config(id, title, seed, width, height),
            data_catalogs: DataCatalogs::new(),
            index,
            command_log: Vec::new(),
            command_cursor: 0,
            snapshots: Vec::new(),
            pinned_systems: BTreeSet::new(),
            pinned_worlds: BTreeSet::new(),
            derivation_cache: DerivationCache::new(),
            dirty: false,
            auto_save_path: None,
            validation_report: None,
            invariant_report: None,
            modal: None,
            pending_jobs: Vec::new(),
            stable_ids_on_rename: true,
        }
    }

    /// Run a [`BuilderCommand`] through the command bus: apply, refresh the
    /// index, drop any redo tail, push onto the log, and mark dirty.
    pub fn run(&mut self, mut cmd: BuilderCommand) -> Result<(), BuilderError> {
        cmd.apply(&mut self.sector)?;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.command_log.truncate(self.command_cursor);
        self.command_log.push(cmd);
        self.command_cursor = self.command_log.len();
        self.dirty = true;
        Ok(())
    }

    /// Undo the most recent command. No-op when the cursor is at 0.
    pub fn undo(&mut self) -> Result<(), BuilderError> {
        if self.command_cursor == 0 {
            return Ok(());
        }
        let cmd = &self.command_log[self.command_cursor - 1];
        cmd.revert(&mut self.sector)?;
        self.command_cursor -= 1;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.dirty = true;
        Ok(())
    }

    /// Re-apply a previously undone command. No-op past the log tail.
    pub fn redo(&mut self) -> Result<(), BuilderError> {
        if self.command_cursor >= self.command_log.len() {
            return Ok(());
        }
        let mut cmd = self.command_log[self.command_cursor].clone();
        cmd.apply(&mut self.sector)?;
        self.command_log[self.command_cursor] = cmd;
        self.command_cursor += 1;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.dirty = true;
        Ok(())
    }

    /// Capture a named snapshot at the current command-log position.
    pub fn snapshot(&mut self, name: impl Into<String>) {
        self.snapshots.push(Snapshot::new(
            name,
            self.sector.clone(),
            self.command_cursor,
        ));
    }
}

fn default_config(id: &str, title: &str, seed: &str, width: u32, height: u32) -> AppConfig {
    use sectorforge::config::{
        GenerationConfig, InputConfig, OutputConfig, PlacementConfig, ProjectConfig,
        RelationsGenerationConfig, RouteGenerationConfig, WorldSelectionConfig,
    };
    AppConfig {
        project: ProjectConfig {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            version: None,
        },
        inputs: InputConfig {
            world_data_dir: "data/worlds".to_string(),
            system_names: None,
            world_names: None,
            factions: None,
            route_rules: None,
            generation_profiles: None,
            relations: None,
            regions: None,
            economy: None,
            history: None,
            personae: None,
            sites: None,
        },
        generation: GenerationConfig {
            seed: seed.to_string(),
            sector_width: width,
            sector_height: height,
            subsector_width: None,
            subsector_height: None,
            system_count: 0,
            min_worlds_per_system: 0,
            max_worlds_per_system: 0,
            allow_empty_hexes: true,
            world_feature_count: 1,
            strict_world_rows: true,
            placement: PlacementConfig::default(),
            world_selection: WorldSelectionConfig::default(),
            routes: RouteGenerationConfig::default(),
            relations: RelationsGenerationConfig::default(),
            search_base_seed: None,
            search_candidate_index: None,
            search_constraints_digest: None,
        },
        outputs: OutputConfig {
            directory: "out".to_string(),
            formats: vec![sectorforge::config::OutputFormat::Json],
            pretty_json: true,
            write_per_system_files: false,
            write_manifest: true,
            write_diagnostics: false,
            bitmap: Default::default(),
            html: Default::default(),
        },
        map_theme: None,
        analyze: Default::default(),
        search: Default::default(),
        diff: Default::default(),
        history: Default::default(),
    }
}
