//! MAP tab (§N3) — hex render + editor toolbox + transient dialogs.
//!
//! Phase B §S1 lives here: ADD SYSTEM / DELETE SYSTEM / MOVE SYSTEM (drag) /
//! RENAME (double-click). Multi-select (shift-click + rect-drag) feeds §S4
//! over in the SYSTEM panel. Coord validity + the collision swap dialog land
//! the §S6 surface.
//!
//! The hex geometry helpers are intentionally inlined rather than shared with
//! the legacy `gui::editor::map_panel` — that surface still talks to
//! `EditorState`, not `BuilderState`.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use sectorforge::ids::{self, SystemId};
use sectorforge::sector_model::HexCoord;

use crate::builder::command::BuilderCommand;
use crate::builder::state::{MapTool, PendingCollision, PendingPlace, PendingRename};
use crate::builder::{BuilderState, ModalKind};
use sectorforge_gui_core::palette::{
    self, darken, star_color, tint, HEX_EMPTY, HEX_OUTLINE, SELECTION, TEXT, TEXT_DIM,
};

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

// ── Hex render + click dispatcher ───────────────────────────────────────────

#[derive(Clone, Copy)]
struct Geom {
    hex_size: f32,
    margin: f32,
}

impl Geom {
    fn new(hex_size: f32) -> Self {
        Self {
            hex_size,
            margin: hex_size * 1.1,
        }
    }
}

fn map_size(width: u32, height: u32, g: &Geom) -> Vec2 {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let odd_shift = if height > 1 { 0.5 } else { 0.0 };
    let w = g.margin * 2.0 + horiz_step * (width as f32 + odd_shift);
    let label_band = g.hex_size * 0.55;
    let h = g.margin * 2.0
        + height.saturating_sub(1) as f32 * vert_step
        + 2.0 * g.hex_size
        + label_band;
    Vec2::new(w, h)
}

fn hex_center(q: i32, r: i32, g: &Geom) -> Pos2 {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = g.margin + horiz_step * (q as f32 + row_shift) + horiz_step / 2.0;
    let y = g.margin + vert_step * r as f32 + g.hex_size;
    Pos2::new(x, y)
}

fn hex_pick(local_pos: Pos2, g: &Geom, width: u32, height: u32) -> Option<HexCoord> {
    let mut best: Option<(HexCoord, f32)> = None;
    for r in 0..height as i32 {
        for q in 0..width as i32 {
            let c = hex_center(q, r, g);
            let d = (c - local_pos).length();
            if d <= g.hex_size * 0.95 {
                let entry = (HexCoord { q, r }, d);
                if best.as_ref().is_none_or(|prev| d < prev.1) {
                    best = Some(entry);
                }
            }
        }
    }
    best.map(|(c, _)| c)
}

fn hex_vertices(c: Pos2, size: f32) -> [Pos2; 6] {
    let mut out = [Pos2::ZERO; 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 30.0);
        *slot = Pos2::new(c.x + size * angle.cos(), c.y + size * angle.sin());
    }
    out
}

fn draw_hex(painter: &egui::Painter, c: Pos2, size: f32, fill: Color32, outline: Color32) {
    let pts = hex_vertices(c, size).to_vec();
    painter.add(egui::Shape::convex_polygon(
        pts,
        fill,
        Stroke::new(1.0, outline),
    ));
}

fn draw_hex_outline_only(
    painter: &egui::Painter,
    c: Pos2,
    size: f32,
    color: Color32,
    thickness: f32,
) {
    let pts = hex_vertices(c, size);
    for i in 0..6 {
        painter.line_segment([pts[i], pts[(i + 1) % 6]], Stroke::new(thickness, color));
    }
}

