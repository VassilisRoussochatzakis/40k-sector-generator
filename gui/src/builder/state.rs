//! Top-level builder state (§D5). One instance is owned by the eframe app
//! while the builder is in focus.
//!
//! # R1: single source of truth
//!
//! `BuilderState` owns the sole live [`GeneratedSector`] for the project. Per
//! BUILDER_REQS §R1, the spec allows `Arc<RwLock<GeneratedSector>>` or
//! `Rc<RefCell<GeneratedSector>>` if the GUI thread is sole writer. We choose
//! the simpler design: direct ownership behind `&mut BuilderState`, since
//! every mutation route must hold an exclusive borrow on the state, and
//! background jobs receive *cloned* read-only snapshots through
//! [`crate::jobs`] (they post results back via mpsc, never mutate in place).
//! This gives the same single-writer guarantee as `Rc<RefCell<>>` without the
//! runtime borrow check or `Arc` overhead.
//!
//! # R10: panel contract
//!
//! Panels live in `gui/src/builder/panels/` and are functions with the
//! signature `fn(ui: &mut egui::Ui, state: &mut BuilderState)`. They never
//! hold module-level mutable state, never use `lazy_static`, and never carry
//! raw string IDs across boundaries. Modal state lives in
//! [`BuilderState::modal`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{SystemId, WorldId};
use sectorforge::invariants::check_sector;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::{InvariantReport, ValidationReport};

use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::DerivationCache;
use super::errors::BuilderError;
use super::file_watcher::FileWatcher;
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
    /// §P5: an external file change was detected and the in-memory buffer is
    /// dirty. The user must decide whether to reload from disk (losing the
    /// in-memory edits) or keep the in-memory buffer (potentially overwriting
    /// the on-disk version on the next save).
    ConflictResolver {
        /// Project-relative path of the file that changed.
        rel_path: String,
    },
}

