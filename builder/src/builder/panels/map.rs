//! MAP tab (§N3) — hex render + editor toolbox + transient dialogs.
//!
//! Renders the sector via [`sectorforge_gui_core::sector_view::SectorView`]
//! so the visual surface matches the main viewer (§N3 / §S2). Builder-only
//! interactions — tool dispatch, drag-move, rect-select, double-click rename,
//! pinned/multi-select overlays, the §S6 collision dialog — are layered on
//! top of the shared widget rather than reimplemented here.
//!
//! Phase B §S1: ADD SYSTEM / DELETE SYSTEM / MOVE SYSTEM (drag) /
//! RENAME (double-click). Multi-select (shift-click + rect-drag) feeds §S4
//! over in the SYSTEM panel. Coord validity + the collision swap dialog land
//! the §S6 surface.

use egui::{Pos2, RichText, Sense, Ui};

use sectorforge::ids::{self, FactionId, RouteId, SystemId};
use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{HexCoord, RouteStability, RouteType, SystemState};
use sectorforge::subsectors::{build_subsectors, SubsectorConfig};

use sectorforge_gui_core::sector_view::{
    paint_system_rings, SectorGeom, SectorMapCache, SectorView,
};

use crate::builder::command::BuilderCommand;
use crate::builder::derivation_cache::digest_input;
use crate::builder::state::{
    BuilderTab, EntityRef, MapTool, MapViewCache, PendingBulkRename, PendingCollision,
    PendingPlace, PendingRegionRename, PendingRename, SectorContextMenu, SectorMenuTarget,
};
use crate::builder::{BuilderState, ModalKind};
use sectorforge_gui_core::palette::TEXT_DIM;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Map");
    ui.add_space(4.0);
    show_toolbox(ui, state);
    ui.horizontal(|ui| {
        ui.label("zoom:");
        ui.add(egui::Slider::new(&mut state.hex_size, 12.0..=64.0).text("hex"));
        if !state.selected_systems.is_empty() {
            ui.label(format!("selected: {}", state.selected_systems.len()));
        }
        if let Some(id) = &state.selected_system_id {
            ui.label(format!("focus: {id}"));
        }
        if let Some(id) = &state.pending_route_start {
            ui.label(format!("route from: {id}"));
        }
        // §CTX1 Phase 4 — surface the live partial-regen anchor so the user
        // can tell why their next primary click will be consumed.
        if let Some(anchor) = state.partial_regen_anchor {
            ui.colored_label(
                egui::Color32::from_rgb(120, 200, 240),
                format!("partial-regen anchor: ({}, {})", anchor.q, anchor.r),
            );
            if ui.small_button("cancel anchor").clicked() {
                state.partial_regen_anchor = None;
            }
        }
    });
    crate::builder::panels::intel::show_map_intel_controls(ui, state);
    ui.separator();

    egui::ScrollArea::both().show(ui, |ui| {
        show_hex_map(ui, state);
    });

    // §CTX1 — Phase 1: floating right-click menu rendered as a free-standing
    // `egui::Area`. Sits above the canvas; dismissed on Escape / focus-loss /
    // outside primary click / item activation.
    show_sector_context_menu(ui.ctx(), state);

    // Transient dialogs — kept inside the panel so the host shell does not need
    // to learn new ModalKind variants for §S1 / §S6.
    show_place_dialog(ui.ctx(), state);
    show_rename_dialog(ui.ctx(), state);
    show_bulk_rename_dialog(ui.ctx(), state);
    show_region_rename_dialog(ui.ctx(), state);
    show_collision_dialog(ui.ctx(), state);
}

/// §N3 toolbox: SELECT / ADD / DELETE / MOVE / ADD ROUTE / REGION-PAINT.
pub fn show_toolbox(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("tool:");
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            let selected = state.map_tool == tool;
            if ui.selectable_label(selected, tool.label()).clicked() {
                state.map_tool = tool;
                if tool != MapTool::AddRoute {
                    state.pending_route_start = None;
                }
            }
        }
    });
}

// ── Map renderer + interaction dispatcher ───────────────────────────────────

fn show_hex_map(ui: &mut Ui, state: &mut BuilderState) {
    let (sector_w, sector_h) = (state.sector.width, state.sector.height);
    if sector_w == 0 || sector_h == 0 {
        ui.label(
            RichText::new("Sector has zero extent — open or create a project first.")
                .color(TEXT_DIM),
        );
        return;
    }

    // Refresh the subsector + lookup cache when the sector slice changes.
    refresh_map_cache(state);

    let geom = SectorGeom::new(state.hex_size, Pos2::ZERO);
    let canvas_size = geom.map_size_px(sector_w, sector_h);
    let (rect, response) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let origin = rect.min;
    let pointer = response.interact_pointer_pos();

    // Live drag override: when a system is being dragged, follow the cursor.
    let drag_override = state.drag_system.clone().zip(pointer);

    // ADD-ROUTE preview line: pending start → cursor.
    let pending_route_preview = if state.map_tool == MapTool::AddRoute {
        state.pending_route_start.clone().zip(pointer)
    } else {
        None
    };

    let rect_select = state.rect_select;

    // Build the view. The internal click dispatch is disabled — builder owns
    // the tool routing below.
    let (subsectors_slice, lookup) = match state.map_view_cache.as_ref() {
        Some(cache) => (
            Some(cache.subsectors.as_slice()),
            Some(&cache.lookup as &SectorMapCache),
        ),
        None => (None, None),
    };

    // §C7 / §C8: optional control-derived overlay supplied to the SectorView
    // heatmap channel. Computed lazily — only paid when an overlay is on.
    // §E7: when no control overlay is on, fall back to the economy/state
    // heatmap mode picked from the ECONOMY tab.
    let overlay_cells = crate::builder::panels::control::build_overlay_cells(
        &state.sector,
        &state.sector.factions,
        state.control_overlay,
    );
    let economy_cells = if overlay_cells.is_none()
        && !matches!(
            state.map_heatmap_mode,
            sectorforge::heatmap::HeatmapMode::Off,
        ) {
        let cells = sectorforge_gui_core::heatmap::compute(&state.sector, state.map_heatmap_mode);
        if cells.is_empty() {
            None
        } else {
            Some(cells)
        }
    } else {
        None
    };
    let heatmap_ref = overlay_cells.as_ref().or(economy_cells.as_ref());

    // §E6: highlight lifeline lanes on the route layer when the toggle is on.
    let lifeline_routes = crate::builder::panels::economy::lifeline_route_ids(state);
    let lifeline_ref = if lifeline_routes.is_empty() {
        None
    } else {
        Some(&lifeline_routes)
    };

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
        SectorView {
            sector: &state.sector,
            selected_system: state.selected_system_id.as_ref().map(|id| id.as_str()),
            selected_route: state.selected_route_id.as_ref().map(|id| id.as_str()),
            hex_size: state.hex_size,
            path_route_ids: lifeline_ref,
            path_waypoints: None,
            subsectors: subsectors_slice,
            cache: lookup,
            selected_subsector: state.selected_subsector_id.as_deref(),
            heatmap: heatmap_ref,
            empty_hex_clicks: false,
            route_view_mode: sectorforge::sector_model::RouteViewMode::Detailed,
            origin,
            multi_selected: Some(&state.selected_systems),
            pinned: Some(&state.pinned_systems),
            drag_override,
            pending_route_preview,
            rect_select,
            sense: Sense::hover(),
            disable_internal_click_dispatch: true,
            theme: None,
            show_hover_coord: true,
        }
        .show(ui);
    });

    // §E4: paint a red ring around every system that contains a stranded
    // world. Drawn after `SectorView::show` so the overlay sits on top of the
    // base hex / route / system render.
    let stranded = crate::builder::panels::economy::stranded_system_ids(state);
    if !stranded.is_empty() {
        let red = egui::Color32::from_rgb(230, 80, 80);
        let stroke = egui::Stroke::new(2.0, red);
        let ring_geom = SectorGeom::new(state.hex_size, origin);
        paint_system_rings(ui, rect, &ring_geom, &state.sector, 0.7, stroke, |id| {
            stranded.contains(id)
        });
    }

    // §CTX1 — Phase 1: yellow ring around the right-click target system while
    // the floating menu is open, so the user can confirm the click hit the
    // intended disk. EmptyHex / Route / Region targets skip this overlay.
    if let Some(menu) = state.sector_context_menu.as_ref() {
        if let SectorMenuTarget::System { id, .. } = &menu.target {
            let yellow = egui::Color32::from_rgb(240, 220, 90);
            let stroke = egui::Stroke::new(2.0, yellow);
            let ring_geom = SectorGeom::new(state.hex_size, origin);
            let target_id = id.clone();
            paint_system_rings(ui, rect, &ring_geom, &state.sector, 0.75, stroke, |sid| {
                sid == &target_id
            });
        }
    }

    // ── interaction ─────────────────────────────────────────────────────────
    let pick_geom = SectorGeom::new(state.hex_size, origin);
    let ctrl_down = ui.ctx().input(|i| i.modifiers.ctrl);

    // double-click → rename
    if response.double_clicked() && state.map_tool != MapTool::AddRoute {
        if let Some(pos) = pointer {
            if let Some(id) = pick_geom.hit_system(&state.sector, pos) {
                let name = state
                    .sector
                    .systems
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.to_string())
                    .unwrap_or_default();
                state.pending_rename = Some(PendingRename { id, text: name });
                return;
            }
        }
    }

    // drag start
    if response.drag_started() {
        if let Some(pos) = pointer {
            match state.map_tool {
                MapTool::Select | MapTool::MoveSystem => {
                    if let Some(id) = pick_geom.hit_system(&state.sector, pos) {
                        state.drag_system = Some(id);
                    } else if state.map_tool == MapTool::Select {
                        if let Some(c) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                            state.rect_select = Some((c, c));
                        }
                    }
                }
                MapTool::AddRoute => {
                    if let Some(id) = pick_geom.hit_system(&state.sector, pos) {
                        state.pending_route_start = Some(id);
                    }
                }
                _ => {}
            }
        }
    }

    // drag in progress
    if response.dragged() {
        if let Some(pos) = pointer {
            if let Some((start, _)) = state.rect_select {
                if let Some(c) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                    state.rect_select = Some((start, c));
                }
            }
        }
    }

    // drag stop
    if response.drag_stopped() {
        if state.map_tool == MapTool::AddRoute {
            if let (Some(from), Some(pos)) = (state.pending_route_start.clone(), pointer) {
                if let Some(to) = pick_geom.hit_system(&state.sector, pos) {
                    add_route_between(state, from, to);
                }
            }
            state.pending_route_start = None;
        } else if let (Some(drag_id), Some(pos)) = (state.drag_system.clone(), pointer) {
            if let Some(coord) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                handle_drag_drop(state, drag_id, coord);
            }
            state.drag_system = None;
        } else if let Some((a, b)) = state.rect_select.take() {
            apply_rect_select(state, a, b, ui.ctx().input(|i| i.modifiers.shift));
        }
    }

    // single click
    if response.clicked() {
        if let Some(pos) = pointer {
            let modifiers = ui.ctx().input(|i| i.modifiers);
            let hit = pick_geom.hit_system(&state.sector, pos);
            let coord = pick_geom.pick_hex(pos, sector_w, sector_h);
            // §CTX1 Phase 4 — primary click while a partial-regen anchor is
            // armed completes the rect (anchor → click coord) and consumes the
            // click without falling through to the tool dispatch below.
            let completed = match (state.partial_regen_anchor, coord) {
                (Some(_), Some(c)) => apply_partial_regen_anchor_click(state, c),
                _ => false,
            };
            if !completed {
                handle_click(state, hit, coord, modifiers.shift);
            }
        }
    }

    // §REG2: secondary-click + drag erase / paint on the region brush.
    // §CTX1: hold Ctrl to bypass the paint-erase binding and open the
    // right-click menu instead (see the secondary_clicked handler below).
    if state.map_tool == MapTool::RegionPaint {
        if response.secondary_clicked() && !ctrl_down {
            if let Some(pos) = pointer {
                if let Some(c) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                    paint_region_at(state, c, true);
                }
            }
        }
        if response.dragged() {
            let secondary = ui.ctx().input(|i| i.pointer.secondary_down());
            let primary = ui.ctx().input(|i| i.pointer.primary_down());
            if let Some(pos) = pointer {
                if let Some(c) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                    if secondary {
                        paint_region_at(state, c, true);
                    } else if primary {
                        paint_region_at(state, c, false);
                    }
                }
            }
        }
    }

    // §CTX1 — Phase 1: secondary-click → resolve target and open the floating
    // menu. Guards live inside `resolve_sector_context`. A second secondary
    // click overrides the prior open menu (§4.1 single-menu invariant).
    if response.secondary_clicked() {
        if let Some(pos) = pointer {
            if let Some(target) =
                resolve_sector_context(state, &pick_geom, pos, sector_w, sector_h, ctrl_down)
            {
                state.sector_context_menu = Some(SectorContextMenu {
                    screen_pos: pos,
                    target,
                    bulk_delete_confirm: false,
                });
            }
        }
    }
}

