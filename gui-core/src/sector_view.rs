//! Sector hex-grid widget: pointy-top hexes, routes, system disks. Clickable.

use std::collections::{BTreeSet, HashMap, HashSet};

use egui::{Align2, Color32, FontId, Pos2, Response, Sense, Stroke, Ui, Vec2};

use sectorforge::ids::{RouteId, SystemId};
use sectorforge::sector_model::{self, GeneratedSector, HexCoord};

use sectorforge::subsectors::Subsector;

use super::heatmap::HeatCell;
use super::map_theme::MapTheme;
use super::palette::{
    darken, draw_route_control_glyph, draw_route_line, stability_color, star_color,
};
use super::visual_tokens::{MapRegionOverlay, MapRouteVisual, MapSystemGlyph};

const SYSTEM_LABEL_MIN_VISIBLE_PX: f32 = 3.0;
const SYSTEM_PIP_MIN_VISIBLE_PX: f32 = 3.0;
const SUBSECTOR_LABEL_MIN_VISIBLE_PX: f32 = 3.0;

pub struct SectorMapCache {
    pub hex_subsector: HashMap<(i32, i32), String>,
    pub hex_system: HashMap<(i32, i32), sectorforge::ids::SystemId>,
    pub hex_region: HashMap<(i32, i32), (String, MapRegionOverlay)>,
    pub region_centroids: HashMap<String, Pos2>,
}

impl SectorMapCache {
    pub fn new(sector: &GeneratedSector, subsectors: &[Subsector]) -> Self {
        let mut hex_subsector = HashMap::new();
        for s in subsectors {
            for &(q, r) in &s.hex_cells {
                hex_subsector.insert((q as i32, r as i32), s.id.as_ref().to_string());
            }
        }

        let mut hex_system = HashMap::new();
        for sys in &sector.systems {
            hex_system.insert((sys.coord.q, sys.coord.r), sys.id.clone());
        }

        let mut hex_region = HashMap::new();
        let mut region_hex_counts: HashMap<String, (f32, f32, f32)> = HashMap::new();

        for reg in sector.regions.iter() {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for h in &reg.hexes {
                hex_region.insert(
                    (h.q, h.r),
                    (
                        reg.id.to_string(),
                        MapRegionOverlay::from_condition(reg.kind),
                    ),
                );
                sx += h.q as f32;
                sy += h.r as f32;
            }
            if !reg.hexes.is_empty() {
                let n = reg.hexes.len() as f32;
                region_hex_counts.insert(reg.id.to_string(), (sx / n, sy / n, n));
            }
        }

        let mut region_centroids = HashMap::new();
        // Centroids are stored in axial coords (q, r) for now, will be projected in render.
        for (id, (q, r, _)) in region_hex_counts {
            region_centroids.insert(id, Pos2::new(q, r));
        }

        Self {
            hex_subsector,
            hex_system,
            hex_region,
            region_centroids,
        }
    }

    /// §CTX1 Phase 5 — region id containing `coord`, if any. O(1) lookup over
    /// the precomputed `hex_region` table. Used by `panels/map.rs` to surface
    /// the §6.5 region-hex context menu.
    #[must_use]
    pub fn region_for_hex(&self, coord: HexCoord) -> Option<&str> {
        self.hex_region
            .get(&(coord.q, coord.r))
            .map(|(id, _)| id.as_str())
    }
}

pub struct SectorView<'a> {
    pub sector: &'a GeneratedSector,
    pub selected_system: Option<&'a str>,
    pub selected_route: Option<&'a str>,
    pub hex_size: f32,
    /// Route ids that belong to the active planned path. Drawn thick on top of the base routes.
    pub path_route_ids: Option<&'a HashSet<sectorforge::ids::RouteId>>,
    /// System ids on the planned path. Rendered with a glowing ring.
    pub path_waypoints: Option<&'a HashSet<sectorforge::ids::SystemId>>,
    /// Subsector overlay: tile boundaries, labels, and capital markers.
    pub subsectors: Option<&'a [Subsector]>,
    /// Pre-calculated lookups for systems, subsectors, and regions.
    pub cache: Option<&'a SectorMapCache>,
    /// When set, every hex of this subsector gets a faint grey tint.
    pub selected_subsector: Option<&'a str>,
    /// Per-system heatmap samples. Hexes for systems present in the map are
    /// blended toward the cell's colour scaled by intensity (§9.5 / §10).
    pub heatmap: Option<&'a HashMap<sectorforge::ids::SystemId, HeatCell>>,
    /// When true, clicks on empty cells are returned to callers for live editing.
    pub empty_hex_clicks: bool,
    pub route_view_mode: sectorforge::sector_model::RouteViewMode,
    /// Viewport transform: added to every hex center.
    pub origin: Pos2,
    /// Secondary selection set (rendered as a softer ring than `selected_system`).
    /// Used by the builder for multi-select with shift-click + rect-drag.
    pub multi_selected: Option<&'a BTreeSet<SystemId>>,
    /// Pinned-system overlay (orange outer ring). Builder §Q1.
    pub pinned: Option<&'a BTreeSet<SystemId>>,
    /// When `Some`, the named system is rendered at `pos` instead of its
    /// stored hex coordinate. Used while dragging to follow the cursor.
    pub drag_override: Option<(SystemId, Pos2)>,
    /// While the user is hovering ADD-ROUTE, draws a dashed amber line from
    /// the start system to the cursor.
    pub pending_route_preview: Option<(SystemId, Pos2)>,
    /// In-progress rect-select on the map. Tints every hex in the AABB amber.
    pub rect_select: Option<(HexCoord, HexCoord)>,
    /// Sense passed to the canvas allocation. Builders that want drag-drop
    /// flip this to `Sense::click_and_drag()`.
    pub sense: Sense,
    /// When true, `show()` skips its own click→`SectorClick` dispatch and lets
    /// the caller pick hits manually via [`SectorGeom`]. Builder uses this.
    pub disable_internal_click_dispatch: bool,
    /// Visual theme — colours + sizing tokens. `None` falls back to
    /// [`MapTheme::default`] so existing call-sites compile unchanged.
    pub theme: Option<&'a MapTheme>,
    /// When true, paint a small chip to the left of the cursor showing the
    /// (q, r) coords of the hovered hex. Off for snapshot tests.
    pub show_hover_coord: bool,
}

