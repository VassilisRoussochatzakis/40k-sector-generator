//! Cohesive transient sub-structs for [`super::BuilderState`] (finding D-S3/D5).
//!
//! These group the per-panel UI scratch / selection / view fields that used to
//! sit flat on `BuilderState`. They are **transient view state only** (CLAUDE.md
//! §R4 carve-out): never serialised into `sector.json`, never on the undo stack,
//! written directly by panels (not through the command bus).
//!
//! Each struct carries a `Default` impl whose values are byte-identical to the
//! per-field initialisers the two `BuilderState` constructors
//! ([`super::BuilderState::new_blank`] and
//! [`crate::builder::session::SessionFile::into_state`]) previously used.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::{HexCoord, RouteStability, RouteType};

use super::super::preview::PreviewState;
use super::nav::EntityRef;
use super::types::{
    ControlOverlay, HistoryWizardState, MapTool, MapViewCache, ModalKind, PartialRegenRect,
    PendingBulkRename, PendingCollision, PendingPlace, PendingRegionRename, PendingRename,
    PendingWorldRename, SectorContextMenu, SystemContextMenu, TickLogEntry,
};

/// §V2 / §LINK1 — entity selection mailbox + cross-tab navigation stacks. Each
/// id field is independent so the active inspector reads only the IDs it cares
/// about; the nav stacks back the §LINK1 back/forward gestures.
#[derive(Default)]
pub(crate) struct SelectionState {
    pub(crate) system_id: Option<SystemId>,
    pub(crate) world_id: Option<WorldId>,
    pub(crate) route_id: Option<RouteId>,
    pub(crate) faction_id: Option<FactionId>,
    pub(crate) region_id: Option<String>,
    pub(crate) subsector_id: Option<String>,
    pub(crate) site_id: Option<String>,
    pub(crate) mission_id: Option<String>,
    pub(crate) history_event: Option<String>,
    pub(crate) persona_id: Option<String>,
    pub(crate) hook_id: Option<String>,
    pub(crate) prose_system_id: Option<SystemId>,
    /// §S4: shift-click / rect-drag multi-selection.
    pub(crate) systems: BTreeSet<SystemId>,
    /// §LINK1 — back-stack of prior focus snapshots; capped at 64.
    pub(crate) nav_back_stack: Vec<EntityRef>,
    /// §LINK1 — forward stack populated by `nav_back`.
    pub(crate) nav_forward_stack: Vec<EntityRef>,
    /// §CTX0 — collapsing-header scroll request consumed once by the SYSTEM tab.
    pub(crate) scroll_target: Option<&'static str>,
}

/// MAP-tab view/render state: zoom, armed tool, overlay/heatmap modes, the lazy
/// render cache, and the open right-click context menus.
pub(crate) struct MapViewState {
    /// §S1: hex render size in screen pixels.
    pub(crate) hex_size: f32,
    /// §S2: lazy subsector + lookup cache used by the MAP panel renderer.
    pub(crate) cache: Option<MapViewCache>,
    /// §N3: armed tool on the MAP tab.
    pub(crate) tool: MapTool,
    /// §E7 / §35 — active MAP-tab heatmap mode.
    pub(crate) heatmap_mode: sectorforge::heatmap::HeatmapMode,
    /// §35 T3: when `Some`, paint a per-dimension stability heatmap.
    pub(crate) stability_dim: Option<sectorforge::heatmap::StabilityDimension>,
    /// §C7 / §C8: active map overlay driven from the CONTROL tab.
    pub(crate) control_overlay: ControlOverlay,
    /// §G5: half-open axial-hex rectangle selected for partial regeneration.
    pub(crate) partial_regen_rect: Option<PartialRegenRect>,
    /// §CTX1 Phase 4: anchor hex armed by `START PARTIAL REGEN HERE`.
    pub(crate) partial_regen_anchor: Option<HexCoord>,
    /// §CTX1 Phase 1: open right-click menu on the MAP tab.
    pub(crate) sector_context_menu: Option<SectorContextMenu>,
    /// §CTX1 Phase 6: open right-click menu on the in-system map.
    pub(crate) system_context_menu: Option<SystemContextMenu>,
}

