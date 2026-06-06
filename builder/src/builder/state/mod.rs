//! Top-level builder state (§D5). One instance is owned by the eframe app
//! while the builder is in focus.
//!
//! # R1: single source of truth
//!
//! `BuilderState` owns the sole live sector as `Arc<GeneratedSector>`. Per
//! docs/BUILDER_REQS §R1 the GUI thread is the sole writer: every mutation
//! route holds an exclusive `&mut BuilderState` and edits the sector through
//! [`BuilderState::sector_mut`] ([`Arc::make_mut`]). The `Arc` is not for
//! shared *writing* — it exists so a background job (e.g. a bundle export) can
//! take an O(1) read-only snapshot via `Arc::clone` instead of deep-copying a
//! 100 MB+ sector on the UI thread. While such a snapshot is alive the next
//! edit copies-on-write, so the worker keeps reading a stable sector and never
//! mutates in place; results come back over [`crate::jobs`] via mpsc.
//!
//! # R10: panel contract
//!
//! Panels live in `builder/src/builder/panels/` and are functions with the
//! signature `fn(ui: &mut egui::Ui, state: &mut BuilderState)`. They never
//! hold module-level mutable state, never use `lazy_static`, and never carry
//! raw string IDs across boundaries. Modal state lives in
//! `BuilderState::feedback.modal`.
//!
//! # Module layout
//!
//! The struct itself plus its `new_blank` constructor live here. Method
//! `impl` blocks are split across sibling files by concern:
//!
//! | Submodule | Concern |
//! |---|---|
//! | [`types`] | Auxiliary enums + dialog payloads + `MapViewCache` + constants |
//! | [`selection`] | `focus_system`, `toggle_system_selection` |
//! | [`undo`] | Command-bus `run` + `undo` / `redo` + ring-buffer + auto-save |
//! | [`derivations`] | Economy / relations / chronicle re-derive + validation pump + health |
//! | [`regions_ops`] | §REG1..§REG3 warp-region paint / add / remove / update / next id |
//! | [`generation_ops`] | §G2..§G5 + §S5 + §W4 preview / per-system regen / world reroll |

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::GeneratedSector;
use sectorforge::{InvariantReport, ValidationReport};

use super::analytics_run::AnalyticsState;
use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::{DerivationCache, DerivationLedger};
use super::diff_run::DiffState;
use super::export_run::ExportState;
use super::file_watcher::FileWatcher;
use super::index::BuilderIndex;
use super::random_run::RandomGenState;
use super::search_run::SearchState;
use super::segmentum_run::SegmentumState;
use super::snapshot::Snapshot;

/// TF-NT-3: identifies a cached `feature_weights_for_world` entry by
/// `(sys_idx, w_idx)` plus the digest of the (world_type, star_colour,
/// notable_features, worlds_catalog) slice it was computed from. The digest
/// guarantees stale entries are ignored even when keys collide on indices.
pub type FeatureWeightsCacheKey = (usize, usize);

/// TF-NT-3: cached value; pair of (digest, weights) so the panel can verify
/// freshness without re-running the expensive `synthesize_project_input` /
/// `build_pool` pipeline on every frame.
#[derive(Debug, Clone)]
pub struct FeatureWeightsCacheValue {
    pub digest: String,
    pub weights: std::sync::Arc<BTreeMap<String, f64>>,
}

pub mod derivations;
pub mod generation_ops;
pub mod nav;
pub mod panel_state;
pub mod regions_ops;
pub mod selection;
pub mod types;
pub mod undo;

#[cfg(test)]
mod tests;

