//! Editable hex-grid map. Renders the sector hex grid and handles clicks:
//! empty hex → open `PlaceSystem` dialog at that coord; system hex → select.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::palette::{
    self, darken, star_color, tint, HEX_EMPTY, HEX_OUTLINE, SELECTION, TEXT, TEXT_DIM,
};
use sectorforge::sector_model::HexCoord;

use super::state::{Dialog, EditorState, RouteEndpoint, SectorEditTool, Selection};
use super::ui_helpers::mono;

pub fn show_map(ui: &mut Ui, state: &mut EditorState) {
    let route_pick = state.route_pick;
    let (sector_width, sector_height) = if let Some(s) = &state.sector {
        (s.width, s.height)
    } else {
        ui.label(RichText::new("no sector loaded — use NEW SECTOR or OPEN").color(TEXT_DIM));
        return;
    };

    let hex_size = state.hex_size;
    let g = Geom::new(hex_size);
    let map_size = map_size(sector_width, sector_height, &g);
    let (rect, response) = ui.allocate_exact_size(map_size, Sense::drag());
    let painter = ui.painter_at(rect);
    let origin = rect.min;
    painter.rect_filled(rect, 0.0, palette::BG);

    // Empty hexes (full grid).
    for r in 0..sector_height as i32 {
        for q in 0..sector_width as i32 {
            let c = hex_center(q, r, &g) + origin.to_vec2();
            draw_hex(&painter, c, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
        }
    }

    // Capture some info before we start rendering systems/routes
    let mut drag_drop_finalize: Option<(sectorforge::ids::SystemId, HexCoord)> = None;
    let mut click_action: Option<ClickAction> = None;
    let mut delete_id: Option<sectorforge::ids::SystemId> = None;
    let mut dirty = false;

    {
        let sector = state.sector.as_ref().unwrap();

        // Routes (so they sit under system disks).
        let mut centers: std::collections::HashMap<&str, Pos2> = Default::default();
        for sys in &sector.systems {
            let mut c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
            if Some(&sys.id) == state.drag_id.as_ref() {
                if let Some(pos) = response.interact_pointer_pos() {
                    c = pos;
                }
            }
            centers.insert(sys.id.as_str(), c);
        }
        let route_thickness = (g.hex_size * 0.08).max(2.0);
        for route in &sector.routes {
            if let (Some(&a), Some(&b)) = (
                centers.get(route.from_system_id.as_str()),
                centers.get(route.to_system_id.as_str()),
            ) {
                crate::palette::draw_route_line(
                    &painter,
                    a,
                    b,
                    route_thickness,
                    crate::palette::stability_color(route.stability),
                    route.route_type.pattern(state.route_view_mode),
                );
            }
        }

        // Potential route line (visual feedback)
        if let Some(from_id) = &state.pending_route_start {
            if let Some(&from_pos) = centers.get(from_id.as_str()) {
                if let Some(mouse_pos) = response.interact_pointer_pos() {
                    painter.line_segment(
                        [from_pos, mouse_pos],
                        Stroke::new(2.0, Color32::from_rgb(235, 200, 90)),
                    );
                }
            }
        }

        // System disks + selection ring.
        let selected_id: Option<&str> = match &state.selection {
            Selection::System(id) => Some(id.as_str()),
            Selection::World { system_id, .. } => Some(system_id.as_str()),
            Selection::None => None,
        };
        for sys in &sector.systems {
            let c = centers[sys.id.as_str()];
            if Some(sys.id.as_str()) == selected_id {
                draw_hex_outline_only(&painter, c, g.hex_size + 2.0, SELECTION, 2.5);
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
                let r = g.hex_size * 0.35;
                let pts = vec![
                    Pos2::new(c.x, c.y - r),
                    Pos2::new(c.x + r, c.y),
                    Pos2::new(c.x, c.y + r),
                    Pos2::new(c.x - r, c.y),
                ];
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    Color32::TRANSPARENT,
                    Stroke::new(1.5, TEXT_DIM),
                ));
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
        for sys in &sector.systems {
            let c = centers[sys.id.as_str()];
            let label = sys.name.to_ascii_uppercase();
            let font = FontId::monospace(label_size);
            let galley = painter.layout_no_wrap(label.clone(), font.clone(), TEXT_DIM);
            let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + g.hex_size + 3.0);
            let pad = Vec2::new(3.0, 1.0);
            let bg_rect = egui::Rect::from_min_size(pos - pad, galley.size() + pad * 2.0);
            painter.rect_filled(bg_rect, 2.0, palette::BG);
            painter.galley(pos, galley, TEXT_DIM);
        }

        // Drag/Click logic (collecting side-effects)
        if response.drag_started() && state.tool == SectorEditTool::Select {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit_sys = sector.systems.iter().find(|s| {
                    let c = hex_center(s.coord.q, s.coord.r, &g) + origin.to_vec2();
                    (c - pos).length() <= g.hex_size * 0.95
                });
                if let Some(sys) = hit_sys {
                    state.drag_id = Some(sys.id.clone());
                }
            }
        }

        if response.drag_stopped() {
            if let (Some(drag_id), Some(pos)) =
                (state.drag_id.clone(), response.interact_pointer_pos())
            {
                if let Some(coord) = hex_pick(pos - origin, &g, sector_width, sector_height) {
                    drag_drop_finalize = Some((drag_id, coord));
                }
            }
            state.drag_id = None;
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit_sys = sector.systems.iter().find(|s| {
                    let c = hex_center(s.coord.q, s.coord.r, &g) + origin.to_vec2();
                    (c - pos).length() <= g.hex_size * 0.95
                });

                if let Some(sys) = hit_sys {
                    match state.tool {
                        SectorEditTool::Delete => {
                            delete_id = Some(sys.id.clone());
                        }
                        SectorEditTool::AddRoute => {
                            if let Some(from) = state.pending_route_start.take() {
                                if from != sys.id {
                                    click_action =
                                        Some(ClickAction::AddRoute(from, sys.id.clone()));
                                }
                            } else {
                                state.pending_route_start = Some(sys.id.clone());
                            }
                        }
                        _ => {
                            if let Some((idx, ep)) = route_pick {
                                click_action =
                                    Some(ClickAction::RoutePick(idx, ep, sys.id.clone()));
                            } else {
                                click_action = Some(ClickAction::SelectSystem(sys.id.clone()));
                            }
                        }
                    }
                } else if route_pick.is_none() {
                    if let Some(coord) = hex_pick(pos - origin, &g, sector_width, sector_height) {
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
                        let mut route = super::state::empty_route(from.clone(), to.clone());
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
                    let coords: std::collections::HashMap<sectorforge::ids::SystemId, HexCoord> =
                        sector
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
    SelectSystem(sectorforge::ids::SystemId),
    AddSystem(HexCoord),
    AddRoute(sectorforge::ids::SystemId, sectorforge::ids::SystemId),
    RoutePick(usize, RouteEndpoint, sectorforge::ids::SystemId),
}

fn draw_toolbox(ui: &mut Ui, state: &mut EditorState) {
    let rect = ui.max_rect();
    let toolbox_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + 10.0, rect.min.y + 10.0),
        Vec2::new(140.0, 130.0),
    );
    let painter = ui.painter();
    painter.rect_filled(toolbox_rect, 4.0, palette::PANEL_BG);
    painter.rect_stroke(toolbox_rect, 4.0, Stroke::new(1.0, HEX_OUTLINE));

    ui.put(toolbox_rect, |ui: &mut Ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("TOOLBOX").font(mono(11.0)).color(TEXT_DIM));
            ui.add_space(5.0);

            for (tool, label) in [
                (SectorEditTool::Select, "SELECT / DRAG"),
                (SectorEditTool::AddSystem, "ADD SYSTEM"),
                (SectorEditTool::AddRoute, "ADD ROUTE"),
                (SectorEditTool::Delete, "DELETE"),
            ] {
                if ui
                    .selectable_label(state.tool == tool, RichText::new(label).font(mono(11.0)))
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

fn hex_pick(local_pos: Vec2, g: &Geom, width: u32, height: u32) -> Option<HexCoord> {
    let mut best: Option<(HexCoord, f32)> = None;
    for r in 0..height as i32 {
        for q in 0..width as i32 {
            let c = hex_center(q, r, g);
            let d = (Pos2::new(c.x, c.y) - Pos2::new(local_pos.x, local_pos.y)).length();
            if d <= g.hex_size * 0.95 {
                let entry = (HexCoord { q, r }, d);
                if best.as_ref().is_none() || d < best.as_ref().unwrap().1 {
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
