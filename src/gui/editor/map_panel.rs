//! Editable hex-grid map. Renders the sector hex grid and handles clicks:
//! empty hex → open `PlaceSystem` dialog at that coord; system hex → select.

use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2};

use crate::gui::palette::{
    self, darken, star_color, tint, HEX_EMPTY, HEX_OUTLINE, SELECTION, TEXT, TEXT_DIM,
};
use crate::sector_model::HexCoord;

use super::state::{Dialog, EditorState, RouteEndpoint, Selection};

pub fn show_map(ui: &mut Ui, state: &mut EditorState) {
    let route_pick = state.route_pick;
    let Some(sector) = state.sector.as_ref() else {
        ui.label(egui::RichText::new("no sector loaded — use NEW SECTOR or OPEN").color(TEXT_DIM));
        return;
    };

    let hex_size = state.hex_size;
    let g = Geom::new(hex_size);
    let map_size = map_size(sector.width, sector.height, &g);
    let (rect, response) = ui.allocate_exact_size(map_size, Sense::click());
    let painter = ui.painter_at(rect);
    let origin = rect.min;
    painter.rect_filled(rect, 0.0, palette::BG);

    // Empty hexes (full grid).
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let c = hex_center(q, r, &g) + origin.to_vec2();
            draw_hex(&painter, c, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
        }
    }

    // Routes (so they sit under system disks).
    let mut centers: std::collections::HashMap<&str, Pos2> = Default::default();
    for sys in &sector.systems {
        let c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
        centers.insert(sys.id.as_str(), c);
    }
    let route_thickness = (g.hex_size * 0.08).max(2.0);
    for route in &sector.routes {
        if let (Some(&a), Some(&b)) = (
            centers.get(route.from_system_id.as_str()),
            centers.get(route.to_system_id.as_str()),
        ) {
            crate::gui::palette::draw_route_line(
                &painter,
                a,
                b,
                route_thickness,
                crate::gui::palette::stability_color(route.stability),
                route.pattern_with_salt(&sector.seed),
            );
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
        let fill = star_color(&sys.star.colour_code);
        draw_hex(&painter, c, g.hex_size, tint(fill, 0.18), HEX_OUTLINE);
        if Some(sys.id.as_str()) == selected_id {
            draw_hex_outline_only(&painter, c, g.hex_size + 2.0, SELECTION, 2.5);
        }
        let r = g.hex_size * 0.42;
        painter.circle_filled(c, r, fill);
        painter.circle_stroke(c, r, Stroke::new(1.5, darken(fill, 0.55)));
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

    // Click handling.
    let mut pending_route_pick: Option<(usize, RouteEndpoint, crate::ids::SystemId)> = None;
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            // hit existing system?
            let hit_sys = sector
                .systems
                .iter()
                .map(|s| {
                    let c = hex_center(s.coord.q, s.coord.r, &g) + origin.to_vec2();
                    (s, (c - pos).length())
                })
                .filter(|(_, d)| *d <= g.hex_size * 0.95)
                .min_by(|(_, a), (_, b)| a.total_cmp(b));
            if let Some((sys, _)) = hit_sys {
                if let Some((idx, ep)) = route_pick {
                    pending_route_pick = Some((idx, ep, sys.id.clone()));
                } else {
                    state.selection = Selection::System(sys.id.clone());
                }
            } else if route_pick.is_none() {
                if let Some(coord) = hex_pick(pos - origin, &g, sector.width, sector.height) {
                    let occupied = sector.systems.iter().any(|s| s.coord == coord);
                    if !occupied {
                        state.dialog = Dialog::PlaceSystem {
                            coord,
                            name: String::new(),
                        };
                    }
                }
            }
        }
    }

    if let Some((idx, ep, sys_id)) = pending_route_pick {
        if let Some(sector) = state.sector.as_mut() {
            let coords: std::collections::HashMap<crate::ids::SystemId, HexCoord> = sector
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
                    route.distance = crate::sector_model::hex_distance(a, b);
                }
                route.id = crate::ids::route_id(&route.from_system_id, &route.to_system_id);
            }
        }
        state.route_pick = None;
        state.mark_dirty();
    }
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

/// Reverse-pick: find the (q,r) whose center is nearest to `local_pos`, if it
/// lies within `hex_size` of any grid cell.
fn hex_pick(local_pos: Vec2, g: &Geom, width: u32, height: u32) -> Option<HexCoord> {
    let mut best: Option<(HexCoord, f32)> = None;
    for r in 0..height as i32 {
        for q in 0..width as i32 {
            let c = hex_center(q, r, g);
            let d = (Pos2::new(c.x, c.y) - Pos2::new(local_pos.x, local_pos.y)).length();
            if d <= g.hex_size * 0.95 {
                let entry = (HexCoord { q, r }, d);
                if best.is_none_or(|b| d < b.1) {
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