impl Default for MapViewState {
    fn default() -> Self {
        Self {
            hex_size: 28.0,
            cache: None,
            tool: MapTool::Select,
            heatmap_mode: sectorforge::heatmap::HeatmapMode::Off,
            stability_dim: None,
            control_overlay: ControlOverlay::None,
            partial_regen_rect: None,
            partial_regen_anchor: None,
            sector_context_menu: None,
            system_context_menu: None,
        }
    }
}

/// Transient drag / pending-dialog scratch shared by the MAP and SYSTEM tabs.
/// Every member is an in-flight gesture or open floating dialog; all blank by
/// default.
#[derive(Default)]
pub(crate) struct DragPendingState {
    /// §S1: id of the system currently being dragged across the hex grid.
    pub(crate) drag_system: Option<SystemId>,
    /// D1: transient brush-stroke scratch taken at `begin_region_stroke`.
    pub(crate) region_stroke_before: Option<Box<sectorforge::regions::WarpRegion>>,
    /// §S4: in-progress rect-select on the map (`(start, current)` corners).
    pub(crate) rect_select: Option<(HexCoord, HexCoord)>,
    /// §R2: ADD ROUTE drag/click start endpoint.
    pub(crate) pending_route_start: Option<SystemId>,
    /// §S1: ADD SYSTEM tool target coord awaiting a name entry.
    pub(crate) pending_place: Option<PendingPlace>,
    /// §S1: double-clicked system awaiting a rename in the floating dialog.
    pub(crate) pending_rename: Option<PendingRename>,
    /// §S6: drag-drop collision waiting on user choice (Swap / Cancel).
    pub(crate) pending_collision: Option<PendingCollision>,
    /// §CTX1 Phase 3: BULK RENAME pattern dialog.
    pub(crate) pending_bulk_rename: Option<PendingBulkRename>,
    /// §CTX1 Phase 5: region RENAME dialog.
    pub(crate) pending_region_rename: Option<PendingRegionRename>,
    /// §CTX1 Phase 6: WORLD RENAME dialog.
    pub(crate) pending_world_rename: Option<PendingWorldRename>,
}

/// Status-bar / modal feedback channel: the last error of each kind, the active
/// modal, the last menu action trail, and the live-validation debounce timer.
pub(crate) struct FeedbackState {
    /// Last error from `trigger_auto_save`.
    pub(crate) last_save_error: Option<String>,
    /// Last error reloading a catalog from disk.
    pub(crate) last_catalog_error: Option<String>,
    /// Last error from `build_subsectors`.
    pub(crate) last_subsector_error: Option<String>,
    /// D10: why the last `revalidate_now` produced no validation report.
    pub(crate) last_validation_skip_reason: Option<String>,
    /// §CTX1 Phase 7: last right-click menu item activated.
    pub(crate) last_menu_action: Option<String>,
    /// Last command-bus dispatch error surfaced from a panel.
    pub(crate) last_command_error: Option<String>,
    /// R10: active modal dialog (only one at a time).
    pub(crate) modal: Option<ModalKind>,
    /// §V3: timestamp of the most recent unflushed mutation.
    pub(crate) validation_dirty_since: Option<Instant>,
    /// §V3: debounce window between mutation and live-validation flush.
    pub(crate) validation_debounce: Duration,
}

impl Default for FeedbackState {
    fn default() -> Self {
        Self {
            last_save_error: None,
            last_catalog_error: None,
            last_subsector_error: None,
            last_validation_skip_reason: None,
            last_menu_action: None,
            last_command_error: None,
            modal: None,
            validation_dirty_since: None,
            validation_debounce: Duration::from_millis(
                super::types::DEFAULT_VALIDATION_DEBOUNCE_MS,
            ),
        }
    }
}

/// §G2..§G5 / §W4 generation-panel scratch: live preview, seed lock + re-roll
/// counters, and the per-world re-roll counter.
#[derive(Default)]
pub(crate) struct GenerationState {
    /// §G3 / §G4: scratch live preview owned by the generation panel.
    pub(crate) preview: PreviewState,
    /// §G2: when true, "Re-roll" preserves `generation.seed`.
    pub(crate) seed_locked: bool,
    /// §G2: monotonic counter mixed into the re-roll derivation.
    pub(crate) seed_reroll_counter: u64,
    /// §W4: monotonic counter mixed into the per-world re-roll discriminator.
    pub(crate) world_reroll_counter: u64,
}