/// §CTX1 — Phase 1: resolve what the right-click landed on. Pure read of
/// `state`, so unit tests can call it directly with a synthesised
/// [`SectorGeom`] + screen position. Returns `None` when the click should be
/// ignored (drag in progress / rect-select live / collision dialog already
/// open / RegionPaint mode without Ctrl).
///
/// Phase 1 only constructs `System` / `MultiSelection` / `EmptyHex`. Route +
/// region hex targets land in Phase 5; star / world targets land in Phase 6.
fn resolve_sector_context(
    state: &BuilderState,
    geom: &SectorGeom,
    pos: Pos2,
    sector_w: u32,
    sector_h: u32,
    ctrl_down: bool,
) -> Option<SectorMenuTarget> {
    // Suppression guards (§4.1).
    if state.drag_system.is_some() || state.rect_select.is_some() {
        return None;
    }
    if state.pending_collision.is_some() {
        return None;
    }
    if state.map_tool == MapTool::RegionPaint && !ctrl_down {
        return None;
    }

    if let Some(id) = geom.hit_system(&state.sector, pos) {
        let coord = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.coord)?;
        if state.selected_systems.contains(&id) && state.selected_systems.len() >= 2 {
            return Some(SectorMenuTarget::MultiSelection {
                ids: state.selected_systems.iter().cloned().collect(),
            });
        }
        return Some(SectorMenuTarget::System { id, coord });
    }

    // §CTX1 Phase 5 — route line hit-test runs before the hex pick so a
    // right-click on a route segment in an otherwise-empty hex opens the
    // route schema rather than the empty-hex one.
    if let Some(route_id) = geom.hit_route(&state.sector, pos) {
        let near_coord = geom
            .pick_hex(pos, sector_w, sector_h)
            .unwrap_or(HexCoord { q: 0, r: 0 });
        return Some(SectorMenuTarget::Route {
            id: route_id,
            near_coord,
        });
    }

    if let Some(coord) = geom.pick_hex(pos, sector_w, sector_h) {
        // §CTX1 Phase 5 — region-hex lookup goes through the cache so unrelated
        // pickers (PLACE / PAINT) keep using the same source of truth (§REG2).
        if let Some(region) = state
            .map_view_cache
            .as_ref()
            .and_then(|c| c.lookup.region_for_hex(coord))
        {
            return Some(SectorMenuTarget::RegionHex {
                region: region.to_string(),
                coord,
            });
        }
        return Some(SectorMenuTarget::EmptyHex { coord });
    }
    None
}

/// §CTX1 — Phase 2: per-item action types. Each variant maps 1:1 to a menu
/// row in the §6.1 / §6.2 schemas. Splitting the actions from the render path
/// lets unit tests assert state mutations without standing up an egui context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SectorMenuAction {
    PlaceSystem {
        coord: HexCoord,
    },
    PaintRegion {
        coord: HexCoord,
    },
    EraseRegion {
        coord: HexCoord,
    },
    FocusSystem {
        id: SystemId,
    },
    RenameSystem {
        id: SystemId,
    },
    DeleteSystem {
        id: SystemId,
    },
    AddRouteFrom {
        id: SystemId,
    },
    AddWorld {
        id: SystemId,
    },
    RegenerateSystem {
        id: SystemId,
        coord: HexCoord,
    },
    TogglePin {
        id: SystemId,
    },
    OpenIn {
        id: SystemId,
        target: OpenInTarget,
    },
    /// §CTX1 Phase 4 — arm the §G5 partial-regen anchor at `coord`. The next
    /// primary click on the map completes the rect anchor→click (see
    /// [`apply_partial_regen_anchor_click`]).
    StartPartialRegen {
        coord: HexCoord,
    },
    // ── §CTX1 Phase 3 — §6.3 multi-selection items ────────────────────────
    /// Focus the first id in `selected_systems` ordering.
    MultiFocusFirst,
    /// Arm the BULK RENAME dialog with a `Sys-{n}` default pattern.
    MultiBulkRenameOpen,
    /// Pin every selected system (idempotent).
    MultiPinAll,
    /// Unpin every selected system (idempotent).
    MultiUnpinAll,
    /// Confirmed bulk delete — only dispatched after the inline
    /// `Confirm? [Yes]` branch. Clears `selected_systems` on completion.
    MultiDeleteAllConfirmed,
    /// Assign one primary faction to every selected system.
    MultiAssignPrimaryFaction {
        fid: FactionId,
    },
    /// Flip the control state on every selected system. `value = None`
    /// clears the flag (matches §6.3 "(none)").
    MultiFlipControlState {
        value: Option<SystemState>,
    },
    /// Re-run `generate_system_here` for every non-pinned selected system.
    MultiReseedWorlds,
    /// Drop `selected_systems` back to empty.
    MultiClearSelection,
    // ── §CTX1 Phase 5 — §6.4 route items ───────────────────────────────────
    /// Cross-tab focus on a route (also lights it up in the MAP overlay).
    FocusRoute {
        id: RouteId,
    },
    /// Hard delete a route via the command bus.
    RemoveRoute {
        id: RouteId,
    },
    /// Replace `route_type` outright. Used by the `RECOLOR ▸` style submenu so
    /// each variant gets a deterministic 1-click action (Q14.1 spec deferral
    /// — submenu over single-cycle for discoverability).
    SetRouteType {
        id: RouteId,
        value: RouteType,
    },
    /// Replace `stability` outright (same pattern as [`Self::SetRouteType`]).
    SetRouteStability {
        id: RouteId,
        value: RouteStability,
    },
    // ── §CTX1 Phase 5 — §6.5 region-hex items ──────────────────────────────
    /// Cross-tab focus on a region.
    FocusRegion {
        region: String,
    },
    /// Erase one hex from a region via the existing overlay path
    /// ([`super::super::state::BuilderState::erase_region_hex`]). Different
    /// from [`Self::EraseRegion`] because we already know which region owns
    /// the hex (the resolver gives us the id directly).
    EraseRegionHex {
        region: String,
        coord: HexCoord,
    },
    /// Replace a region's `kind` (drives the overlay colour — the menu label
    /// is "RECOLOR" even though the model field is `kind`).
    SetRegionKind {
        region: String,
        value: RegionConditionKind,
    },
    /// Open the rename dialog. Commit goes through the command bus on
    /// `show_region_rename_dialog`.
    RenameRegionOpen {
        region: String,
    },
    /// §10 #12 — abort an in-flight ADD-ROUTE: clears
    /// `pending_route_start` and disarms the `MapTool::AddRoute` tool. Exposed
    /// from the §6.2 system menu when a half-route is already armed.
    CancelRoute,
    // Clipboard side-effects belong on `ui.ctx()`, so the render path handles
    // COPY COORD / COPY ID inline rather than threading them through this
    // enum.
}

/// §CTX1 — Phase 2 "Open in ▸" targets wired in Phase 2. Conflict / Orbital /
/// Archetype / Intel are deferred until the SYSTEM tab grows per-section
/// scroll anchors (a polish-phase refactor across conflict.rs / orbital.rs /
/// intel.rs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OpenInTarget {
    System,
    World,
    Routes,
}

/// §CTX1 Phase 7 polish — short human label for the telemetry tail rendered
/// in the status bar. Mirrors the spec's "ctx_menu: <schema> :: <item>"
/// format. Pure mapping; tested by `ctx_menu_telemetry_label_covers_*`.
pub(super) fn sector_menu_action_label(action: &SectorMenuAction) -> &'static str {
    match action {
        SectorMenuAction::PlaceSystem { .. } => "sector :: PLACE SYSTEM",
        SectorMenuAction::PaintRegion { .. } => "sector :: PAINT REGION",
        SectorMenuAction::EraseRegion { .. } => "sector :: ERASE REGION",
        SectorMenuAction::FocusSystem { .. } => "sector :: FOCUS SYSTEM",
        SectorMenuAction::RenameSystem { .. } => "sector :: RENAME",
        SectorMenuAction::DeleteSystem { .. } => "sector :: DELETE",
        SectorMenuAction::AddRouteFrom { .. } => "sector :: ADD ROUTE FROM",
        SectorMenuAction::AddWorld { .. } => "sector :: ADD WORLD",
        SectorMenuAction::RegenerateSystem { .. } => "sector :: REGENERATE SYSTEM",
        SectorMenuAction::TogglePin { .. } => "sector :: TOGGLE PIN",
        SectorMenuAction::OpenIn { .. } => "sector :: OPEN IN",
        SectorMenuAction::StartPartialRegen { .. } => "sector :: START PARTIAL REGEN",
        SectorMenuAction::MultiFocusFirst => "multi :: FOCUS FIRST",
        SectorMenuAction::MultiBulkRenameOpen => "multi :: BULK RENAME",
        SectorMenuAction::MultiPinAll => "multi :: PIN ALL",
        SectorMenuAction::MultiUnpinAll => "multi :: UNPIN ALL",
        SectorMenuAction::MultiDeleteAllConfirmed => "multi :: DELETE ALL",
        SectorMenuAction::MultiAssignPrimaryFaction { .. } => "multi :: ASSIGN PRIMARY FACTION",
        SectorMenuAction::MultiFlipControlState { .. } => "multi :: FLIP CONTROL STATE",
        SectorMenuAction::MultiReseedWorlds => "multi :: RESEED WORLDS",
        SectorMenuAction::MultiClearSelection => "multi :: CLEAR SELECTION",
        SectorMenuAction::FocusRoute { .. } => "route :: FOCUS",
        SectorMenuAction::RemoveRoute { .. } => "route :: REMOVE",
        SectorMenuAction::SetRouteType { .. } => "route :: SET TYPE",
        SectorMenuAction::SetRouteStability { .. } => "route :: SET STABILITY",
        SectorMenuAction::FocusRegion { .. } => "region :: FOCUS",
        SectorMenuAction::EraseRegionHex { .. } => "region :: ERASE HEX",
        SectorMenuAction::SetRegionKind { .. } => "region :: RECOLOR",
        SectorMenuAction::RenameRegionOpen { .. } => "region :: RENAME",
        SectorMenuAction::CancelRoute => "route :: CANCEL",
    }
}

