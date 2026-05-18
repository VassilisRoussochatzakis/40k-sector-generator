//! Sector hex-grid widget: pointy-top hexes, routes, system disks. Clickable.

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, FontId, Pos2, Response, Sense, Stroke, Ui, Vec2};

use crate::sector_model::{self, GeneratedSector};

use crate::subsectors::Subsector;

use super::palette::{
    self, darken, draw_route_line, stability_color, star_color, HEX_EMPTY, HEX_OUTLINE,
    PATH_HIGHLIGHT, PATH_WAYPOINT, SELECTION, TEXT, TEXT_DIM,
};

pub struct SectorView<'a> {
    pub sector: &'a GeneratedSector,
    pub selected_system: Option<&'a str>,
    pub hex_size: f32,
    /// Route ids that belong to the active planned path. Drawn thick on top of the base routes.
    pub path_route_ids: Option<&'a HashSet<String>>,
    /// System ids on the planned path. Rendered with a glowing ring.
    pub path_waypoints: Option<&'a HashSet<String>>,
    /// Subsector overlay: tile boundaries, labels, and capital markers.
    pub subsectors: Option<&'a [Subsector]>,
    /// When set, every hex of this subsector gets a faint grey tint.
    pub selected_subsector: Option<&'a str>,
}

const SUBSECTOR_BORDER: Color32 = Color32::from_rgb(160, 160, 160);
const SUBSECTOR_LABEL: Color32 = Color32::from_rgb(230, 195, 120);
const SUBSECTOR_HIGHLIGHT: Color32 = Color32::from_rgba_premultiplied(40, 40, 44, 70);
const CAPITAL_MARKER: Color32 = Color32::from_rgb(255, 220, 100);

pub enum SectorClick {
    System(String),
    Subsector(String),
}