pub enum SectorClick {
    System(sectorforge::ids::SystemId),
    Route(sectorforge::ids::RouteId),
    Subsector(String),
    Region(String),
    EmptyHex(HexCoord),
}

impl<'a> SectorView<'a> {
    pub fn show(self, ui: &mut Ui) -> (Response, Option<SectorClick>) {
        let g = SectorGeom::new(self.hex_size, self.origin);
        let (rect, response) = ui.allocate_at_least(ui.available_size(), self.sense);
        let painter = ui.painter_at(rect).with_clip_rect(rect);
        let origin = self.origin;

        let default_theme = MapTheme::default();
        let theme: &MapTheme = self.theme.unwrap_or(&default_theme);

        painter.rect_filled(rect, 0.0, theme.bg);

        // System-id keyed hex coords for heatmap lookup (still needed if cache not present)
        let mut hex_system_fallback: HashMap<(i32, i32), &str> = HashMap::new();
        if self.cache.is_none() {
            for sys in &self.sector.systems {
                hex_system_fallback.insert((sys.coord.q, sys.coord.r), sys.id.as_str());
            }
        }

        let horiz_step = g.hex_size * 3f32.sqrt();
        let vert_step = g.hex_size * 1.5;

        // Performance: only iterate hexes visible in the current viewport
        let min_r =
            ((rect.min.y - origin.y - g.margin - g.hex_size * 2.0) / vert_step).floor() as i32;
        let max_r = ((rect.max.y - origin.y - g.margin + g.hex_size) / vert_step).ceil() as i32;

        let min_q =
            ((rect.min.x - origin.x - g.margin - horiz_step * 1.5) / horiz_step).floor() as i32;
        let max_q = ((rect.max.x - origin.x - g.margin + horiz_step) / horiz_step).ceil() as i32;

        for r in min_r.max(0)..max_r.min(self.sector.height as i32) {
            for q in min_q.max(0)..max_q.min(self.sector.width as i32) {
                let c = hex_center(q, r, &g) + origin.to_vec2();

                let sid_opt = if let Some(cache) = self.cache {
                    cache.hex_system.get(&(q, r)).map(|s| s.as_str())
                } else {
                    hex_system_fallback.get(&(q, r)).copied()
                };

                let base = match (self.heatmap, sid_opt) {
                    (Some(map), Some(sid)) => map
                        .get(sid)
                        .map(|cell| blend_heat(theme.hex_empty, cell.color, cell.intensity))
                        .unwrap_or(theme.hex_empty),
                    _ => theme.hex_empty,
                };

                let fill = if let Some(cache) = self.cache {
                    match cache.hex_region.get(&(q, r)) {
                        Some((_, kind)) => blend_heat(base, theme.region_color(*kind), 0.5),
                        None => base,
                    }
                } else {
                    // Fallback region lookup (slow but safe)
                    let mut f = base;
                    for reg in self.sector.regions.iter() {
                        if reg.hexes.iter().any(|h| h.q == q && h.r == r) {
                            f = blend_heat(
                                base,
                                theme.region_color(MapRegionOverlay::from_condition(reg.kind)),
                                0.5,
                            );
                            break;
                        }
                    }
                    f
                };

                draw_hex(&painter, c, g.hex_size, fill, theme.hex_outline);
            }
        }

        // Rect-select tint (builder §S4). Painted before subsector overlays so
        // it stays visually quiet underneath the route + system layers.
        if let Some((a, b)) = self.rect_select {
            let (sq, eq) = (a.q.min(b.q), a.q.max(b.q));
            let (sr, er) = (a.r.min(b.r), a.r.max(b.r));
            for r in sr.max(min_r.max(0))..=er.min(max_r.min(self.sector.height as i32 - 1)) {
                for q in sq.max(min_q.max(0))..=eq.min(max_q.min(self.sector.width as i32 - 1)) {
                    let c = hex_center(q, r, &g) + origin.to_vec2();
                    draw_hex_fill(&painter, c, g.hex_size, theme.rect_select_tint);
                }
            }
        }

        // Selected subsector: faint grey wash over every hex in the cluster.
        if let Some(sel) = self.selected_subsector {
            if let Some(cache) = self.cache {
                for (&(q, r), sid) in &cache.hex_subsector {
                    if sid == sel {
                        let c = hex_center(q, r, &g) + origin.to_vec2();
                        if rect.expand(g.hex_size).contains(c) {
                            draw_hex_fill(&painter, c, g.hex_size, theme.subsector_highlight);
                        }
                    }
                }
            } else {
                // Fallback (slow)
                if let Some(subs) = self.subsectors {
                    if let Some(s) = subs.iter().find(|s| s.id.as_ref() == sel) {
                        for &(q, r) in &s.hex_cells {
                            let c = hex_center(q as i32, r as i32, &g) + origin.to_vec2();
                            if rect.expand(g.hex_size).contains(c) {
                                draw_hex_fill(&painter, c, g.hex_size, theme.subsector_highlight);
                            }
                        }
                    }
                }
            }
        }

        // Subsector tile borders: always visible, solid lines
        if let Some(subs) = self.subsectors {
            if !subs.is_empty() {
                let border_thick = theme.subsector_border_thickness.px(g.hex_size);
                for r in min_r.max(0)..max_r.min(self.sector.height as i32) {
                    let neighbor_deltas = sectorforge::sector_model::offset_r_neighbors(r);
                    for q in min_q.max(0)..max_q.min(self.sector.width as i32) {
                        let here_id = if let Some(cache) = self.cache {
                            cache.hex_subsector.get(&(q, r)).map(|s| s.as_str())
                        } else {
                            subs.iter()
                                .find(|s| {
                                    s.hex_cells
                                        .iter()
                                        .any(|&(hq, hr)| hq as i32 == q && hr as i32 == r)
                                })
                                .map(|s| s.id.as_ref())
                        };
                        let Some(here_id) = here_id else { continue };

                        let c = hex_center(q, r, &g) + origin.to_vec2();
                        let v = hex_vertices(c, g.hex_size);
                        for (i, (dq, dr)) in neighbor_deltas.iter().enumerate() {
                            let (nq, nr) = (q + dq, r + dr);
                            let other_id = if let Some(cache) = self.cache {
                                cache.hex_subsector.get(&(nq, nr)).map(|s| s.as_str())
                            } else {
                                subs.iter()
                                    .find(|s| {
                                        s.hex_cells
                                            .iter()
                                            .any(|&(hq, hr)| hq as i32 == nq && hr as i32 == nr)
                                    })
                                    .map(|s| s.id.as_ref())
                            };

                            let differs = match other_id {
                                Some(id) => id != here_id,
                                None => true, // sector rim
                            };
                            if differs {
                                let a = v[i];
                                let b = v[(i + 1) % 6];
                                painter.line_segment(
                                    [a, b],
                                    Stroke::new(border_thick, theme.subsector_border_color),
                                );
                            }
                        }
                    }
                }
            }
        }

        let mut centers: std::collections::HashMap<&str, Pos2> =
            HashMap::with_capacity(self.sector.systems.len());
        for sys in &self.sector.systems {
            let mut c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
            if let Some((drag_id, drag_pos)) = self.drag_override.as_ref() {
                if drag_id.as_str() == sys.id.as_str() {
                    c = *drag_pos;
                }
            }
            centers.insert(sys.id.as_str(), c);
        }

        let route_thickness = theme.route_thickness.px(g.hex_size);
        let star_r = g.hex_size * theme.star_radius_mul;
        let route_cull_margin = (route_thickness * 6.0).max(12.0);
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
            if !segment_intersects_rect(a2, b2, rect, route_cull_margin) {
                continue;
            }
            draw_route_line(
                &painter,
                a2,
                b2,
                route_thickness,
                stability_color(route.stability),
                MapRouteVisual::from_route_type(route.route_type, self.route_view_mode).pattern(),
            );
            draw_route_control_glyph(
                &painter,
                &self.sector.factions,
                route,
                a2,
                b2,
                route_thickness,
            );
        }

