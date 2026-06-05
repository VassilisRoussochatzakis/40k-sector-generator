//! Editable hex-grid map. Delegates rendering to
//! [`sectorforge_gui_core::sector_view::SectorView`] so the editor surface
//! stays visually identical to the viewer + builder map. Editor-only
//! interactions (tool dispatch, drag-to-move, route picking, delete) are
//! layered on top via [`SectorGeom`] hit-testing.

use egui::{Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::sector_view::{SectorGeom, SectorMapCache, SectorView};

use crate::palette::{self, HEX_OUTLINE};

use super::state::{Dialog, EditorState, RouteEndpoint, SectorEditTool, Selection};

pub(crate) fn show_map(ui: &mut Ui, state: &mut EditorState) {
    let route_pick = state.route_pick;
    let (sector_w, sector_h) = if let Some(s) = &state.sector {
        (s.width, s.height)
    } else {
        ui.label(
            RichText::new("no sector loaded — use NEW SECTOR or OPEN")
                .color(palette::chrome_text_dim()),
        );
        return;
    };

    let hex_size = state.hex_size;
    let canvas_size = SectorGeom::new(hex_size, Pos2::ZERO).map_size_px(sector_w, sector_h);
    let (rect, response) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let origin = rect.min;
    let pointer = response.interact_pointer_pos();

    let selected_id: Option<&str> = match &state.selection {
        Selection::System(id) => Some(id.as_str()),
        Selection::World { system_id, .. } => Some(system_id.as_str()),
        Selection::None => None,
    };

    let drag_override = state.drag_id.clone().zip(pointer);

    let pending_route_preview = if state.tool == SectorEditTool::AddRoute {
        state.pending_route_start.clone().zip(pointer)
    } else {
        None
    };

    // F4: build (or reuse) the per-sector render cache so the SectorView render
    // skips the O(regions·hexes) hex→region fallback scan every frame. Invalidated
    // to `None` on any sector change via `EditorState::{set_sector,mark_dirty}`.
    // Editor passes `subsectors: None`, so the cache is built with no subsectors.
    if state.map_cache.is_none() {
        let cache = state.sector.as_ref().map(|s| SectorMapCache::new(s, &[]));
        state.map_cache = cache;
    }

    {
        let sector = state
            .sector
            .as_ref()
            .expect("sector presence checked above");
        let cache = state.map_cache.as_ref();
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            SectorView {
                sector,
                selected_system: selected_id,
                selected_route: None,
                hex_size,
                path_route_ids: None,
                path_waypoints: None,
                subsectors: None,
                cache,
                selected_subsector: None,
                heatmap: None,
                empty_hex_clicks: false,
                route_view_mode: state.route_view_mode,
                origin,
                multi_selected: None,
                pinned: None,
                drag_override,
                pending_route_preview,
                rect_select: None,
                sense: Sense::hover(),
                disable_internal_click_dispatch: true,
                theme: None,
                show_hover_coord: true,
            }
            .show(ui);
        });
    }

    // ── interaction dispatcher ──────────────────────────────────────────────
    let pick_geom = SectorGeom::new(hex_size, origin);

    let mut drag_drop_finalize: Option<(SystemId, HexCoord)> = None;
    let mut click_action: Option<ClickAction> = None;
    let mut delete_id: Option<SystemId> = None;
    let mut dirty = false;

    {
        let sector = state
            .sector
            .as_ref()
            .expect("sector presence checked above");

        if response.drag_started() && state.tool == SectorEditTool::Select {
            if let Some(pos) = pointer {
                if let Some(id) = pick_geom.hit_system(sector, pos) {
                    state.drag_id = Some(id);
                }
            }
        }

        if response.drag_stopped() {
            if let (Some(drag_id), Some(pos)) = (state.drag_id.clone(), pointer) {
                if let Some(coord) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                    drag_drop_finalize = Some((drag_id, coord));
                }
            }
            state.drag_id = None;
        }

        if response.clicked() {
            if let Some(pos) = pointer {
                if let Some(sys_id) = pick_geom.hit_system(sector, pos) {
                    match state.tool {
                        SectorEditTool::Delete => {
                            delete_id = Some(sys_id);
                        }
                        SectorEditTool::AddRoute => {
                            if let Some(from) = state.pending_route_start.take() {
                                if from != sys_id {
                                    click_action = Some(ClickAction::AddRoute(from, sys_id));
                                }
                            } else {
                                state.pending_route_start = Some(sys_id);
                            }
                        }
                        _ => {
                            if let Some((idx, ep)) = route_pick {
                                click_action = Some(ClickAction::RoutePick(idx, ep, sys_id));
                            } else {
                                click_action = Some(ClickAction::SelectSystem(sys_id));
                            }
                        }
                    }
                } else if route_pick.is_none() {
                    if let Some(coord) = pick_geom.pick_hex(pos, sector_w, sector_h) {
                        let occupied = sector.systems.iter().any(|s| s.coord == coord);
                        if !occupied
                            && (state.tool == SectorEditTool::AddSystem
                                || state.tool == SectorEditTool::Select)
                        {
                            click_action = Some(ClickAction::AddSystem(coord));
                        }
                    }
                }
            }
        }
    }

    // Apply side-effects
    if let Some((drag_id, coord)) = drag_drop_finalize {
        if let Some(sector) = state.sector.as_mut() {
            let occupied = sector
                .systems
                .iter()
                .any(|s| s.coord == coord && s.id != drag_id);
            if !occupied {
                if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == drag_id) {
                    sys.coord = coord;
                    dirty = true;
                    for r in &mut sector.routes {
                        if r.from_system_id == drag_id || r.to_system_id == drag_id {
                            let from_coord = sector
                                .systems
                                .iter()
                                .find(|s| s.id == r.from_system_id)
                                .map(|s| s.coord)
                                .unwrap_or(coord);
                            let to_coord = sector
                                .systems
                                .iter()
                                .find(|s| s.id == r.to_system_id)
                                .map(|s| s.coord)
                                .unwrap_or(coord);
                            r.distance =
                                sectorforge::sector_model::hex_distance(from_coord, to_coord);
                        }
                    }
                }
            }
        }
    }

    if let Some(id) = delete_id {
        if let Some(sector) = state.sector.as_mut() {
            sector.systems.retain(|s| s.id != id);
            sector
                .routes
                .retain(|r| r.from_system_id != id && r.to_system_id != id);
            dirty = true;
            if matches!(&state.selection, Selection::System(sid) if *sid == id) {
                state.selection = Selection::None;
            }
        }
    }

    if let Some(action) = click_action {
        match action {
            ClickAction::SelectSystem(id) => {
                state.selection = Selection::System(id);
            }
            ClickAction::AddSystem(coord) => {
                state.dialog = Dialog::PlaceSystem {
                    coord,
                    name: String::new(),
                    kind: sectorforge::sector_model::SystemKind::Star,
                    has_star: true,
                };
            }
            ClickAction::AddRoute(from, to) => {
                if let Some(sector) = state.sector.as_mut() {
                    let route_id = sectorforge::ids::route_id(&from, &to);
                    if !sector.routes.iter().any(|r| r.id == route_id) {
                        let mut route =
                            sectorforge::sector_model::empty_route(from.clone(), to.clone());
                        let from_coord = sector
                            .systems
                            .iter()
                            .find(|s| s.id == from)
                            .map(|s| s.coord);
                        let to_coord = sector.systems.iter().find(|s| s.id == to).map(|s| s.coord);
                        if let (Some(a), Some(b)) = (from_coord, to_coord) {
                            route.distance = sectorforge::sector_model::hex_distance(a, b);
                        }
                        sector.routes.push(route);
                        dirty = true;
                    }
                }
            }
            ClickAction::RoutePick(idx, ep, sys_id) => {
                if let Some(sector) = state.sector.as_mut() {
                    let coords: std::collections::HashMap<SystemId, HexCoord> = sector
                        .systems
                        .iter()
                        .map(|s| (s.id.clone(), s.coord))
                        .collect();
                    if let Some(route) = sector.routes.get_mut(idx) {
                        match ep {
                            RouteEndpoint::From => route.from_system_id = sys_id,
                            RouteEndpoint::To => route.to_system_id = sys_id,
                        }
                        if let (Some(&a), Some(&b)) = (
                            coords.get(&route.from_system_id),
                            coords.get(&route.to_system_id),
                        ) {
                            route.distance = sectorforge::sector_model::hex_distance(a, b);
                        }
                        route.id =
                            sectorforge::ids::route_id(&route.from_system_id, &route.to_system_id);
                    }
                }
                state.route_pick = None;
                dirty = true;
            }
        }
    }

    if dirty {
        state.mark_dirty();
    }

    draw_toolbox(ui, state);
}

