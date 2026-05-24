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
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::input::ProjectInput;
use sectorforge::invariants::check_sector;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::validation::validate;
use sectorforge::{InvariantReport, ValidationReport};

use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::DerivationCache;
use super::errors::BuilderError;
use super::file_watcher::FileWatcher;
use super::index::BuilderIndex;
use super::preview::PreviewState;
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
    /// §G6: "New sector from preset" modal opened from the generation panel.
    /// On confirm the host scaffolds the project and pushes the resulting
    /// state onto a [`super::workspace::BuilderWorkspace`].
    NewFromPreset {
        preset_id: String,
        dest: Utf8PathBuf,
        seed: String,
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

/// §V3: default debounce window between a mutation and the live re-validation
/// pass. Validation walks the full project so we don't want to repay the cost
/// for every keystroke — 200 ms is the same window used by the §G3 live
/// preview.
pub const DEFAULT_VALIDATION_DEBOUNCE_MS: u64 = 200;

/// §V3: combined health summary surfaced by the status bar. Green = clean
/// validation + invariants, yellow = warnings or no report yet, red = at least
/// one validation error or invariant violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthLevel {
    Green,
    Yellow,
    Red,
}

/// §N1: top-level builder tab. Each variant maps 1:1 to a panel module under
/// `gui/src/builder/panels/` (§N2). The router lives in
/// [`crate::builder::panels::nav`]; the active tab is persisted on
/// [`BuilderState::active_tab`] so panels never reach for global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuilderTab {
    Project,
    Map,
    System,
    World,
    Factions,
    Control,
    Regions,
    Routes,
    Subsectors,
    Economy,
    Relations,
    History,
    Personae,
    Hooks,
    Sites,
    Missions,
    Prose,
    Analytics,
    Interestingness,
    Search,
    Diff,
    Briefing,
    Segmentum,
    Export,
}

impl BuilderTab {
    /// Stable display label rendered in the top tab strip.
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "PROJECT",
            Self::Map => "MAP",
            Self::System => "SYSTEM",
            Self::World => "WORLD",
            Self::Factions => "FACTIONS",
            Self::Control => "CONTROL",
            Self::Regions => "REGIONS",
            Self::Routes => "ROUTES",
            Self::Subsectors => "SUBSECTORS",
            Self::Economy => "ECONOMY",
            Self::Relations => "RELATIONS",
            Self::History => "HISTORY",
            Self::Personae => "PERSONAE",
            Self::Hooks => "HOOKS",
            Self::Sites => "SITES",
            Self::Missions => "MISSIONS",
            Self::Prose => "PROSE",
            Self::Analytics => "ANALYTICS",
            Self::Interestingness => "INTERESTINGNESS",
            Self::Search => "SEARCH",
            Self::Diff => "DIFF",
            Self::Briefing => "BRIEFING",
            Self::Segmentum => "SEGMENTUM",
            Self::Export => "EXPORT",
        }
    }

    /// Canonical ordering used by the top tab strip (§N1).
    pub const ALL: &'static [BuilderTab] = &[
        Self::Project,
        Self::Map,
        Self::System,
        Self::World,
        Self::Factions,
        Self::Control,
        Self::Regions,
        Self::Routes,
        Self::Subsectors,
        Self::Economy,
        Self::Relations,
        Self::History,
        Self::Personae,
        Self::Hooks,
        Self::Sites,
        Self::Missions,
        Self::Prose,
        Self::Analytics,
        Self::Interestingness,
        Self::Search,
        Self::Diff,
        Self::Briefing,
        Self::Segmentum,
        Self::Export,
    ];
}

/// §N3: armed tool on the MAP tab. Bound to [`BuilderState::map_tool`] so the
/// router and inspector tabs can read it without reaching for global state.
/// Phase B panels (§S1 / §R2 / §REG2) consume this when the user clicks the
/// hex map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTool {
    /// Default — clicks select the entity under the cursor.
    Select,
    /// Click an empty hex → `BuilderCommand::AddSystem`.
    AddSystem,
    /// Click a system → `BuilderCommand::RemoveSystem`.
    DeleteSystem,
    /// Drag a system → `BuilderCommand::MoveSystem`.
    MoveSystem,
    /// Click two systems → `BuilderCommand::AddRoute`.
    AddRoute,
    /// Brush hexes → `GeneratedSector::add_region_hex` / `remove_region_hex`.
    RegionPaint,
}