impl<'a> SectorView<'a> {
    pub fn show(self, ui: &mut Ui) -> (Response, Option<SectorClick>) {
        let g = Geom::new(self.hex_size);
        let map_size = map_size(self.sector, &g);
        let (rect, response) = ui.allocate_exact_size(map_size, Sense::click());
        let painter = ui.painter_at(rect);
        let origin = rect.min;

        painter.rect_filled(rect, 0.0, palette::BG);

        // Map each hex coord to its subsector id so we know where to paint
        // borders. Clusters are arbitrary shapes — use the per-subsector
        // `hex_cells` membership rather than a rectangular bounding box.
        let mut hex_subsector: HashMap<(i32, i32), &str> = HashMap::new();
        if let Some(subs) = self.subsectors {
            for s in subs {
                for &(q, r) in &s.hex_cells {
                    hex_subsector.insert((q as i32, r as i32), s.id.as_str());
                }
            }
        }

        for r in 0..self.sector.height as i32 {
            for q in 0..self.sector.width as i32 {
                let c = hex_center(q, r, &g) + origin.to_vec2();
                draw_hex(&painter, c, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
            }
        }

        // Selected subsector: faint grey wash over every hex in the cluster.
        // Drawn after the base hex fills so it tints them, but before borders,
        // routes, and stars so those remain crisp.
        if let Some(sel) = self.selected_subsector {
            for (&(q, r), &sid) in &hex_subsector {
                if sid == sel {
                    let c = hex_center(q, r, &g) + origin.to_vec2();
                    draw_hex_fill(&painter, c, g.hex_size, SUBSECTOR_HIGHLIGHT);
                }
            }
        }

        // Subsector tile borders: draw thick line along each hex edge that
        // separates two different subsectors (or the sector outer rim).
        if !hex_subsector.is_empty() {
            let border_thick = (g.hex_size * 0.10).max(2.5);
            // Pointy-top odd-r offset edges (vertex i → vertex i+1):
            // 0:E, 1:SE, 2:SW, 3:W, 4:NW, 5:NE — neighbor offsets depend
            // on row parity, see `sector_model::offset_r_neighbors`.
            for r in 0..self.sector.height as i32 {
                let neighbor_deltas = crate::sector_model::offset_r_neighbors(r);
                for q in 0..self.sector.width as i32 {
                    let here = hex_subsector.get(&(q, r)).copied();
                    let Some(here_id) = here else { continue };
                    let c = hex_center(q, r, &g) + origin.to_vec2();
                    let v = hex_vertices(c, g.hex_size);
                    for (i, (dq, dr)) in neighbor_deltas.iter().enumerate() {
                        let other = hex_subsector.get(&(q + dq, r + dr)).copied();
                        let differs = match other {
                            Some(id) => id != here_id,
                            None => true, // sector rim
                        };
                        if differs {
                            let a = v[i];
                            let b = v[(i + 1) % 6];
                            let edge_len = a.distance(b);
                            let dot_radius = (border_thick * 0.8).max(2.0);
                            let spacing = dot_radius * 2.5;
                            let segments = (edge_len / spacing).ceil() as usize;
                            for j in 0..=segments {
                                let t = j as f32 / segments as f32;
                                let mid = Pos2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                                painter.circle_filled(mid, dot_radius, SUBSECTOR_BORDER);
                            }
                        }
                    }
                }
            }
        }

        let mut centers: std::collections::HashMap<&str, Pos2> = Default::default();
        for sys in &self.sector.systems {
            let c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
            centers.insert(sys.id.as_str(), c);
        }

        let route_thickness = (g.hex_size * 0.08).max(2.0);
        let star_r = g.hex_size * 0.2016;
        let shorten = |a: Pos2, b: Pos2| -> Option<(Pos2, Pos2)> {
            let delta = b - a;
            let len = delta.length();
            if len <= star_r * 2.0 {
                return None;
            }
            let dir = delta / len;
            Some((a + dir * star_r, b - dir * star_r))
        };
        for route in &self.sector.routes {
            let (Some(&a), Some(&b)) = (
                centers.get(route.from_system_id.as_str()),
                centers.get(route.to_system_id.as_str()),
            ) else {
                continue;
            };
            let Some((a2, b2)) = shorten(a, b) else {
                continue;
            };
            draw_route_line(
                &painter,
                a2,
                b2,
                route_thickness,
                stability_color(route.stability),
                route.route_type.pattern(),
            );
        }

        if let Some(ids) = self.path_route_ids {
            let glow_thick = route_thickness * 3.2;
            let core_thick = route_thickness * 1.8;
            for route in &self.sector.routes {
                if !ids.contains(&route.id) {
                    continue;
                }
                let (Some(&a), Some(&b)) = (
                    centers.get(route.from_system_id.as_str()),
                    centers.get(route.to_system_id.as_str()),
                ) else {
                    continue;
                };
                let Some((a2, b2)) = shorten(a, b) else {
                    continue;
                };
                let glow = Color32::from_rgba_unmultiplied(
                    PATH_HIGHLIGHT.r(),
                    PATH_HIGHLIGHT.g(),
                    PATH_HIGHLIGHT.b(),
                    70,
                );
                painter.line_segment([a2, b2], Stroke::new(glow_thick, glow));
                painter.line_segment([a2, b2], Stroke::new(core_thick, PATH_HIGHLIGHT));
            }
        }

        // Pass 1: all system hex fills + stars + pips. So later hexes can't
        // paint over earlier system labels.
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            let fill = star_color(&sys.star.colour_code);
            let is_sel = self.selected_system == Some(sys.id.as_str());
            if is_sel {
                draw_hex_outline_only(&painter, c, g.hex_size + 2.0, SELECTION, 2.5);
            }
            if self
                .path_waypoints
                .map(|s| s.contains(&sys.id))
                .unwrap_or(false)
            {
                draw_hex_outline_only(&painter, c, g.hex_size + 4.0, PATH_WAYPOINT, 2.5);
            }
            let r = star_r;
            painter.circle_filled(c, r, fill);
            painter.circle_stroke(c, r, Stroke::new(1.5, darken(fill, 0.55)));

            // Subsector capital marker: small filled diamond above the star.
            if let Some(subs) = self.subsectors {
                let is_capital = subs.iter().any(|s| {
                    s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str())
                });
                if is_capital {
                    draw_capital_marker(&painter, c, g.hex_size);
                }
            }

            let pip = sys.worlds.len();
            if pip > 0 {
                // Top-right corner of the hex. Name label sits below the
                // star and the capital marker sits at top-center, so this
                // corner stays clear.
                let pip_font = FontId::monospace((g.hex_size * 0.36).max(11.0));
                let pip_center = Pos2::new(c.x + g.hex_size * 0.55, c.y - g.hex_size * 0.55);
                let disc_r = (g.hex_size * 0.22).max(8.0);
                painter.circle_filled(pip_center, disc_r, palette::BG);
                painter.circle_stroke(pip_center, disc_r, Stroke::new(1.2, darken(fill, 0.4)));
                painter.text(
                    pip_center,
                    Align2::CENTER_CENTER,
                    pip.to_string(),
                    pip_font,
                    TEXT,
                );
            }
        }