/// §CTX1 — apply one menu item. Returns the menu's close intent (always
/// `true`: every action dismisses the menu).
pub(super) fn apply_sector_menu_action(state: &mut BuilderState, action: SectorMenuAction) -> bool {
    // §CTX1 Phase 7 — capture the activation label up-front so we record
    // *what* the user clicked even when the action errors out below.
    state.last_menu_action = Some(sector_menu_action_label(&action).to_string());
    match action {
        SectorMenuAction::PlaceSystem { coord } => {
            let default_name = format!("Sys-{}", state.sector.systems.len() + 1);
            state.pending_place = Some(PendingPlace {
                coord,
                name: default_name,
            });
        }
        SectorMenuAction::PaintRegion { coord } => {
            paint_region_at(state, coord, false);
        }
        SectorMenuAction::EraseRegion { coord } => {
            let owning = state
                .sector
                .regions
                .iter()
                .find(|r| r.hexes.iter().any(|h| *h == coord))
                .map(|r| r.id.clone());
            if let Some(rid) = owning {
                if let Err(e) = state.erase_region_hex(&rid, coord) {
                    state.modal = Some(ModalKind::Message(format!("Region erase failed: {e}")));
                }
            }
        }
        SectorMenuAction::FocusSystem { id } => {
            state.focus_entity(EntityRef::System(id));
        }
        SectorMenuAction::RenameSystem { id } => {
            let name = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.to_string())
                .unwrap_or_default();
            state.pending_rename = Some(PendingRename { id, text: name });
        }
        SectorMenuAction::DeleteSystem { id } => {
            let cmd = BuilderCommand::RemoveSystem {
                id,
                before: None,
                removed_routes: Vec::new(),
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Delete failed: {e}")));
            }
        }
        SectorMenuAction::AddRouteFrom { id } => {
            state.map_tool = MapTool::AddRoute;
            state.pending_route_start = Some(id);
        }
        SectorMenuAction::AddWorld { id } => {
            let n = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.worlds.len() + 1)
                .unwrap_or(1);
            let cmd = BuilderCommand::AddWorld {
                system: id,
                name: format!("World-{n}"),
                result_id: None,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Add world failed: {e}")));
            }
        }
        SectorMenuAction::RegenerateSystem { id, coord } => {
            if state.pinned_systems.contains(&id) {
                return true;
            }
            let index = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.index)
                .unwrap_or(0);
            match state.generate_system_here(coord, index, None) {
                Ok(new_id) => state.focus_system(new_id),
                Err(e) => {
                    state.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
                }
            }
        }
        SectorMenuAction::TogglePin { id } => {
            if state.pinned_systems.contains(&id) {
                state.pinned_systems.remove(&id);
            } else {
                state.pinned_systems.insert(id);
            }
        }
        SectorMenuAction::StartPartialRegen { coord } => {
            state.partial_regen_anchor = Some(coord);
        }
        SectorMenuAction::OpenIn { id, target } => match target {
            OpenInTarget::System => {
                state.focus_entity(EntityRef::System(id));
            }
            OpenInTarget::World => {
                let first_world = state
                    .sector
                    .systems
                    .iter()
                    .find(|s| s.id == id)
                    .and_then(|s| s.worlds.first().map(|w| w.id.clone()));
                if let Some(world) = first_world {
                    state.focus_entity(EntityRef::World { system: id, world });
                }
            }
            OpenInTarget::Routes => {
                state.selected_system_id = Some(id);
                state.focus_entity(EntityRef::Tab(BuilderTab::Routes));
            }
        },
        // ── §CTX1 Phase 3 — multi-selection branches ──────────────────────
        SectorMenuAction::MultiFocusFirst => {
            if let Some(first) = state.selected_systems.iter().next().cloned() {
                state.focus_entity(EntityRef::System(first));
            }
        }
        SectorMenuAction::MultiBulkRenameOpen => {
            state.pending_bulk_rename = Some(PendingBulkRename {
                pattern: "Sys-{n}".to_string(),
            });
        }
        SectorMenuAction::MultiPinAll => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.insert(id);
            }
        }
        SectorMenuAction::MultiUnpinAll => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.remove(&id);
            }
        }
        SectorMenuAction::MultiDeleteAllConfirmed => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                let cmd = BuilderCommand::RemoveSystem {
                    id: id.clone(),
                    before: None,
                    removed_routes: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!(
                        "Bulk delete failed at {id}: {e}"
                    )));
                    break;
                }
            }
            state.selected_systems.clear();
            state.selected_system_id = None;
        }
        SectorMenuAction::MultiAssignPrimaryFaction { fid } => {
            crate::builder::panels::system::apply_bulk_primary_faction(state, fid);
        }
        SectorMenuAction::MultiFlipControlState { value } => {
            crate::builder::panels::system::apply_bulk_control_state(state, value);
        }
        SectorMenuAction::MultiReseedWorlds => {
            crate::builder::panels::system::apply_bulk_reseed(state);
        }
        SectorMenuAction::MultiClearSelection => {
            state.selected_systems.clear();
            state.selected_system_id = None;
        }
        // ── §CTX1 Phase 5 — §6.4 route branches ────────────────────────────
        SectorMenuAction::FocusRoute { id } => {
            state.focus_entity(EntityRef::Route(id));
        }
        SectorMenuAction::RemoveRoute { id } => {
            let cmd = BuilderCommand::RemoveRoute { id, before: None };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Remove route failed: {e}")));
            }
        }
        SectorMenuAction::SetRouteType { id, value } => {
            let before = state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.route_type);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRouteType {
                id,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Set route type failed: {e}")));
            }
        }
        SectorMenuAction::SetRouteStability { id, value } => {
            let before = state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.stability);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRouteStability {
                id,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Set stability failed: {e}")));
            }
        }
        // ── §CTX1 Phase 5 — §6.5 region branches ───────────────────────────
        SectorMenuAction::FocusRegion { region } => {
            state.focus_entity(EntityRef::Region(region));
        }
        SectorMenuAction::EraseRegionHex { region, coord } => {
            if let Err(e) = state.erase_region_hex(&region, coord) {
                state.modal = Some(ModalKind::Message(format!("Region erase failed: {e}")));
            }
        }
        SectorMenuAction::SetRegionKind { region, value } => {
            let before = state
                .sector
                .regions
                .iter()
                .find(|r| r.id == region)
                .map(|r| r.kind);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRegionKind {
                region,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Recolor region failed: {e}")));
            }
        }
        SectorMenuAction::RenameRegionOpen { region } => {
            let text = state
                .sector
                .regions
                .iter()
                .find(|r| r.id == region)
                .map(|r| r.name.clone())
                .unwrap_or_default();
            state.pending_region_rename = Some(PendingRegionRename { region, text });
        }
        SectorMenuAction::CancelRoute => {
            state.pending_route_start = None;
            state.map_tool = MapTool::Select;
        }
    }
    true
}

/// §CTX1 — Phase 2 §6.1: render the empty-hex schema. Returns `true` when
/// any item activated, so the caller dismisses the menu. Side effects funnel
/// through `state` only — no global modal is opened here (PLACE / RENAME
/// dialogs hook through the existing pending fields).
fn render_empty_hex_menu(ui: &mut egui::Ui, state: &mut BuilderState, coord: HexCoord) -> bool {
    let mut close = false;

    if ui.selectable_label(false, "PLACE SYSTEM HERE…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::PlaceSystem { coord });
    }

    let paint_enabled = state.selected_region_id.is_some();
    let paint_resp = ui.add_enabled(
        paint_enabled,
        egui::SelectableLabel::new(false, "PAINT REGION HERE"),
    );
    if paint_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::PaintRegion { coord });
    }
    if !paint_enabled {
        paint_resp.on_hover_text("Pick a region in the REGIONS tab first.");
    }

    let erase_enabled = state
        .sector
        .regions
        .iter()
        .any(|r| r.hexes.iter().any(|h| *h == coord));
    let erase_resp = ui.add_enabled(
        erase_enabled,
        egui::SelectableLabel::new(false, "ERASE REGION HERE"),
    );
    if erase_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::EraseRegion { coord });
    }
    if !erase_enabled {
        erase_resp.on_hover_text("Hex is not part of any region.");
    }

    ui.separator();

    // §CTX1 Phase 4 — arm the §G5 partial-regen anchor at this hex. While the
    // anchor is live the GENERATION tab surfaces a hint and the next primary
    // click on the map completes the rect (see
    // [`apply_partial_regen_anchor_click`]).
    if ui
        .selectable_label(false, "START PARTIAL REGEN HERE")
        .clicked()
    {
        close |= apply_sector_menu_action(state, SectorMenuAction::StartPartialRegen { coord });
    }

    let label = format!("COPY COORD ({},{})", coord.q, coord.r);
    if ui.selectable_label(false, label).clicked() {
        ui.output_mut(|o| o.copied_text = format!("{},{}", coord.q, coord.r));
        close = true;
    }

    close
}

/// §CTX1 — Phase 2 §6.2: render the single-system schema. Returns `true`
/// when any item activated. The "Open in ▸" submenu is rendered as flat
/// indented buttons in Phase 2 — Phase 7 polish lifts them into a nested
/// menu with a proper `▸` indicator.
fn render_system_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    id: SystemId,
    coord: HexCoord,
) -> bool {
    // §CTX1 §10 row 12 — while ADD-ROUTE is half-armed, the system menu
    // collapses to "CANCEL ROUTE" + "Open in ROUTES" so the user can't
    // accidentally start a second pending route or run a destructive item
    // (DELETE / REGENERATE) that would also tear down the half-route.
    if state.pending_route_start.is_some() {
        let mut close = false;
        if ui.selectable_label(false, "CANCEL ROUTE").clicked() {
            close |= apply_sector_menu_action(state, SectorMenuAction::CancelRoute);
        }
        if ui.selectable_label(false, "Open in ROUTES").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id,
                    target: OpenInTarget::Routes,
                },
            );
        }
        let _ = coord;
        return close;
    }

    let mut close = false;

    if ui.selectable_label(false, "FOCUS SYSTEM").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusSystem { id: id.clone() });
    }
    if ui.selectable_label(false, "RENAME…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::RenameSystem { id: id.clone() });
    }
    if ui.selectable_label(false, "DELETE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::DeleteSystem { id: id.clone() });
    }

    ui.separator();

    if ui.selectable_label(false, "ADD ROUTE FROM HERE…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::AddRouteFrom { id: id.clone() });
    }
    if ui.selectable_label(false, "ADD WORLD").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::AddWorld { id: id.clone() });
    }

    let pinned = state.pinned_systems.contains(&id);
    let regen_resp = ui.add_enabled(
        !pinned,
        egui::SelectableLabel::new(false, "REGENERATE SYSTEM"),
    );
    if regen_resp.clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::RegenerateSystem {
                id: id.clone(),
                coord,
            },
        );
    }
    if pinned {
        regen_resp.on_hover_text("Unpin first (§S3).");
    }

    let pin_label = if pinned { "UNPIN" } else { "TOGGLE PIN" };
    if ui.selectable_label(false, pin_label).clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::TogglePin { id: id.clone() });
    }

    ui.separator();

    // "Open in ▸" — flat indented buttons in Phase 2; Phase 7 polish wraps
    // them in a proper nested submenu.
    ui.label(egui::RichText::new("Open in").italics());
    ui.indent("open_in_indent", |ui| {
        if ui.selectable_label(false, "SYSTEM").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::System,
                },
            );
        }

        let has_world = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| !s.worlds.is_empty())
            .unwrap_or(false);
        let world_resp = ui.add_enabled(has_world, egui::SelectableLabel::new(false, "WORLD"));
        if world_resp.clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::World,
                },
            );
        } else if !has_world {
            world_resp.on_hover_text("System has no worlds.");
        }

        if ui.selectable_label(false, "ROUTES").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::Routes,
                },
            );
        }
    });

    ui.separator();

    if ui
        .selectable_label(false, format!("COPY ID ({id})"))
        .clicked()
    {
        ui.output_mut(|o| o.copied_text = id.to_string());
        close = true;
    }
    if ui
        .selectable_label(false, format!("COPY COORD ({},{})", coord.q, coord.r))
        .clicked()
    {
        ui.output_mut(|o| o.copied_text = format!("{},{}", coord.q, coord.r));
        close = true;
    }

    close
}