pub use nav::EntityRef;
pub(crate) use panel_state::{
    BriefingPanelState, ConflictPanelState, DragPendingState, EconomyPanelState, FeedbackState,
    GenerationState, HiddenRoutesState, HistoryPanelState, HooksPanelState,
    InterestingnessPanelState, MapViewState, MissionsPanelState, PersonaePanelState,
    RegionGrowState, RelationsPanelState, RouteBulkState, SelectionState, SitesPanelState,
    SystemViewState, ThemePanelState,
};
pub use types::{
    validate_toml, BuilderTab, ConfirmAction, ControlOverlay, HealthLevel, HistoryAnchorKind,
    HistoryWizardState, JobHandle, MapTool, MapViewCache, ModalKind, OpenTomlBuffer,
    PartialRegenRect, PendingBulkRename, PendingCollision, PendingPlace, PendingRegionRename,
    PendingRename, PendingWorldRename, SectorContextMenu, SectorMenuTarget, SystemBitmapPreview,
    SystemContextMenu, SystemMenuTarget, TickLogEntry, TickLogScope, TomlEditorState,
    DEFAULT_COMMAND_LOG_CAPACITY, DEFAULT_VALIDATION_DEBOUNCE_MS,
};

/// The live, editable sector, held behind an `Arc` so a background job (e.g. a
/// bundle export) can take an O(1) read-only snapshot via [`LiveSector::share`]
/// instead of deep-copying a 100 MB+ sector on the UI thread.
///
/// Reads go through [`Deref`] (no clone). Mutations go through [`DerefMut`],
/// which routes every in-place edit through [`Arc::make_mut`]: a no-op while the
/// sector is uniquely held (the steady state), copying on write only while a
/// worker still shares the allocation — so an edit during an in-flight export
/// never disturbs the stable snapshot the worker is serialising.
#[derive(Debug, Clone)]
pub struct LiveSector(Arc<GeneratedSector>);

impl LiveSector {
    #[must_use]
    pub fn new(sector: GeneratedSector) -> Self {
        Self(Arc::new(sector))
    }

    /// O(1) read-only snapshot (`Arc::clone`) for a background job.
    #[must_use]
    pub fn share(&self) -> Arc<GeneratedSector> {
        Arc::clone(&self.0)
    }
}

impl From<GeneratedSector> for LiveSector {
    fn from(sector: GeneratedSector) -> Self {
        Self::new(sector)
    }
}

impl Deref for LiveSector {
    type Target = GeneratedSector;
    fn deref(&self) -> &GeneratedSector {
        &self.0
    }
}

impl DerefMut for LiveSector {
    fn deref_mut(&mut self) -> &mut GeneratedSector {
        Arc::make_mut(&mut self.0)
    }
}

impl serde::Serialize for LiveSector {
    /// Serialize transparently as the inner [`GeneratedSector`] so `&state.sector`
    /// keeps round-tripping through `serde_json` (auto-save, session, exports).
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&*self.0, serializer)
    }
}