        // Subsector labels: name chip placed inside the subsector, avoiding
        // system markers, system name labels, other subsector labels, and the
        // map edges. Uses cluster name ("Subsector Aurelia" -> "AURELIA") rather
        // than the spreadsheet letter so the map reads politically. Skip
        // clusters with no member systems so empty frontier regions stay
        // uncluttered.
        if let Some(subs) = self.subsectors {
            let sub_label_size = (g.hex_size * 0.36).max(11.0);
            let font = FontId::monospace(sub_label_size);
            let sys_label_size = (g.hex_size * 0.28).max(9.0);
            let sys_font = FontId::monospace(sys_label_size);
            let pad = Vec2::new(6.0, 2.0);
            let sys_pad = Vec2::new(3.0, 1.0);
            let line_gap = 2.0;
            let hex_half_w = g.hex_size * 3f32.sqrt() / 2.0;

            // Static obstacles: every system marker bbox + every system name
            // label rect. Coordinates are screen-space (already include origin).
            let mut obstacles: Vec<egui::Rect> = Vec::with_capacity(self.sector.systems.len() * 2);
            for sys in &self.sector.systems {
                let c = centers[sys.id.as_str()];
                obstacles.push(egui::Rect::from_min_max(
                    Pos2::new(c.x - hex_half_w, c.y - g.hex_size),
                    Pos2::new(c.x + hex_half_w, c.y + g.hex_size),
                ));
                let name = sys.name.to_ascii_uppercase();
                let galley = painter.layout_no_wrap(name, sys_font.clone(), TEXT_DIM);
                let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + star_r + 3.0);
                obstacles.push(egui::Rect::from_min_size(
                    pos - sys_pad,
                    galley.size() + sys_pad * 2.0,
                ));
            }

            // Already-placed subsector labels (avoid stacking on each other).
            let mut placed: Vec<egui::Rect> = Vec::with_capacity(subs.len());

            let sys_cells: HashSet<(i32, i32)> = self
                .sector
                .systems
                .iter()
                .map(|sys| (sys.coord.q, sys.coord.r))
                .collect();