/// §CTX1 — Phase 3 §6.3: render the multi-selection schema. Returns `true`
/// when any item activated. The `DELETE ALL` row uses an in-place confirm
/// gate (`bulk_delete_confirm` on the open [`SectorContextMenu`]) so the
/// confirmation lives inside the menu instead of a global modal (§7 Phase 3
/// spec — see [docs/CONTEXT_MENU.txt]).
fn render_multi_selection_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    ids: &[SystemId],
) -> bool {
    let mut close = false;
    ui.label(format!("{} systems selected", ids.len()));
    ui.separator();

    if ui.selectable_label(false, "FOCUS FIRST").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiFocusFirst);
    }
    if ui.selectable_label(false, "BULK RENAME…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiBulkRenameOpen);
    }

    let any_unpinned = ids.iter().any(|id| !state.pinned_systems.contains(id));
    let any_pinned = ids.iter().any(|id| state.pinned_systems.contains(id));
    let pin_resp = ui.add_enabled(any_unpinned, egui::SelectableLabel::new(false, "PIN ALL"));
    if pin_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiPinAll);
    }
    if !any_unpinned {
        pin_resp.on_hover_text("Every selected system is already pinned.");
    }
    let unpin_resp = ui.add_enabled(any_pinned, egui::SelectableLabel::new(false, "UNPIN ALL"));
    if unpin_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiUnpinAll);
    }
    if !any_pinned {
        unpin_resp.on_hover_text("Nothing in the selection is pinned.");
    }

    ui.separator();

    // §CTX1 Phase 3 — inline DELETE ALL confirm. First click flips the
    // `bulk_delete_confirm` flag on the live menu state and keeps the menu
    // open; the second pass swaps the row out for "Confirm? [Yes] [No]".
    let confirming = state
        .sector_context_menu
        .as_ref()
        .map(|m| m.bulk_delete_confirm)
        .unwrap_or(false);
    if confirming {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Confirm DELETE ALL?").strong());
            if ui.button("Yes").clicked() {
                close |= apply_sector_menu_action(state, SectorMenuAction::MultiDeleteAllConfirmed);
            }
            if ui.button("No").clicked() {
                if let Some(menu) = state.sector_context_menu.as_mut() {
                    menu.bulk_delete_confirm = false;
                }
            }
        });
    } else if ui
        .selectable_label(false, format!("✕ DELETE ALL ({})", ids.len()))
        .clicked()
    {
        if let Some(menu) = state.sector_context_menu.as_mut() {
            menu.bulk_delete_confirm = true;
        }
    }

    ui.separator();

    // ASSIGN PRIMARY FACTION ▸ submenu — disabled (with tooltip) when no
    // factions exist (§6.3 enabled-when).
    let factions: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    if factions.is_empty() {
        ui.add_enabled(
            false,
            egui::SelectableLabel::new(false, "ASSIGN PRIMARY FACTION ▸"),
        )
        .on_disabled_hover_text("Sector has no factions — add one in the FACTIONS tab.");
    } else {
        ui.menu_button("ASSIGN PRIMARY FACTION ▸", |ui| {
            for (fid, name) in &factions {
                if ui
                    .selectable_label(false, format!("→ {name} ({fid})"))
                    .clicked()
                {
                    close |= apply_sector_menu_action(
                        state,
                        SectorMenuAction::MultiAssignPrimaryFaction { fid: fid.clone() },
                    );
                    ui.close_menu();
                }
            }
        });
    }

    // FLIP CONTROL STATE ▸ submenu — always enabled, includes "(none)" clear.
    ui.menu_button("FLIP CONTROL STATE ▸", |ui| {
        for value in [
            None,
            Some(SystemState::Pacified),
            Some(SystemState::Fragmented),
            Some(SystemState::Blockaded),
            Some(SystemState::Warzone),
            Some(SystemState::Infiltrated),
            Some(SystemState::Quarantined),
            Some(SystemState::Uncharted),
        ] {
            let label = match value {
                None => "(none)".to_string(),
                Some(v) => format!("{v:?}"),
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::MultiFlipControlState { value },
                );
                ui.close_menu();
            }
        }
    });

    let reseed_enabled = ids.iter().any(|id| !state.pinned_systems.contains(id));
    let reseed_resp = ui.add_enabled(
        reseed_enabled,
        egui::SelectableLabel::new(false, "RESEED WORLDS"),
    );
    if reseed_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiReseedWorlds);
    }
    if !reseed_enabled {
        reseed_resp.on_hover_text("All selected systems are pinned — unpin first (§S3).");
    }

    ui.separator();

    if ui.selectable_label(false, "CLEAR SELECTION").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiClearSelection);
    }

    close
}

/// §CTX1 Phase 5: human-readable label for a [`RouteStability`]. Local copy of
/// the helper in [`super::routes::stability_label`] — that one is private to
/// the ROUTES panel and we don't want to plumb a `pub(super)` re-export for
/// two call sites.
fn stability_label(value: RouteStability) -> &'static str {
    match value {
        RouteStability::Stable => "stable",
        RouteStability::Unstable => "unstable",
        RouteStability::Hazardous => "hazardous",
        RouteStability::Perilous => "perilous",
    }
}

