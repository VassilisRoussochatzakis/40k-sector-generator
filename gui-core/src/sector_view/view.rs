//! The `SectorView` widget (struct + `SectorClick`) and its `show()` render
//! method. Split verbatim from the former `sector_view.rs` god-file (AREA_F
//! F3). F-S3: a `SectorView::new(sector)` constructor + `..new(sector)`
//! struct-update at the call sites means new fields no longer cascade into every
//! caller. The former monolithic `show()` body is decomposed into one
//! `pub(super)`/private helper per paint phase (Finding #13); all helpers share a
//! [`RenderCtx`] borrow-struct so the per-frame refs are threaded without
//! reborrowing or cloning. The decomposition is purely structural — paint order,
//! every computed value, and the produced pixels are byte-identical (the
//! `map_snapshots` golden suite stays green unblessed).

use std::collections::{BTreeSet, HashMap, HashSet};

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{self, GeneratedSector, HexCoord};
use sectorforge::subsectors::Subsector;

use crate::design;
use crate::heatmap::HeatCell;
use crate::map_theme::RenderMapTheme;
use crate::palette::{darken, draw_route_control_glyph, draw_route_line, star_color};
use crate::visual_tokens::{MapRegionOverlay, MapRouteVisual, MapSystemGlyph};

use super::cache::SectorMapCache;
use super::render::{
    blend_heat, distance_to_segment, draw_capital_marker, draw_hex, draw_hex_fill,
    draw_hex_outline_only, draw_region_labels, draw_system_glyph, hex_center, hex_pick,
    hex_vertices, is_dark, label_intersects_rect, paint_chart_frame, paint_star_dust,
    paint_vignette, segment_intersects_rect, subsector_label_font_px, system_label_font_px,
    system_pip_metrics, SectorGeom,
};

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
    /// [`RenderMapTheme::default`] so existing call-sites compile unchanged.
    pub theme: Option<&'a RenderMapTheme>,
    /// When true, paint a small chip to the left of the cursor showing the
    /// (q, r) coords of the hovered hex. Off for snapshot tests.
    pub show_hover_coord: bool,
}

#[non_exhaustive]
pub enum SectorClick {
    System(sectorforge::ids::SystemId),
    Route(sectorforge::ids::RouteId),
    Subsector(String),
    Region(String),
    EmptyHex(HexCoord),
}

/// Per-frame render state shared by every `show()` phase helper. Bundles the
/// painter, theme, geometry, viewport rects, the integer cull bounds, and the
/// derived route/star scalars so each helper takes one `&RenderCtx` instead of a
/// dozen positional params. All fields are borrows or `Copy` scalars — no
/// per-frame allocation or clone is introduced versus the former inline body.
struct RenderCtx<'p> {
    painter: &'p egui::Painter,
    theme: &'p RenderMapTheme,
    g: SectorGeom,
    origin: Pos2,
    /// Visible viewport ∩ content rect — everything culls against this. (The
    /// full content `rect` is used only by `show` itself — bg fill, star dust,
    /// vignette, chart frame — so it is not carried on the ctx.)
    cull: Rect,
    /// Integer hex-coord bounds of the visible viewport (pre-clamp; helpers clamp
    /// to `[0, width|height)` exactly as the inline body did).
    min_q: i32,
    max_q: i32,
    min_r: i32,
    max_r: i32,
    route_thickness: f32,
    star_r: f32,
    route_cull_margin: f32,
}

/// Trim a route segment back from each endpoint by the star radius so the line
/// stops at the disc edge, returning `None` when the systems are too close to
/// draw between. Extracted verbatim from the former inline `shorten` closure;
/// `star_r` was its only capture.
fn shorten_segment(a: Pos2, b: Pos2, star_r: f32) -> Option<(Pos2, Pos2)> {
    let delta = b - a;
    let len = delta.length();
    if len <= star_r * 2.0 {
        return None;
    }
    let dir = delta / len;
    Some((a + dir * star_r, b - dir * star_r))
}

