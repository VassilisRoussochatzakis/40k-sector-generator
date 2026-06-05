//! Low-level rendering & geometry primitives for the sector map: the
//! `SectorGeom` hit-test helper, hex math, paint helpers, label layout, and the
//! geometry unit tests. Split verbatim from the former `sector_view.rs`
//! god-file (AREA_F F3). Helpers consumed by `view::SectorView::show` are
//! raised to `pub(super)`; `SectorGeom`/`paint_system_rings`/
//! `point_segment_distance` keep their original public visibility.

use egui::{Color32, FontId, Pos2, Shape, Stroke, Ui, Vec2};

use sectorforge::ids::{RouteId, SystemId};
use sectorforge::sector_model::{GeneratedSector, HexCoord};

use crate::map_theme::RenderMapTheme;
use crate::palette::darken;
use crate::visual_tokens::MapSystemGlyph;

use super::cache::{SectorMapCache, StarDust};

const SYSTEM_LABEL_MIN_VISIBLE_PX: f32 = 3.0;
const SYSTEM_PIP_MIN_VISIBLE_PX: f32 = 3.0;
const SUBSECTOR_LABEL_MIN_VISIBLE_PX: f32 = 3.0;
const REGION_LABEL_MIN_VISIBLE_PX: f32 = 3.0;

/// True when a map theme's background is dark enough for the void flourishes
/// (star dust + vignette). Light themes such as `print_mono` skip them.
pub(super) fn is_dark(bg: Color32) -> bool {
    (u32::from(bg.r()) + u32::from(bg.g()) + u32::from(bg.b())) < 384
}

/// §BEAUTY — deterministic faint star dust across the void, painted *behind* the
/// hex chart so it reads as a star field framing the cartography. Positions are
/// hashed from a stable index (never RNG), so the dust does not shimmer
/// frame-to-frame or move when the sector regenerates. Live-only — the golden
/// PNG/SVG exporters never call this.
pub(super) fn paint_star_dust(
    painter: &egui::Painter,
    rect: egui::Rect,
    cache: Option<&SectorMapCache>,
) {
    // F10: the field is a pure function of `rect`, so memoize the built shapes per
    // rect on the cache and just re-`extend` the painter each frame instead of
    // re-running the hash loop. Without a cache (export-style callers don't reach
    // here anyway) fall back to building once.
    let key = (
        rect.left() as u32,
        rect.top() as u32,
        rect.width() as u32,
        rect.height() as u32,
    );
    match cache {
        Some(cache) => {
            let mut slot = cache.star_dust.borrow_mut();
            if slot.as_ref().map(|s| s.key) != Some(key) {
                *slot = Some(StarDust {
                    key,
                    shapes: build_star_dust(rect),
                });
            }
            if let Some(s) = slot.as_ref() {
                painter.extend(s.shapes.iter().cloned());
            }
        }
        None => painter.extend(build_star_dust(rect)),
    }
}

/// Build the deterministic star-dust shapes for `rect`. Byte-identical to the
/// former inline `painter.circle_filled` loop (same order, positions, colors), so
/// memoizing it touches no golden bytes (F10).
fn build_star_dust(rect: egui::Rect) -> Vec<Shape> {
    // One pseudo-random float in `[0,1)` from a 32-bit integer (an fmix32 finaliser).
    fn hash01(mut x: u32) -> f32 {
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^= x >> 16;
        (x as f32) / (u32::MAX as f32)
    }
    // Density tracks area but is capped so a maximised window stays cheap.
    let n = ((rect.area() / 2200.0) as u32).clamp(70, 540);
    let mut shapes = Vec::with_capacity(n as usize);
    for i in 0..n {
        let fx = hash01(i.wrapping_mul(2_654_435_761).wrapping_add(1));
        let fy = hash01(i.wrapping_mul(40_503).wrapping_add(7));
        let pos = Pos2::new(
            rect.left() + fx * rect.width(),
            rect.top() + fy * rect.height(),
        );
        let roll = hash01(i.wrapping_mul(2_246_822_519).wrapping_add(13));
        if roll > 0.91 {
            // A handful of brighter "stars" with a faint halo.
            let a = 80 + (hash01(i.wrapping_mul(97).wrapping_add(5)) * 60.0) as u8;
            shapes.push(Shape::circle_filled(
                pos,
                1.4,
                Color32::from_rgba_unmultiplied(234, 228, 238, a),
            ));
            shapes.push(Shape::circle_filled(
                pos,
                3.0,
                Color32::from_rgba_unmultiplied(234, 228, 238, a / 5),
            ));
        } else {
            // Dim dust — the bulk of the field.
            let a = 22 + (roll * 58.0) as u8;
            let s = 0.6 + roll * 0.6;
            shapes.push(Shape::circle_filled(
                pos,
                s,
                Color32::from_rgba_unmultiplied(222, 216, 230, a),
            ));
        }
    }
    shapes
}