            for s in subs {
                if s.system_ids.is_empty() || s.hex_cells.is_empty() {
                    continue;
                }

                let cells: HashSet<(i32, i32)> = s
                    .hex_cells
                    .iter()
                    .map(|&(q, r)| (q as i32, r as i32))
                    .collect();

                let name_part = s
                    .name
                    .strip_prefix("Subsector ")
                    .unwrap_or(s.name.as_str())
                    .to_ascii_uppercase();
                let top_galley =
                    painter.layout_no_wrap("SUBSECTOR".to_string(), font.clone(), SUBSECTOR_LABEL);
                let bot_galley = painter.layout_no_wrap(name_part, font.clone(), SUBSECTOR_LABEL);
                let block_w = top_galley.size().x.max(bot_galley.size().x);
                let block_h = top_galley.size().y + line_gap + bot_galley.size().y;

                // Centroid in screen coords.
                let mut sx: f32 = 0.0;
                let mut sy: f32 = 0.0;
                for &(q, r) in &s.hex_cells {
                    let c = hex_center(q as i32, r as i32, &g) + origin.to_vec2();
                    sx += c.x;
                    sy += c.y;
                }
                let n = s.hex_cells.len() as f32;
                let cen = Pos2::new(sx / n, sy / n);

                // Candidate empty cells sorted by distance to centroid.
                let mut cands: Vec<(i32, i32, f32)> = s
                    .hex_cells
                    .iter()
                    .filter(|&&(q, r)| !sys_cells.contains(&(q as i32, r as i32)))
                    .map(|&(q, r)| {
                        let c = hex_center(q as i32, r as i32, &g) + origin.to_vec2();
                        let d = (c - cen).length_sq();
                        (q as i32, r as i32, d)
                    })
                    .collect();
                cands.sort_by(|a, b| a.2.total_cmp(&b.2));

                let try_place =
                    |q: i32, r: i32, above: bool, placed: &[egui::Rect]| -> Option<(f32, f32)> {
                        // offset_r_neighbors order: 0:E 1:SE 2:SW 3:W 4:NW 5:NE.
                        let nbrs = sector_model::offset_r_neighbors(r);
                        if above {
                            let nw = (q + nbrs[4].0, r + nbrs[4].1);
                            let ne = (q + nbrs[5].0, r + nbrs[5].1);
                            if !cells.contains(&nw) || !cells.contains(&ne) {
                                return None;
                            }
                        } else {
                            let se = (q + nbrs[1].0, r + nbrs[1].1);
                            let sw = (q + nbrs[2].0, r + nbrs[2].1);
                            if !cells.contains(&se) || !cells.contains(&sw) {
                                return None;
                            }
                        }
                        let anchor = hex_center(q, r, &g) + origin.to_vec2();
                        let block_top_y = if above {
                            anchor.y - g.hex_size - block_h - 2.0
                        } else {
                            anchor.y + g.hex_size + 2.0
                        };
                        let block_min_x = anchor.x - block_w / 2.0;
                        let bg = egui::Rect::from_min_size(
                            Pos2::new(block_min_x, block_top_y) - pad,
                            Vec2::new(block_w, block_h) + pad * 2.0,
                        );
                        if !rect.contains_rect(bg) {
                            return None;
                        }
                        for o in obstacles.iter().chain(placed.iter()) {
                            if bg.intersects(*o) {
                                return None;
                            }
                        }
                        Some((block_min_x, block_top_y))
                    };

                let mut chosen: Option<(f32, f32)> = None;
                'outer: for &(q, r, _) in &cands {
                    for above in [true, false] {
                        if let Some(p) = try_place(q, r, above, &placed) {
                            chosen = Some(p);
                            break 'outer;
                        }
                    }
                }

                // Fallback: nearest-centroid cell, above, clamped inside rect.
                let (block_min_x, block_top_y) = chosen.unwrap_or_else(|| {
                    let &(q0, r0) = s
                        .hex_cells
                        .iter()
                        .min_by(|&&(q1, r1), &&(q2, r2)| {
                            let c1 = hex_center(q1 as i32, r1 as i32, &g) + origin.to_vec2();
                            let c2 = hex_center(q2 as i32, r2 as i32, &g) + origin.to_vec2();
                            (c1 - cen).length_sq().total_cmp(&(c2 - cen).length_sq())
                        })
                        .expect("non-empty");
                    let anchor = hex_center(q0 as i32, r0 as i32, &g) + origin.to_vec2();
                    let bt = anchor.y - g.hex_size - block_h - 2.0;
                    let bmx = (anchor.x - block_w / 2.0)
                        .max(rect.left() + pad.x)
                        .min(rect.right() - block_w - pad.x);
                    let bty = bt
                        .max(rect.top() + pad.y)
                        .min(rect.bottom() - block_h - pad.y);
                    (bmx, bty)
                });