fn show_hex_map(ui: &mut Ui, state: &mut BuilderState) {
    let g = Geom::new(state.hex_size);
    let (sector_w, sector_h) = (state.sector.width, state.sector.height);
    if sector_w == 0 || sector_h == 0 {
        ui.label(
            RichText::new("Sector has zero extent — open or create a project first.")
                .color(TEXT_DIM),
        );
        return;
    }

    let size = map_size(sector_w, sector_h, &g);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let origin = rect.min;
    let pointer = response.interact_pointer_pos();
    painter.rect_filled(rect, 0.0, palette::BG);

    // Empty hexes.
    for r in 0..sector_h as i32 {
        for q in 0..sector_w as i32 {
            let c = hex_center(q, r, &g) + origin.to_vec2();
            draw_hex(&painter, c, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
        }
    }

    // §S4 rect-select highlight.
    if let Some((a, b)) = state.rect_select {
        for r in a.r.min(b.r)..=a.r.max(b.r) {
            for q in a.q.min(b.q)..=a.q.max(b.q) {
                if r < 0 || q < 0 || (r as u32) >= sector_h || (q as u32) >= sector_w {
                    continue;
                }
                let c = hex_center(q, r, &g) + origin.to_vec2();
                draw_hex(
                    &painter,
                    c,
                    g.hex_size,
                    Color32::from_rgba_unmultiplied(255, 240, 120, 30),
                    HEX_OUTLINE,
                );
            }
        }
    }

    // Pre-compute system centres (drag override).
    let mut centers: std::collections::HashMap<String, Pos2> = Default::default();
    for sys in &state.sector.systems {
        let mut c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
        if Some(&sys.id) == state.drag_system.as_ref() {
            if let Some(pos) = response.interact_pointer_pos() {
                c = pos;
            }
        }
        centers.insert(sys.id.to_string(), c);
    }

    // Routes (under disks).
    let route_thickness = (g.hex_size * 0.08).max(2.0);
    for route in &state.sector.routes {
        if let (Some(&a), Some(&b)) = (
            centers.get(route.from_system_id.as_str()),
            centers.get(route.to_system_id.as_str()),
        ) {
            palette::draw_route_line(
                &painter,
                a,
                b,
                route_thickness,
                palette::stability_color(route.stability),
                route
                    .route_type
                    .pattern(sectorforge::sector_model::RouteViewMode::Detailed),
            );
        }
    }
    if state.map_tool == MapTool::AddRoute {
        if let (Some(start), Some(pos)) = (&state.pending_route_start, pointer) {
            if let Some(&a) = centers.get(start.as_str()) {
                palette::draw_route_line(
                    &painter,
                    a,
                    pos,
                    route_thickness * 1.4,
                    Color32::from_rgb(255, 220, 120),
                    sectorforge::sector_model::RoutePattern::Dashed,
                );
            }
        }
    }

    // System disks + selection + pinned ring.
    for sys in &state.sector.systems {
        let c = centers[sys.id.as_str()];
        let is_focus = state.selected_system_id.as_ref() == Some(&sys.id);
        let is_multi = state.selected_systems.contains(&sys.id);
        if is_focus || is_multi {
            let colour = if is_focus {
                SELECTION
            } else {
                Color32::from_rgb(180, 200, 255)
            };
            draw_hex_outline_only(&painter, c, g.hex_size + 2.0, colour, 2.5);
        }
        if state.pinned_systems.contains(&sys.id) {
            draw_hex_outline_only(
                &painter,
                c,
                g.hex_size + 5.0,
                Color32::from_rgb(255, 120, 90),
                1.5,
            );
        }

        if let Some(star) = &sys.star {
            let fill = star_color(&star.colour_code);
            draw_hex(&painter, c, g.hex_size, tint(fill, 0.18), HEX_OUTLINE);
            let r = g.hex_size * 0.42;
            painter.circle_filled(c, r, fill);
            painter.circle_stroke(c, r, Stroke::new(1.5, darken(fill, 0.55)));
        } else {
            let fill = Color32::from_rgb(100, 100, 110);
            draw_hex(&painter, c, g.hex_size, tint(fill, 0.18), HEX_OUTLINE);
        }

        let pip = sys.worlds.len();
        if pip > 0 {
            painter.text(
                Pos2::new(c.x + g.hex_size * 0.55, c.y + g.hex_size * 0.55),
                Align2::RIGHT_BOTTOM,
                pip.to_string(),
                FontId::monospace((g.hex_size * 0.34).max(10.0)),
                TEXT,
            );
        }
    }

    // Labels.
    let label_size = (g.hex_size * 0.28).max(9.0);
    for sys in &state.sector.systems {
        let c = centers[sys.id.as_str()];
        let label = sys.name.to_ascii_uppercase();
        let font = FontId::monospace(label_size);
        let galley = painter.layout_no_wrap(label, font, TEXT_DIM);
        let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + g.hex_size + 3.0);
        let pad = Vec2::new(3.0, 1.0);
        let bg_rect = Rect::from_min_size(pos - pad, galley.size() + pad * 2.0);
        painter.rect_filled(bg_rect, 2.0, palette::BG);
        painter.galley(pos, galley, TEXT_DIM);
    }

    // ── interaction ─────────────────────────────────────────────────────────
    let hit_system = |state: &BuilderState, pos: Pos2| -> Option<SystemId> {
        state
            .sector
            .systems
            .iter()
            .find(|s| {
                let c = hex_center(s.coord.q, s.coord.r, &g) + origin.to_vec2();
                (c - pos).length() <= g.hex_size * 0.95
            })
            .map(|s| s.id.clone())
    };

    // double-click → rename
    if response.double_clicked() && state.map_tool != MapTool::AddRoute {
        if let Some(pos) = pointer {
            if let Some(id) = hit_system(state, pos) {
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
                    if let Some(id) = hit_system(state, pos) {
                        state.drag_system = Some(id);
                    } else if state.map_tool == MapTool::Select {
                        if let Some(c) = hex_pick(pos - origin.to_vec2(), &g, sector_w, sector_h) {
                            state.rect_select = Some((c, c));
                        }
                    }
                }
                MapTool::AddRoute => {
                    if let Some(id) = hit_system(state, pos) {
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
                if let Some(c) = hex_pick(pos - origin.to_vec2(), &g, sector_w, sector_h) {
                    state.rect_select = Some((start, c));
                }
            }
        }
    }

    // drag stop
    if response.drag_stopped() {
        if state.map_tool == MapTool::AddRoute {
            if let (Some(from), Some(pos)) = (state.pending_route_start.clone(), pointer) {
                if let Some(to) = hit_system(state, pos) {
                    add_route_between(state, from, to);
                }
            }
            state.pending_route_start = None;
        } else if let (Some(drag_id), Some(pos)) = (state.drag_system.clone(), pointer) {
            if let Some(coord) = hex_pick(pos - origin.to_vec2(), &g, sector_w, sector_h) {
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
            let hit = hit_system(state, pos);
            let coord = hex_pick(pos - origin.to_vec2(), &g, sector_w, sector_h);
            handle_click(state, hit, coord, modifiers.shift);
        }
    }
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
            // §REG panel territory; no-op on the §S surface.
        }
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
}