/// §CTX1 — Phase 5 §6.4: render the route schema. Returns `true` when any
/// item activated. RECOLOR / CYCLE STABILITY are rendered as nested submenus
/// so each variant gets a deterministic 1-click action (resolves Q14.1 in
/// favour of submenu over single-cycle for discoverability).
fn render_route_menu(ui: &mut egui::Ui, state: &mut BuilderState, id: RouteId) -> bool {
    let mut close = false;
    let route_summary = state
        .sector
        .routes
        .iter()
        .find(|r| r.id == id)
        .map(|r| (r.route_type, r.stability));
    let Some((cur_type, cur_stab)) = route_summary else {
        if ui.selectable_label(false, "CLOSE").clicked() {
            close = true;
        }
        return close;
    };

    ui.label(
        egui::RichText::new(format!(
            "ROUTE {id} — {} / {}",
            cur_type.editor_label(),
            stability_label(cur_stab)
        ))
        .italics(),
    );
    ui.separator();

    if ui.selectable_label(false, "FOCUS ROUTE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusRoute { id: id.clone() });
    }
    if ui.selectable_label(false, "✕ REMOVE ROUTE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::RemoveRoute { id: id.clone() });
    }

    ui.menu_button("CYCLE ROUTE TYPE ▸", |ui| {
        for value in RouteType::ALL {
            let label = if value == cur_type {
                format!("• {}", value.editor_label())
            } else {
                value.editor_label().to_string()
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRouteType {
                        id: id.clone(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    ui.menu_button("CYCLE STABILITY ▸", |ui| {
        for value in [
            RouteStability::Stable,
            RouteStability::Unstable,
            RouteStability::Hazardous,
            RouteStability::Perilous,
        ] {
            let label = if value == cur_stab {
                format!("• {}", stability_label(value))
            } else {
                stability_label(value).to_string()
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRouteStability {
                        id: id.clone(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    ui.separator();
    if ui.selectable_label(false, "Open in ROUTES ▸").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusRoute { id });
    }

    close
}

/// §CTX1 — Phase 5 §6.5: render the region-hex schema. The hex was already
/// resolved to its owning region in `resolve_sector_context`.
fn render_region_hex_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    region: &str,
    coord: HexCoord,
) -> bool {
    let mut close = false;
    let summary = state
        .sector
        .regions
        .iter()
        .find(|r| r.id == region)
        .map(|r| (r.name.clone(), r.kind));
    let Some((name, cur_kind)) = summary else {
        if ui.selectable_label(false, "CLOSE").clicked() {
            close = true;
        }
        return close;
    };

    ui.label(
        egui::RichText::new(format!("REGION {region} — {name} [{}]", cur_kind.label())).italics(),
    );
    ui.separator();

    if ui.selectable_label(false, "FOCUS REGION").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::FocusRegion {
                region: region.to_string(),
            },
        );
    }
    if ui.selectable_label(false, "ERASE FROM REGION").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::EraseRegionHex {
                region: region.to_string(),
                coord,
            },
        );
    }

    ui.menu_button("RECOLOR ▸", |ui| {
        for value in RegionConditionKind::ALL.iter().copied() {
            let label = if value == cur_kind {
                format!("• {} {}", value.glyph(), value.label())
            } else {
                format!("  {} {}", value.glyph(), value.label())
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRegionKind {
                        region: region.to_string(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    if ui.selectable_label(false, "RENAME REGION…").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::RenameRegionOpen {
                region: region.to_string(),
            },
        );
    }

    close
}

/// §CTX1 — Phase 1: pure dismiss predicate. Factored out of
/// [`show_sector_context_menu`] so unit tests can verify the Escape /
/// focus-loss / outside-click rules without standing up an egui context.
fn should_dismiss_sector_context_menu(
    esc_pressed: bool,
    focused: bool,
    primary_click_outside: bool,
) -> bool {
    esc_pressed || !focused || primary_click_outside
}

/// §CTX1 §10 — returns `true` when the target referenced by an open menu no
/// longer exists in `state.sector`. The renderer calls this each frame so an
/// undo / redo / replace_sector that removes the referenced entity drops the
/// menu instead of acting on a vanished id. Pure helper so unit tests can
/// exercise the System / Route / RegionHex / MultiSelection cases without an
/// egui context. `EmptyHex` and `SubsectorBorder` are inert — staleness only
/// makes sense for entity-backed variants.
pub(super) fn sector_menu_target_is_stale(state: &BuilderState, target: &SectorMenuTarget) -> bool {
    match target {
        SectorMenuTarget::System { id, .. } => !state.sector.systems.iter().any(|s| &s.id == id),
        SectorMenuTarget::Route { id, .. } => !state.sector.routes.iter().any(|r| &r.id == id),
        SectorMenuTarget::RegionHex { region, .. } => {
            !state.sector.regions.iter().any(|r| r.id == *region)
        }
        SectorMenuTarget::MultiSelection { ids } => ids
            .iter()
            .all(|id| !state.sector.systems.iter().any(|s| &s.id == id)),
        SectorMenuTarget::EmptyHex { .. } | SectorMenuTarget::SubsectorBorder { .. } => false,
    }
}

/// §CTX1 — Phase 7 polish: pick the [`Align2`] pivot that should anchor a
/// floating right-click menu at `cursor`, so the menu opens *away from* the
/// nearer screen edge. Combined with `Area::constrain(true)` this keeps the
/// menu fully on-screen even at the corners.
///
/// The pivot is the corner of the menu that `Area::fixed_pos(cursor)` refers
/// to; for example `RIGHT_TOP` makes the menu grow leftwards / downwards
/// from the cursor, which is what we want when the cursor sits on the right
/// half of the viewport.
///
/// Pure helper so unit tests can exercise the four quadrant cases without an
/// egui context.
pub(super) fn menu_anchor_pivot(cursor: egui::Pos2, screen: egui::Rect) -> egui::Align2 {
    let centre = screen.center();
    let right = cursor.x > centre.x;
    let bottom = cursor.y > centre.y;
    match (right, bottom) {
        (false, false) => egui::Align2::LEFT_TOP,
        (true, false) => egui::Align2::RIGHT_TOP,
        (false, true) => egui::Align2::LEFT_BOTTOM,
        (true, true) => egui::Align2::RIGHT_BOTTOM,
    }
}

/// §CTX1 — render the floating right-click menu. Anchored at
/// `menu.screen_pos`. Phase 2 wires the §6.1 (empty hex) and §6.2 (single
/// system) schemas; multi-selection, route, region-hex, and subsector-border
/// targets keep the Phase 1 placeholder until Phases 3/5 land. Dismissed on
/// Escape / focus-loss / primary click outside the area / item activation.
fn show_sector_context_menu(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(menu) = state.sector_context_menu.as_ref() else {
        return;
    };
    // §CTX1 §10 — re-resolve target validity on every render so an undo/redo
    // that removed the referenced system/route/region drops the now-stale menu
    // instead of dispatching actions against a vanished id.
    if sector_menu_target_is_stale(state, &menu.target) {
        state.sector_context_menu = None;
        return;
    }
    let screen_pos = menu.screen_pos;
    let target = menu.target.clone();
    let mut close = false;
    // §CTX1 Phase 7 — flip the anchor pivot based on cursor quadrant so the
    // menu grows away from the nearer screen edge. `constrain(true)` is the
    // final safety net for any remaining overflow.
    let screen_rect = ctx.input(|i| i.screen_rect());
    let pivot = menu_anchor_pivot(screen_pos, screen_rect);
    let area_resp = egui::Area::new(egui::Id::new("sector_context_menu"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(screen_pos)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                match &target {
                    SectorMenuTarget::EmptyHex { coord } => {
                        close |= render_empty_hex_menu(ui, state, *coord);
                    }
                    SectorMenuTarget::System { id, coord } => {
                        close |= render_system_menu(ui, state, id.clone(), *coord);
                    }
                    SectorMenuTarget::MultiSelection { ids } => {
                        close |= render_multi_selection_menu(ui, state, ids);
                    }
                    SectorMenuTarget::Route { id, .. } => {
                        close |= render_route_menu(ui, state, id.clone());
                    }
                    SectorMenuTarget::RegionHex { region, coord } => {
                        close |= render_region_hex_menu(ui, state, region, *coord);
                    }
                    SectorMenuTarget::SubsectorBorder { .. } => {
                        // Future (subsector border) replaces this placeholder.
                        if ui.selectable_label(false, "CLOSE").clicked() {
                            close = true;
                        }
                    }
                }
            });
        });

    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let focused = ctx.input(|i| i.focused);
    let area_rect = area_resp.response.rect;
    let primary_click_outside = ctx.input(|i| {
        i.pointer.primary_clicked()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area_rect.contains(p))
    });

    if close || should_dismiss_sector_context_menu(esc, focused, primary_click_outside) {
        state.sector_context_menu = None;
    }
}

/// Rebuilds [`MapViewCache`] when the underlying sector slice digest changes.
/// Pure — no UI side effects. Cheap when the cache is hot.
fn refresh_map_cache(state: &mut BuilderState) {
    let digest = sector_view_digest(state);
    let stale = state
        .map_view_cache
        .as_ref()
        .map(|c| c.digest != digest)
        .unwrap_or(true);
    if !stale {
        return;
    }
    let mut subsectors = build_subsectors(
        &state.sector,
        SubsectorConfig {
            target_systems_per_subsector: state.subsector_target_systems.max(1),
            ..SubsectorConfig::default()
        },
    )
    .unwrap_or_default();
    crate::builder::panels::subsectors::apply_subsector_overrides(&mut subsectors, state);
    let lookup = SectorMapCache::new(&state.sector, &subsectors);
    state.map_view_cache = Some(MapViewCache {
        digest,
        subsectors,
        lookup,
    });
}

fn sector_view_digest(state: &BuilderState) -> String {
    // Hash the minimal slice that drives subsector clustering + region tints.
    // Keeping the slice narrow avoids invalidating the cache on unrelated
    // edits (e.g. faction prose). §SUB2..§SUB4 overrides also feed in so the
    // cache rebuilds when the user reclusters, moves systems between cells,
    // or overrides a capital.
    let sector = &state.sector;
    #[derive(serde::Serialize)]
    struct Slice<'a> {
        w: u32,
        h: u32,
        systems: Vec<(&'a str, i32, i32)>,
        routes: Vec<(&'a str, &'a str, &'a str)>,
        regions: Vec<(&'a str, Vec<(i32, i32)>)>,
        sub_target: u32,
        sub_sys: Vec<(&'a str, &'a str)>,
        sub_cap: Vec<(&'a str, &'a str)>,
    }
    let slice = Slice {
        w: sector.width,
        h: sector.height,
        systems: sector
            .systems
            .iter()
            .map(|s| (s.id.as_str(), s.coord.q, s.coord.r))
            .collect(),
        routes: sector
            .routes
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    r.from_system_id.as_str(),
                    r.to_system_id.as_str(),
                )
            })
            .collect(),
        regions: sector
            .regions
            .iter()
            .map(|r| (r.id.as_str(), r.hexes.iter().map(|h| (h.q, h.r)).collect()))
            .collect(),
        sub_target: state.subsector_target_systems,
        sub_sys: state
            .subsector_system_overrides
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
        sub_cap: state
            .subsector_capital_overrides
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
    };
    digest_input(&slice)
}

fn handle_click(
    state: &mut BuilderState,
    hit: Option<SystemId>,
    coord: Option<HexCoord>,
    shift: bool,
) {
    match state.map_tool {
        MapTool::Select | MapTool::MoveSystem => match hit {
            Some(id) => {
                if shift {
                    state.toggle_system_selection(id);
                } else {
                    state.focus_system(id);
                }
            }
            None => {
                if !shift {
                    state.selected_systems.clear();
                    state.selected_system_id = None;
                }
            }
        },
        MapTool::AddSystem => {
            if let (None, Some(c)) = (hit, coord) {
                let default_name = format!("Sys-{}", state.sector.systems.len() + 1);
                state.pending_place = Some(PendingPlace {
                    coord: c,
                    name: default_name,
                });
            }
        }
        MapTool::DeleteSystem => {
            if let Some(id) = hit {
                let cmd = BuilderCommand::RemoveSystem {
                    id,
                    before: None,
                    removed_routes: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("Delete failed: {e}")));
                }
            }
        }
        MapTool::AddRoute => {
            if let Some(id) = hit {
                if let Some(from) = state.pending_route_start.take() {
                    add_route_between(state, from, id);
                } else {
                    state.pending_route_start = Some(id);
                }
            }
        }
        MapTool::RegionPaint => {
            if let Some(c) = coord {
                paint_region_at(state, c, false);
            }
        }
    }
}

/// §CTX1 Phase 4 — primary-click completion of the partial-regen anchor flow.
/// Reads the live anchor from `state`, normalises a [`PartialRegenRect`] with
/// the click coord, writes it into [`BuilderState::partial_regen_rect`], clears
/// the anchor, and returns `true` so the caller can swallow the click without
/// double-dispatching the armed map tool. Returns `false` when no anchor is
/// armed.
pub(super) fn apply_partial_regen_anchor_click(
    state: &mut BuilderState,
    click_coord: HexCoord,
) -> bool {
    let Some(anchor) = state.partial_regen_anchor else {
        return false;
    };
    state.partial_regen_rect = Some(crate::builder::state::PartialRegenRect::from_corners(
        anchor,
        click_coord,
    ));
    state.partial_regen_anchor = None;
    true
}

/// §REG2 left/right click brush — `erase=true` removes the hex, otherwise
/// it is painted into `state.selected_region_id`. No-op when no region is
/// selected (panel surfaces a hint in that case).
pub(super) fn paint_region_at(state: &mut BuilderState, hex: HexCoord, erase: bool) {
    let Some(id) = state.selected_region_id.clone() else {
        state.modal = Some(ModalKind::Message(
            "Pick a region in the REGIONS tab before painting.".into(),
        ));
        return;
    };
    let result = if erase {
        state.erase_region_hex(&id, hex)
    } else {
        state.paint_region_hex(&id, hex)
    };
    if let Err(e) = result {
        state.modal = Some(ModalKind::Message(format!("Region paint failed: {e}")));
    }
}

fn add_route_between(state: &mut BuilderState, from: SystemId, to: SystemId) {
    if from == to {
        state.modal = Some(ModalKind::Message(
            "Route needs two distinct systems.".into(),
        ));
        return;
    }
    let selected_route = ids::route_id(&from, &to);
    let cmd = BuilderCommand::AddRoute {
        from,
        to,
        route_type: sectorforge::sector_model::RouteType::ChartedPassage,
        stability: sectorforge::sector_model::RouteStability::Stable,
        result_id: None,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Add route failed: {e}")));
        return;
    }
    state.focus_entity(crate::builder::state::EntityRef::Route(selected_route));
}

fn handle_drag_drop(state: &mut BuilderState, drag_id: SystemId, coord: HexCoord) {
    let from_coord = state
        .sector
        .systems
        .iter()
        .find(|s| s.id == drag_id)
        .map(|s| s.coord);
    let Some(from_coord) = from_coord else { return };
    if from_coord == coord {
        return;
    }
    // §S6: bounds + collision.
    if coord.q < 0
        || coord.r < 0
        || (coord.q as u32) >= state.sector.width
        || (coord.r as u32) >= state.sector.height
    {
        state.modal = Some(ModalKind::Message(format!(
            "Coord ({},{}) is outside sector {}x{}.",
            coord.q, coord.r, state.sector.width, state.sector.height
        )));
        return;
    }
    if let Some(occupant) = state
        .sector
        .systems
        .iter()
        .find(|s| s.coord == coord && s.id != drag_id)
        .map(|s| s.id.clone())
    {
        state.pending_collision = Some(PendingCollision {
            dragging: drag_id,
            target: coord,
            occupant,
        });
        return;
    }
    let cmd = BuilderCommand::MoveSystem {
        id: drag_id,
        from: from_coord,
        to: coord,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Move failed: {e}")));
    }
}

fn apply_rect_select(state: &mut BuilderState, a: HexCoord, b: HexCoord, additive: bool) {
    let (min_q, max_q) = (a.q.min(b.q), a.q.max(b.q));
    let (min_r, max_r) = (a.r.min(b.r), a.r.max(b.r));
    if !additive {
        state.selected_systems.clear();
    }
    for sys in &state.sector.systems {
        if sys.coord.q >= min_q
            && sys.coord.q <= max_q
            && sys.coord.r >= min_r
            && sys.coord.r <= max_r
        {
            state.selected_systems.insert(sys.id.clone());
        }
    }
    state.selected_system_id = state.selected_systems.iter().next().cloned();
}

// ── Transient dialogs ───────────────────────────────────────────────────────

fn show_place_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_place.clone() else {
        return;
    };
    let mut name = pending.name.clone();
    let mut close = false;
    let mut commit = false;
    egui::Window::new("Place system")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("hex ({}, {})", pending.coord.q, pending.coord.r));
            ui.text_edit_singleline(&mut name);
            ui.horizontal(|ui| {
                if ui.button("Place").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let cmd = BuilderCommand::AddSystem {
            coord: pending.coord,
            name: name.clone(),
            result_id: None,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Add failed: {e}")));
        }
    }
    if close {
        state.pending_place = None;
    } else {
        state.pending_place = Some(PendingPlace {
            coord: pending.coord,
            name,
        });
    }
}

fn show_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_rename.clone() else {
        return;
    };
    let mut text = pending.text.clone();
    let mut close = false;
    let mut commit = false;
    egui::Window::new("Rename system")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(pending.id.to_string());
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let from = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == pending.id)
            .map(|s| s.name.to_string())
            .unwrap_or_default();
        let cmd = BuilderCommand::RenameSystem {
            id: pending.id.clone(),
            from,
            to: text.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Rename failed: {e}")));
        }
    }
    if close {
        state.pending_rename = None;
    } else {
        state.pending_rename = Some(PendingRename {
            id: pending.id,
            text,
        });
    }
}