/// §BEAUTY — a soft radial vignette: a triangle fan from a transparent centre to
/// darker corners, so the void deepens toward the frame like a chart under glass.
/// Subtle (corner alpha ~66). Live-only.
pub(super) fn paint_vignette(painter: &egui::Painter, rect: egui::Rect) {
    let center = rect.center();
    let clear = Color32::from_black_alpha(0);
    let corner = Color32::from_black_alpha(80);
    let edge = Color32::from_black_alpha(24);
    // 8 perimeter points: 4 corners (darker) + 4 edge midpoints (lighter). Their
    // straight edges lie on the rect boundary, so the fan covers it exactly.
    let ring = [
        (rect.left_top(), corner),
        (Pos2::new(center.x, rect.top()), edge),
        (rect.right_top(), corner),
        (Pos2::new(rect.right(), center.y), edge),
        (rect.right_bottom(), corner),
        (Pos2::new(center.x, rect.bottom()), edge),
        (rect.left_bottom(), corner),
        (Pos2::new(rect.left(), center.y), edge),
    ];
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, clear); // index 0
    for (p, c) in ring {
        mesh.colored_vertex(p, c);
    }
    let n = ring.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// §BEAUTY — the gilded cartographic frame: a hairline brass border just inside
/// the canvas plus corner brackets and rivets, so the live map reads as an
/// Imperial instrument rather than a bare rectangle. `accent` is the active
/// chrome accent, so the frame recolors with the theme. Live-only.
pub(super) fn paint_chart_frame(painter: &egui::Painter, rect: egui::Rect, accent: Color32) {
    let dim = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 70);
    let bright = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 200);
    // Inset hairline border.
    painter.rect_stroke(rect.shrink(1.5), 2.0, Stroke::new(1.0, dim));
    // Corner brackets + rivets — short L-marks at each corner, brass.
    let r = rect.shrink(6.0);
    let len = 16.0;
    let stroke = Stroke::new(1.5, bright);
    let corners = [
        (r.left_top(), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)),
        (r.right_top(), Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
        (r.right_bottom(), Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0)),
        (r.left_bottom(), Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0)),
    ];
    for (corner, dx, dy) in corners {
        painter.line_segment([corner, corner + dx * len], stroke);
        painter.line_segment([corner, corner + dy * len], stroke);
        painter.circle_filled(corner, 1.8, bright);
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

pub(super) fn hex_center(q: i32, r: i32, g: &SectorGeom) -> Pos2 {
    hex_center_xy(q, r, g)
}

pub(super) fn hex_pick(
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

pub(super) fn hex_vertices(c: Pos2, size: f32) -> [Pos2; 6] {
    let mut out = [Pos2::ZERO; 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 30.0);
        *slot = Pos2::new(c.x + size * angle.cos(), c.y + size * angle.sin());
    }
    out
}

pub(super) fn draw_hex(painter: &egui::Painter, c: Pos2, size: f32, fill: Color32, outline: Color32) {
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

pub(super) fn draw_hex_fill(painter: &egui::Painter, c: Pos2, size: f32, fill: Color32) {
    if size < 3.0 {
        painter.circle_filled(c, size * 0.9, fill);
        return;
    }
    let pts = hex_vertices(c, size).to_vec();
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
}

pub(super) fn draw_system_glyph(
    painter: &egui::Painter,
    c: Pos2,
    radius: f32,
    glyph: MapSystemGlyph,
    fill: Color32,
    theme: &RenderMapTheme,
) {
    match glyph {
        MapSystemGlyph::Star => {
            painter.circle_filled(c, radius, fill);
            painter.circle_stroke(c, radius, Stroke::new(1.5, darken(fill, 0.55)));
        }
        MapSystemGlyph::UncataloguedStar | MapSystemGlyph::SpecialLocation => {
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
        MapSystemGlyph::BlackHole => {
            painter.circle_filled(c, radius * 0.55, Color32::BLACK);
            painter.circle_stroke(c, radius * 0.9, Stroke::new(2.0, theme.text_dim));
            painter.circle_stroke(
                c,
                radius * 1.1,
                Stroke::new(1.0, darken(theme.text_dim, 0.4)),
            );
        }
        MapSystemGlyph::WarpAnomaly => {
            let r = radius * 0.95;
            let h = r * 0.866;
            let stroke = Stroke::new(1.3, theme.text_dim);
            let up = vec![
                Pos2::new(c.x, c.y - r),
                Pos2::new(c.x + h, c.y + r * 0.5),
                Pos2::new(c.x - h, c.y + r * 0.5),
            ];
            let dn = vec![
                Pos2::new(c.x, c.y + r),
                Pos2::new(c.x + h, c.y - r * 0.5),
                Pos2::new(c.x - h, c.y - r * 0.5),
            ];
            painter.add(egui::Shape::convex_polygon(
                up,
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.add(egui::Shape::convex_polygon(
                dn,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        MapSystemGlyph::SpaceStation => {
            let r = radius * 0.65;
            let rect = egui::Rect::from_center_size(c, Vec2::splat(r * 2.0));
            let stroke = Stroke::new(1.5, theme.text_dim);
            painter.rect_stroke(rect, 0.0, stroke);
            let arm = r * 1.3;
            painter.line_segment(
                [Pos2::new(c.x - arm, c.y), Pos2::new(c.x + arm, c.y)],
                Stroke::new(1.0, theme.text_dim),
            );
            painter.line_segment(
                [Pos2::new(c.x, c.y - arm), Pos2::new(c.x, c.y + arm)],
                Stroke::new(1.0, theme.text_dim),
            );
        }
    }
}

pub(super) fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
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

pub(super) fn segment_intersects_rect(a: Pos2, b: Pos2, rect: egui::Rect, margin: f32) -> bool {
    egui::Rect::from_two_pos(a, b)
        .expand(margin)
        .intersects(rect)
}

pub(super) fn label_intersects_rect(
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

pub(super) fn system_label_font_px(theme: &RenderMapTheme, hex_size: f32) -> Option<f32> {
    // Names must shrink with zoom; a fixed UI-text floor overwhelms huge maps.
    let label_size = hex_size * theme.system_label_font.mul;
    (label_size >= SYSTEM_LABEL_MIN_VISIBLE_PX).then_some(label_size)
}

pub(super) fn system_pip_metrics(theme: &RenderMapTheme, hex_size: f32) -> Option<(f32, f32)> {
    // Pips follow labels: shrink with zoom, then vanish when unreadable.
    let font_size = hex_size * theme.pip_font.mul;
    let disc_r = hex_size * theme.pip_disc_radius.mul;
    (font_size >= SYSTEM_PIP_MIN_VISIBLE_PX).then_some((font_size, disc_r))
}

fn region_label_font_px(theme: &RenderMapTheme, hex_size: f32) -> Option<f32> {
    // Match system labels: scale linearly with zoom, then vanish when too
    // small to read — never floor at a fixed px. A floor makes the chip
    // balloon relative to the shrinking map as the user zooms out.
    let label_size = hex_size * theme.region_label_font.mul;
    (label_size >= REGION_LABEL_MIN_VISIBLE_PX).then_some(label_size)
}

pub(super) fn subsector_label_font_px(theme: &RenderMapTheme, hex_size: f32) -> Option<f32> {
    if hex_size >= 40.0 {
        return None;
    }
    let label_size = hex_size * theme.subsector_label_font.mul;
    (label_size >= SUBSECTOR_LABEL_MIN_VISIBLE_PX).then_some(label_size)
}

pub(super) fn draw_capital_marker(painter: &egui::Painter, c: Pos2, hex_size: f32, theme: &RenderMapTheme) {
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
pub(super) fn blend_heat(from: Color32, to: Color32, t: f32) -> Color32 {
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

pub(super) fn draw_region_labels(
    painter: &egui::Painter,
    sector: &GeneratedSector,
    origin: Pos2,
    g: &SectorGeom,
    bounds: egui::Rect,
    cache: Option<&SectorMapCache>,
    theme: &RenderMapTheme,
) {
    if sector.regions.is_empty() {
        return;
    }
    let Some(font_px) = region_label_font_px(theme, g.hex_size) else {
        return;
    };
    let font = FontId::monospace(font_px);
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

pub(super) fn draw_hex_outline_only(
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