/// §U2: default cap for the undo/redo ring buffer.
pub const DEFAULT_COMMAND_LOG_CAPACITY: usize = 200;

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
    /// §U2: bounded ring-buffer cap for `command_log`. When the log exceeds
    /// this size after a new mutation, the oldest commands are dropped and
    /// `command_cursor` plus snapshot positions are shifted accordingly.
    /// Default 200 — see [`DEFAULT_COMMAND_LOG_CAPACITY`].
    pub command_log_capacity: usize,
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
    /// §P4: per-file dirty markers keyed by project-relative path. Populated
    /// by panels that edit individual catalogs and cleared on save.
    pub dirty_files: BTreeSet<String>,
    /// §P4: file currently selected in the PROJECT tree. Optional — Phase E's
    /// TOML editor tabs (§PF2) will read this to decide which buffer to open.
    pub selected_file: Option<Utf8PathBuf>,
    /// §P4 + §P5: project-relative mtime snapshot taken at load time. The
    /// file watcher uses this baseline to spot external changes; the tree
    /// view uses it to draw the "● dirty" marker when the catalog mirror
    /// diverges from disk.
    pub file_mtimes: BTreeMap<String, std::time::SystemTime>,
    /// §P5: background watcher polling the project directory for external
    /// changes. `None` when no project is open.
    pub file_watcher: Option<FileWatcher>,
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
            command_log_capacity: DEFAULT_COMMAND_LOG_CAPACITY,
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
            dirty_files: BTreeSet::new(),
            selected_file: None,
            file_mtimes: BTreeMap::new(),
            file_watcher: None,
        }
    }

    /// Run a [`BuilderCommand`] through the command bus.
    ///
    /// Per R4 the bus enforces, in order:
    ///   (a) invariant re-check after apply (stored in
    ///       [`Self::invariant_report`] so the status bar can surface red),
    ///   (b) snapshot/undo stack maintenance — the redo tail is dropped and
    ///       the command is pushed onto the log,
    ///   (c) auto-save trigger via [`Self::trigger_auto_save`] when an
    ///       `auto_save_path` is configured,
    ///   (d) cache invalidation for all cached overlays
    ///       (subsectors / heatmaps / derivations).
    ///
    /// The command itself is never rolled back here even if invariants fail —
    /// the report exposes the violation so the user can choose to undo. This
    /// matches the spec's "soft" invariant policy outside of export.
    pub fn run(&mut self, mut cmd: BuilderCommand) -> Result<(), BuilderError> {
        cmd.apply(&mut self.sector)?;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.command_log.truncate(self.command_cursor);
        self.command_log.push(cmd);
        self.command_cursor = self.command_log.len();
        self.enforce_command_log_capacity();
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.trigger_auto_save();
        Ok(())
    }

    /// §U2: drop the oldest commands when the log exceeds the configured
    /// ring-buffer capacity. The cursor and snapshot positions are shifted
    /// by the same drop-count so undo/redo references stay coherent.
    /// `command_log_capacity == 0` disables the cap (unbounded log).
    fn enforce_command_log_capacity(&mut self) {
        let cap = self.command_log_capacity;
        if cap == 0 || self.command_log.len() <= cap {
            return;
        }
        let drop = self.command_log.len() - cap;
        self.command_log.drain(0..drop);
        self.command_cursor = self.command_cursor.saturating_sub(drop);
        for snap in &mut self.snapshots {
            snap.command_log_position = snap.command_log_position.saturating_sub(drop);
        }
    }

    /// Write the sector to [`Self::auto_save_path`] as pretty JSON when set.
    /// No-op otherwise. Errors are reported by clearing `dirty = true` so the
    /// next event can retry; they do not propagate (the command already
    /// succeeded).
    pub fn trigger_auto_save(&mut self) {
        let Some(path) = self.auto_save_path.as_ref() else {
            return;
        };
        let Ok(text) = serde_json::to_string_pretty(&self.sector) else {
            return;
        };
        if std::fs::write(Path::new(path.as_std_path()), text).is_ok() {
            self.dirty = false;
        }
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
        self.invariant_report = Some(check_sector(&self.sector));
        self.trigger_auto_save();
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
        self.invariant_report = Some(check_sector(&self.sector));
        self.trigger_auto_save();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::command::BuilderCommand;
    use sectorforge::sector_model::HexCoord;

    fn add_n_systems(state: &mut BuilderState, n: u32) {
        let base = state.sector.systems.len() as u32;
        for k in 0..n {
            let i = base + k;
            state
                .run(BuilderCommand::AddSystem {
                    coord: HexCoord {
                        q: (i % 8) as i32,
                        r: (i / 8) as i32,
                    },
                    name: format!("sys-{i}"),
                    result_id: None,
                })
                .unwrap();
        }
    }

    #[test]
    fn ring_buffer_caps_command_log() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
        state.command_log_capacity = 5;
        add_n_systems(&mut state, 12);
        assert_eq!(state.command_log.len(), 5);
        assert_eq!(state.command_cursor, 5);
        assert_eq!(state.sector.systems.len(), 12);
    }

    #[test]
    fn ring_buffer_shifts_snapshot_positions() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
        state.command_log_capacity = 4;
        add_n_systems(&mut state, 2);
        state.snapshot("after-2");
        let snap_pos_before = state.snapshots[0].command_log_position;
        assert_eq!(snap_pos_before, 2);
        add_n_systems(&mut state, 6);
        assert_eq!(state.command_log.len(), 4);
        assert_eq!(state.snapshots[0].command_log_position, 0);
    }

    #[test]
    fn unbounded_capacity_zero_keeps_all_commands() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
        state.command_log_capacity = 0;
        add_n_systems(&mut state, 50);
        assert_eq!(state.command_log.len(), 50);
    }

    #[test]
    fn default_capacity_is_200() {
        let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert_eq!(state.command_log_capacity, DEFAULT_COMMAND_LOG_CAPACITY);
        assert_eq!(DEFAULT_COMMAND_LOG_CAPACITY, 200);
    }

    #[test]
    fn undo_redo_basic_round_trip() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        add_n_systems(&mut state, 3);
        assert_eq!(state.sector.systems.len(), 3);
        state.undo().unwrap();
        assert_eq!(state.sector.systems.len(), 2);
        assert_eq!(state.command_cursor, 2);
        state.redo().unwrap();
        assert_eq!(state.sector.systems.len(), 3);
        assert_eq!(state.command_cursor, 3);
    }

    #[test]
    fn undo_clamps_at_zero() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.undo().unwrap();
        assert_eq!(state.command_cursor, 0);
    }
}