/// §CTX1 Phase 3 — BULK RENAME pattern dialog opened from the MAP tab's
/// right-click multi-selection menu. Pattern tokens (`{n}`, `{id}`,
/// `{name}`) match the §S4 bulk-ops dialog and dispatch through
/// [`crate::builder::panels::system::apply_bulk_rename`] on commit.
fn show_bulk_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_bulk_rename.clone() else {
        return;
    };
    let n = state.selected_systems.len();
    let mut pattern = pending.pattern.clone();
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Bulk rename selection")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("{n} system(s) selected"));
            ui.label("Pattern — `{n}` = sequence, `{id}` = system id, `{name}` = current name");
            ui.text_edit_singleline(&mut pattern);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        crate::builder::panels::system::apply_bulk_rename(state, &pattern);
    }
    if close {
        state.pending_bulk_rename = None;
    } else {
        state.pending_bulk_rename = Some(PendingBulkRename { pattern });
    }
}

/// §CTX1 Phase 5 — modal rename dialog for the §6.5 "RENAME REGION…" entry.
/// Commits through [`BuilderCommand::RenameRegion`] so the change is undoable.
fn show_region_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_region_rename.clone() else {
        return;
    };
    let before = state
        .sector
        .regions
        .iter()
        .find(|r| r.id == pending.region)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let mut text = pending.text.clone();
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Rename region")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("region: {} — current: {}", pending.region, before));
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                let enabled = !text.trim().is_empty() && text != before;
                if ui
                    .add_enabled(enabled, egui::Button::new("Rename"))
                    .clicked()
                {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let cmd = BuilderCommand::RenameRegion {
            region: pending.region.clone(),
            before: before.clone(),
            after: text.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Rename region failed: {e}")));
        }
    }
    if close {
        state.pending_region_rename = None;
    } else {
        state.pending_region_rename = Some(PendingRegionRename {
            region: pending.region,
            text,
        });
    }
}

