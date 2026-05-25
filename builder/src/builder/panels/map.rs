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

use sectorforge::ids::{self, SystemId};
use sectorforge::sector_model::HexCoord;
use sectorforge::subsectors::{build_subsectors, SubsectorConfig};

use sectorforge_gui_core::sector_view::{
    paint_system_rings, SectorGeom, SectorMapCache, SectorView,
};

use crate::builder::command::BuilderCommand;
use crate::builder::derivation_cache::digest_input;
use crate::builder::state::{MapTool, MapViewCache, PendingCollision, PendingPlace, PendingRename};
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
    ui.separator();

    egui::ScrollArea::both().show(ui, |ui| {
        show_hex_map(ui, state);
    });

    // Transient dialogs — kept inside the panel so the host shell does not need
    // to learn new ModalKind variants for §S1 / §S6.
    show_place_dialog(ui.ctx(), state);
    show_rename_dialog(ui.ctx(), state);
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

    // ── interaction ─────────────────────────────────────────────────────────
    let pick_geom = SectorGeom::new(state.hex_size, origin);

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
    if state.map_tool == MapTool::RegionPaint {
        if response.secondary_clicked() {
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
    state.selected_route_id = Some(selected_route);
    state.active_tab = crate::builder::state::BuilderTab::Routes;
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
