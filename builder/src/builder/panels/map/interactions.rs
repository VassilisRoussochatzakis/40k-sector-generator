//! Hex render dispatcher + tool routing (§S1 / §S6 / §CTX1 Phase 4).
//!
//! Owns the per-frame `show_hex_map` widget, the click/drag/rect-select
//! handlers, the partial-regen anchor consumer, the region-paint brush, and
//! the route-add command boundary. All transient state lives on
//! [`BuilderState`] so undo/redo and snapshotting work as expected.

use egui::{Pos2, RichText, Sense, Ui};

use sectorforge::ids;
use sectorforge::ids::SystemId;
use sectorforge::sector_model::HexCoord;

use sectorforge_gui_core::palette::TEXT_DIM;
use sectorforge_gui_core::sector_view::{
    paint_system_rings, SectorGeom, SectorMapCache, SectorView,
};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{
    MapTool, PartialRegenRect, PendingCollision, PendingPlace, PendingRename, SectorContextMenu,
    SectorMenuTarget,
};
use crate::builder::{BuilderState, ModalKind};

use super::cache::refresh_map_cache;
use super::context_menu::resolve_sector_context;

pub(super) fn show_hex_map(ui: &mut Ui, state: &mut BuilderState) {
    let (sector_w, sector_h) = (state.sector.width, state.sector.height);
    if sector_w == 0 || sector_h == 0 {
        ui.label(
            RichText::new("Sector has zero extent — open or create a project first.")
                .color(TEXT_DIM),
        );
        return;
    }

    refresh_map_cache(state);

    let geom = SectorGeom::new(state.hex_size, Pos2::ZERO);
    let canvas_size = geom.map_size_px(sector_w, sector_h);
    let (rect, response) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let origin = rect.min;
    let pointer = response.interact_pointer_pos();

    let drag_override = state.drag_system.clone().zip(pointer);

    let pending_route_preview = if state.map_tool == MapTool::AddRoute {
        state.pending_route_start.clone().zip(pointer)
    } else {
        None
    };

    let rect_select = state.rect_select;

    let (subsectors_slice, lookup) = match state.map_view_cache.as_ref() {
        Some(cache) => (
            Some(cache.subsectors.as_slice()),
            Some(&cache.lookup as &SectorMapCache),
        ),
        None => (None, None),
    };

    let overlay_cells = crate::builder::panels::control::build_overlay_cells(
        &state.sector,
        &state.sector.factions,
        state.control_overlay,
        lookup,
    );
    // §35 T3: scalar heatmap layer. A CONTROL overlay (§C7/§C8) always wins;
    // otherwise a per-dimension stability overlay takes precedence over the
    // economy/HeatmapMode picker, which itself is skipped when `Off`.
    let scalar_cells = if overlay_cells.is_some() {
        None
    } else if let Some(dim) = state.map_stability_dim {
        let cells = sectorforge_gui_core::heatmap::compute_stability(&state.sector, dim);
        (!cells.is_empty()).then_some(cells)
    } else if !matches!(
        state.map_heatmap_mode,
        sectorforge::heatmap::HeatmapMode::Off
    ) {
        let cells = sectorforge_gui_core::heatmap::compute(&state.sector, state.map_heatmap_mode);
        (!cells.is_empty()).then_some(cells)
    } else {
        None
    };
    let heatmap_ref = overlay_cells.as_ref().or(scalar_cells.as_ref());

    let lifeline_routes = crate::builder::panels::economy::lifeline_route_ids(state);
    let lifeline_ref = if lifeline_routes.is_empty() {
        None
    } else {
        Some(&lifeline_routes)
    };

    // §35 T1/T4: retint the live view from the project's map theme. A bad
    // custom colour falls back to the default render theme rather than panicking.
    let render_theme =
        sectorforge::map_theme::resolve_map_theme(&state.config.outputs.bitmap.theme)
            .map(|mt| sectorforge_gui_core::map_theme::RenderMapTheme::from_map_theme(&mt))
            .unwrap_or_default();

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
            theme: Some(&render_theme),
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
    // the floating menu is open.
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

    // §CTX1 — Phase 1: secondary-click → resolve target and open the floating menu.
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

pub(super) fn handle_click(
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
/// Returns `true` when a click was consumed by the anchor → rect path.
pub(super) fn apply_partial_regen_anchor_click(
    state: &mut BuilderState,
    click_coord: HexCoord,
) -> bool {
    let Some(anchor) = state.partial_regen_anchor else {
        return false;
    };
    state.partial_regen_rect = Some(PartialRegenRect::from_corners(anchor, click_coord));
    state.partial_regen_anchor = None;
    true
}

/// §REG2 left/right click brush.
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

pub(super) fn add_route_between(state: &mut BuilderState, from: SystemId, to: SystemId) {
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

pub(super) fn handle_drag_drop(state: &mut BuilderState, drag_id: SystemId, coord: HexCoord) {
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

pub(super) fn apply_rect_select(
    state: &mut BuilderState,
    a: HexCoord,
    b: HexCoord,
    additive: bool,
) {
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