impl MapTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::AddSystem => "ADD SYSTEM",
            Self::DeleteSystem => "DELETE SYSTEM",
            Self::MoveSystem => "MOVE SYSTEM",
            Self::AddRoute => "ADD ROUTE",
            Self::RegionPaint => "REGION PAINT",
        }
    }
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
    /// §V3: timestamp of the most recent mutation that has not yet flushed to
    /// a live-validation pass. The UI calls [`Self::pump_validation`] each
    /// frame; once the timer exceeds [`Self::validation_debounce`] the
    /// validation run fires and the timer clears.
    pub validation_dirty_since: Option<Instant>,
    /// §V3: debounce window between mutation and live-validation flush.
    pub validation_debounce: Duration,
    /// §V2: entity selection mailbox — invariant / validation panels write
    /// here so the inspector tabs can focus the offending entity. Each field
    /// is independent so the active inspector reads only the IDs it cares
    /// about.
    pub selected_system_id: Option<SystemId>,
    pub selected_world_id: Option<WorldId>,
    pub selected_route_id: Option<RouteId>,
    pub selected_faction_id: Option<FactionId>,
    pub selected_region_id: Option<String>,
    /// §N1: active top-level tab. Defaults to [`BuilderTab::Project`] so a
    /// blank session lands on the project chrome.
    pub active_tab: BuilderTab,
    /// §N3: armed tool on the MAP tab. Defaults to [`MapTool::Select`].
    pub map_tool: MapTool,
    /// §G2: when true, "Re-roll" preserves `generation.seed`; when false it
    /// derives a fresh seed via blake3("sectorforge:{seed}:reroll:{n}").
    pub seed_locked: bool,
    /// §G2: monotonic counter mixed into the re-roll derivation. Incremented
    /// each time the user clicks "Re-roll" while the seed lock is off.
    pub seed_reroll_counter: u64,
    /// §G3 / §G4: scratch live preview owned by the generation panel.
    pub preview: PreviewState,
    /// §G5: half-open axial-hex rectangle selected for partial regeneration.
    /// `None` means "full sector"; the regen action then refuses to run.
    pub partial_regen_rect: Option<PartialRegenRect>,
    /// §S4: shift-click / rect-drag multi-selection. Always contains
    /// `selected_system_id` when both are populated. Bulk operations in the
    /// SYSTEM tab operate on this set.
    pub selected_systems: BTreeSet<SystemId>,
    /// §S1: id of the system currently being dragged across the hex grid.
    /// Transient — cleared on drag-stop.
    pub drag_system: Option<SystemId>,
    /// §S1: ADD SYSTEM tool target coord awaiting a name entry. The map
    /// panel pops a small naming dialog while this is `Some`.
    pub pending_place: Option<PendingPlace>,
    /// §S1: double-clicked system awaiting a rename in the floating dialog.
    pub pending_rename: Option<PendingRename>,
    /// §S6: drag-drop collision waiting on user choice (Swap / Cancel).
    pub pending_collision: Option<PendingCollision>,
    /// §S4: in-progress rect-select on the map (`(start, current)` corners).
    pub rect_select: Option<(
        sectorforge::sector_model::HexCoord,
        sectorforge::sector_model::HexCoord,
    )>,
    /// §S1: hex render size in screen pixels. Persisted per session so users
    /// can zoom the map without re-tuning each frame.
    pub hex_size: f32,
}

/// §S1: pending ADD SYSTEM input — coord chosen by the user, name being typed.
#[derive(Debug, Clone)]
pub struct PendingPlace {
    pub coord: sectorforge::sector_model::HexCoord,
    pub name: String,
}