impl<'a> SectorView<'a> {
    /// A `SectorView` over `sector` with every other field at its neutral default
    /// (no selection/overlays/path, `hex_size` 40, `origin` zero, hover-only sense,
    /// internal click dispatch on). Call sites override only what they need via
    /// struct-update — `SectorView { hex_size, cache, ..SectorView::new(sector) }`
    /// — so adding a field no longer forces an edit in every caller (F-S3).
    #[must_use]
    pub fn new(sector: &'a GeneratedSector) -> Self {
        Self {
            sector,
            selected_system: None,
            selected_route: None,
            hex_size: 40.0,
            path_route_ids: None,
            path_waypoints: None,
            subsectors: None,
            cache: None,
            selected_subsector: None,
            heatmap: None,
            empty_hex_clicks: false,
            route_view_mode: sector_model::RouteViewMode::default(),
            origin: Pos2::ZERO,
            multi_selected: None,
            pinned: None,
            drag_override: None,
            pending_route_preview: None,
            rect_select: None,
            sense: Sense::hover(),
            disable_internal_click_dispatch: false,
            theme: None,
            show_hover_coord: false,
        }
    }

    pub fn show(self, ui: &mut Ui) -> (Response, Option<SectorClick>) {
        let g = SectorGeom::new(self.hex_size, self.origin);
        let (rect, response) = ui.allocate_at_least(ui.available_size(), self.sense);
        // Cull against the visible viewport, not the full content rect. Inside a
        // ScrollArea `ui.clip_rect()` is the scroll viewport (egui clones the
        // parent painter's clip into child uis), so this confines the per-hex,
        // per-route, and per-label work to what is actually on screen. Callers
        // that are not scrolled (clip ⊇ content) get `cull == rect`, leaving
        // their output unchanged.
        let cull = ui.clip_rect().intersect(rect);
        let painter = ui.painter_at(rect).with_clip_rect(rect);
        let origin = self.origin;

        let default_theme = RenderMapTheme::default();
        let theme: &RenderMapTheme = self.theme.unwrap_or(&default_theme);

        painter.rect_filled(rect, 0.0, theme.bg);

        // §BEAUTY — live-only void flourishes on the egui `RenderMapTheme` path.
        // The byte-stable PNG/SVG exporters never reach here (separate `MapTheme`
        // + canvas), so these touch no golden bytes. Faint deterministic star dust
        // sits *behind* the chart so the void reads as a star field; it is skipped
        // on light map themes (`print_mono`) and on tiny embedded previews.
        let accent = design::accent(ui);
        let dark_map = is_dark(theme.bg);
        let framed = rect.width() > 260.0 && rect.height() > 200.0;
        if framed && dark_map {
            paint_star_dust(&painter, rect, self.cache);
        }

        let horiz_step = g.hex_size * 3f32.sqrt();
        let vert_step = g.hex_size * 1.5;

        // Performance: only iterate hexes visible in the current viewport
        let min_r =
            ((cull.min.y - origin.y - g.margin - g.hex_size * 2.0) / vert_step).floor() as i32;
        let max_r = ((cull.max.y - origin.y - g.margin + g.hex_size) / vert_step).ceil() as i32;

        let min_q =
            ((cull.min.x - origin.x - g.margin - horiz_step * 1.5) / horiz_step).floor() as i32;
        let max_q = ((cull.max.x - origin.x - g.margin + horiz_step) / horiz_step).ceil() as i32;

        let route_thickness = theme.route_thickness.px(g.hex_size);
        let star_r = g.hex_size * theme.star_radius_mul;
        let route_cull_margin = (route_thickness * 6.0).max(12.0);

        let ctx = RenderCtx {
            painter: &painter,
            theme,
            g,
            origin,
            cull,
            min_q,
            max_q,
            min_r,
            max_r,
            route_thickness,
            star_r,
            route_cull_margin,
        };

        // --- Paint passes, in strict z-order (back to front). ---
        self.paint_hex_fills(&ctx);
        self.paint_rect_select_tint(&ctx);
        self.paint_selected_subsector_wash(&ctx);
        self.paint_subsector_borders(&ctx);

        let centers = self.build_system_centers(&ctx);

        self.paint_routes(&ctx, &centers);
        self.paint_pending_route_preview(&ctx, &centers);
        self.paint_path_route_glow(&ctx, &centers);
        self.paint_selected_route(&ctx, &centers);

        draw_region_labels(&painter, self.sector, origin, &g, cull, self.cache, theme);

        self.paint_systems(&ctx, &centers);
        self.place_subsector_labels(&ctx, &centers);
        self.paint_system_labels(&ctx, &centers);

        // §BEAUTY — vignette + gilded chart frame, above the map content but below
        // the hover-coord tooltip. Live-only (`RenderMapTheme` path; see above).
        if framed {
            if dark_map {
                paint_vignette(&painter, rect);
            }
            paint_chart_frame(&painter, rect, accent);
        }

        self.paint_hover_coord(&ctx, &response);

        let click = self.dispatch_click(&ctx, &centers, &response);

        (response, click)
    }