/// §R4 bulk-route predicate controls for the ROUTES tab.
pub(crate) struct RouteBulkState {
    pub(crate) filter_type: Option<RouteType>,
    pub(crate) filter_stability: Option<RouteStability>,
    pub(crate) filter_tag: String,
    pub(crate) filter_region: Option<String>,
    pub(crate) set_type: RouteType,
    pub(crate) set_stability: RouteStability,
}

impl Default for RouteBulkState {
    fn default() -> Self {
        Self {
            filter_type: None,
            filter_stability: None,
            filter_tag: String::new(),
            filter_region: None,
            set_type: RouteType::ChartedPassage,
            set_stability: RouteStability::Hazardous,
        }
    }
}

/// §R6 explicit hidden-route builder controls.
pub(crate) struct HiddenRoutesState {
    pub(crate) kind: RouteType,
    pub(crate) k_nearest: usize,
    pub(crate) exclude_blackout: bool,
    pub(crate) endpoints: BTreeSet<SystemId>,
}

impl Default for HiddenRoutesState {
    fn default() -> Self {
        Self {
            kind: RouteType::Webway,
            k_nearest: sectorforge::hidden_routes::DEFAULT_HIDDEN_K_NEAREST,
            exclude_blackout: true,
            endpoints: BTreeSet::new(),
        }
    }
}

/// §REG3 scratch for the "Grow seeded region" form on the REGIONS tab.
pub(crate) struct RegionGrowState {
    pub(crate) q: i32,
    pub(crate) r: i32,
    pub(crate) size: u32,
    pub(crate) kind: sectorforge::regions::RegionConditionKind,
}

impl Default for RegionGrowState {
    fn default() -> Self {
        Self {
            q: 0,
            r: 0,
            size: 6,
            kind: sectorforge::regions::RegionConditionKind::Turbulence,
        }
    }
}

/// §HK1..§HK6 HOOKS-tab runtime knobs.
pub(crate) struct HooksPanelState {
    pub(crate) auto_recompute: bool,
    pub(crate) player_edition: bool,
    pub(crate) filter_kind: Option<sectorforge::hooks::HookKind>,
    pub(crate) edit_target: Option<String>,
}

impl Default for HooksPanelState {
    fn default() -> Self {
        Self {
            auto_recompute: true,
            player_edition: false,
            filter_kind: None,
            edit_target: None,
        }
    }
}

/// §ST1..§ST4 SITES-tab runtime knobs.
pub(crate) struct SitesPanelState {
    pub(crate) auto_recompute: bool,
    pub(crate) player_edition: bool,
    pub(crate) filter_kind: Option<sectorforge::sites::SiteKind>,
    pub(crate) edit_target: Option<String>,
}

impl Default for SitesPanelState {
    fn default() -> Self {
        Self {
            auto_recompute: true,
            player_edition: false,
            filter_kind: None,
            edit_target: None,
        }
    }
}

/// §M1..§M5 MISSIONS-tab runtime knobs.
pub(crate) struct MissionsPanelState {
    pub(crate) auto_recompute: bool,
    pub(crate) player_edition: bool,
    pub(crate) filter_kind: Option<sectorforge::missions::MissionKind>,
    pub(crate) edit_target: Option<String>,
}

impl Default for MissionsPanelState {
    fn default() -> Self {
        Self {
            auto_recompute: true,
            player_edition: false,
            filter_kind: None,
            edit_target: None,
        }
    }
}

/// §PER2..§PER3 PERSONAE-tab runtime knobs.
pub(crate) struct PersonaePanelState {
    pub(crate) auto_recompute: bool,
    pub(crate) edit_target: Option<String>,
}

impl Default for PersonaePanelState {
    fn default() -> Self {
        Self {
            auto_recompute: true,
            edit_target: None,
        }
    }
}