pub struct BuilderState {
    pub(crate) sector: LiveSector,
    pub(crate) project_path: Option<Utf8PathBuf>,
    pub(crate) config: AppConfig,
    pub(crate) data_catalogs: DataCatalogs,
    pub(crate) index: BuilderIndex,
    pub(crate) command_log: Vec<BuilderCommand>,
    /// Position of the cursor inside `command_log`. Used for redo: commands
    /// past `cursor` are redo candidates; commands before are undoable.
    pub(crate) command_cursor: usize,
    pub(crate) snapshots: Vec<Snapshot>,
    /// §U2: bounded ring-buffer cap for `command_log`. When the log exceeds
    /// this size after a new mutation, the oldest commands are dropped and
    /// `command_cursor` plus snapshot positions are shifted accordingly.
    /// Default 200 — see [`DEFAULT_COMMAND_LOG_CAPACITY`].
    pub(crate) command_log_capacity: usize,
    /// §Q1: pinned systems live in this side-table — never written to JSON.
    pub(crate) pinned_systems: BTreeSet<SystemId>,
    pub(crate) pinned_worlds: BTreeSet<WorldId>,
    pub(crate) derivation_cache: DerivationCache,
    /// §39 (LD1..LD4) live-derivation ledger. Tracks per-kind input
    /// fingerprints, staleness, and in-flight recomputes so the command bus can
    /// invalidate exactly the overlays a mutation touches (LD2) and the overlay
    /// panels can read freshness before rendering (LD4). See
    /// [`Self::invalidate_derivations`] / [`Self::ensure_fresh`].
    pub(crate) derivations: DerivationLedger,
    /// §39 LD3 — transient store of in-flight **off-thread** overlay
    /// re-derivations, keyed by [`DerivationKind`]. Each slot holds the worker
    /// [`sectorforge_gui_core::jobs::JobHandle`] plus the input fingerprint
    /// captured at dispatch (the stale-guard). Runtime UI state only: never
    /// serialized and never undoable — written directly (not via a
    /// `BuilderCommand`), like the sibling `search` job slot. Dispatched in
    /// [`Self::dispatch_background_derivations`] and drained in
    /// [`Self::pump_derivation_jobs`].
    pub(crate) derivation_jobs: super::derivation_jobs::DerivationJobs,
    pub(crate) dirty: bool,
    pub(crate) auto_save_path: Option<Utf8PathBuf>,
    /// Status-bar / modal feedback channel grouped on [`FeedbackState`]: the
    /// last error of each kind, the active modal, the last menu action trail,
    /// and the §V3 live-validation debounce timer. Transient view state.
    pub(crate) feedback: FeedbackState,
    /// TF-NT-3: cached output of `feature_weights_for_world`. Keyed by
    /// `(sys_idx, w_idx, input_digest)`. Stale entries are simply ignored
    /// (digest mismatch) — the cache grows bounded by the number of worlds.
    /// Cleared on project reload via [`Self::clear_feature_weights_cache`].
    pub(crate) feature_weights_cache: BTreeMap<FeatureWeightsCacheKey, FeatureWeightsCacheValue>,
    pub(crate) validation_report: Option<ValidationReport>,
    pub(crate) invariant_report: Option<InvariantReport>,
    pub(crate) pending_jobs: Vec<JobHandle>,
    /// §P4: per-file dirty markers keyed by project-relative path. Populated
    /// by panels that edit individual catalogs and cleared on save.
    pub(crate) dirty_files: BTreeSet<String>,
    /// §P4: file currently selected in the PROJECT tree. Optional — the §PF2
    /// TOML editor reads this to decide which buffer to open.
    pub(crate) selected_file: Option<Utf8PathBuf>,
    /// §PF2 / §PF4 / §PF5: raw-TOML editor surface on the PROJECT tab. Holds the
    /// open file buffers ("editor tabs"), their dirty/validation state, and the
    /// active tab. In-memory only.
    pub(crate) toml_editor: TomlEditorState,
    /// §P4 + §P5: project-relative mtime snapshot taken at load time. The
    /// file watcher uses this baseline to spot external changes; the tree
    /// view uses it to draw the "● dirty" marker when the catalog mirror
    /// diverges from disk.
    pub(crate) file_mtimes: BTreeMap<String, std::time::SystemTime>,
    /// §P5: background watcher polling the project directory for external
    /// changes. `None` when no project is open.
    pub(crate) file_watcher: Option<FileWatcher>,
    /// §V4 — strict validation toggle. When set, validation *warnings* are
    /// promoted to errors for the status-bar health pip
    /// ([`Self::health_level`]) and the §V6 pre-export gate
    /// ([`Self::export_block_reason`]), matching
    /// `sectorforge generate --strict`. Off by default. In-memory only.
    pub(crate) validation_strict: bool,
    /// §COLUMNS §6.1 — when set, the left cluster nav rail is hidden and only a
    /// `☰` toggle in the top bar brings it back, so a master-detail tab can
    /// reclaim the full width on a narrow window. In-memory view state, off by
    /// default (the rail shows).
    pub(crate) nav_rail_collapsed: bool,
    /// §V2 / §LINK1: entity selection mailbox + cross-tab navigation stacks.
    /// Grouped on [`SelectionState`] — transient view state (§R4 carve-out).
    pub(crate) selection: SelectionState,
    /// §N1: active top-level tab. Defaults to [`BuilderTab::Project`] so a
    /// blank session lands on the project chrome.
    pub(crate) active_tab: BuilderTab,
    /// MAP-tab view/render state: zoom, armed tool, overlay/heatmap modes, the
    /// lazy render cache, and the open right-click context menus. Grouped on
    /// [`MapViewState`].
    pub(crate) map_view: MapViewState,
    /// §G2..§G5 / §W4 generation-panel scratch grouped on [`GenerationState`].
    pub(crate) generation: GenerationState,
    /// Transient drag / pending-dialog scratch grouped on [`DragPendingState`].
    pub(crate) drag: DragPendingState,
    /// §R4: bulk route predicate controls grouped on [`RouteBulkState`].
    pub(crate) route_bulk: RouteBulkState,
    /// §R6: explicit hidden-route builder controls grouped on
    /// [`HiddenRoutesState`].
    pub(crate) hidden_routes: HiddenRoutesState,
    /// §C3: per-(world, faction) dominance lock. When the pair is present the
    /// CONTROL panel leaves `WorldFactionPresence::dominance` alone; otherwise
    /// it is recomputed from the presence's local-control score every time the
    /// panel is rendered.
    pub(crate) dominance_locked: BTreeSet<(WorldId, FactionId)>,
    /// §C5: per-system `primary_factions` override lock. When the system is in
    /// the set the CONTROL panel preserves whatever is in
    /// `GeneratedSystem::primary_factions`; otherwise it auto-derives the
    /// top-3 from `derive_system_control`.
    pub(crate) primary_factions_locked: BTreeSet<SystemId>,
    /// §REG3: scratch state for the "Grow seeded region" form on the REGIONS
    /// tab. Grouped on [`RegionGrowState`].
    pub(crate) region_grow: RegionGrowState,
    /// §SUB2: live `target_systems_per_subsector` for the recluster button.
    /// Defaults to [`sectorforge::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR`].
    /// Folded into the [`MapViewCache`] digest so changes invalidate the cache
    /// and the renderer rebuilds with the new clustering.
    pub(crate) subsector_target_systems: u32,
    /// §SUB3: per-system manual reassignment table. After the lib runs
    /// `build_subsectors`, the panel reapplies these overrides so manual moves
    /// survive reclustering. Key = `SystemId`, value = destination subsector id.
    pub(crate) subsector_system_overrides: BTreeMap<SystemId, String>,
    /// §SUB3: subsectors the user has touched manually. Stored separately from
    /// the system overrides so the panel can flag a cluster as "manual" even
    /// when its current member list happens to match the algorithmic output.
    pub(crate) subsector_manual: BTreeSet<String>,
    /// §SUB4: capital override per subsector. Overrides the algorithmic
    /// `summary.subsector_capital_system_id` after the lib clusters.
    pub(crate) subsector_capital_overrides: BTreeMap<String, SystemId>,
    /// §SUB5: per-subsector colour override. Default for each subsector is the
    /// `FactionStyle` fill of its controlling faction; the override is only
    /// recorded when the user picks a custom swatch.
    pub(crate) subsector_colour_overrides: BTreeMap<String, [u8; 3]>,
    /// §E1: per-world `ResourceVector` override. When present
    /// [`Self::recompute_economy`] pins the world's vector to this value,
    /// recomputes the shortage list, and lets the stranded check re-run from
    /// the override (the routes layer is unchanged so a manual surplus can
    /// still resolve a real deficit). Never written to JSON.
    pub(crate) world_economy_overrides: BTreeMap<WorldId, sectorforge::economy::ResourceVector>,
    /// §E2: per-world `StrategicOutput` override. When present
    /// [`Self::recompute_economy`] pins the world's 10-axis strategic vector
    /// and the system-level aggregates inherit the override before the
    /// dependency-edge pass runs.
    pub(crate) world_strategic_overrides: BTreeMap<WorldId, sectorforge::economy::StrategicOutput>,
    /// §E3: per-system `TitheStatus` override applied after the auto-derived
    /// pass so users can mark a system Delinquent/Falsified by hand.
    pub(crate) system_tithe_overrides: BTreeMap<SystemId, sectorforge::economy::TitheStatus>,
    /// §E3: per-system `SupplyRisk` override.
    pub(crate) system_supply_overrides: BTreeMap<SystemId, sectorforge::economy::SupplyRisk>,
    /// §E3: per-system `StrategicPriority` override.
    pub(crate) system_priority_overrides: BTreeMap<SystemId, sectorforge::economy::StrategicPriority>,
    /// §35 T5: cached per-system bitmap preview texture, keyed by a digest of
    /// (system id, theme, faction_fill, scale) so the PNG renderer only re-runs
    /// when one of those changes. Never serialized (runtime-only).
    pub(crate) system_bitmap_preview: Option<SystemBitmapPreview>,
    /// §E6: ECONOMY-tab lifeline-lane highlight controls grouped on
    /// [`EconomyPanelState`].
    pub(crate) economy_panel: EconomyPanelState,
    /// §REL1 / §REL9: RELATIONS-tab runtime grouped on [`RelationsPanelState`].
    pub(crate) relations_panel: RelationsPanelState,
    /// §H5..§I5: HISTORY-tab runtime grouped on [`HistoryPanelState`].
    pub(crate) history_panel: HistoryPanelState,
    /// §35 T2: custom-theme editor scratch grouped on [`ThemePanelState`].
    pub(crate) theme_panel: ThemePanelState,
    /// §AR3: per-axis enable mask used by `BuilderCommand::AutoAssignArchetypes`.
    /// Defaults to all axes enabled. Stored on `BuilderState` only — never
    /// serialised into `sector.json` because `src/archetypes.rs` has no TOML
    /// config layer.
    pub(crate) archetype_flags: super::command::ArchetypeApplyFlags,
    /// §CF2: per-system "override aggregate" toggle. When the system id is in
    /// the set the SYSTEM-level conflict editor pins
    /// `GeneratedSystem::conflict` to whatever the panel saved; otherwise the
    /// section re-derives via `conflict::derive_system_conflict` each frame.
    /// Never serialised — purely an editor mode flag.
    pub(crate) system_conflict_override: BTreeSet<SystemId>,
    /// §CF4 / §CF5: CONFLICT-section runtime (ticks-to-advance scratch + tick
    /// log) grouped on [`ConflictPanelState`].
    pub(crate) conflict_panel: ConflictPanelState,
    /// §PER1..§PER5: latest dramatis-personae overlay. Personae are not part
    /// of `GeneratedSector`, so the builder caches the most recent
    /// `derive_personae` result here. Rebuilt by [`Self::recompute_personae`].
    pub(crate) personae_report: Option<sectorforge::personae::PersonaeReport>,
    /// §PER2..§PER3: PERSONAE-tab runtime grouped on [`PersonaePanelState`].
    pub(crate) personae_panel: PersonaePanelState,
    /// §HK1..§HK6: latest plot-hook overlay. Hooks are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_hooks_with` result here. Rebuilt by [`Self::recompute_hooks`].
    pub(crate) hooks_report: Option<sectorforge::hooks::HooksReport>,
    /// §HK1..§HK6: HOOKS-tab runtime knobs grouped on [`HooksPanelState`].
    pub(crate) hooks_panel: HooksPanelState,
    /// §ST1..§ST4: latest planetary-sites overlay. Sites are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_sites_with` result here. Rebuilt by [`Self::recompute_sites`].
    pub(crate) sites_report: Option<sectorforge::sites::SitesReport>,
    /// §ST1..§ST4: SITES-tab runtime knobs grouped on [`SitesPanelState`].
    pub(crate) sites_panel: SitesPanelState,
    /// §M1..§M5: latest mission-seed overlay. Missions are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_missions_with` result here. Rebuilt by
    /// [`Self::recompute_missions`].
    pub(crate) missions_report: Option<sectorforge::missions::MissionsReport>,
    /// §M1..§M5: MISSIONS-tab runtime knobs grouped on [`MissionsPanelState`].
    pub(crate) missions_panel: MissionsPanelState,
    /// §PR1..§PR4: latest gazetteer prose overlay. Prose is not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `prose::derive_with` result here. Rebuilt by
    /// [`Self::recompute_prose`].
    pub(crate) prose_report: Option<sectorforge::prose::ProseReport>,
    /// §PR4: when true, mutations that touch the prose catalog trigger an
    /// immediate [`Self::recompute_prose`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub(crate) prose_auto_recompute: bool,
    /// §BR1..§BR5: BRIEFING-tab runtime grouped on [`BriefingPanelState`].
    pub(crate) briefing_panel: BriefingPanelState,
    /// §INT1..§INT4: INTERESTINGNESS-tab runtime grouped on
    /// [`InterestingnessPanelState`].
    pub(crate) interestingness_panel: InterestingnessPanelState,
    /// SYSTEM-tab embedded `SystemView` layout + pixel side length grouped on
    /// [`SystemViewState`].
    pub(crate) system_view: SystemViewState,
    /// §SR1..§SR5: SEARCH tab runtime — the editable `wishes.toml` document,
    /// the off-thread constraint search job, its live progress snapshot, and
    /// the latest outcome. In-memory only; the wishes doc is round-tripped to
    /// disk separately. See [`super::search_run::SearchState`].
    pub(crate) search: SearchState,
    /// §DF1..§DF5: DIFF tab runtime — the two scratch sector slots, the
    /// diff/tick filter config, and the most recently computed diff. In-memory
    /// only. See [`super::diff_run::DiffState`].
    pub(crate) diff: DiffState,
    /// §A1..§A4: ANALYTICS tab runtime — the editable `[analyze]` config, the
    /// strict CI-parity toggle, the most recently computed `SectorAnalysis`,
    /// and the last export folder. In-memory only. See
    /// [`super::analytics_run::AnalyticsState`].
    pub(crate) analytics: AnalyticsState,
    /// §SG1..§SG5: SEGMENTUM tab runtime — the editable `segmentum.toml`
    /// document, the off-thread compose job + its per-child progress, and the
    /// most recently composed segmentum (super-manifest + inter-sector links).
    /// In-memory only; the document round-trips to disk separately. See
    /// [`super::segmentum_run::SegmentumState`].
    pub(crate) segmentum: SegmentumState,
    /// RANDOM.md §7.4: off-thread random-sector generation runtime (job +
    /// live progress snapshot) backing the §7.3 wizard's progress popup.
    pub(crate) random_gen: RandomGenState,
    /// §EX1..§EX8: EXPORT tab runtime — the chosen output folder, the
    /// standalone-system export form, the cached markdown preview, and the
    /// last export error. The per-format / bitmap / HTML knobs live on
    /// `config.outputs`, not here. In-memory only. See
    /// [`super::export_run::ExportState`].
    pub(crate) export: ExportState,
}