                let block_min = Pos2::new(block_min_x, block_top_y);
                let bg_rect = egui::Rect::from_min_size(
                    block_min - pad,
                    Vec2::new(block_w, block_h) + pad * 2.0,
                );
                painter.rect_filled(
                    bg_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(20, 16, 28, 210),
                );
                painter.galley(
                    Pos2::new(
                        block_min_x + (block_w - top_galley.size().x) / 2.0,
                        block_top_y,
                    ),
                    top_galley.clone(),
                    SUBSECTOR_LABEL,
                );
                painter.galley(
                    Pos2::new(
                        block_min_x + (block_w - bot_galley.size().x) / 2.0,
                        block_top_y + top_galley.size().y + line_gap,
                    ),
                    bot_galley,
                    SUBSECTOR_LABEL,
                );
                placed.push(bg_rect);
            }
        }

        // Pass 2: labels last, always on top of every hex.
        let label_size = (g.hex_size * 0.28).max(9.0);
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            let label = sys.name.to_ascii_uppercase();
            // Pill background behind label so it stays readable when an
            // adjacent row's hex tip pokes through.
            let font = FontId::monospace(label_size);
            let galley = painter.layout_no_wrap(label.clone(), font.clone(), TEXT_DIM);
            let star_r = g.hex_size * 0.2016;
            let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + star_r + 3.0);
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
                    .min_by(|(_, a), (_, b)| a.total_cmp(b));
                if let Some((sys, _)) = hit {
                    click = Some(SectorClick::System(sys.id.clone()));
                } else if !hex_subsector.is_empty() {
                    // No system under cursor — try empty hex inside a known
                    // subsector. Nearest hex center wins as long as it's within
                    // the hex's inscribed radius.
                    let inscribed = g.hex_size * 3f32.sqrt() / 2.0;
                    let mut best: Option<((i32, i32), f32)> = None;
                    for r in 0..self.sector.height as i32 {
                        for q in 0..self.sector.width as i32 {
                            let c = hex_center(q, r, &g) + origin.to_vec2();
                            let d = (c - pos).length();
                            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                                best = Some(((q, r), d));
                            }
                        }
                    }
                    if let Some(((q, r), d)) = best {
                        if d <= inscribed {
                            if let Some(&sid) = hex_subsector.get(&(q, r)) {
                                click = Some(SectorClick::Subsector(sid.to_string()));
                            }
                        }
                    }
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
    // Odd-r offset layout: odd rows shift right by half a step, so the
    // bounding rectangle is `width * horiz_step` wide plus one extra
    // half-step when height > 1 to cover the staggered odd rows.
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = g.margin * 2.0 + horiz_step * (sector.width as f32 + odd_shift);
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
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = g.margin + horiz_step * (q as f32 + row_shift) + horiz_step / 2.0;
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

fn draw_hex_fill(painter: &egui::Painter, c: Pos2, size: f32, fill: Color32) {
    let pts = hex_vertices(c, size).to_vec();
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn draw_capital_marker(painter: &egui::Painter, c: Pos2, hex_size: f32) {
    let r = (hex_size * 0.15).max(3.5);
    let cy = c.y - hex_size * 0.55;
    let pts = vec![
        Pos2::new(c.x, cy - r),
        Pos2::new(c.x + r, cy),
        Pos2::new(c.x, cy + r),
        Pos2::new(c.x - r, cy),
    ];
    painter.add(egui::Shape::convex_polygon(
        pts,
        CAPITAL_MARKER,
        Stroke::new(1.2, Color32::from_rgb(60, 40, 10)),
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
