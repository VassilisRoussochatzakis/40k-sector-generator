//! Sector hex-grid widget: pointy-top hexes, routes, system disks. Clickable.

use egui::{Align2, Color32, FontId, Pos2, Response, Sense, Stroke, Ui, Vec2};

use crate::sector_model::GeneratedSector;

use super::palette::{
    self, darken, stability_color, star_color, tint, HEX_EMPTY, HEX_OUTLINE, SELECTION, TEXT,
    TEXT_DIM,
};

pub struct SectorView<'a> {
    pub sector: &'a GeneratedSector,
    pub selected_system: Option<&'a str>,
    pub hex_size: f32,
}

pub struct SectorClick {
    pub system_id: String,
}

impl<'a> SectorView<'a> {
    pub fn show(self, ui: &mut Ui) -> (Response, Option<SectorClick>) {
        let g = Geom::new(self.hex_size);
        let map_size = map_size(self.sector, &g);
        let (rect, response) = ui.allocate_exact_size(map_size, Sense::click());
        let painter = ui.painter_at(rect);
        let origin = rect.min;

        painter.rect_filled(rect, 0.0, palette::BG);

        for r in 0..self.sector.height as i32 {
            for q in 0..self.sector.width as i32 {
                let c = hex_center(q, r, &g) + origin.to_vec2();
                draw_hex(&painter, c, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
            }
        }

        let mut centers: std::collections::HashMap<&str, Pos2> = Default::default();
        for sys in &self.sector.systems {
            let c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
            centers.insert(sys.id.as_str(), c);
        }

        let route_thickness = (g.hex_size * 0.08).max(2.0);
        for route in &self.sector.routes {
            let (Some(&a), Some(&b)) = (
                centers.get(route.from_system_id.as_str()),
                centers.get(route.to_system_id.as_str()),
            ) else {
                continue;
            };
            painter.line_segment(
                [a, b],
                Stroke::new(route_thickness, stability_color(route.stability)),
            );
        }

        // Pass 1: all system hex fills + stars + pips. So later hexes can't
        // paint over earlier system labels.
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            let fill = star_color(&sys.star.colour_code);
            draw_hex(&painter, c, g.hex_size, tint(fill, 0.18), HEX_OUTLINE);
            let is_sel = self.selected_system == Some(sys.id.as_str());
            if is_sel {
                draw_hex_outline_only(&painter, c, g.hex_size + 2.0, SELECTION, 2.5);
            }
            let r = g.hex_size * 0.42;
            painter.circle_filled(c, r, fill);
            painter.circle_stroke(c, r, Stroke::new(1.5, darken(fill, 0.55)));

            let pip = sys.worlds.len();
            if pip > 0 {
                let tx = c.x + g.hex_size * 0.55;
                let ty = c.y + g.hex_size * 0.55;
                painter.text(
                    Pos2::new(tx, ty),
                    Align2::RIGHT_BOTTOM,
                    pip.to_string(),
                    FontId::monospace((g.hex_size * 0.34).max(10.0)),
                    TEXT,
                );
            }
        }

        // Pass 2: labels last, always on top of every hex.
        let label_size = (g.hex_size * 0.28).max(9.0);
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            let label = short_id_tail(&sys.id);
            // Pill background behind label so it stays readable when an
            // adjacent row's hex tip pokes through.
            let font = FontId::monospace(label_size);
            let galley = painter.layout_no_wrap(label.clone(), font.clone(), TEXT_DIM);
            let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + g.hex_size + 3.0);
            let pad = Vec2::new(3.0, 1.0);
            let bg_rect = egui::Rect::from_min_size(pos - pad, galley.size() + pad * 2.0);
            painter.rect_filled(bg_rect, 2.0, palette::BG);
            painter.galley(pos, galley, TEXT_DIM);
        }

        let mut click = None;
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit = self
                    .sector
                    .systems
                    .iter()
                    .map(|s| {
                        let c = hex_center(s.coord.q, s.coord.r, &g) + origin.to_vec2();
                        (s, (c - pos).length())
                    })
                    .filter(|(_, d)| *d <= g.hex_size * 0.95)
                    .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());
                if let Some((sys, _)) = hit {
                    click = Some(SectorClick {
                        system_id: sys.id.clone(),
                    });
                }
            }
        }

        (response, click)
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

fn map_size(sector: &GeneratedSector, g: &Geom) -> Vec2 {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    // Rightmost cell sits at q=width-1, r=height-1. Its center x is
    // margin + horiz_step*((width-1) + 0.5*(height-1)) + horiz_step/2.
    // Add another horiz_step/2 for the hex's own right edge + right margin.
    let w = g.margin * 2.0
        + horiz_step * sector.width as f32
        + 0.5 * horiz_step * (sector.height.saturating_sub(1) as f32);
    // Add a label band below the bottom row.
    let label_band = g.hex_size * 0.55;
    let h = g.margin * 2.0
        + sector.height.saturating_sub(1) as f32 * vert_step
        + 2.0 * g.hex_size
        + label_band;
    Vec2::new(w, h)
}

fn hex_center(q: i32, r: i32, g: &Geom) -> Pos2 {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let x = g.margin + horiz_step * (q as f32 + 0.5 * r as f32) + horiz_step / 2.0;
    let y = g.margin + vert_step * r as f32 + g.hex_size;
    Pos2::new(x, y)
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

fn short_id_tail(id: &str) -> String {
    id.rsplit('-').next().unwrap_or(id).to_ascii_uppercase()
}
