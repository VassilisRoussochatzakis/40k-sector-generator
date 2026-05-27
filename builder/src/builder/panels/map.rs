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

use sectorforge::ids::{self, FactionId, SystemId};
use sectorforge::sector_model::{HexCoord, SystemState};
use sectorforge::subsectors::{build_subsectors, SubsectorConfig};

use sectorforge_gui_core::sector_view::{
    paint_system_rings, SectorGeom, SectorMapCache, SectorView,
};

use crate::builder::command::BuilderCommand;
use crate::builder::derivation_cache::digest_input;
use crate::builder::state::{
    BuilderTab, EntityRef, MapTool, MapViewCache, PendingBulkRename, PendingCollision,
    PendingPlace, PendingRename, SectorContextMenu, SectorMenuTarget,
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
            handle_click(state, hit, coord, modifiers.shift);
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

    if let Some(coord) = geom.pick_hex(pos, sector_w, sector_h) {
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

/// §CTX1 — apply one menu item. Returns the menu's close intent (always
/// `true`: every action dismisses the menu).
pub(super) fn apply_sector_menu_action(state: &mut BuilderState, action: SectorMenuAction) -> bool {
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

/// §CTX1 — render the floating right-click menu. Anchored at
/// `menu.screen_pos`. Phase 2 wires the §6.1 (empty hex) and §6.2 (single
/// system) schemas; multi-selection, route, region-hex, and subsector-border
/// targets keep the Phase 1 placeholder until Phases 3/5 land. Dismissed on
/// Escape / focus-loss / primary click outside the area / item activation.
fn show_sector_context_menu(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(menu) = state.sector_context_menu.as_ref() else {
        return;
    };
    let screen_pos = menu.screen_pos;
    let target = menu.target.clone();
    let mut close = false;
    let area_resp = egui::Area::new(egui::Id::new("sector_context_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_pos)
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
                    SectorMenuTarget::Route { .. }
                    | SectorMenuTarget::RegionHex { .. }
                    | SectorMenuTarget::SubsectorBorder { .. } => {
                        // Phase 5 (route + region) / future (subsector
                        // border) replace this placeholder.
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
}