fn show_collision_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_collision.clone() else {
        return;
    };
    let mut close = false;
    let mut action: Option<CollisionAction> = None;
    egui::Window::new("Hex occupied")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!(
                "Hex ({},{}) is held by {}.",
                pending.target.q, pending.target.r, pending.occupant
            ));
            ui.horizontal(|ui| {
                if ui.button("Swap").clicked() {
                    action = Some(CollisionAction::Swap);
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if let Some(CollisionAction::Swap) = action {
        let cmd = BuilderCommand::SwapSystems {
            a: pending.dragging.clone(),
            b: pending.occupant.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Swap failed: {e}")));
        }
    }
    if close {
        state.pending_collision = None;
    }
}

enum CollisionAction {
    Swap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_tool_labels_are_non_empty() {
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            assert!(!tool.label().is_empty());
        }
    }

    fn blank(width: u32, height: u32) -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", width, height)
    }

    #[test]
    fn handle_click_select_focuses_system() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        handle_click(
            &mut state,
            Some(id.clone()),
            Some(HexCoord { q: 1, r: 1 }),
            false,
        );
        assert_eq!(state.selected_system_id, Some(id.clone()));
        assert!(state.selected_systems.contains(&id));
    }

    #[test]
    fn handle_click_shift_adds_to_selection() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 0 }, "B")
            .unwrap();
        handle_click(
            &mut state,
            Some(a.clone()),
            Some(HexCoord { q: 0, r: 0 }),
            false,
        );
        handle_click(
            &mut state,
            Some(b.clone()),
            Some(HexCoord { q: 1, r: 0 }),
            true,
        );
        assert!(state.selected_systems.contains(&a));
        assert!(state.selected_systems.contains(&b));
    }

    #[test]
    fn handle_drag_drop_move_succeeds() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        handle_drag_drop(&mut state, id.clone(), HexCoord { q: 3, r: 3 });
        let sys = state.sector.systems.iter().find(|s| s.id == id).unwrap();
        assert_eq!(sys.coord, HexCoord { q: 3, r: 3 });
    }

    #[test]
    fn handle_drag_drop_collision_arms_dialog() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        handle_drag_drop(&mut state, a.clone(), HexCoord { q: 2, r: 2 });
        let pending = state.pending_collision.expect("collision dialog armed");
        assert_eq!(pending.dragging, a);
        assert_eq!(pending.occupant, b);
    }

    #[test]
    fn handle_drag_drop_out_of_bounds_rejected() {
        let mut state = blank(4, 4);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        handle_drag_drop(&mut state, id.clone(), HexCoord { q: 9, r: 9 });
        let sys = state.sector.systems.iter().find(|s| s.id == id).unwrap();
        assert_eq!(sys.coord, HexCoord { q: 1, r: 1 });
        assert!(matches!(state.modal, Some(ModalKind::Message(_))));
    }

    #[test]
    fn apply_rect_select_picks_systems_in_box() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        let _outside = state
            .sector
            .add_system(HexCoord { q: 6, r: 6 }, "C")
            .unwrap();
        apply_rect_select(
            &mut state,
            HexCoord { q: 0, r: 0 },
            HexCoord { q: 3, r: 3 },
            false,
        );
        assert!(state.selected_systems.contains(&a));
        assert!(state.selected_systems.contains(&b));
        assert_eq!(state.selected_systems.len(), 2);
    }

    #[test]
    fn map_cache_refresh_populates_subsectors() {
        let mut state = blank(8, 8);
        state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        state
            .sector
            .add_system(HexCoord { q: 3, r: 3 }, "B")
            .unwrap();
        refresh_map_cache(&mut state);
        let cache = state.map_view_cache.as_ref().expect("cache populated");
        assert!(!cache.subsectors.is_empty());
        assert_eq!(
            cache.lookup.hex_system.len(),
            state.sector.systems.len(),
            "lookup table contains every system"
        );
    }

    // ── §CTX1 Phase 1 tests ────────────────────────────────────────────────

    #[test]
    fn secondary_click_on_system_opens_menu() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false)
            .expect("right-click on a system resolves");
        match target {
            SectorMenuTarget::System {
                id: hit_id,
                coord: hit_coord,
            } => {
                assert_eq!(hit_id, id);
                assert_eq!(hit_coord, HexCoord { q: 2, r: 2 });
            }
            other => panic!("expected System target, got {other:?}"),
        }
    }

    #[test]
    fn secondary_click_on_empty_hex_returns_empty_hex_target() {
        let state = blank(8, 8);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(3, 3);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false)
            .expect("right-click inside sector resolves");
        assert!(matches!(
            target,
            SectorMenuTarget::EmptyHex { coord } if coord == HexCoord { q: 3, r: 3 }
        ));
    }

    #[test]
    fn secondary_click_dismissed_during_drag() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.drag_system = Some(id);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none(),
            "drag in progress suppresses the menu"
        );
    }

    #[test]
    fn secondary_click_dismissed_during_rect_select() {
        let mut state = blank(8, 8);
        state.rect_select = Some((HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 4 }));
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none());
    }

    #[test]
    fn secondary_click_in_region_paint_needs_ctrl() {
        let mut state = blank(8, 8);
        state.map_tool = MapTool::RegionPaint;
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(1, 1);
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none(),
            "RegionPaint without Ctrl yields to paint-erase"
        );
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, true).is_some(),
            "Ctrl modifier opens the menu even in RegionPaint mode"
        );
    }

    #[test]
    fn multi_selection_target_when_two_selected() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(1, 1);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false).unwrap();
        match target {
            SectorMenuTarget::MultiSelection { ids } => {
                assert!(ids.contains(&a) && ids.contains(&b));
                assert_eq!(ids.len(), 2);
            }
            other => panic!("expected MultiSelection, got {other:?}"),
        }
    }

    #[test]
    fn escape_closes_menu() {
        assert!(should_dismiss_sector_context_menu(true, true, false));
        assert!(
            should_dismiss_sector_context_menu(false, false, false),
            "focus loss dismisses"
        );
        assert!(
            should_dismiss_sector_context_menu(false, true, true),
            "outside primary click dismisses"
        );
        assert!(!should_dismiss_sector_context_menu(false, true, false));
    }

    #[test]
    fn context_menu_field_default_none() {
        let state = blank(4, 4);
        assert!(state.sector_context_menu.is_none());
    }

    // ── §CTX1 Phase 2 tests — per-item action assertions ──────────────────

    fn add_region(state: &mut BuilderState, id: &str, hex: HexCoord) {
        let mut regions = (*state.sector.regions).clone();
        regions.push(sectorforge::regions::WarpRegion {
            id: id.to_string(),
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            name: format!("Region {id}"),
            hexes: vec![hex],
            centre: hex,
        });
        state.sector.regions = std::sync::Arc::new(regions);
    }

    #[test]
    fn ctx_action_place_system_arms_pending_place() {
        let mut state = blank(8, 8);
        let closed = apply_sector_menu_action(
            &mut state,
            SectorMenuAction::PlaceSystem {
                coord: HexCoord { q: 2, r: 3 },
            },
        );
        assert!(closed);
        let pending = state.pending_place.expect("pending_place armed");
        assert_eq!(pending.coord, HexCoord { q: 2, r: 3 });
        assert!(pending.name.starts_with("Sys-"));
    }

    #[test]
    fn ctx_action_paint_region_paints_when_region_selected() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-a", HexCoord { q: 0, r: 0 });
        state.selected_region_id = Some("reg-a".into());
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::PaintRegion {
                coord: HexCoord { q: 4, r: 4 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-a")
            .unwrap();
        assert!(region.hexes.contains(&HexCoord { q: 4, r: 4 }));
    }

    #[test]
    fn ctx_action_erase_region_removes_hex() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-a", HexCoord { q: 1, r: 1 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::EraseRegion {
                coord: HexCoord { q: 1, r: 1 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-a")
            .unwrap();
        assert!(!region.hexes.contains(&HexCoord { q: 1, r: 1 }));
    }

    #[test]
    fn ctx_action_focus_system_switches_tab_and_selection() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id: id.clone() });
        assert_eq!(state.selected_system_id, Some(id));
        assert_eq!(state.active_tab, BuilderTab::System);
    }

    #[test]
    fn ctx_action_rename_arms_pending_rename_with_current_name() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RenameSystem { id: id.clone() },
        );
        let pending = state.pending_rename.expect("rename armed");
        assert_eq!(pending.id, id);
        assert_eq!(pending.text, "Alpha");
    }

    #[test]
    fn ctx_action_delete_removes_system() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::DeleteSystem { id: id.clone() },
        );
        assert!(state.sector.systems.iter().all(|s| s.id != id));
    }

    #[test]
    fn ctx_action_add_route_from_arms_tool() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::AddRouteFrom { id: id.clone() },
        );
        assert_eq!(state.map_tool, MapTool::AddRoute);
        assert_eq!(state.pending_route_start, Some(id));
    }

    #[test]
    fn ctx_action_add_world_appends_world() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let before = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .len();
        apply_sector_menu_action(&mut state, SectorMenuAction::AddWorld { id: id.clone() });
        let after = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .len();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn ctx_action_regenerate_pinned_is_noop() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.pinned_systems.insert(id.clone());
        let before_modal = state.modal.is_some();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RegenerateSystem {
                id,
                coord: HexCoord { q: 2, r: 2 },
            },
        );
        // Pinned guard returns early — no modal, no command in log.
        assert_eq!(state.modal.is_some(), before_modal);
    }

    #[test]
    fn ctx_action_toggle_pin_flips_membership() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        assert!(!state.pinned_systems.contains(&id));
        apply_sector_menu_action(&mut state, SectorMenuAction::TogglePin { id: id.clone() });
        assert!(state.pinned_systems.contains(&id));
        apply_sector_menu_action(&mut state, SectorMenuAction::TogglePin { id: id.clone() });
        assert!(!state.pinned_systems.contains(&id));
    }

    #[test]
    fn ctx_action_open_in_routes_switches_to_routes_tab() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::OpenIn {
                id: id.clone(),
                target: OpenInTarget::Routes,
            },
        );
        assert_eq!(state.active_tab, BuilderTab::Routes);
        assert_eq!(state.selected_system_id, Some(id));
    }

    #[test]
    fn ctx_action_open_in_world_selects_first_world() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let cmd = BuilderCommand::AddWorld {
            system: id.clone(),
            name: "World-1".into(),
            result_id: None,
        };
        state.run(cmd).unwrap();
        let first_world = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .first()
            .unwrap()
            .id
            .clone();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::OpenIn {
                id: id.clone(),
                target: OpenInTarget::World,
            },
        );
        assert_eq!(state.active_tab, BuilderTab::World);
        assert_eq!(state.selected_system_id, Some(id));
        assert_eq!(state.selected_world_id, Some(first_world));
    }

    // ── §CTX1 Phase 3 tests — multi-selection menu ────────────────────────

    fn multi_state(width: u32, height: u32) -> (BuilderState, SystemId, SystemId) {
        let mut state = blank(width, height);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        // Mirror the live click path: the menu is opened with the
        // MultiSelection target the resolver hands out.
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::MultiSelection {
                ids: vec![a.clone(), b.clone()],
            },
            bulk_delete_confirm: false,
        });
        (state, a, b)
    }

    #[test]
    fn ctx_multi_focus_first_focuses_first_id() {
        let (mut state, a, _b) = multi_state(8, 8);
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiFocusFirst);
        assert_eq!(state.selected_system_id, Some(a));
        assert_eq!(state.active_tab, BuilderTab::System);
    }

    #[test]
    fn ctx_multi_bulk_rename_open_arms_pending_dialog() {
        let (mut state, _a, _b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiBulkRenameOpen);
        let pending = state.pending_bulk_rename.expect("dialog armed");
        assert_eq!(pending.pattern, "Sys-{n}");
    }

    #[test]
    fn ctx_multi_pin_all_pins_every_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiPinAll);
        assert!(state.pinned_systems.contains(&a));
        assert!(state.pinned_systems.contains(&b));
    }

    #[test]
    fn ctx_multi_unpin_all_clears_every_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        state.pinned_systems.insert(a.clone());
        state.pinned_systems.insert(b.clone());
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiUnpinAll);
        assert!(!state.pinned_systems.contains(&a));
        assert!(!state.pinned_systems.contains(&b));
    }

    #[test]
    fn ctx_multi_delete_all_confirmed_removes_and_clears_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiDeleteAllConfirmed);
        assert!(state.sector.systems.iter().all(|s| s.id != a && s.id != b));
        assert!(state.selected_systems.is_empty());
        assert!(state.selected_system_id.is_none());
    }

    #[test]
    fn ctx_multi_delete_requires_confirm_gate() {
        // Simulates the inline confirm flow: an unarmed menu must not
        // dispatch DELETE on a stray first click. The render path only
        // flips `bulk_delete_confirm`; the apply path is only reached on
        // the second [Yes] click.
        let (mut state, a, b) = multi_state(8, 8);
        let confirming = state
            .sector_context_menu
            .as_ref()
            .map(|m| m.bulk_delete_confirm)
            .unwrap();
        assert!(!confirming, "fresh menu starts with confirm unarmed");
        // First click on DELETE ALL just flips the flag (mirrored here).
        state
            .sector_context_menu
            .as_mut()
            .unwrap()
            .bulk_delete_confirm = true;
        // Systems must still exist until the [Yes] branch runs.
        assert!(state.sector.systems.iter().any(|s| s.id == a));
        assert!(state.sector.systems.iter().any(|s| s.id == b));
        // Second click ([Yes]) finally dispatches.
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiDeleteAllConfirmed);
        assert!(state.sector.systems.iter().all(|s| s.id != a && s.id != b));
    }

    #[test]
    fn ctx_multi_assign_primary_faction_writes_each() {
        let (mut state, a, b) = multi_state(8, 8);
        // Inject a faction directly into the sector — the menu's submenu
        // is only enabled when at least one faction exists.
        let fid = sectorforge::ids::FactionId::from("imperium");
        state
            .sector
            .factions
            .push(sectorforge::sector_model::GeneratedFaction {
                id: fid.clone(),
                name: std::sync::Arc::from("Imperium"),
                kind: std::sync::Arc::from("Imperium"),
                disposition: std::sync::Arc::from("Order"),
                subfactions: Vec::new(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: Default::default(),
            });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::MultiAssignPrimaryFaction { fid: fid.clone() },
        );
        for id in [&a, &b] {
            let sys = state.sector.systems.iter().find(|s| s.id == **id).unwrap();
            assert!(
                sys.primary_factions.contains(&fid),
                "{id} should carry the new primary faction"
            );
        }
    }

    #[test]
    fn ctx_multi_flip_control_state_writes_each() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::MultiFlipControlState {
                value: Some(SystemState::Warzone),
            },
        );
        for id in [&a, &b] {
            let sys = state.sector.systems.iter().find(|s| s.id == **id).unwrap();
            assert_eq!(sys.control.state, Some(SystemState::Warzone));
        }
    }

    #[test]
    fn ctx_multi_clear_selection_drops_selected_systems() {
        let (mut state, _a, _b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiClearSelection);
        assert!(state.selected_systems.is_empty());
        assert!(state.selected_system_id.is_none());
    }

    #[test]
    fn ctx_multi_reseed_skips_when_all_pinned() {
        // RESEED ignores pinned systems — when every selection is pinned,
        // it becomes a no-op (matches the render path's disabled state).
        let (mut state, a, b) = multi_state(8, 8);
        state.pinned_systems.insert(a.clone());
        state.pinned_systems.insert(b.clone());
        let before_modal = state.modal.is_some();
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiReseedWorlds);
        assert_eq!(state.modal.is_some(), before_modal);
    }

    // ── §CTX1 Phase 4 tests — partial-regen anchor ────────────────────────

    #[test]
    fn ctx_partial_regen_anchor_defaults_none() {
        let state = blank(4, 4);
        assert!(state.partial_regen_anchor.is_none());
    }

    #[test]
    fn ctx_action_start_partial_regen_arms_anchor() {
        let mut state = blank(8, 8);
        let coord = HexCoord { q: 2, r: 3 };
        let closed =
            apply_sector_menu_action(&mut state, SectorMenuAction::StartPartialRegen { coord });
        assert!(closed, "menu dismisses after arming the anchor");
        assert_eq!(state.partial_regen_anchor, Some(coord));
        // Arming the anchor must not pre-fill the rect.
        assert!(state.partial_regen_rect.is_none());
    }

    #[test]
    fn ctx_partial_regen_anchor_click_completes_rect() {
        let mut state = blank(8, 8);
        state.partial_regen_anchor = Some(HexCoord { q: 1, r: 5 });
        let consumed = apply_partial_regen_anchor_click(&mut state, HexCoord { q: 4, r: 2 });
        assert!(consumed, "click consumed while anchor was armed");
        assert!(state.partial_regen_anchor.is_none(), "anchor cleared");
        let rect = state.partial_regen_rect.expect("rect populated");
        // Corners are normalised so min <= max regardless of click order.
        assert_eq!(rect.min_q, 1);
        assert_eq!(rect.max_q, 4);
        assert_eq!(rect.min_r, 2);
        assert_eq!(rect.max_r, 5);
    }

    #[test]
    fn ctx_partial_regen_anchor_click_noop_without_anchor() {
        let mut state = blank(8, 8);
        let consumed = apply_partial_regen_anchor_click(&mut state, HexCoord { q: 0, r: 0 });
        assert!(!consumed);
        assert!(state.partial_regen_rect.is_none());
    }

    #[test]
    fn ctx_partial_regen_anchor_not_in_session_file() {
        // §CTX1 Phase 4 acceptance: anchor is in-memory only. `SessionFile`
        // is the only on-disk encoding of `BuilderState`; round-tripping a
        // state with the anchor armed must drop it.
        use crate::builder::session::SessionFile;
        let mut state = blank(4, 4);
        state.partial_regen_anchor = Some(HexCoord { q: 2, r: 2 });
        let file = SessionFile::from_state(&state, Vec::new());
        let round_tripped = file.into_state();
        assert!(round_tripped.partial_regen_anchor.is_none());
    }

    // ── §CTX1 Phase 5 tests — route + region-hex menus ───────────────────

    fn add_route(state: &mut BuilderState, a: HexCoord, b: HexCoord) -> sectorforge::ids::RouteId {
        let sa = state.sector.add_system(a, "A").unwrap();
        let sb = state.sector.add_system(b, "B").unwrap();
        state
            .sector
            .add_route(&sa, &sb, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap()
    }

    #[test]
    fn resolve_returns_route_target_when_clicking_segment() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 0 });
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let a = geom.hex_center(0, 0);
        let b = geom.hex_center(4, 0);
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let target = resolve_sector_context(&state, &geom, mid, 8, 8, false)
            .expect("midpoint resolves to route");
        assert!(matches!(
            target,
            SectorMenuTarget::Route { id: hit, .. } if hit == id
        ));
    }

    #[test]
    fn resolve_returns_region_hex_when_cache_has_region() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-z", HexCoord { q: 3, r: 4 });
        refresh_map_cache(&mut state);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(3, 4);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false).unwrap();
        assert!(matches!(
            target,
            SectorMenuTarget::RegionHex { ref region, coord }
                if region == "reg-z" && coord == HexCoord { q: 3, r: 4 }
        ));
    }

    #[test]
    fn ctx_action_set_route_type_runs_command_and_undoes() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteType {
                id: id.clone(),
                value: RouteType::SmugglingLane,
            },
        );
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .route_type,
            RouteType::SmugglingLane
        );
        state.undo().unwrap();
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .route_type,
            RouteType::StableWarpLane
        );
    }

    #[test]
    fn ctx_action_set_route_stability_runs_command() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteStability {
                id: id.clone(),
                value: RouteStability::Perilous,
            },
        );
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .stability,
            RouteStability::Perilous
        );
    }

    #[test]
    fn ctx_action_remove_route_drops_route() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(&mut state, SectorMenuAction::RemoveRoute { id: id.clone() });
        assert!(state.sector.routes.iter().all(|r| r.id != id));
    }

    #[test]
    fn ctx_action_set_route_type_noop_when_unchanged() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        let log_before = state.command_log.len();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteType {
                id,
                value: RouteType::StableWarpLane,
            },
        );
        assert_eq!(
            state.command_log.len(),
            log_before,
            "same-value cycle should not push a command"
        );
    }

    #[test]
    fn ctx_action_focus_route_switches_to_routes_tab() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusRoute { id: id.clone() });
        assert_eq!(state.selected_route_id, Some(id));
        assert_eq!(state.active_tab, BuilderTab::Routes);
    }

    #[test]
    fn ctx_action_set_region_kind_runs_command() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRegionKind {
                region: "reg-x".into(),
                value: RegionConditionKind::CalmCorridor,
            },
        );
        assert_eq!(
            state
                .sector
                .regions
                .iter()
                .find(|r| r.id == "reg-x")
                .unwrap()
                .kind,
            RegionConditionKind::CalmCorridor
        );
    }

    #[test]
    fn ctx_action_focus_region_switches_tab() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::FocusRegion {
                region: "reg-x".into(),
            },
        );
        assert_eq!(state.selected_region_id.as_deref(), Some("reg-x"));
        assert_eq!(state.active_tab, BuilderTab::Regions);
    }

    #[test]
    fn ctx_action_erase_region_hex_drops_hex() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::EraseRegionHex {
                region: "reg-x".into(),
                coord: HexCoord { q: 2, r: 2 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-x")
            .unwrap();
        assert!(!region.hexes.contains(&HexCoord { q: 2, r: 2 }));
    }

    #[test]
    fn ctx_action_rename_region_open_arms_dialog() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 0, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RenameRegionOpen {
                region: "reg-x".into(),
            },
        );
        let pending = state.pending_region_rename.expect("dialog armed");
        assert_eq!(pending.region, "reg-x");
        assert_eq!(pending.text, "Region reg-x");
    }

    #[test]
    fn map_cache_stable_across_idempotent_calls() {
        let mut state = blank(8, 8);
        state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        refresh_map_cache(&mut state);
        let digest = state
            .map_view_cache
            .as_ref()
            .map(|c| c.digest.clone())
            .unwrap();
        refresh_map_cache(&mut state);
        assert_eq!(
            digest,
            state.map_view_cache.as_ref().unwrap().digest,
            "digest unchanged when sector slice unchanged"
        );
    }

    // ── §CTX1 Phase 7 polish tests ────────────────────────────────────────

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(1000.0, 800.0))
    }

    #[test]
    fn menu_anchor_pivot_top_left_when_cursor_top_left() {
        // Cursor on the left/top half → menu grows down-right from cursor.
        assert_eq!(
            menu_anchor_pivot(Pos2::new(100.0, 100.0), screen()),
            egui::Align2::LEFT_TOP,
        );
    }

    #[test]
    fn menu_anchor_pivot_flips_horizontally_on_right_half() {
        // Cursor near the right edge → pivot at the menu's right edge, so the
        // menu unfurls to the *left* of the cursor.
        assert_eq!(
            menu_anchor_pivot(Pos2::new(950.0, 100.0), screen()),
            egui::Align2::RIGHT_TOP,
        );
    }

    #[test]
    fn menu_anchor_pivot_flips_vertically_on_bottom_half() {
        // Cursor near the bottom edge → pivot at the menu's bottom edge, so
        // the menu unfurls *upwards*.
        assert_eq!(
            menu_anchor_pivot(Pos2::new(100.0, 780.0), screen()),
            egui::Align2::LEFT_BOTTOM,
        );
    }

    #[test]
    fn menu_anchor_pivot_clamps_to_viewport_in_corner() {
        // Bottom-right corner → both axes flip.
        assert_eq!(
            menu_anchor_pivot(Pos2::new(990.0, 790.0), screen()),
            egui::Align2::RIGHT_BOTTOM,
        );
    }

    #[test]
    fn ctx_menu_telemetry_records_last_action_label() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        assert!(state.last_menu_action.is_none());
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id });
        assert_eq!(
            state.last_menu_action.as_deref(),
            Some("sector :: FOCUS SYSTEM"),
        );
    }

    #[test]
    fn ctx_menu_telemetry_label_covers_every_sector_variant() {
        // Spot-check one variant per schema group. The labels themselves are
        // exhaustively matched in `sector_menu_action_label` so the compiler
        // will block any future variant from being missed.
        for (action, label) in [
            (
                SectorMenuAction::PlaceSystem {
                    coord: HexCoord { q: 0, r: 0 },
                },
                "sector :: PLACE SYSTEM",
            ),
            (
                SectorMenuAction::MultiClearSelection,
                "multi :: CLEAR SELECTION",
            ),
            (
                SectorMenuAction::FocusRoute {
                    id: sectorforge::ids::route_id(
                        &sectorforge::ids::system_id(1),
                        &sectorforge::ids::system_id(2),
                    ),
                },
                "route :: FOCUS",
            ),
            (
                SectorMenuAction::FocusRegion {
                    region: "reg-a".into(),
                },
                "region :: FOCUS",
            ),
        ] {
            assert_eq!(sector_menu_action_label(&action), label);
        }
    }

    #[test]
    fn ctx_menu_telemetry_resets_through_session_round_trip() {
        // The telemetry tail is in-memory only — a session save/load must
        // drop it so the status bar starts clean after reopen.
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id });
        assert!(state.last_menu_action.is_some());
        let file = crate::builder::session::SessionFile::from_state(&state, Vec::new());
        let restored = file.into_state();
        assert!(restored.last_menu_action.is_none());
    }

    // ── §CTX1 §10 edge-case tests ─────────────────────────────────────────

    #[test]
    fn ctx_resolve_returns_none_outside_sector_bounds() {
        // §10 row 1: right-clicking outside the hex grid yields no menu —
        // `pick_hex` returns None and `resolve_sector_context` falls through.
        let state = blank(4, 4);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        // Position well outside any hex centre (negative quadrant + far enough
        // that no hex is within the 0.95 * hex_size inscribed radius).
        let outside = Pos2::new(-10_000.0, -10_000.0);
        assert!(resolve_sector_context(&state, &geom, outside, 4, 4, false).is_none());
    }

    #[test]
    fn ctx_resolve_suppressed_when_pending_collision() {
        // §10 row 8: while the §S6 collision dialog is armed, the menu must
        // not open (a second modal layered on top would steal focus).
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.pending_collision = Some(PendingCollision {
            dragging: a,
            target: HexCoord { q: 2, r: 2 },
            occupant: b,
        });
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none());
    }

    #[test]
    fn sector_menu_target_stale_when_system_removed() {
        // §10 row 11: an undo that removes the targeted system drops the menu
        // next render. Pure helper drives the dismiss path.
        let mut state = blank(4, 4);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        let target = SectorMenuTarget::System {
            id: id.clone(),
            coord: HexCoord { q: 1, r: 1 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.systems.retain(|s| s.id != id);
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_stale_when_route_removed() {
        let mut state = blank(8, 8);
        let route_id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 0 });
        let target = SectorMenuTarget::Route {
            id: route_id,
            near_coord: HexCoord { q: 0, r: 0 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.routes.clear();
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_stale_when_region_removed() {
        let mut state = blank(4, 4);
        add_region(&mut state, "reg-x", HexCoord { q: 0, r: 0 });
        let target = SectorMenuTarget::RegionHex {
            region: "reg-x".into(),
            coord: HexCoord { q: 0, r: 0 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.regions = std::sync::Arc::new(vec![]);
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_not_stale_for_empty_hex_or_subsector_border() {
        // EmptyHex / SubsectorBorder don't reference an entity id, so they're
        // never stale — they survive an undo that, say, removes nearby systems.
        let state = blank(4, 4);
        assert!(!sector_menu_target_is_stale(
            &state,
            &SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 1, r: 1 }
            }
        ));
        assert!(!sector_menu_target_is_stale(
            &state,
            &SectorMenuTarget::SubsectorBorder {
                subsector: "sub-A".into(),
                coord: HexCoord { q: 1, r: 1 },
            }
        ));
    }

    #[test]
    fn sector_menu_target_stale_when_every_multi_id_removed() {
        let mut state = blank(4, 4);
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "B")
            .unwrap();
        let target = SectorMenuTarget::MultiSelection {
            ids: vec![a.clone(), b.clone()],
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        // Removing only one of the two leaves the multi-selection non-stale —
        // the menu still has *something* to act on.
        state.sector.systems.retain(|s| s.id != a);
        assert!(!sector_menu_target_is_stale(&state, &target));
        // Remove both — now every referenced id is gone.
        state.sector.systems.clear();
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn set_active_tab_drops_sector_context_menu() {
        // §10 row 9: switching tabs while a menu is open must drop it so the
        // ghost menu doesn't reappear when the user returns to MAP.
        let mut state = blank(4, 4);
        state.active_tab = BuilderTab::Map;
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        state.set_active_tab(BuilderTab::Routes);
        assert!(state.sector_context_menu.is_none());
        assert_eq!(state.active_tab, BuilderTab::Routes);
    }

    #[test]
    fn set_active_tab_keeps_menu_when_tab_unchanged() {
        // Setting the same tab is idempotent — no spurious menu dismissal.
        let mut state = blank(4, 4);
        state.active_tab = BuilderTab::Map;
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        state.set_active_tab(BuilderTab::Map);
        assert!(state.sector_context_menu.is_some());
    }

    #[test]
    fn ctx_action_cancel_route_clears_pending_start_and_disarms_tool() {
        // §10 row 12: with ADD-ROUTE half-armed, the `CANCEL ROUTE` action
        // resets `pending_route_start` and drops the `MapTool::AddRoute` arm.
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        state.map_tool = MapTool::AddRoute;
        state.pending_route_start = Some(a);
        let closed = apply_sector_menu_action(&mut state, SectorMenuAction::CancelRoute);
        assert!(closed);
        assert!(state.pending_route_start.is_none());
        assert_eq!(state.map_tool, MapTool::Select);
        assert_eq!(state.last_menu_action.as_deref(), Some("route :: CANCEL"));
    }

    #[test]
    fn ctx_menu_dropped_through_session_round_trip() {
        // §10 row 10: project close/reload must dismiss an open menu — the
        // session file is the funnel and `SectorContextMenu` is in-memory only.
        let mut state = blank(4, 4);
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        let file = crate::builder::session::SessionFile::from_state(&state, Vec::new());
        let restored = file.into_state();
        assert!(restored.sector_context_menu.is_none());
    }
}
