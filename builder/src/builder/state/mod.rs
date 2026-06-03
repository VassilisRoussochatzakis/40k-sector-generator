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
//! [`BuilderState::modal`].
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
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::{GeneratedSector, RouteStability, RouteType};
use sectorforge::{InvariantReport, ValidationReport};

use super::analytics_run::AnalyticsState;
use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::{DerivationCache, DerivationLedger};
use super::diff_run::DiffState;
use super::export_run::ExportState;
use super::file_watcher::FileWatcher;
use super::index::BuilderIndex;
use super::preview::PreviewState;
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
pub mod regions_ops;
pub mod selection;
pub mod types;
pub mod undo;

#[cfg(test)]
mod tests;

pub use nav::EntityRef;
pub use types::{
    validate_toml, BuilderTab, ControlOverlay, HealthLevel, HistoryAnchorKind, HistoryWizardState,
    JobHandle, MapTool, MapViewCache, ModalKind, OpenTomlBuffer, PartialRegenRect,
    PendingBulkRename, PendingCollision, PendingPlace, PendingRegionRename, PendingRename,
    PendingWorldRename, SectorContextMenu, SectorMenuTarget, SystemBitmapPreview,
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
    pub sector: LiveSector,
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
    /// §39 (LD1..LD4) live-derivation ledger. Tracks per-kind input
    /// fingerprints, staleness, and in-flight recomputes so the command bus can
    /// invalidate exactly the overlays a mutation touches (LD2) and the overlay
    /// panels can read freshness before rendering (LD4). See
    /// [`Self::invalidate_derivations`] / [`Self::ensure_fresh`].
    pub derivations: DerivationLedger,
    pub dirty: bool,
    pub auto_save_path: Option<Utf8PathBuf>,
    /// Last error encountered by [`Self::trigger_auto_save`], or `None` if
    /// the most recent attempt succeeded. Rendered in the status bar.
    pub last_save_error: Option<String>,
    /// Last error encountered when reloading a catalog from disk (e.g. when
    /// the file watcher fired after an edit on disk). Rendered in the status
    /// bar so silent TOML parse failures no longer leave the editor showing
    /// stale state without warning. Cleared on the next successful reload.
    pub last_catalog_error: Option<String>,
    /// Last error from `build_subsectors`. Cleared on success. Rendered in
    /// the status bar so a broken cluster doesn't silently produce an empty
    /// subsector list.
    pub last_subsector_error: Option<String>,
    /// TF-NT-3: cached output of `feature_weights_for_world`. Keyed by
    /// `(sys_idx, w_idx, input_digest)`. Stale entries are simply ignored
    /// (digest mismatch) — the cache grows bounded by the number of worlds.
    /// Cleared on project reload via [`Self::clear_feature_weights_cache`].
    pub feature_weights_cache: BTreeMap<FeatureWeightsCacheKey, FeatureWeightsCacheValue>,
    pub validation_report: Option<ValidationReport>,
    pub invariant_report: Option<InvariantReport>,
    pub modal: Option<ModalKind>,
    pub pending_jobs: Vec<JobHandle>,
    /// §P4: per-file dirty markers keyed by project-relative path. Populated
    /// by panels that edit individual catalogs and cleared on save.
    pub dirty_files: BTreeSet<String>,
    /// §P4: file currently selected in the PROJECT tree. Optional — the §PF2
    /// TOML editor reads this to decide which buffer to open.
    pub selected_file: Option<Utf8PathBuf>,
    /// §PF2 / §PF4 / §PF5: raw-TOML editor surface on the PROJECT tab. Holds the
    /// open file buffers ("editor tabs"), their dirty/validation state, and the
    /// active tab. In-memory only.
    pub toml_editor: TomlEditorState,
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
    /// §V4 — strict validation toggle. When set, validation *warnings* are
    /// promoted to errors for the status-bar health pip
    /// ([`Self::health_level`]) and the §V6 pre-export gate
    /// ([`Self::export_block_reason`]), matching
    /// `sectorforge generate --strict`. Off by default. In-memory only.
    pub validation_strict: bool,
    /// §COLUMNS §6.1 — when set, the left cluster nav rail is hidden and only a
    /// `☰` toggle in the top bar brings it back, so a master-detail tab can
    /// reclaim the full width on a narrow window. In-memory view state, off by
    /// default (the rail shows).
    pub nav_rail_collapsed: bool,
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
    /// §R2: ADD ROUTE drag/click start endpoint.
    pub pending_route_start: Option<SystemId>,
    /// §S1: ADD SYSTEM tool target coord awaiting a name entry. The map
    /// panel pops a small naming dialog while this is `Some`.
    pub pending_place: Option<PendingPlace>,
    /// §S1: double-clicked system awaiting a rename in the floating dialog.
    pub pending_rename: Option<PendingRename>,
    /// §S6: drag-drop collision waiting on user choice (Swap / Cancel).
    pub pending_collision: Option<PendingCollision>,
    /// §CTX1 Phase 3: BULK RENAME pattern dialog armed from the MAP tab
    /// right-click multi-selection menu. `None` when no dialog is open.
    pub pending_bulk_rename: Option<PendingBulkRename>,
    /// §CTX1 Phase 5: region RENAME dialog armed from the MAP tab right-click
    /// region-hex menu (`§6.5`). `None` when no dialog is open. Dispatches
    /// through `BuilderCommand::RenameRegion` on commit.
    pub pending_region_rename: Option<PendingRegionRename>,
    /// §S4: in-progress rect-select on the map (`(start, current)` corners).
    pub rect_select: Option<(
        sectorforge::sector_model::HexCoord,
        sectorforge::sector_model::HexCoord,
    )>,
    /// §S1: hex render size in screen pixels. Persisted per session so users
    /// can zoom the map without re-tuning each frame.
    pub hex_size: f32,
    /// §S2: lazy subsector + lookup cache used by the MAP panel so the modern
    /// [`sectorforge_gui_core::sector_view::SectorView`] renderer can draw
    /// subsector borders / capital markers / region tints without rebuilding
    /// every frame. Keyed by a digest over the sector slice it depends on;
    /// refreshed in the panel when the digest changes.
    pub map_view_cache: Option<MapViewCache>,
    /// §W4: monotonic counter mixed into the per-world re-roll discriminator
    /// so repeated clicks on "Re-roll" yield distinct draws while staying
    /// deterministic for replay.
    pub world_reroll_counter: u64,
    /// §R4: bulk route predicate controls.
    pub route_bulk_filter_type: Option<RouteType>,
    pub route_bulk_filter_stability: Option<RouteStability>,
    pub route_bulk_filter_tag: String,
    pub route_bulk_filter_region: Option<String>,
    pub route_bulk_set_type: RouteType,
    pub route_bulk_set_stability: RouteStability,
    /// §R6: explicit hidden-route builder controls.
    pub hidden_route_kind: RouteType,
    pub hidden_route_k_nearest: usize,
    pub hidden_route_exclude_blackout: bool,
    pub hidden_route_endpoints: BTreeSet<SystemId>,
    /// §C3: per-(world, faction) dominance lock. When the pair is present the
    /// CONTROL panel leaves `WorldFactionPresence::dominance` alone; otherwise
    /// it is recomputed from the presence's local-control score every time the
    /// panel is rendered.
    pub dominance_locked: BTreeSet<(WorldId, FactionId)>,
    /// §C5: per-system `primary_factions` override lock. When the system is in
    /// the set the CONTROL panel preserves whatever is in
    /// `GeneratedSystem::primary_factions`; otherwise it auto-derives the
    /// top-3 from `derive_system_control`.
    pub primary_factions_locked: BTreeSet<SystemId>,
    /// §C7 / §C8: active map overlay driven from the CONTROL tab.
    pub control_overlay: ControlOverlay,
    /// §REG3: scratch state for the "Grow seeded region" form on the
    /// REGIONS tab.
    pub region_grow_q: i32,
    pub region_grow_r: i32,
    pub region_grow_size: u32,
    pub region_grow_kind: sectorforge::regions::RegionConditionKind,
    /// §SUB1: currently focused subsector in the SUBSECTORS panel. Drives the
    /// per-cluster inspector and the MAP-tab faint-grey highlight overlay.
    pub selected_subsector_id: Option<String>,
    /// §SUB2: live `target_systems_per_subsector` for the recluster button.
    /// Defaults to [`sectorforge::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR`].
    /// Folded into the [`MapViewCache`] digest so changes invalidate the cache
    /// and the renderer rebuilds with the new clustering.
    pub subsector_target_systems: u32,
    /// §SUB3: per-system manual reassignment table. After the lib runs
    /// `build_subsectors`, the panel reapplies these overrides so manual moves
    /// survive reclustering. Key = `SystemId`, value = destination subsector id.
    pub subsector_system_overrides: BTreeMap<SystemId, String>,
    /// §SUB3: subsectors the user has touched manually. Stored separately from
    /// the system overrides so the panel can flag a cluster as "manual" even
    /// when its current member list happens to match the algorithmic output.
    pub subsector_manual: BTreeSet<String>,
    /// §SUB4: capital override per subsector. Overrides the algorithmic
    /// `summary.subsector_capital_system_id` after the lib clusters.
    pub subsector_capital_overrides: BTreeMap<String, SystemId>,
    /// §SUB5: per-subsector colour override. Default for each subsector is the
    /// `FactionStyle` fill of its controlling faction; the override is only
    /// recorded when the user picks a custom swatch.
    pub subsector_colour_overrides: BTreeMap<String, [u8; 3]>,
    /// §E1: per-world `ResourceVector` override. When present
    /// [`Self::recompute_economy`] pins the world's vector to this value,
    /// recomputes the shortage list, and lets the stranded check re-run from
    /// the override (the routes layer is unchanged so a manual surplus can
    /// still resolve a real deficit). Never written to JSON.
    pub world_economy_overrides: BTreeMap<WorldId, sectorforge::economy::ResourceVector>,
    /// §E2: per-world `StrategicOutput` override. When present
    /// [`Self::recompute_economy`] pins the world's 10-axis strategic vector
    /// and the system-level aggregates inherit the override before the
    /// dependency-edge pass runs.
    pub world_strategic_overrides: BTreeMap<WorldId, sectorforge::economy::StrategicOutput>,
    /// §E3: per-system `TitheStatus` override applied after the auto-derived
    /// pass so users can mark a system Delinquent/Falsified by hand.
    pub system_tithe_overrides: BTreeMap<SystemId, sectorforge::economy::TitheStatus>,
    /// §E3: per-system `SupplyRisk` override.
    pub system_supply_overrides: BTreeMap<SystemId, sectorforge::economy::SupplyRisk>,
    /// §E3: per-system `StrategicPriority` override.
    pub system_priority_overrides: BTreeMap<SystemId, sectorforge::economy::StrategicPriority>,
    /// §E7 / §35 — active MAP-tab heatmap mode when no §C7/§C8 control overlay
    /// is on. Defaults to `Off`. Trade-volume / food / tithe / supply modes
    /// read straight off `sector.economy` and require
    /// [`Self::recompute_economy`] to have run at least once.
    pub map_heatmap_mode: sectorforge::heatmap::HeatmapMode,
    /// §35 T3: when `Some`, the MAP panel paints a per-dimension stability
    /// heatmap instead of [`Self::map_heatmap_mode`]. Mutually exclusive with
    /// it — selecting a stability dimension forces `map_heatmap_mode = Off`,
    /// and selecting any [`sectorforge::heatmap::HeatmapMode`] clears this.
    pub map_stability_dim: Option<sectorforge::heatmap::StabilityDimension>,
    /// §35 T2: filename stem the custom-theme editor saves to under
    /// `data/map_themes/<name>.toml`. Defaults to the active theme name.
    pub theme_save_name: String,
    /// §35 T2: transient one-line result of the last theme save/load action,
    /// shown beneath the editor. Runtime-only.
    pub theme_status: Option<String>,
    /// §35 T5: cached per-system bitmap preview texture, keyed by a digest of
    /// (system id, theme, faction_fill, scale) so the PNG renderer only re-runs
    /// when one of those changes. Never serialized (runtime-only).
    pub system_bitmap_preview: Option<SystemBitmapPreview>,
    /// §E6: when true, the MAP panel highlights the route ids carrying the
    /// top-N supplier→consumer dependency edges (lifeline lanes) using the
    /// existing `SectorView::path_route_ids` channel.
    pub economy_highlight_lifelines: bool,
    /// §E6: minimum dependency-edge score that qualifies as a lifeline. Edges
    /// below this score are not painted. Defaults to 35.0 — same threshold
    /// `economy::derive_dependency_edges` uses for the supplier cutoff.
    pub economy_lifeline_min_score: f32,
    /// §REL1: currently focused pair in the diplomacy matrix grid. Stored as
    /// the canonical (lo, hi) ordering used by [`sectorforge::relations`] so
    /// the cell editor can locate or create an entry in `RelationsConfig::
    /// overrides` deterministically.
    pub relations_selected_pair: Option<(FactionId, FactionId)>,
    /// §REL9: when true, mutations that touch the relations catalog or the
    /// faction roster trigger an immediate [`Self::recompute_relations`] pass.
    /// Defaults to `true`.
    pub relations_auto_recompute: bool,
    /// §H7: currently focused event id in the HISTORY tab. Drives the per-event
    /// inspector and the "highlight on map" anchor lookup.
    pub selected_history_event: Option<String>,
    /// §H6: when true, mutations that touch the history catalog or the
    /// `[history]` config trigger an immediate [`Self::recompute_chronicle`]
    /// pass. Defaults to `false` because chronicle derivation is heavier than
    /// relations / economy — users opt in.
    pub history_auto_recompute: bool,
    /// §H5: scratch state for the "Add event" wizard. `None` when the wizard is
    /// closed; populated by the panel when the user clicks "+ event".
    pub history_wizard: Option<HistoryWizardState>,
    /// §I4: observer-faction lens for the MAP / SYSTEM / WORLD intel tabs.
    /// `None` = omniscient view (default). When set, the intel editors render
    /// the observer's recorded view and the §I5 cutoff redaction kicks in.
    pub intel_observer: Option<FactionId>,
    /// §I5: player-edition confidence cutoff. Hidden-tier presences below this
    /// value are redacted from per-world readouts. 0 = show everything,
    /// 100 = redact everything outside the observer's own presences.
    pub intel_player_min_confidence: u8,
    /// §AR3: per-axis enable mask used by `BuilderCommand::AutoAssignArchetypes`.
    /// Defaults to all axes enabled. Stored on `BuilderState` only — never
    /// serialised into `sector.json` because `src/archetypes.rs` has no TOML
    /// config layer.
    pub archetype_flags: super::command::ArchetypeApplyFlags,
    /// §CF2: per-system "override aggregate" toggle. When the system id is in
    /// the set the SYSTEM-level conflict editor pins
    /// `GeneratedSystem::conflict` to whatever the panel saved; otherwise the
    /// section re-derives via `conflict::derive_system_conflict` each frame.
    /// Never serialised — purely an editor mode flag.
    pub system_conflict_override: BTreeSet<SystemId>,
    /// §CF4 ticks-to-advance scratch input bound to the "Advance N ticks"
    /// button. Defaults to 1.
    pub conflict_ticks_to_advance: u32,
    /// §CF5: chronological tick log captured after each
    /// `BuilderCommand::AdvanceConflictTicks` run. Bounded ring of the most
    /// recent [`Self::tick_log_capacity`] entries; in-memory only.
    pub tick_log: std::collections::VecDeque<TickLogEntry>,
    /// §CF5: ring-buffer cap for [`Self::tick_log`].
    pub tick_log_capacity: usize,
    /// §LINK1 — back-stack of prior focus snapshots, populated by
    /// [`Self::focus_entity`]. Capped at 64; not serialised; not part of undo.
    pub nav_back_stack: Vec<EntityRef>,
    /// §LINK1 — forward stack populated by [`Self::nav_back`]. Cleared by any
    /// new `focus_entity` call.
    pub nav_forward_stack: Vec<EntityRef>,
    /// §LINK1 — selected persona id used by PERSONAE inbound links. Drives the
    /// row highlight + click-select in `panels/personae.rs` and cross-tab focus
    /// via `state/selection.rs`.
    pub selected_persona_id: Option<String>,
    /// §LINK1 — selected hook id used by HOOKS inbound links. Same wiring as
    /// [`Self::selected_persona_id`], scoped to `panels/hooks.rs`.
    pub selected_hook_id: Option<String>,
    /// §PER1..§PER5: latest dramatis-personae overlay. Personae are not part
    /// of `GeneratedSector`, so the builder caches the most recent
    /// `derive_personae` result here. Rebuilt by [`Self::recompute_personae`].
    pub personae_report: Option<sectorforge::personae::PersonaeReport>,
    /// §PER3: when true, mutations that touch the personae catalog trigger an
    /// immediate [`Self::recompute_personae`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub personae_auto_recompute: bool,
    /// §PER2: id of the persona currently expanded in the per-anchor editor.
    /// Mirrors the `[[manual]]` row keyed by id, or a derived persona row.
    /// Stored separately from [`Self::selected_persona_id`] so cross-tab
    /// linking and inline editing don't fight each other.
    pub personae_edit_target: Option<String>,
    /// §HK1..§HK6: latest plot-hook overlay. Hooks are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_hooks_with` result here. Rebuilt by [`Self::recompute_hooks`].
    pub hooks_report: Option<sectorforge::hooks::HooksReport>,
    /// §HK4: when true, mutations that touch the hooks catalog trigger an
    /// immediate [`Self::recompute_hooks`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub hooks_auto_recompute: bool,
    /// §HK5: player-edition toggle. Mirrors `--player` on the CLI by setting
    /// `HooksConfig::hide_hidden_hooks` on each recompute so the cached
    /// report has GM-only rows stripped.
    pub hooks_player_edition: bool,
    /// §HK1: kind filter for the panel's hook list. `None` shows everything;
    /// a specific kind narrows the list. Stored here so the active filter
    /// survives tab switches.
    pub hooks_filter_kind: Option<sectorforge::hooks::HookKind>,
    /// §HK2: id of the hook currently expanded in the panel detail view.
    pub hooks_edit_target: Option<String>,
    /// §ST1..§ST4: latest planetary-sites overlay. Sites are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_sites_with` result here. Rebuilt by [`Self::recompute_sites`].
    pub sites_report: Option<sectorforge::sites::SitesReport>,
    /// §ST2: when true, mutations that touch the sites catalog trigger an
    /// immediate [`Self::recompute_sites`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub sites_auto_recompute: bool,
    /// §ST3: player-edition toggle. Mirrors `--player` on the CLI by setting
    /// `SitesConfig::player_edition` on each recompute so the cached report
    /// drops rows whose `public_status` masks the `actual_status`.
    pub sites_player_edition: bool,
    /// §ST1: kind filter for the panel's site list. `None` shows everything.
    pub sites_filter_kind: Option<sectorforge::sites::SiteKind>,
    /// §ST1: selected site id used by SITES inbound links + detail card.
    pub selected_site_id: Option<String>,
    /// §ST1: id of the site currently expanded in the panel detail view.
    pub sites_edit_target: Option<String>,
    /// §M1..§M5: latest mission-seed overlay. Missions are not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `derive_missions_with` result here. Rebuilt by
    /// [`Self::recompute_missions`].
    pub missions_report: Option<sectorforge::missions::MissionsReport>,
    /// §M3: when true, mutations that touch the missions catalog trigger an
    /// immediate [`Self::recompute_missions`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub missions_auto_recompute: bool,
    /// §M4: player-edition toggle. Mirrors `--player` on the CLI by setting
    /// `MissionsConfig::player_edition` on each recompute so the cached
    /// report drops Hidden-tier-derived missions.
    pub missions_player_edition: bool,
    /// §M1: kind filter for the panel's mission list. `None` shows everything.
    pub missions_filter_kind: Option<sectorforge::missions::MissionKind>,
    /// §M1: selected mission id used for the detail card.
    pub selected_mission_id: Option<String>,
    /// §M1: id of the mission currently expanded in the panel detail view.
    pub missions_edit_target: Option<String>,
    /// §PR1..§PR4: latest gazetteer prose overlay. Prose is not part of
    /// `GeneratedSector`, so the builder caches the most recent
    /// `prose::derive_with` result here. Rebuilt by
    /// [`Self::recompute_prose`].
    pub prose_report: Option<sectorforge::prose::ProseReport>,
    /// §PR4: when true, mutations that touch the prose catalog trigger an
    /// immediate [`Self::recompute_prose`] pass. Defaults to `true` — the
    /// derivation is cheap.
    pub prose_auto_recompute: bool,
    /// §PR1: id of the system currently expanded in the per-system prose
    /// editor. Mirrors [`Self::selected_system_id`] on first focus so the
    /// PROSE tab inherits the SYSTEM tab's selection; a per-tab pick may
    /// then diverge.
    pub selected_prose_system_id: Option<SystemId>,
    /// §BR1: audience preset selected in the BRIEFING tab's profile picker.
    /// Defaults to [`sectorforge::briefing::AudiencePreset::GmFullTruth`]
    /// (no redaction). Used as the seed for
    /// [`sectorforge::briefing::preset`] before observer / confidence
    /// overrides are layered on top.
    pub briefing_preset: sectorforge::briefing::AudiencePreset,
    /// §BR2: optional observer faction. When set, presences are filtered
    /// through the observer's visibility and the briefing pack only keeps
    /// the observer's intel sub-record on each system.
    pub briefing_observer: Option<FactionId>,
    /// §BR3: `minimum_intel_confidence` slider value (0..=100). 0 keeps
    /// everything visible; 100 keeps only directly-observable presences.
    /// Defaults to 30 — the same default as
    /// [`sectorforge::briefing::BriefingProfile::default`].
    pub briefing_min_confidence: u8,
    /// §BR4: cached redacted Markdown produced by the last "Generate
    /// briefing" pass. Rendered into the side preview pane; cleared on
    /// preset / observer / confidence change so a stale preview never
    /// shows.
    pub briefing_preview_md: Option<String>,
    /// §BR4: cached redacted pack produced by the last "Generate briefing"
    /// pass. Held alongside `briefing_preview_md` so the §BR5 export
    /// writes the same pack the user previewed.
    pub briefing_preview_pack: Option<sectorforge::briefing::BriefingPack>,
    /// §BR5: last folder picked by the export dialog. Defaults to the
    /// project's `out/` directory if a project is open; cleared otherwise.
    pub briefing_export_dir: Option<Utf8PathBuf>,
    /// §INT1: active built-in profile selected in the INTERESTINGNESS tab.
    /// Defaults to
    /// [`sectorforge::interestingness::ProfileId::PoliticalSandbox`].
    /// Used as the seed for
    /// [`sectorforge::interestingness::InterestingnessConfig`] before any
    /// per-profile overrides from
    /// [`Self::interestingness_custom_overrides`] are layered on top.
    pub interestingness_profile: sectorforge::interestingness::ProfileId,
    /// §INT2: cached scorecard produced by the last "Score sector" pass.
    /// Cleared whenever the profile or any per-profile override changes so
    /// the chart never shows a stale fit.
    pub interestingness_report: Option<sectorforge::interestingness::InterestingnessReport>,
    /// §INT4: per-profile metric overrides. Outer key is the snake-case id
    /// of [`sectorforge::interestingness::ProfileId`] (matching the serde
    /// representation); inner key is the metric name (`faction_gini`,
    /// `contested_world_ratio`, …). Overrides survive switching to another
    /// profile and back so a user can tune each profile independently.
    /// Never serialised — purely an editor scratch table.
    pub interestingness_custom_overrides:
        BTreeMap<String, BTreeMap<String, sectorforge::interestingness::MetricTarget>>,
    /// §INT4: scratch metric name selected in the "Add override" combo so a
    /// re-render keeps the picker's selection. Empty string = nothing
    /// picked.
    pub interestingness_custom_pick: String,
    /// §CTX0 — Phase 0 of `docs/CONTEXT_MENU.txt`: when the SYSTEM tab renders,
    /// it consumes this field and scrolls the named collapsing header into
    /// view exactly once. Set by the embedded `SystemView` widget when the
    /// user clicks the central star disk. Carried as a `&'static str` so the
    /// id matches the literal passed to `egui::Grid::new` (e.g.
    /// `"sys_star_grid"`). In-memory only — never serialised.
    pub scroll_target: Option<&'static str>,
    /// §CTX1 — Phase 1 of `docs/CONTEXT_MENU.txt`: open right-click menu on the
    /// MAP tab. `None` when no menu is open. Set by `panels/map.rs` on
    /// `secondary_clicked`; cleared on Escape / outside-click / focus-loss /
    /// item activation. In-memory only — never serialised.
    pub sector_context_menu: Option<SectorContextMenu>,
    /// §CTX1 — Phase 4 of `docs/CONTEXT_MENU.txt`: anchor hex armed by the MAP
    /// tab's right-click `START PARTIAL REGEN HERE` item. When `Some`, the next
    /// primary click on any sector hex completes [`Self::partial_regen_rect`]
    /// (anchor → click coord, normalised by [`PartialRegenRect::from_corners`])
    /// and clears the anchor. The GENERATION tab surfaces a hint while the
    /// anchor is live and offers a "Cancel anchor" button. In-memory only —
    /// never serialised to `project.toml`.
    pub partial_regen_anchor: Option<sectorforge::sector_model::HexCoord>,
    /// §CTX1 — Phase 6 of `docs/CONTEXT_MENU.txt`: open right-click menu on the
    /// in-system map embedded under the SYSTEM tab. `None` when no menu is
    /// open. Set by `panels/system_map.rs` on `secondary_clicked`; cleared on
    /// Escape / outside-click / focus-loss / item activation. In-memory only —
    /// never serialised.
    pub system_context_menu: Option<SystemContextMenu>,
    /// §CTX1 Phase 6: pending WORLD RENAME dialog armed from the SYSTEM tab's
    /// in-system right-click menu (`§6.7`). `None` when no dialog is open.
    /// Dispatches through `BuilderCommand::RenameWorld` on commit.
    pub pending_world_rename: Option<PendingWorldRename>,
    /// §CTX1 Phase 7 polish — last right-click menu item activated (sector or
    /// in-system), surfaced as a short string on the status bar so the user
    /// gets a single-line trail of what the menu just dispatched. Replaces
    /// the optional `log_*progress` telemetry hook from the spec (the CLI
    /// `log_progress` channel only writes to `eprintln!`, which the builder
    /// has no UI for). `None` until the first activation; cleared on session
    /// load — in-memory only.
    pub last_menu_action: Option<String>,
    /// Last command-bus dispatch error surfaced from a panel; cleared by the
    /// next successful `run`. In-memory only.
    pub last_command_error: Option<String>,
    /// Visual layout for the SYSTEM tab's embedded
    /// [`sectorforge_gui_core::system_view::SystemView`]. Defaults to
    /// [`SystemLayout::Horizontal`] — planets to the right of the star in
    /// orbit order. In-memory only; never serialised.
    pub system_layout: sectorforge_gui_core::system_view::SystemLayout,
    /// Pixel side length of the SYSTEM tab's embedded
    /// [`sectorforge_gui_core::system_view::SystemView`]. Drives `star_r` /
    /// `planet_r` / `orbit_step` proportionally — bumping the slider grows
    /// the whole widget. In-memory only.
    pub system_view_side: f32,
    /// §SR1..§SR5: SEARCH tab runtime — the editable `wishes.toml` document,
    /// the off-thread constraint search job, its live progress snapshot, and
    /// the latest outcome. In-memory only; the wishes doc is round-tripped to
    /// disk separately. See [`super::search_run::SearchState`].
    pub search: SearchState,
    /// §DF1..§DF5: DIFF tab runtime — the two scratch sector slots, the
    /// diff/tick filter config, and the most recently computed diff. In-memory
    /// only. See [`super::diff_run::DiffState`].
    pub diff: DiffState,
    /// §A1..§A4: ANALYTICS tab runtime — the editable `[analyze]` config, the
    /// strict CI-parity toggle, the most recently computed `SectorAnalysis`,
    /// and the last export folder. In-memory only. See
    /// [`super::analytics_run::AnalyticsState`].
    pub analytics: AnalyticsState,
    /// §SG1..§SG5: SEGMENTUM tab runtime — the editable `segmentum.toml`
    /// document, the off-thread compose job + its per-child progress, and the
    /// most recently composed segmentum (super-manifest + inter-sector links).
    /// In-memory only; the document round-trips to disk separately. See
    /// [`super::segmentum_run::SegmentumState`].
    pub segmentum: SegmentumState,
    /// RANDOM.md §7.4: off-thread random-sector generation runtime (job +
    /// live progress snapshot) backing the §7.3 wizard's progress popup.
    pub random_gen: RandomGenState,
    /// §EX1..§EX8: EXPORT tab runtime — the chosen output folder, the
    /// standalone-system export form, the cached markdown preview, and the
    /// last export error. The per-format / bitmap / HTML knobs live on
    /// `config.outputs`, not here. In-memory only. See
    /// [`super::export_run::ExportState`].
    pub export: ExportState,
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
            dirty: false,
            auto_save_path: None,
            last_save_error: None,
            last_catalog_error: None,
            last_subsector_error: None,
            feature_weights_cache: BTreeMap::new(),
            validation_report: None,
            invariant_report: None,
            modal: None,
            pending_jobs: Vec::new(),
            dirty_files: BTreeSet::new(),
            selected_file: None,
            toml_editor: TomlEditorState::default(),
            file_mtimes: BTreeMap::new(),
            file_watcher: None,
            validation_dirty_since: None,
            validation_debounce: Duration::from_millis(DEFAULT_VALIDATION_DEBOUNCE_MS),
            validation_strict: false,
            nav_rail_collapsed: false,
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
            pending_route_start: None,
            pending_place: None,
            pending_rename: None,
            pending_collision: None,
            pending_bulk_rename: None,
            pending_region_rename: None,
            rect_select: None,
            hex_size: 28.0,
            map_view_cache: None,
            world_reroll_counter: 0,
            route_bulk_filter_type: None,
            route_bulk_filter_stability: None,
            route_bulk_filter_tag: String::new(),
            route_bulk_filter_region: None,
            route_bulk_set_type: RouteType::ChartedPassage,
            route_bulk_set_stability: RouteStability::Hazardous,
            hidden_route_kind: RouteType::Webway,
            hidden_route_k_nearest: sectorforge::hidden_routes::DEFAULT_HIDDEN_K_NEAREST,
            hidden_route_exclude_blackout: true,
            hidden_route_endpoints: BTreeSet::new(),
            dominance_locked: BTreeSet::new(),
            primary_factions_locked: BTreeSet::new(),
            control_overlay: ControlOverlay::None,
            region_grow_q: 0,
            region_grow_r: 0,
            region_grow_size: 6,
            region_grow_kind: sectorforge::regions::RegionConditionKind::Turbulence,
            selected_subsector_id: None,
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
            map_heatmap_mode: sectorforge::heatmap::HeatmapMode::Off,
            map_stability_dim: None,
            theme_save_name: String::new(),
            theme_status: None,
            system_bitmap_preview: None,
            economy_highlight_lifelines: false,
            economy_lifeline_min_score: 35.0,
            relations_selected_pair: None,
            relations_auto_recompute: true,
            selected_history_event: None,
            history_auto_recompute: false,
            history_wizard: None,
            intel_observer: None,
            intel_player_min_confidence: 0,
            archetype_flags: super::command::ArchetypeApplyFlags::default(),
            system_conflict_override: BTreeSet::new(),
            conflict_ticks_to_advance: 1,
            tick_log: std::collections::VecDeque::new(),
            tick_log_capacity: 500,
            nav_back_stack: Vec::new(),
            nav_forward_stack: Vec::new(),
            selected_persona_id: None,
            selected_hook_id: None,
            personae_report: None,
            personae_auto_recompute: true,
            personae_edit_target: None,
            hooks_report: None,
            hooks_auto_recompute: true,
            hooks_player_edition: false,
            hooks_filter_kind: None,
            hooks_edit_target: None,
            sites_report: None,
            sites_auto_recompute: true,
            sites_player_edition: false,
            sites_filter_kind: None,
            selected_site_id: None,
            sites_edit_target: None,
            missions_report: None,
            missions_auto_recompute: true,
            missions_player_edition: false,
            missions_filter_kind: None,
            selected_mission_id: None,
            missions_edit_target: None,
            prose_report: None,
            prose_auto_recompute: true,
            selected_prose_system_id: None,
            briefing_preset: sectorforge::briefing::AudiencePreset::GmFullTruth,
            briefing_observer: None,
            briefing_min_confidence: 30,
            briefing_preview_md: None,
            briefing_preview_pack: None,
            briefing_export_dir: None,
            interestingness_profile: sectorforge::interestingness::ProfileId::PoliticalSandbox,
            interestingness_report: None,
            interestingness_custom_overrides: BTreeMap::new(),
            interestingness_custom_pick: String::new(),
            scroll_target: None,
            sector_context_menu: None,
            partial_regen_anchor: None,
            system_context_menu: None,
            pending_world_rename: None,
            last_menu_action: None,
            last_command_error: None,
            system_layout: sectorforge_gui_core::system_view::SystemLayout::default(),
            system_view_side: 720.0,
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