/// §H5..§I5 HISTORY-tab runtime: auto-recompute toggle, add-event wizard, and
/// the §I4/§I5 intel observer lens.
#[derive(Default)]
pub(crate) struct HistoryPanelState {
    /// §H6: when true, history/`[history]` edits trigger a chronicle recompute.
    pub(crate) auto_recompute: bool,
    /// §H5: scratch state for the "Add event" wizard.
    pub(crate) wizard: Option<HistoryWizardState>,
    /// §I4: observer-faction lens for the MAP / SYSTEM / WORLD intel tabs.
    pub(crate) intel_observer: Option<FactionId>,
    /// §I5: player-edition confidence cutoff.
    pub(crate) intel_player_min_confidence: u8,
}

/// §BR1..§BR5 BRIEFING-tab runtime: profile / observer / confidence knobs and
/// the cached preview + export folder.
pub(crate) struct BriefingPanelState {
    pub(crate) preset: sectorforge::briefing::AudiencePreset,
    pub(crate) observer: Option<FactionId>,
    pub(crate) min_confidence: u8,
    pub(crate) preview_md: Option<String>,
    pub(crate) preview_pack: Option<sectorforge::briefing::BriefingPack>,
    pub(crate) export_dir: Option<Utf8PathBuf>,
}

impl Default for BriefingPanelState {
    fn default() -> Self {
        Self {
            preset: sectorforge::briefing::AudiencePreset::GmFullTruth,
            observer: None,
            min_confidence: 30,
            preview_md: None,
            preview_pack: None,
            export_dir: None,
        }
    }
}

/// §INT1..§INT4 INTERESTINGNESS-tab runtime: active profile, cached report, and
/// per-profile metric overrides.
pub(crate) struct InterestingnessPanelState {
    pub(crate) profile: sectorforge::interestingness::ProfileId,
    pub(crate) report: Option<sectorforge::interestingness::InterestingnessReport>,
    pub(crate) custom_overrides:
        BTreeMap<String, BTreeMap<String, sectorforge::interestingness::MetricTarget>>,
    pub(crate) custom_pick: String,
}

impl Default for InterestingnessPanelState {
    fn default() -> Self {
        Self {
            profile: sectorforge::interestingness::ProfileId::PoliticalSandbox,
            report: None,
            custom_overrides: BTreeMap::new(),
            custom_pick: String::new(),
        }
    }
}

/// §CF4 / §CF5 CONFLICT-section runtime: ticks-to-advance scratch and the
/// chronological tick log (bounded ring).
pub(crate) struct ConflictPanelState {
    /// §CF4: ticks-to-advance scratch input.
    pub(crate) ticks_to_advance: u32,
    /// §CF5: chronological tick log captured after each advance.
    pub(crate) tick_log: VecDeque<TickLogEntry>,
    /// §CF5: ring-buffer cap for `tick_log`.
    pub(crate) tick_log_capacity: usize,
}

impl Default for ConflictPanelState {
    fn default() -> Self {
        Self {
            ticks_to_advance: 1,
            tick_log: VecDeque::new(),
            tick_log_capacity: 500,
        }
    }
}

/// §REL1 / §REL9 RELATIONS-tab runtime: focused pair + auto-recompute toggle.
pub(crate) struct RelationsPanelState {
    pub(crate) selected_pair: Option<(FactionId, FactionId)>,
    pub(crate) auto_recompute: bool,
}

impl Default for RelationsPanelState {
    fn default() -> Self {
        Self {
            selected_pair: None,
            auto_recompute: true,
        }
    }
}

/// §E6 ECONOMY-tab lifeline-lane highlight controls.
pub(crate) struct EconomyPanelState {
    pub(crate) highlight_lifelines: bool,
    pub(crate) lifeline_min_score: f32,
}

impl Default for EconomyPanelState {
    fn default() -> Self {
        Self {
            highlight_lifelines: false,
            lifeline_min_score: 35.0,
        }
    }
}

/// §35 T2 custom-theme editor scratch: save-name stem + last action status.
#[derive(Default)]
pub(crate) struct ThemePanelState {
    pub(crate) save_name: String,
    pub(crate) status: Option<String>,
}

/// SYSTEM-tab embedded `SystemView` layout + pixel side length.
pub(crate) struct SystemViewState {
    pub(crate) layout: sectorforge_gui_core::system_view::SystemLayout,
    pub(crate) side: f32,
}

impl Default for SystemViewState {
    fn default() -> Self {
        Self {
            layout: sectorforge_gui_core::system_view::SystemLayout::default(),
            side: 720.0,
        }
    }
}