/// §S1: pending RENAME input — target id and the editable buffer.
#[derive(Debug, Clone)]
pub struct PendingRename {
    pub id: SystemId,
    pub text: String,
}

/// §S6: drag-drop collision payload surfaced to the swap / cancel dialog.
#[derive(Debug, Clone)]
pub struct PendingCollision {
    pub dragging: SystemId,
    pub target: sectorforge::sector_model::HexCoord,
    pub occupant: SystemId,
}

/// §G5: inclusive rectangle of hex coordinates that
/// [`BuilderState::regenerate_partial`] operates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialRegenRect {
    pub min_q: i32,
    pub min_r: i32,
    pub max_q: i32,
    pub max_r: i32,
}

impl PartialRegenRect {
    /// Build a rect from any two corners. Always normalises so `min <= max`.
    #[must_use]
    pub fn from_corners(
        a: sectorforge::sector_model::HexCoord,
        b: sectorforge::sector_model::HexCoord,
    ) -> Self {
        Self {
            min_q: a.q.min(b.q),
            min_r: a.r.min(b.r),
            max_q: a.q.max(b.q),
            max_r: a.r.max(b.r),
        }
    }

    #[must_use]
    pub fn contains(&self, coord: sectorforge::sector_model::HexCoord) -> bool {
        coord.q >= self.min_q
            && coord.q <= self.max_q
            && coord.r >= self.min_r
            && coord.r <= self.max_r
    }
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
            validation_dirty_since: None,
            validation_debounce: Duration::from_millis(DEFAULT_VALIDATION_DEBOUNCE_MS),
            selected_system_id: None,
            selected_world_id: None,
            selected_route_id: None,
            selected_faction_id: None,
            selected_region_id: None,
            active_tab: BuilderTab::Project,
            map_tool: MapTool::Select,
            seed_locked: false,
            seed_reroll_counter: 0,
            preview: PreviewState::new(),
            partial_regen_rect: None,
            selected_systems: BTreeSet::new(),
            drag_system: None,
            pending_place: None,
            pending_rename: None,
            pending_collision: None,
            rect_select: None,
            hex_size: 28.0,
        }
    }

    /// §S1: focus a single system. Replaces any multi-selection with `{id}`.
    pub fn focus_system(&mut self, id: SystemId) {
        self.selected_systems.clear();
        self.selected_systems.insert(id.clone());
        self.selected_system_id = Some(id);
    }

    /// §S4: shift-click toggle — add/remove the system from `selected_systems`
    /// while leaving `selected_system_id` pointing at the most recent pick.
    pub fn toggle_system_selection(&mut self, id: SystemId) {
        if self.selected_systems.contains(&id) {
            self.selected_systems.remove(&id);
            if self.selected_system_id.as_ref() == Some(&id) {
                self.selected_system_id = self.selected_systems.iter().next().cloned();
            }
        } else {
            self.selected_systems.insert(id.clone());
            self.selected_system_id = Some(id);
        }
    }

    /// §S5: drop a freshly generated system at `coord`. The new system is built
    /// via [`sectorforge::generate_system_standalone`] using the in-memory
    /// catalogs, then committed through the command bus as
    /// [`super::command::BuilderCommand::ReplaceSystem`]. Any existing system
    /// at `coord` is replaced (the prior payload survives in the command log
    /// so undo restores it). Pinned systems at `coord` refuse the operation.
    pub fn generate_system_here(
        &mut self,
        coord: sectorforge::sector_model::HexCoord,
        index: usize,
        seed_override: Option<&str>,
    ) -> Result<SystemId, super::errors::BuilderError> {
        if coord.q < 0
            || coord.r < 0
            || (coord.q as u32) >= self.sector.width
            || (coord.r as u32) >= self.sector.height
        {
            return Err(super::errors::BuilderError::ParseFailed {
                file: "generate-system-here".into(),
                message: format!(
                    "coord ({},{}) outside sector {}x{}",
                    coord.q, coord.r, self.sector.width, self.sector.height
                ),
            });
        }
        if let Some(existing) = self.sector.systems.iter().find(|s| s.coord == coord) {
            if self.pinned_systems.contains(&existing.id) {
                return Err(super::errors::BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: format!(
                        "system {} at ({},{}) is pinned",
                        existing.id, coord.q, coord.r
                    ),
                });
            }
        }
        let mut input = self.synthesize_project_input().ok_or_else(|| {
            super::errors::BuilderError::ParseFailed {
                file: "generate-system-here".into(),
                message: "worlds catalog not loaded".into(),
            }
        })?;
        if let Some(seed) = seed_override {
            input.config.generation.seed = seed.to_string();
        }
        let new_sys =
            sectorforge::generate_system_standalone(input, index, coord).map_err(|e| {
                super::errors::BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: e.to_string(),
                }
            })?;
        let id = new_sys.id.clone();
        let cmd = super::command::BuilderCommand::ReplaceSystem {
            coord,
            new_system: Box::new(new_sys),
            before: None,
        };
        self.run(cmd)?;
        Ok(id)
    }

    /// §G2: derive the next seed. When [`Self::seed_locked`] is true, returns
    /// the current seed unchanged. Otherwise increments
    /// [`Self::seed_reroll_counter`] and returns the
    /// `blake3("sectorforge:{seed}:reroll:{n}")` digest.
    pub fn reroll_seed(&mut self) -> String {
        if self.seed_locked {
            return self.config.generation.seed.clone();
        }
        self.seed_reroll_counter = self.seed_reroll_counter.wrapping_add(1);
        let new_seed = super::preview::derive_reroll_seed(
            &self.config.generation.seed,
            self.seed_reroll_counter,
        );
        self.config.generation.seed = new_seed.clone();
        new_seed
    }

    /// §G4: promote the live preview to the active sector. Manual edits to
    /// systems flagged in [`Self::pinned_systems`] survive — their
    /// pre-preview snapshot replaces the regenerated entry at the same id.
    /// Returns `true` on success; `false` when no preview is ready.
    pub fn apply_preview(&mut self) -> bool {
        let Some(mut preview_sector) = self.preview.sector.take() else {
            return false;
        };
        if !self.pinned_systems.is_empty() {
            let pinned: BTreeMap<_, _> = self
                .sector
                .systems
                .iter()
                .filter(|s| self.pinned_systems.contains(&s.id))
                .map(|s| (s.id.clone(), s.clone()))
                .collect();
            if !pinned.is_empty() {
                let mut seen: BTreeSet<_> = BTreeSet::new();
                for sys in preview_sector.systems.iter_mut() {
                    if let Some(orig) = pinned.get(&sys.id) {
                        *sys = orig.clone();
                        seen.insert(sys.id.clone());
                    }
                }
                for (id, orig) in pinned.into_iter() {
                    if !seen.contains(&id) {
                        preview_sector.systems.push(orig);
                    }
                }
                preview_sector.systems.sort_by(|a, b| a.id.cmp(&b.id));
            }
        }
        self.sector = preview_sector;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.preview.clear();
        true
    }

    /// §G5: regenerate only the systems whose coord falls inside
    /// [`Self::partial_regen_rect`]. Pinned systems are skipped. Each
    /// regenerated slot is rebuilt via
    /// [`sectorforge::generate_system_standalone`] so the rest of the sector
    /// stays untouched. Returns the number of systems regenerated, or an
    /// error when no rect is selected / no project input can be synthesised.
    pub fn regenerate_partial(&mut self) -> Result<usize, BuilderError> {
        let rect = self
            .partial_regen_rect
            .ok_or_else(|| BuilderError::ParseFailed {
                file: "partial-regen".into(),
                message: "no rectangle selected".into(),
            })?;
        let input = self
            .synthesize_project_input()
            .ok_or_else(|| BuilderError::ParseFailed {
                file: "partial-regen".into(),
                message: "project worlds catalog is not loaded".into(),
            })?;

        let mut regenerated = 0usize;
        let mut replacements: Vec<sectorforge::sector_model::GeneratedSystem> = Vec::new();
        for sys in &self.sector.systems {
            if !rect.contains(sys.coord) {
                continue;
            }
            if self.pinned_systems.contains(&sys.id) {
                continue;
            }
            let new_sys =
                sectorforge::generate_system_standalone(input.clone(), sys.index, sys.coord)
                    .map_err(|e| BuilderError::ParseFailed {
                        file: format!("partial-regen:{}", sys.id),
                        message: e.to_string(),
                    })?;
            replacements.push(new_sys);
            regenerated += 1;
        }
        for new_sys in replacements {
            if let Some(slot) = self.sector.systems.iter_mut().find(|s| s.id == new_sys.id) {
                *slot = new_sys;
            }
        }
        self.sector.systems.sort_by(|a, b| a.id.cmp(&b.id));
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        if regenerated > 0 {
            self.dirty = true;
            self.invariant_report = Some(check_sector(&self.sector));
            self.mark_validation_dirty();
            self.trigger_auto_save();
        }
        Ok(regenerated)
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
        self.mark_validation_dirty();
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
        self.mark_validation_dirty();
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
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
    }

    /// §V3: arm the debounced live-validation timer. Cheap — just stamps
    /// `Instant::now`. The actual `validate(&project)` call happens in
    /// [`Self::pump_validation`] after `validation_debounce` elapses.
    pub fn mark_validation_dirty(&mut self) {
        self.validation_dirty_since = Some(Instant::now());
    }

    /// §V3: per-frame poll from the UI. When the debounce window has elapsed
    /// since the last mutation, build a synthetic [`ProjectInput`] from the
    /// in-memory catalogs and run [`validate`] against it. Returns `true`
    /// when a fresh report was produced this tick so the caller can request a
    /// repaint.
    pub fn pump_validation(&mut self) -> bool {
        let Some(since) = self.validation_dirty_since else {
            return false;
        };
        if since.elapsed() < self.validation_debounce {
            return false;
        }
        self.revalidate_now();
        true
    }

    /// §V3: synchronous re-validation. Clears the debounce timer regardless
    /// of whether catalogs were complete enough to build a `ProjectInput` —
    /// otherwise an incomplete catalog would re-arm every tick.
    pub fn revalidate_now(&mut self) {
        self.validation_dirty_since = None;
        if let Some(input) = self.synthesize_project_input() {
            self.validation_report = Some(validate(&input));
        }
    }

    /// §V3: build a read-only [`ProjectInput`] from the in-memory catalogs so
    /// [`sectorforge::validation::validate`] can run without touching disk.
    /// Returns `None` when the worlds catalog is missing — validation needs at
    /// minimum a workbook to walk.
    pub fn synthesize_project_input(&self) -> Option<ProjectInput> {
        use sectorforge::input::ProjectInput;
        let worlds = self.data_catalogs.worlds.as_ref()?;
        let (world_tables, world_rows) = worlds.to_loader_inputs();
        let authored_features = worlds.resolved_features().ok();
        let root_dir = self
            .project_path
            .clone()
            .unwrap_or_else(|| Utf8PathBuf::from("."));
        Some(ProjectInput {
            root_dir,
            config: self.config.clone(),
            world_tables,
            world_rows,
            authored_features,
            names: self.data_catalogs.names.clone().unwrap_or_default(),
            factions: self
                .data_catalogs
                .factions
                .as_ref()
                .map(|f| f.factions.clone())
                .unwrap_or_default(),
            route_rules: self.data_catalogs.route_rules.clone().unwrap_or_default(),
            relations: self.data_catalogs.relations.clone().unwrap_or_default(),
            regions: self.data_catalogs.regions.clone().unwrap_or_default(),
            economy: self.data_catalogs.economy.clone().unwrap_or_default(),
            history: self.data_catalogs.history.clone().unwrap_or_default(),
            personae: sectorforge::personae::PersonaeConfig::default(),
            sites: sectorforge::sites::SitesConfig::default(),
            input_digests: BTreeMap::new(),
        })
    }

    /// §V3: derive the status-bar health pip from validation + invariants.
    /// Red — any validation error or invariant violation. Yellow — warnings or
    /// no report yet. Green — both clean.
    pub fn health_level(&self) -> HealthLevel {
        let v_has_err = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.errors.is_empty());
        let inv_has_violation = self
            .invariant_report
            .as_ref()
            .is_some_and(|r| !r.violations.is_empty());
        if v_has_err || inv_has_violation {
            return HealthLevel::Red;
        }
        let v_has_warn = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.warnings.is_empty());
        let v_missing = self.validation_report.is_none();
        let inv_missing = self.invariant_report.is_none();
        if v_has_warn || v_missing || inv_missing {
            return HealthLevel::Yellow;
        }
        HealthLevel::Green
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

    // ── §V3 ──────────────────────────────────────────────────────────────

    #[test]
    fn mutation_arms_validation_debounce() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert!(state.validation_dirty_since.is_none());
        add_n_systems(&mut state, 1);
        assert!(state.validation_dirty_since.is_some());
    }

    #[test]
    fn pump_validation_holds_within_debounce_window() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.validation_debounce = Duration::from_secs(5);
        add_n_systems(&mut state, 1);
        assert!(!state.pump_validation());
        assert!(state.validation_dirty_since.is_some());
    }

    #[test]
    fn pump_validation_flushes_after_debounce() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.validation_debounce = Duration::from_millis(0);
        add_n_systems(&mut state, 1);
        // No worlds catalog => synth returns None; debounce still clears so
        // we don't burn cycles every frame.
        assert!(state.pump_validation());
        assert!(state.validation_dirty_since.is_none());
    }

    #[test]
    fn revalidate_now_populates_report_when_worlds_present() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.data_catalogs.worlds = Some(sectorforge::worlds_toml::WorldsConfig::default());
        state.revalidate_now();
        assert!(state.validation_report.is_some(), "report should be set");
    }

    #[test]
    fn health_level_red_on_invariant_violation() {
        use sectorforge::invariants::{InvariantReport, InvariantViolation};
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.invariant_report = Some(InvariantReport {
            ok: false,
            violations: vec![InvariantViolation {
                code: "X".into(),
                message: "m".into(),
                path: None,
            }],
        });
        assert_eq!(state.health_level(), HealthLevel::Red);
    }

    #[test]
    fn health_level_yellow_when_reports_missing() {
        let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert_eq!(state.health_level(), HealthLevel::Yellow);
    }

    // ── §N1 / §N3 ────────────────────────────────────────────────────────

    #[test]
    fn default_tab_is_project() {
        let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert_eq!(state.active_tab, BuilderTab::Project);
    }

    #[test]
    fn default_map_tool_is_select() {
        let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert_eq!(state.map_tool, MapTool::Select);
    }

    #[test]
    fn builder_tab_all_is_full_n1_set() {
        // §N1 lists 24 tabs (PROJECT..EXPORT).
        assert_eq!(BuilderTab::ALL.len(), 24);
        assert_eq!(BuilderTab::ALL[0], BuilderTab::Project);
        assert_eq!(BuilderTab::ALL[1], BuilderTab::Map);
        assert_eq!(*BuilderTab::ALL.last().unwrap(), BuilderTab::Export);
    }

    #[test]
    fn builder_tab_labels_are_uppercase_words() {
        for tab in BuilderTab::ALL {
            let label = tab.label();
            assert!(!label.is_empty());
            assert_eq!(label, label.to_uppercase());
        }
    }

    #[test]
    fn health_level_green_when_both_clean() {
        use sectorforge::invariants::InvariantReport;
        use sectorforge::validation::{ValidationReport, WorldWorkbookValidation};
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.invariant_report = Some(InvariantReport {
            ok: true,
            violations: Vec::new(),
        });
        state.validation_report = Some(ValidationReport {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            world_workbook: WorldWorkbookValidation {
                row_count: 0,
                usable_candidate_count: 0,
                excluded_row_count: 0,
                exclusion_reasons: Default::default(),
                key_table_counts: Default::default(),
            },
        });
        assert_eq!(state.health_level(), HealthLevel::Green);
    }
}