    /// Pass 0: the base hex grid — heatmap-blended empty fill, region tint, and
    /// outline for every cell visible in the viewport.
    fn paint_hex_fills(&self, ctx: &RenderCtx<'_>) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            min_q,
            max_q,
            min_r,
            max_r,
            ..
        } = *ctx;

        // System-id keyed hex coords for heatmap lookup (still needed if cache not present)
        let mut hex_system_fallback: HashMap<(i32, i32), &str> = HashMap::new();
        if self.cache.is_none() {
            for sys in &self.sector.systems {
                hex_system_fallback.insert((sys.coord.q, sys.coord.r), sys.id.as_str());
            }
        }

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
                        Some((_, kind)) => {
                            blend_heat(base, theme.region_color(*kind), theme.region_tint_strength)
                        }
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
                                theme.region_tint_strength,
                            );
                            break;
                        }
                    }
                    f
                };

                draw_hex(painter, c, g.hex_size, fill, theme.hex_outline);
            }
        }
    }

    /// Rect-select tint (builder §S4). Painted before subsector overlays so
    /// it stays visually quiet underneath the route + system layers.
    fn paint_rect_select_tint(&self, ctx: &RenderCtx<'_>) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            min_q,
            max_q,
            min_r,
            max_r,
            ..
        } = *ctx;
        if let Some((a, b)) = self.rect_select {
            let (sq, eq) = (a.q.min(b.q), a.q.max(b.q));
            let (sr, er) = (a.r.min(b.r), a.r.max(b.r));
            for r in sr.max(min_r.max(0))..=er.min(max_r.min(self.sector.height as i32 - 1)) {
                for q in sq.max(min_q.max(0))..=eq.min(max_q.min(self.sector.width as i32 - 1)) {
                    let c = hex_center(q, r, &g) + origin.to_vec2();
                    draw_hex_fill(painter, c, g.hex_size, theme.rect_select_tint);
                }
            }
        }
    }

    /// Selected subsector: faint grey wash over every hex in the cluster.
    fn paint_selected_subsector_wash(&self, ctx: &RenderCtx<'_>) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            cull,
            ..
        } = *ctx;
        if let Some(sel) = self.selected_subsector {
            if let Some(cache) = self.cache {
                for (&(q, r), sid) in &cache.hex_subsector {
                    if sid == sel {
                        let c = hex_center(q, r, &g) + origin.to_vec2();
                        if cull.expand(g.hex_size).contains(c) {
                            draw_hex_fill(painter, c, g.hex_size, theme.subsector_highlight);
                        }
                    }
                }
            } else {
                // Fallback (slow)
                if let Some(subs) = self.subsectors {
                    if let Some(s) = subs.iter().find(|s| s.id.as_ref() == sel) {
                        for &(q, r) in &s.hex_cells {
                            let c = hex_center(q as i32, r as i32, &g) + origin.to_vec2();
                            if cull.expand(g.hex_size).contains(c) {
                                draw_hex_fill(painter, c, g.hex_size, theme.subsector_highlight);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Subsector tile borders: always visible, solid lines.
    fn paint_subsector_borders(&self, ctx: &RenderCtx<'_>) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            min_q,
            max_q,
            min_r,
            max_r,
            ..
        } = *ctx;
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
    }

    /// Screen-space center of every system, keyed by id, honouring the active
    /// drag override. Built once per frame and shared by the route, system,
    /// label, and click-dispatch phases.
    fn build_system_centers(&self, ctx: &RenderCtx<'_>) -> HashMap<&'a str, Pos2> {
        let RenderCtx { g, origin, .. } = *ctx;
        // Copy the `&'a GeneratedSector` ref out of `self` so the id `&str` keys
        // borrow for `'a` (the sector's lifetime), not the shorter `&self`
        // borrow — the returned map outlives this method's `&self`.
        let sector: &'a GeneratedSector = self.sector;
        let mut centers: HashMap<&str, Pos2> = HashMap::with_capacity(sector.systems.len());
        for sys in &sector.systems {
            let mut c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
            if let Some((drag_id, drag_pos)) = self.drag_override.as_ref() {
                if drag_id.as_str() == sys.id.as_str() {
                    c = *drag_pos;
                }
            }
            centers.insert(sys.id.as_str(), c);
        }
        centers
    }

    /// Base route layer: stability-coloured, type-patterned lines plus the
    /// faction control glyph, for every route both endpoints of which resolve.
    fn paint_routes(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            cull,
            route_thickness,
            star_r,
            route_cull_margin,
            ..
        } = *ctx;
        for route in &self.sector.routes {
            let (Some(&a), Some(&b)) = (
                centers.get(route.from_system_id.as_str()),
                centers.get(route.to_system_id.as_str()),
            ) else {
                continue;
            };

            let Some((a2, b2)) = shorten_segment(a, b, star_r) else {
                continue;
            };
            if !segment_intersects_rect(a2, b2, cull, route_cull_margin) {
                continue;
            }
            draw_route_line(
                painter,
                a2,
                b2,
                route_thickness,
                theme.route_color(route.stability),
                MapRouteVisual::from_route_type(route.route_type, self.route_view_mode).pattern(),
            );
            draw_route_control_glyph(
                painter,
                &self.sector.factions,
                route,
                a2,
                b2,
                route_thickness,
            );
        }
    }

    /// Builder ADD-ROUTE preview: dashed amber line from the picked start
    /// system to the cursor (or to nothing if the start is off-screen).
    fn paint_pending_route_preview(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            route_thickness,
            ..
        } = *ctx;
        if let Some((start_id, cursor)) = self.pending_route_preview.as_ref() {
            if let Some(&a) = centers.get(start_id.as_str()) {
                let preview_thickness = (route_thickness * theme.pending_route_preview_mul)
                    .max(theme.pending_route_preview_min);
                draw_route_line(
                    painter,
                    a,
                    *cursor,
                    preview_thickness,
                    theme.pending_route_preview,
                    sectorforge::sector_model::RoutePattern::Dashed,
                );
            }
        }
    }

    /// Planned-path glow: a wide translucent under-stroke plus a bright core for
    /// each route id on the active path.
    fn paint_path_route_glow(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            cull,
            route_thickness,
            star_r,
            route_cull_margin,
            ..
        } = *ctx;
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
                let Some((a2, b2)) = shorten_segment(a, b, star_r) else {
                    continue;
                };
                if !segment_intersects_rect(a2, b2, cull, route_cull_margin) {
                    continue;
                }
                painter.line_segment([a2, b2], Stroke::new(glow_thick, glow));
                painter.line_segment([a2, b2], Stroke::new(core_thick, theme.path_highlight));
            }
        }
    }

    /// Selected-route highlight: glow under-stroke + a recoloured, type-patterned
    /// core for the one selected route.
    fn paint_selected_route(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            cull,
            route_thickness,
            star_r,
            route_cull_margin,
            ..
        } = *ctx;
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
                let Some((a2, b2)) = shorten_segment(a, b, star_r) else {
                    continue;
                };
                if !segment_intersects_rect(a2, b2, cull, route_cull_margin) {
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
                    painter,
                    a2,
                    b2,
                    route_thickness * theme.selection_core_mul,
                    theme.selection,
                    MapRouteVisual::from_route_type(route.route_type, self.route_view_mode)
                        .pattern(),
                );
            }
        }
    }

    /// Pass 1: per-system selection/pin/waypoint rings, the star/object glyph,
    /// the subsector capital diamond, and the world-count pip.
    fn paint_systems(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            g,
            cull,
            star_r,
            ..
        } = *ctx;
        for sys in &self.sector.systems {
            let c = centers[sys.id.as_str()];
            if !cull.expand(g.hex_size).contains(c) {
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
                    painter,
                    c,
                    g.hex_size + theme.pinned_ring_bump,
                    theme.pinned_outline,
                    theme.pinned_ring_thickness,
                );
            }
            if is_sel {
                draw_hex_outline_only(
                    painter,
                    c,
                    g.hex_size + theme.selection_ring_bump,
                    theme.selection,
                    theme.selection_ring_thickness,
                );
            } else if is_multi {
                draw_hex_outline_only(
                    painter,
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
                    painter,
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
                painter,
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
                    draw_capital_marker(painter, c, g.hex_size, theme);
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
    }

    /// Subsector labels: name chip placed inside the subsector, avoiding
    /// system markers, system name labels, other subsector labels, and the
    /// map edges. Uses cluster name ("Subsector Aurelia" -> "AURELIA") rather
    /// than the spreadsheet letter so the map reads politically. Skip
    /// clusters with no member systems so empty frontier regions stay
    /// uncluttered. Hide if zoomed in too far.
    fn place_subsector_labels(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            cull,
            min_q,
            max_q,
            min_r,
            max_r,
            star_r,
            ..
        } = *ctx;
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
                    if !cull.expand(g.hex_size * 2.0).contains(c) {
                        continue;
                    }
                    obstacles.push(egui::Rect::from_min_max(
                        Pos2::new(c.x - hex_half_w, c.y - g.hex_size),
                        Pos2::new(c.x + hex_half_w, c.y + g.hex_size),
                    ));
                    if let Some(sys_font) = &sys_font {
                        // TF-P-3: reuse the prebuilt ASCII-upper label instead of
                        // re-`to_ascii_uppercase`-ing every system every frame.
                        let name = self
                            .cache
                            .and_then(|mc| mc.system_label(&sys.id))
                            .map(|s| s.as_ref().to_owned())
                            .unwrap_or_else(|| sys.name.to_ascii_uppercase());
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
                    // Skip clusters whose hex bounds fall entirely outside the
                    // viewport — their label can't land on screen anyway, and
                    // building the candidate-cell list per cluster is the bulk
                    // of this block's per-frame cost on large sectors.
                    if (s.bounds.q_max as i32) < min_q
                        || (s.bounds.q_min as i32) > max_q
                        || (s.bounds.r_max as i32) < min_r
                        || (s.bounds.r_min as i32) > max_r
                    {
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
                        if !cull.contains_rect(bg) {
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
                    let (block_min_x, block_top_y) = match chosen {
                        Some(p) => p,
                        None => {
                            let Some(&(q0, r0)) =
                                s.hex_cells.iter().min_by(|&&(q1, r1), &&(q2, r2)| {
                                    let d1 = (q1 as f32 - aq).powi(2) + (r1 as f32 - ar).powi(2);
                                    let d2 = (q2 as f32 - aq).powi(2) + (r2 as f32 - ar).powi(2);
                                    d1.total_cmp(&d2)
                                })
                            else {
                                continue;
                            };
                            let anchor = hex_center(q0 as i32, r0 as i32, &g) + origin.to_vec2();
                            let bt = anchor.y - g.hex_size - block_h - 2.0;
                            let bmx = (anchor.x - block_w / 2.0)
                                .max(cull.left() + pad.x)
                                .min(cull.right() - block_w - pad.x);
                            let bty = bt
                                .max(cull.top() + pad.y)
                                .min(cull.bottom() - block_h - pad.y);
                            (bmx, bty)
                        }
                    };

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
    }

    /// Pass 2: system name labels last, always on top of every hex.
    fn paint_system_labels(&self, ctx: &RenderCtx<'_>, centers: &HashMap<&str, Pos2>) {
        let RenderCtx {
            painter,
            theme,
            cull,
            star_r,
            ..
        } = *ctx;
        if let Some(label_size) = system_label_font_px(theme, ctx.g.hex_size) {
            let font = FontId::monospace(label_size);
            let pad = Vec2::new(3.0, 1.0);
            for sys in &self.sector.systems {
                let c = centers[sys.id.as_str()];
                if !label_intersects_rect(sys.name.as_ref(), c, star_r, label_size, pad, cull) {
                    continue;
                }

                // Pill background behind label so it stays readable when an
                // adjacent row's hex tip pokes through.
                // TF-P-3: prebuilt ASCII-upper label (see pass 1 above).
                let label = self
                    .cache
                    .and_then(|mc| mc.system_label(&sys.id))
                    .map(|s| s.as_ref().to_owned())
                    .unwrap_or_else(|| sys.name.to_ascii_uppercase());
                let galley = painter.layout_no_wrap(label, font.clone(), theme.text_dim);
                let pos = Pos2::new(c.x - galley.size().x / 2.0, c.y + star_r + 3.0);
                let bg_rect = egui::Rect::from_min_size(pos - pad, galley.size() + pad * 2.0);
                painter.rect_filled(bg_rect, 2.0, theme.bg);
                painter.galley(pos, galley, theme.text_dim);
            }
        }
    }

    /// Hover-coord chip: paints the `(q, r)` of the hovered hex to the side of
    /// the cursor. Above the map but below nothing else; off for snapshot tests.
    fn paint_hover_coord(&self, ctx: &RenderCtx<'_>, response: &Response) {
        let RenderCtx {
            painter,
            theme,
            g,
            origin,
            cull,
            ..
        } = *ctx;
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
                    if anchor.x < cull.left() + 2.0 {
                        anchor.x = pos.x + 14.0;
                    }
                    anchor.y = anchor
                        .y
                        .clamp(cull.top() + 2.0, cull.bottom() - size.y - 2.0);
                    let bg = egui::Rect::from_min_size(anchor, size);
                    painter.rect_filled(bg, 2.0, theme.bg);
                    painter.rect_stroke(bg, 2.0, Stroke::new(1.0, theme.hex_outline));
                    painter.galley(anchor + pad, galley, theme.text);
                }
            }
        }
    }

    /// Pointer hit-test + click dispatch: system (nearest within 0.95·hex) →
    /// route (nearest within the route hit radius) → empty hex / region /
    /// subsector. Returns `None` when dispatch is disabled or nothing was hit.
    fn dispatch_click(
        &self,
        ctx: &RenderCtx<'_>,
        centers: &HashMap<&str, Pos2>,
        response: &Response,
    ) -> Option<SectorClick> {
        let RenderCtx {
            g,
            origin,
            min_q,
            max_q,
            min_r,
            max_r,
            route_thickness,
            star_r,
            ..
        } = *ctx;
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
                        let (a2, b2) = shorten_segment(a, b, star_r)?;
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
        click
    }
}
