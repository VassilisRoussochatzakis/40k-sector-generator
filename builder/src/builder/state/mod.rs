//! Top-level builder state (§D5). One instance is owned by the eframe app
//! while the builder is in focus.
//!
//! # R1: single source of truth
//!
//! `BuilderState` owns the sole live [`GeneratedSector`] for the project. Per
//! docs/BUILDER_REQS §R1, the spec allows `Arc<RwLock<GeneratedSector>>` or
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
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use sectorforge::config::AppConfig;
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::{GeneratedSector, RouteStability, RouteType};
use sectorforge::{InvariantReport, ValidationReport};

use super::command::BuilderCommand;
use super::data_catalogs::DataCatalogs;
use super::derivation_cache::DerivationCache;
use super::file_watcher::FileWatcher;
use super::index::BuilderIndex;
use super::preview::PreviewState;
use super::snapshot::Snapshot;

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
    BuilderTab, ControlOverlay, HealthLevel, HistoryAnchorKind, HistoryWizardState, JobHandle,
    MapTool, MapViewCache, ModalKind, PartialRegenRect, PendingCollision, PendingPlace,
    PendingRename, TickLogEntry, TickLogScope, DEFAULT_COMMAND_LOG_CAPACITY,
    DEFAULT_VALIDATION_DEBOUNCE_MS,
};

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
    /// §R2: ADD ROUTE drag/click start endpoint.
    pub pending_route_start: Option<SystemId>,
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
    /// §LINK1 — selected persona id used by PERSONAE inbound links. The
    /// PERSONAE panel is a Phase D stub today; the field exists now so links
    /// land first-class when the panel ships.
    pub selected_persona_id: Option<String>,
    /// §LINK1 — selected hook id used by HOOKS inbound links. Mirrors the
    /// persona stub above.
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
            pending_route_start: None,
            pending_place: None,
            pending_rename: None,
            pending_collision: None,
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