enum ClickAction {
    SelectSystem(SystemId),
    AddSystem(HexCoord),
    AddRoute(SystemId, SystemId),
    RoutePick(usize, RouteEndpoint, SystemId),
}

fn draw_toolbox(ui: &mut Ui, state: &mut EditorState) {
    let rect = ui.max_rect();
    let toolbox_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + 10.0, rect.min.y + 10.0),
        Vec2::new(140.0, 130.0),
    );
    palette::paint_rect_filled(ui, toolbox_rect, toolbox_rect, 4.0, palette::chrome_panel());
    palette::paint_rect_stroke(
        ui,
        toolbox_rect,
        toolbox_rect,
        4.0,
        Stroke::new(1.0, HEX_OUTLINE),
    );

    ui.put(toolbox_rect, |ui: &mut Ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("TOOLBOX").color(palette::chrome_text_dim()));
            ui.add_space(5.0);

            for (tool, label) in [
                (SectorEditTool::Select, "SELECT / DRAG"),
                (SectorEditTool::AddSystem, "ADD SYSTEM"),
                (SectorEditTool::AddRoute, "ADD ROUTE"),
                (SectorEditTool::Delete, "DELETE"),
            ] {
                if ui
                    .selectable_label(state.tool == tool, RichText::new(label))
                    .clicked()
                {
                    state.tool = tool;
                    state.pending_route_start = None;
                }
            }
        });
        ui.response()
    });
}