        // Builder ADD-ROUTE preview: dashed amber line from the picked start
        // system to the cursor (or to nothing if the start is off-screen).
        if let Some((start_id, cursor)) = self.pending_route_preview.as_ref() {
            if let Some(&a) = centers.get(start_id.as_str()) {
                let preview_thickness = (route_thickness * theme.pending_route_preview_mul)
                    .max(theme.pending_route_preview_min);
                draw_route_line(
                    &painter,
                    a,
                    *cursor,
                    preview_thickness,
                    theme.pending_route_preview,
                    sectorforge::sector_model::RoutePattern::Dashed,
                );
            }
        }

        if let Some(ids) = self.path_route_ids {
            let glow_thick = route_thickness * theme.path_glow_mul;
            let core_thick = route_thickness * theme.path_core_mul;
            let glow = Color32::from_rgba_unmultiplied(
                theme.path_highlight.r(),
                theme.path_highlight.g(),
                theme.path_highlight.b(),
                theme.path_glow_alpha,
            );
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
                if !segment_intersects_rect(a2, b2, rect, route_cull_margin) {
                    continue;
                }
                painter.line_segment([a2, b2], Stroke::new(glow_thick, glow));
                painter.line_segment([a2, b2], Stroke::new(core_thick, theme.path_highlight));
            }
        }

        if let Some(sel) = self.selected_route {
            for route in &self.sector.routes {
                if route.id.as_str() != sel {
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
                if !segment_intersects_rect(a2, b2, rect, route_cull_margin) {
                    continue;
                }
                let glow = Color32::from_rgba_unmultiplied(
                    theme.selection.r(),
                    theme.selection.g(),
                    theme.selection.b(),
                    theme.selection_glow_alpha,
                );
                painter.line_segment(
                    [a2, b2],
                    Stroke::new(route_thickness * theme.selection_glow_mul, glow),
                );
                draw_route_line(
                    &painter,
                    a2,
                    b2,
                    route_thickness * theme.selection_core_mul,
                    theme.selection,
                    MapRouteVisual::from_route_type(route.route_type, self.route_view_mode)
                        .pattern(),
                );
            }
        }

        draw_region_labels(&painter, self.sector, origin, &g, rect, self.cache, theme);

        // Pass 1: all system hex fills + stars + pips.
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            if !rect.expand(g.hex_size).contains(c) {
                continue;
            }
            let is_sel = self.selected_system == Some(sys.id.as_str());
            let is_multi = self
                .multi_selected
                .map(|s| s.contains(&sys.id))
                .unwrap_or(false);
            let is_pinned = self.pinned.map(|s| s.contains(&sys.id)).unwrap_or(false);
            if is_pinned {
                draw_hex_outline_only(
                    &painter,
                    c,
                    g.hex_size + theme.pinned_ring_bump,
                    theme.pinned_outline,
                    theme.pinned_ring_thickness,
                );
            }
            if is_sel {
                draw_hex_outline_only(
                    &painter,
                    c,
                    g.hex_size + theme.selection_ring_bump,
                    theme.selection,
                    theme.selection_ring_thickness,
                );
            } else if is_multi {
                draw_hex_outline_only(
                    &painter,
                    c,
                    g.hex_size + theme.selection_ring_bump,
                    theme.multi_select_outline,
                    theme.multi_select_ring_thickness,
                );
            }
            if self
                .path_waypoints
                .map(|s| s.contains(&sys.id))
                .unwrap_or(false)
            {
                draw_hex_outline_only(
                    &painter,
                    c,
                    g.hex_size + theme.waypoint_ring_bump,
                    theme.path_waypoint,
                    theme.waypoint_ring_thickness,
                );
            }

            let fill = if let Some(star) = &sys.star {
                star_color(&star.colour_code)
            } else {
                theme.text_dim
            };

            draw_system_glyph(
                &painter,
                c,
                star_r,
                MapSystemGlyph::from_system(sys),
                fill,
                theme,
            );

            // Subsector capital marker: small filled diamond above the star.
            if let Some(subs) = self.subsectors {
                let is_capital = subs.iter().any(|s| {
                    s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str())
                });
                if is_capital {
                    draw_capital_marker(&painter, c, g.hex_size, theme);
                }
            }

            let pip = sys.worlds.len();
            if let (true, Some((pip_font_size, disc_r))) =
                (pip > 0, system_pip_metrics(theme, g.hex_size))
            {
                // Top-right corner of the hex. Name label sits below the
                // star and the capital marker sits at top-center, so this
                // corner stays clear.
                let pip_font = FontId::monospace(pip_font_size);
                let pip_center = Pos2::new(c.x + g.hex_size * 0.55, c.y - g.hex_size * 0.55);
                painter.circle_filled(pip_center, disc_r, theme.bg);
                let stroke_w = (disc_r * 0.25).clamp(0.5, 1.2);
                painter.circle_stroke(pip_center, disc_r, Stroke::new(stroke_w, darken(fill, 0.4)));
                painter.text(
                    pip_center,
                    Align2::CENTER_CENTER,
                    pip.to_string(),
                    pip_font,
                    theme.text,
                );
            }
        }

        // Subsector labels: name chip placed inside the subsector, avoiding
        // system markers, system name labels, other subsector labels, and the
        // map edges. Uses cluster name ("Subsector Aurelia" -> "AURELIA") rather
        // than the spreadsheet letter so the map reads politically. Skip
        // clusters with no member systems so empty frontier regions stay
        // uncluttered. Hide if zoomed in too far.
        if let Some(subs) = self.subsectors {
            if let Some(sub_label_size) = subsector_label_font_px(theme, g.hex_size) {
                let font = FontId::monospace(sub_label_size);
                let sys_font = system_label_font_px(theme, g.hex_size).map(FontId::monospace);
                let pad = Vec2::new(4.0, 1.5);
                let sys_pad = Vec2::new(2.0, 0.5);
                let line_gap = 2.0;
                let hex_half_w = g.hex_size * 3f32.sqrt() / 2.0;

                // Static obstacles: every system marker bbox + every system name
                // label rect. Coordinates are screen-space.
                let mut obstacles: Vec<egui::Rect> =
                    Vec::with_capacity(self.sector.systems.len() * 2);
                for sys in &self.sector.systems {
                    let c = centers[sys.id.as_str()];
                    obstacles.push(egui::Rect::from_min_max(
                        Pos2::new(c.x - hex_half_w, c.y - g.hex_size),
                        Pos2::new(c.x + hex_half_w, c.y + g.hex_size),
                    ));
                    if let Some(sys_font) = &sys_font {
                        let name = sys.name.to_ascii_uppercase();
                        let galley = painter.layout_no_wrap(name, sys_font.clone(), theme.text_dim);
                        let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + star_r + 3.0);
                        obstacles.push(egui::Rect::from_min_size(
                            pos - sys_pad,
                            galley.size() + sys_pad * 2.0,
                        ));
                    }
                }

                // Already-placed subsector labels.
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
                        .unwrap_or_else(|| s.name.as_ref())
                        .to_ascii_uppercase();
                    let top_galley = painter.layout_no_wrap(
                        "SUBSECTOR".to_string(),
                        font.clone(),
                        theme.subsector_label,
                    );
                    let bot_galley =
                        painter.layout_no_wrap(name_part, font.clone(), theme.subsector_label);
                    let block_w = top_galley.size().x.max(bot_galley.size().x);
                    let block_h = top_galley.size().y + line_gap + bot_galley.size().y;

                    // Stable centroid in axial coords.
                    let mut sq: f32 = 0.0;
                    let mut sr: f32 = 0.0;
                    for &(q, r) in &s.hex_cells {
                        sq += q as f32;
                        sr += r as f32;
                    }
                    let n = s.hex_cells.len() as f32;
                    let aq = sq / n;
                    let ar = sr / n;

                    // Candidate empty cells sorted by axial distance to centroid.
                    let mut cands: Vec<(i32, i32, f32)> = s
                        .hex_cells
                        .iter()
                        .filter(|&&(q, r)| !sys_cells.contains(&(q as i32, r as i32)))
                        .map(|&(q, r)| {
                            let dq = q as f32 - aq;
                            let dr = r as f32 - ar;
                            (q as i32, r as i32, dq * dq + dr * dr)
                        })
                        .collect();
                    cands.sort_by(|a, b| a.2.total_cmp(&b.2));

                    let try_place = |q: i32,
                                     r: i32,
                                     above: bool,
                                     placed: &[egui::Rect]|
                     -> Option<(f32, f32)> {
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
                                let d1 = (q1 as f32 - aq).powi(2) + (r1 as f32 - ar).powi(2);
                                let d2 = (q2 as f32 - aq).powi(2) + (r2 as f32 - ar).powi(2);
                                d1.total_cmp(&d2)
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
                    painter.rect_filled(bg_rect, 3.0, theme.subsector_label_bg);
                    painter.galley(
                        Pos2::new(
                            block_min_x + (block_w - top_galley.size().x) / 2.0,
                            block_top_y,
                        ),
                        top_galley.clone(),
                        theme.subsector_label,
                    );
                    painter.galley(
                        Pos2::new(
                            block_min_x + (block_w - bot_galley.size().x) / 2.0,
                            block_top_y + top_galley.size().y + line_gap,
                        ),
                        bot_galley,
                        theme.subsector_label,
                    );
                    placed.push(bg_rect);
                }
            }
        }

        // Pass 2: labels last, always on top of every hex.
        if let Some(label_size) = system_label_font_px(theme, g.hex_size) {
            let font = FontId::monospace(label_size);
            let pad = Vec2::new(3.0, 1.0);
            for sys in &self.sector.systems {
                let c = centers[sys.id.as_str()];
                if !label_intersects_rect(sys.name.as_ref(), c, star_r, label_size, pad, rect) {
                    continue;
                }

                // Pill background behind label so it stays readable when an
                // adjacent row's hex tip pokes through.
                let label = sys.name.to_ascii_uppercase();
                let galley = painter.layout_no_wrap(label, font.clone(), theme.text_dim);
                let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + star_r + 3.0);
                let bg_rect = egui::Rect::from_min_size(pos - pad, galley.size() + pad * 2.0);
                painter.rect_filled(bg_rect, 2.0, theme.bg);
                painter.galley(pos, galley, theme.text_dim);
            }
        }

        if self.show_hover_coord {
            if let Some(pos) = response.hover_pos() {
                if let Some(HexCoord { q, r }) =
                    hex_pick(pos, &g, origin, self.sector.width, self.sector.height)
                {
                    let txt = format!("{q:02},{r:02}");
                    let font = FontId::monospace(11.0);
                    let galley = painter.layout_no_wrap(txt, font, theme.text);
                    let pad = Vec2::new(4.0, 2.0);
                    let size = galley.size() + pad * 2.0;
                    let mut anchor = Pos2::new(pos.x - 14.0 - size.x, pos.y - size.y / 2.0);
                    if anchor.x < rect.left() + 2.0 {
                        anchor.x = pos.x + 14.0;
                    }
                    anchor.y = anchor
                        .y
                        .clamp(rect.top() + 2.0, rect.bottom() - size.y - 2.0);
                    let bg = egui::Rect::from_min_size(anchor, size);
                    painter.rect_filled(bg, 2.0, theme.bg);
                    painter.rect_stroke(bg, 2.0, Stroke::new(1.0, theme.hex_outline));
                    painter.galley(anchor + pad, galley, theme.text);
                }
            }
        }

        let mut click = None;
        if response.clicked() && !self.disable_internal_click_dispatch {
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
                } else if let Some((route, _)) = self
                    .sector
                    .routes
                    .iter()
                    .filter_map(|route| {
                        let (Some(&a), Some(&b)) = (
                            centers.get(route.from_system_id.as_str()),
                            centers.get(route.to_system_id.as_str()),
                        ) else {
                            return None;
                        };
                        let (a2, b2) = shorten(a, b)?;
                        let d = distance_to_segment(pos, a2, b2);
                        let hit_radius = (g.hex_size * 0.16).max(route_thickness * 2.4).max(7.0);
                        (d <= hit_radius).then_some((route, d))
                    })
                    .min_by(|(_, a), (_, b)| a.total_cmp(b))
                {
                    click = Some(SectorClick::Route(route.id.clone()));
                } else if self.empty_hex_clicks {
                    if let Some(coord) =
                        hex_pick(pos, &g, origin, self.sector.width, self.sector.height)
                    {
                        let occupied = self.sector.systems.iter().any(|s| s.coord == coord);
                        if !occupied {
                            click = Some(SectorClick::EmptyHex(coord));
                        }
                    }
                } else {
                    // No system under cursor — try empty hex inside a known
                    // region or subsector. Nearest hex center wins as long as it's within
                    // the hex's inscribed radius.
                    let inscribed = g.hex_size * 3f32.sqrt() / 2.0;
                    let mut best: Option<((i32, i32), f32)> = None;
                    for r in min_r.max(0)..max_r.min(self.sector.height as i32) {
                        for q in min_q.max(0)..max_q.min(self.sector.width as i32) {
                            let c = hex_center(q, r, &g) + origin.to_vec2();
                            let d = (c - pos).length();
                            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                                best = Some(((q, r), d));
                            }
                        }
                    }
                    if let Some(((q, r), d)) = best {
                        if d <= inscribed {
                            if let Some(cache) = self.cache {
                                if let Some((rid, _)) = cache.hex_region.get(&(q, r)) {
                                    click = Some(SectorClick::Region(rid.to_string()));
                                } else if let Some(sid) = cache.hex_subsector.get(&(q, r)) {
                                    click = Some(SectorClick::Subsector(sid.to_string()));
                                }
                            } else {
                                // Fallback (slow)
                                let rid_opt = self
                                    .sector
                                    .regions
                                    .iter()
                                    .find(|reg| reg.hexes.iter().any(|h| h.q == q && h.r == r))
                                    .map(|reg| reg.id.to_string());
                                if let Some(rid) = rid_opt {
                                    click = Some(SectorClick::Region(rid));
                                } else if let Some(subs) = self.subsectors {
                                    let sid_opt = subs
                                        .iter()
                                        .find(|s| {
                                            s.hex_cells
                                                .iter()
                                                .any(|&(hq, hr)| hq as i32 == q && hr as i32 == r)
                                        })
                                        .map(|s| s.id.to_string());
                                    if let Some(sid) = sid_opt {
                                        click = Some(SectorClick::Subsector(sid));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (response, click)
    }
}

/// Pointy-top hex geometry shared by the renderer and external pickers.
///
/// Constructed once per frame from the active hex size and the viewport
/// origin (top-left + any pan offset). Exposed publicly so editor surfaces
/// (the builder map panel) can drive their own hit detection without
/// duplicating the math.
#[derive(Clone, Copy)]
pub struct SectorGeom {
    pub hex_size: f32,
    pub margin: f32,
    pub origin: Pos2,
}

impl SectorGeom {
    #[must_use]
    pub fn new(hex_size: f32, origin: Pos2) -> Self {
        Self {
            hex_size,
            margin: hex_size * 1.1,
            origin,
        }
    }

    /// Screen-space center of axial cell `(q, r)`.
    #[must_use]
    pub fn hex_center(&self, q: i32, r: i32) -> Pos2 {
        hex_center_xy(q, r, self) + self.origin.to_vec2()
    }

    /// Total pixel size of the hex grid (used for `Sense::drag` allocation in
    /// fixed-canvas hosts like the builder's map panel).
    #[must_use]
    pub fn map_size_px(&self, width: u32, height: u32) -> Vec2 {
        let horiz_step = self.hex_size * 3f32.sqrt();
        let vert_step = self.hex_size * 1.5;
        let odd_shift = if height > 1 { 0.5 } else { 0.0 };
        let w = self.margin * 2.0 + horiz_step * (width as f32 + odd_shift);
        let label_band = self.hex_size * 0.55;
        let h = self.margin * 2.0
            + height.saturating_sub(1) as f32 * vert_step
            + 2.0 * self.hex_size
            + label_band;
        Vec2::new(w, h)
    }

    /// Nearest hex within the inscribed radius of `screen_pos`. Returns
    /// `None` outside any hex.
    #[must_use]
    pub fn pick_hex(&self, screen_pos: Pos2, width: u32, height: u32) -> Option<HexCoord> {
        let mut best: Option<(HexCoord, f32)> = None;
        for r in 0..height as i32 {
            for q in 0..width as i32 {
                let c = self.hex_center(q, r);
                let d = (c - screen_pos).length();
                if d <= self.hex_size * 0.95 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((HexCoord { q, r }, d));
                }
            }
        }
        best.map(|(coord, _)| coord)
    }

    /// First system whose hex center is within picking radius of `screen_pos`.
    #[must_use]
    pub fn hit_system(&self, sector: &GeneratedSector, screen_pos: Pos2) -> Option<SystemId> {
        sector
            .systems
            .iter()
            .find(|s| {
                (self.hex_center(s.coord.q, s.coord.r) - screen_pos).length()
                    <= self.hex_size * 0.95
            })
            .map(|s| s.id.clone())
    }

    /// §CTX1 Phase 5 — route hit-test. Picks the route whose poly-line passes
    /// closest to `screen_pos` within `hex_size * 0.18`. Ties on segment-distance
    /// resolve to the lexicographically smaller `RouteId` (matches §10 edge-case
    /// rule for overlapping routes). Routes whose endpoints don't both resolve
    /// in `sector.systems` are skipped.
    #[must_use]
    pub fn hit_route(&self, sector: &GeneratedSector, screen_pos: Pos2) -> Option<RouteId> {
        let threshold = self.hex_size * 0.18;
        let coord_of = |id: &str| -> Option<Pos2> {
            sector
                .systems
                .iter()
                .find(|s| s.id.as_str() == id)
                .map(|s| self.hex_center(s.coord.q, s.coord.r))
        };
        let mut best: Option<(RouteId, f32)> = None;
        for route in &sector.routes {
            let (Some(a), Some(b)) = (
                coord_of(route.from_system_id.as_str()),
                coord_of(route.to_system_id.as_str()),
            ) else {
                continue;
            };
            let d = point_segment_distance(screen_pos, a, b);
            if d > threshold {
                continue;
            }
            match &best {
                None => best = Some((route.id.clone(), d)),
                Some((cur_id, cur_d)) => {
                    if d < *cur_d || (d == *cur_d && route.id.as_str() < cur_id.as_str()) {
                        best = Some((route.id.clone(), d));
                    }
                }
            }
        }
        best.map(|(id, _)| id)
    }
}

/// Shortest distance from `p` to the line segment `a`→`b`. Used by
/// [`SectorGeom::hit_route`]; standalone so it can be unit-tested without
/// constructing a sector.
#[must_use]
pub(crate) fn point_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq <= f32::EPSILON {
        return (p - a).length();
    }
    let ap = p - a;
    let t = ((ap.x * ab.x + ap.y * ab.y) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

/// Paint a stroked circle around every system whose id satisfies `include`.
/// Centralised here so builder panels do not have to call `Ui::painter_at`
/// directly (which is on the builder's `disallowed-methods` list).
pub fn paint_system_rings<F>(
    ui: &Ui,
    rect: egui::Rect,
    geom: &SectorGeom,
    sector: &GeneratedSector,
    radius_factor: f32,
    stroke: Stroke,
    include: F,
) where
    F: Fn(&SystemId) -> bool,
{
    let painter = ui.painter_at(rect).with_clip_rect(rect);
    let radius = geom.hex_size * radius_factor;
    for sys in &sector.systems {
        if !include(&sys.id) {
            continue;
        }
        let centre = geom.hex_center(sys.coord.q, sys.coord.r);
        painter.circle_stroke(centre, radius, stroke);
    }
}

fn hex_center_xy(q: i32, r: i32, g: &SectorGeom) -> Pos2 {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = g.margin + horiz_step * (q as f32 + row_shift) + horiz_step / 2.0;
    let y = g.margin + vert_step * r as f32 + g.hex_size;
    Pos2::new(x, y)
}

fn hex_center(q: i32, r: i32, g: &SectorGeom) -> Pos2 {
    hex_center_xy(q, r, g)
}

fn hex_pick(
    screen_pos: Pos2,
    g: &SectorGeom,
    origin: Pos2,
    width: u32,
    height: u32,
) -> Option<HexCoord> {
    // Kept for callers that still pass a separate `origin`; internal uses go
    // through `SectorGeom::pick_hex`.
    let g = SectorGeom {
        hex_size: g.hex_size,
        margin: g.margin,
        origin,
    };
    g.pick_hex(screen_pos, width, height)
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
    if size < 3.0 {
        painter.circle_filled(c, size * 0.9, fill);
        return;
    }
    let pts = hex_vertices(c, size).to_vec();
    painter.add(egui::Shape::convex_polygon(
        pts,
        fill,
        Stroke::new(if size < 8.0 { 0.5 } else { 1.0 }, outline),
    ));
}

fn draw_hex_fill(painter: &egui::Painter, c: Pos2, size: f32, fill: Color32) {
    if size < 3.0 {
        painter.circle_filled(c, size * 0.9, fill);
        return;
    }
    let pts = hex_vertices(c, size).to_vec();
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn draw_system_glyph(
    painter: &egui::Painter,
    c: Pos2,
    radius: f32,
    glyph: MapSystemGlyph,
    fill: Color32,
    theme: &MapTheme,
) {
    match glyph {
        MapSystemGlyph::Star => {
            painter.circle_filled(c, radius, fill);
            painter.circle_stroke(c, radius, Stroke::new(1.5, darken(fill, 0.55)));
        }
        MapSystemGlyph::UncataloguedStar
        | MapSystemGlyph::SpecialLocation
        | MapSystemGlyph::BlackHole
        | MapSystemGlyph::WarpAnomaly
        | MapSystemGlyph::SpaceStation => {
            let r = radius * 0.8;
            let pts = vec![
                Pos2::new(c.x, c.y - r),
                Pos2::new(c.x + r, c.y),
                Pos2::new(c.x, c.y + r),
                Pos2::new(c.x - r, c.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts,
                Color32::TRANSPARENT,
                Stroke::new(1.5, theme.text_dim),
            ));
        }
    }
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = ab.length_sq();
    if len_sq <= f32::EPSILON {
        return p.distance(a);
    }
    let dot = ap.x * ab.x + ap.y * ab.y;
    let t = (dot / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn segment_intersects_rect(a: Pos2, b: Pos2, rect: egui::Rect, margin: f32) -> bool {
    egui::Rect::from_two_pos(a, b)
        .expand(margin)
        .intersects(rect)
}

fn label_intersects_rect(
    name: &str,
    center: Pos2,
    star_r: f32,
    label_size: f32,
    pad: Vec2,
    rect: egui::Rect,
) -> bool {
    let half_w = name.chars().count().max(1) as f32 * label_size * 0.34 + pad.x;
    let top = center.y + star_r + 3.0 - pad.y;
    let height = label_size * 1.35 + pad.y * 2.0;
    egui::Rect::from_min_max(
        Pos2::new(center.x - half_w, top),
        Pos2::new(center.x + half_w, top + height),
    )
    .intersects(rect)
}

fn system_label_font_px(theme: &MapTheme, hex_size: f32) -> Option<f32> {
    // Names must shrink with zoom; a fixed UI-text floor overwhelms huge maps.
    let label_size = hex_size * theme.system_label_font.mul;
    (label_size >= SYSTEM_LABEL_MIN_VISIBLE_PX).then_some(label_size)
}

fn system_pip_metrics(theme: &MapTheme, hex_size: f32) -> Option<(f32, f32)> {
    // Pips follow labels: shrink with zoom, then vanish when unreadable.
    let font_size = hex_size * theme.pip_font.mul;
    let disc_r = hex_size * theme.pip_disc_radius.mul;
    (font_size >= SYSTEM_PIP_MIN_VISIBLE_PX).then_some((font_size, disc_r))
}

fn subsector_label_font_px(theme: &MapTheme, hex_size: f32) -> Option<f32> {
    if hex_size >= 40.0 {
        return None;
    }
    let label_size = hex_size * theme.subsector_label_font.mul;
    (label_size >= SUBSECTOR_LABEL_MIN_VISIBLE_PX).then_some(label_size)
}

fn draw_capital_marker(painter: &egui::Painter, c: Pos2, hex_size: f32, theme: &MapTheme) {
    let r = theme.capital_marker_radius.px(hex_size);
    let cy = c.y - hex_size * 0.55;
    let pts = vec![
        Pos2::new(c.x, cy - r),
        Pos2::new(c.x + r, cy),
        Pos2::new(c.x, cy + r),
        Pos2::new(c.x - r, cy),
    ];
    painter.add(egui::Shape::convex_polygon(
        pts,
        theme.capital_marker_fill,
        Stroke::new(1.2, theme.capital_marker_outline),
    ));
}

/// Blend `from` toward `to` by `t`, clamped to `0..=1`. Used for the heatmap
/// pass so low-intensity cells stay close to the base hex colour and read as
/// "barely there".
fn blend_heat(from: Color32, to: Color32, t: f32) -> Color32 {
    // Apply a floor so any hex with non-zero intensity is visibly tinted, but
    // cap at 0.85 so the brightest cell still keeps a hint of the base panel
    // tone for visual continuity.
    let t = if t > 0.0 {
        (0.20 + t * 0.65).min(0.85)
    } else {
        0.0
    };
    let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

fn draw_region_labels(
    painter: &egui::Painter,
    sector: &GeneratedSector,
    origin: Pos2,
    g: &SectorGeom,
    bounds: egui::Rect,
    cache: Option<&SectorMapCache>,
    theme: &MapTheme,
) {
    if sector.regions.is_empty() {
        return;
    }
    let font = FontId::monospace(theme.region_label_font.px(g.hex_size));
    let pad = Vec2::new(6.0, 3.0);
    for region in sector.regions.iter() {
        let anchor = if let Some(cache) = cache {
            cache.region_centroids.get(region.id.as_str()).map(|&cp| {
                // cp is in axial (q, r) coords, project it
                hex_center(cp.x as i32, cp.y as i32, g) + origin.to_vec2()
            })
        } else {
            region_label_anchor(region, origin, g)
        };

        let Some(anchor) = anchor else {
            continue;
        };

        if !bounds.expand(100.0).contains(anchor) {
            continue;
        }

        let label = region_label_text(&region.name);
        let galley = painter.layout_no_wrap(label, font.clone(), theme.region_label);
        let size = galley.size();
        let min_x = bounds.left() + pad.x;
        let max_x = bounds.right() - size.x - pad.x;
        let min_y = bounds.top() + pad.y;
        let max_y = bounds.bottom() - size.y - pad.y;
        let x = (anchor.x - size.x / 2.0).clamp(min_x, max_x.max(min_x));
        let y = (anchor.y - size.y / 2.0).clamp(min_y, max_y.max(min_y));
        let pos = Pos2::new(x, y);
        let bg = egui::Rect::from_min_size(pos - pad, size + pad * 2.0);
        painter.rect_filled(bg, 3.0, theme.region_label_bg);
        painter.rect_stroke(bg, 3.0, Stroke::new(1.0, theme.region_label_outline));
        painter.galley(pos, galley, theme.region_label);
    }
}

fn region_label_anchor(
    region: &sectorforge::regions::WarpRegion,
    origin: Pos2,
    g: &SectorGeom,
) -> Option<Pos2> {
    if region.hexes.is_empty() {
        return None;
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    for h in &region.hexes {
        let c = hex_center(h.q, h.r, g) + origin.to_vec2();
        sx += c.x;
        sy += c.y;
    }
    let n = region.hexes.len() as f32;
    Some(Pos2::new(sx / n, sy / n))
}

fn region_label_text(name: &str) -> String {
    let up = name.to_ascii_uppercase();
    if up.chars().count() <= 18 {
        up
    } else {
        let mut out: String = up.chars().take(17).collect();
        out.push('.');
        out
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{GeneratedSector, RouteStability, RouteType};

    fn geom() -> SectorGeom {
        SectorGeom::new(20.0, Pos2::ZERO)
    }

    fn sector_with_two_routes() -> GeneratedSector {
        let mut s = GeneratedSector::empty("t", "T", "seed", 8, 8);
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 4, r: 0 }, "B").unwrap();
        let c = s.add_system(HexCoord { q: 0, r: 4 }, "C").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        s.add_route(&a, &c, RouteType::ChartedPassage, RouteStability::Hazardous)
            .unwrap();
        s
    }

    #[test]
    fn hit_route_picks_nearest_segment() {
        let g = geom();
        let sector = sector_with_two_routes();
        let a = g.hex_center(0, 0);
        let b = g.hex_center(4, 0);
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let hit = g.hit_route(&sector, mid).expect("midpoint should hit a-b");
        // Route id is built from sorted endpoints; assert by checking the
        // matching route's endpoints contain coord (0,0) ↔ (4,0).
        let route = sector.routes.iter().find(|r| r.id == hit).unwrap();
        let coords: Vec<(i32, i32)> = sector
            .systems
            .iter()
            .filter(|s| s.id == route.from_system_id || s.id == route.to_system_id)
            .map(|s| (s.coord.q, s.coord.r))
            .collect();
        assert!(coords.contains(&(0, 0)));
        assert!(coords.contains(&(4, 0)));
    }

    #[test]
    fn hit_route_returns_none_outside_threshold() {
        let g = geom();
        let sector = sector_with_two_routes();
        // Far above the horizontal route — well outside hex_size * 0.18 = 3.6 px.
        let a = g.hex_center(0, 0);
        let probe = Pos2::new(a.x + 40.0, a.y - 80.0);
        assert!(g.hit_route(&sector, probe).is_none());
    }

    #[test]
    fn hit_route_resolves_ambiguous_by_route_id_lex() {
        // Two routes that pass through the exact same point: A-B and B-C are
        // collinear when A, B, C share a row. The probe at the shared endpoint
        // is equidistant from both segments (distance 0). The lex-smaller id
        // wins.
        let g = geom();
        let mut s = GeneratedSector::empty("t", "T", "seed", 12, 8);
        let a = s.add_system(HexCoord { q: 0, r: 2 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 4, r: 2 }, "B").unwrap();
        let c = s.add_system(HexCoord { q: 8, r: 2 }, "C").unwrap();
        let r_ab = s
            .add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let r_bc = s
            .add_route(&b, &c, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let probe = g.hex_center(4, 2);
        let hit = g.hit_route(&s, probe).expect("midpoint at B hits a route");
        let expected = if r_ab.as_str() < r_bc.as_str() {
            r_ab.clone()
        } else {
            r_bc.clone()
        };
        assert_eq!(hit, expected);
    }

    #[test]
    fn point_segment_distance_basic() {
        let p = Pos2::new(0.0, 5.0);
        let a = Pos2::new(-10.0, 0.0);
        let b = Pos2::new(10.0, 0.0);
        assert!((point_segment_distance(p, a, b) - 5.0).abs() < 1e-4);
        // Beyond the segment endpoint clamps to the nearer end.
        let q = Pos2::new(20.0, 0.0);
        assert!((point_segment_distance(q, a, b) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn region_for_hex_returns_id_or_none() {
        use sectorforge::regions::RegionConditionKind;
        let mut s = GeneratedSector::empty("t", "T", "seed", 8, 8);
        s.add_region(
            "reg-0001",
            "Sealed Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 2, r: 3 },
        );
        let cache = SectorMapCache::new(&s, &[]);
        assert_eq!(
            cache.region_for_hex(HexCoord { q: 2, r: 3 }),
            Some("reg-0001")
        );
        assert_eq!(cache.region_for_hex(HexCoord { q: 5, r: 5 }), None);
    }
}