impl BuilderState {
    /// Mutable access to the live sector. Uses [`Arc::make_mut`]: a no-op when
    /// the sector is uniquely held (the steady state), copying on write only if
    /// a background job (e.g. an in-flight bundle export) still shares the
    /// allocation — so edits never disturb the snapshot a worker is reading.
    /// All in-place sector mutations must go through this rather than the `Arc`.
    pub fn sector_mut(&mut self) -> &mut GeneratedSector {
        &mut self.sector
    }

    /// O(log n) lookup of a system by id. Backed by [`BuilderIndex::systems`].
    /// Prefer this over `state.sector.systems.iter().find(|s| s.id == ...)`.
    #[must_use]
    pub fn system_by_id(
        &self,
        id: &sectorforge::ids::SystemId,
    ) -> Option<&sectorforge::sector_model::GeneratedSystem> {
        self.index
            .systems
            .get(id)
            .and_then(|&i| self.sector.systems.get(i))
    }

    /// O(log n) lookup of the slice index for a system id.
    #[must_use]
    pub fn system_index_by_id(&self, id: &sectorforge::ids::SystemId) -> Option<usize> {
        self.index.systems.get(id).copied()
    }

    /// Construct a brand-new blank builder session sized `width x height`.
    pub fn new_blank(id: &str, title: &str, seed: &str, width: u32, height: u32) -> Self {
        let sector = GeneratedSector::empty(id, title, seed, width, height);
        let index = BuilderIndex::rebuild(&sector);
        Self {
            sector: LiveSector::new(sector),
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
            derivations: DerivationLedger::new(),
            derivation_jobs: super::derivation_jobs::DerivationJobs::default(),
            dirty: false,
            auto_save_path: None,
            feedback: FeedbackState::default(),
            feature_weights_cache: BTreeMap::new(),
            validation_report: None,
            invariant_report: None,
            pending_jobs: Vec::new(),
            dirty_files: BTreeSet::new(),
            selected_file: None,
            toml_editor: TomlEditorState::default(),
            file_mtimes: BTreeMap::new(),
            file_watcher: None,
            validation_strict: false,
            nav_rail_collapsed: false,
            selection: SelectionState::default(),
            active_tab: BuilderTab::Project,
            map_view: MapViewState::default(),
            generation: GenerationState::default(),
            drag: DragPendingState::default(),
            route_bulk: RouteBulkState::default(),
            hidden_routes: HiddenRoutesState::default(),
            dominance_locked: BTreeSet::new(),
            primary_factions_locked: BTreeSet::new(),
            region_grow: RegionGrowState::default(),
            subsector_target_systems: sectorforge::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR,
            subsector_system_overrides: BTreeMap::new(),
            subsector_manual: BTreeSet::new(),
            subsector_capital_overrides: BTreeMap::new(),
            subsector_colour_overrides: BTreeMap::new(),
            world_economy_overrides: BTreeMap::new(),
            world_strategic_overrides: BTreeMap::new(),
            system_tithe_overrides: BTreeMap::new(),
            system_supply_overrides: BTreeMap::new(),
            system_priority_overrides: BTreeMap::new(),
            system_bitmap_preview: None,
            economy_panel: EconomyPanelState::default(),
            relations_panel: RelationsPanelState::default(),
            history_panel: HistoryPanelState::default(),
            theme_panel: ThemePanelState::default(),
            archetype_flags: super::command::ArchetypeApplyFlags::default(),
            system_conflict_override: BTreeSet::new(),
            conflict_panel: ConflictPanelState::default(),
            personae_report: None,
            personae_panel: PersonaePanelState::default(),
            hooks_report: None,
            hooks_panel: HooksPanelState::default(),
            sites_report: None,
            sites_panel: SitesPanelState::default(),
            missions_report: None,
            missions_panel: MissionsPanelState::default(),
            prose_report: None,
            prose_auto_recompute: true,
            briefing_panel: BriefingPanelState::default(),
            interestingness_panel: InterestingnessPanelState::default(),
            system_view: SystemViewState::default(),
            search: SearchState::default(),
            diff: DiffState::new(),
            analytics: AnalyticsState::new(),
            segmentum: SegmentumState::default(),
            random_gen: RandomGenState::default(),
            export: ExportState::new(),
        }
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
            hooks: None,
            missions: None,
            prose: None,
        },
        generation: GenerationConfig {
            seed: seed.to_string(),
            sector_width: width,
            sector_height: height,
            subsector_width: None,
            subsector_height: None,
            system_count: 0,
            min_worlds_per_system: 2,
            max_worlds_per_system: 4,
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
