//! Route lines: control glyphs, pattern motifs, parallel rails, bursts.

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::faction_style::faction_style_rgb_by_id;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedSector, RoutePattern};

use super::colors::{darken, dim_rgba, route_thickness, stability_color, stroke_px};
use super::geom::{hex_center, Geom};
use super::primitives::{draw_circle, draw_line_thick, draw_rect_outline, fill_circle, fill_rect};
use super::RenderOptions;

pub(super) fn draw_routes(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    g: &Geom,
    opts: &RenderOptions,
) {
    let mut centers: HashMap<&str, (i32, i32)> = HashMap::new();
    for sys in sector.systems.iter() {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        centers.insert(sys.id.as_str(), (cx, cy));
    }
    let star_r = g.hex_size * star_radius_ratio();
    for route in &sector.routes {
        let (Some(&a), Some(&b)) = (
            centers.get(route.from_system_id.as_str()),
            centers.get(route.to_system_id.as_str()),
        ) else {
            continue;
        };
        let Some(((sx, sy), (ex, ey))) = shorten_to_star(a, b, star_r) else {
            continue;
        };
        let color = stability_color(&opts.theme, route.stability);
        let thickness = route_thickness(&opts.theme, route.stability, g);
        draw_route_line_thick(RouteLineParams {
            img,
            x0: sx,
            y0: sy,
            x1: ex,
            y1: ey,
            color,
            thickness,
            pattern: route.pattern_with_salt(&sector.seed, opts.route_view_mode),
        });
        draw_route_control_glyph(
            img,
            sector,
            route,
            (sx, sy),
            (ex, ey),
            thickness,
            &opts.theme,
        );
    }
}

/// §3: at the midpoint of a route, draw a symbol for the single strongest
/// `RouteControl` category (patrol / toll / interdiction / piracy) when its
/// score is >= 40. Colour comes from the controlling faction's
/// `FactionStyle.fill` so the reader can identify who is asserting along the
/// route.
fn draw_route_control_glyph(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    route: &crate::sector_model::GeneratedRoute,
    a: (i32, i32),
    b: (i32, i32),
    thickness: i32,
    theme: &MapTheme,
) {
    let Some((faction_id, kind, score)) = top_route_control(route) else {
        return;
    };
    if score < 40.0 {
        return;
    }
    let style = faction_style_rgb_by_id(&sector.factions, &faction_id);
    let color = if matches!(theme.symbol_set, SymbolSet::Redacted) {
        theme.route_control_neutral
    } else {
        Rgba([style.fill.0, style.fill.1, style.fill.2, 255])
    };
    let dark = darken(color, 0.5);
    let mx = (a.0 + b.0) / 2;
    let my = (a.1 + b.1) / 2;
    let size = (thickness * 3).max(6);
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        let half = size;
        draw_line_thick(
            img,
            mx - half,
            my - half,
            mx + half,
            my + half,
            color,
            thickness.max(2),
        );
        draw_line_thick(
            img,
            mx - half,
            my + half,
            mx + half,
            my - half,
            color,
            thickness.max(2),
        );
        return;
    }
    match kind {
        ControlKind::Interdiction => {
            // Crossbar perpendicular to the line.
            let dx = (b.0 - a.0) as f32;
            let dy = (b.1 - a.1) as f32;
            let len = dx.hypot(dy).max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size as f32;
            let x0 = (mx as f32 - px * half) as i32;
            let y0 = (my as f32 - py * half) as i32;
            let x1 = (mx as f32 + px * half) as i32;
            let y1 = (my as f32 + py * half) as i32;
            draw_line_thick(img, x0, y0, x1, y1, color, thickness.max(2));
            draw_line_thick(img, x0, y0, x1, y1, dark, 1);
        }
        ControlKind::Patrol => {
            // Filled disc.
            fill_circle(img, mx, my, size / 2, color);
            draw_circle(img, mx, my, size / 2, dark);
        }
        ControlKind::Toll => {
            // Filled square.
            let half = size / 2;
            fill_rect(img, mx - half, my - half, size, size, color);
            draw_rect_outline(img, mx - half, my - half, size, size, dark);
        }
        ControlKind::Piracy => {
            // X — two short diagonals.
            let half = size / 2;
            draw_line_thick(
                img,
                mx - half,
                my - half,
                mx + half,
                my + half,
                color,
                thickness.max(2),
            );
            draw_line_thick(
                img,
                mx - half,
                my + half,
                mx + half,
                my - half,
                color,
                thickness.max(2),
            );
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ControlKind {
    Patrol,
    Toll,
    Interdiction,
    Piracy,
}

fn top_route_control(
    route: &crate::sector_model::GeneratedRoute,
) -> Option<(String, ControlKind, f32)> {
    let mut best: Option<(&str, ControlKind, f32)> = None;
    for c in &route.controls {
        for (kind, score) in [
            (ControlKind::Interdiction, c.interdiction),
            (ControlKind::Patrol, c.patrol),
            (ControlKind::Piracy, c.piracy),
            (ControlKind::Toll, c.toll),
        ] {
            if best.map(|(_, _, s)| score > s).unwrap_or(true) {
                best = Some((c.faction_id.as_str(), kind, score));
            }
        }
    }
    best.map(|(id, k, s)| (id.to_string(), k, s))
}

pub(super) fn star_radius_ratio() -> f32 {
    0.2016
}

fn shorten_to_star(a: (i32, i32), b: (i32, i32), star_r: f32) -> Option<((i32, i32), (i32, i32))> {
    let dx = (b.0 - a.0) as f32;
    let dy = (b.1 - a.1) as f32;
    let len = dx.hypot(dy);
    if len <= star_r * 2.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let s = (
        ux.mul_add(star_r, a.0 as f32).round() as i32,
        uy.mul_add(star_r, a.1 as f32).round() as i32,
    );
    let e = (
        ux.mul_add(-star_r, b.0 as f32).round() as i32,
        uy.mul_add(-star_r, b.1 as f32).round() as i32,
    );
    Some((s, e))
}

/// Draws a route styled by `pattern`. Motif-heavy patterns use rails, ladders,
/// ticks, chevrons, bursts, and triangles so the PNG map stays visually close
/// to the live GUI.
pub struct RouteLineParams<'a> {
    pub img: &'a mut RgbaImage,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub color: Rgba<u8>,
    pub thickness: i32,
    pub pattern: RoutePattern,
}

pub(crate) fn draw_route_line_thick(params: RouteLineParams) {
    let RouteLineParams {
        img,
        x0,
        y0,
        x1,
        y1,
        color,
        thickness,
        pattern,
    } = params;
    let Some(geom) = BitmapRouteGeom::new(x0, y0, x1, y1, thickness) else {
        return;
    };
    match pattern {
        RoutePattern::Solid => draw_line_thick(img, x0, y0, x1, y1, color, thickness),
        RoutePattern::Dashed | RoutePattern::DotDash | RoutePattern::Dotted => {
            draw_bitmap_strided_route(img, geom, color, thickness, pattern.strides());
        }
        RoutePattern::Cracked => {
            draw_bitmap_jagged_route(
                img,
                geom,
                color,
                thickness,
                geom.unit * 3.0,
                thickness as f32 * 1.7,
            );
        }
        RoutePattern::Ghost => {
            draw_bitmap_strided_route(
                img,
                geom,
                dim_rgba(color, 0.62),
                thickness,
                pattern.strides(),
            );
        }
        RoutePattern::Burst => {
            draw_bitmap_bursts(img, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Staccato => {
            draw_bitmap_zigzag_route(
                img,
                geom,
                color,
                thickness,
                geom.unit * 3.2,
                thickness as f32 * 1.8,
            );
        }
        RoutePattern::Gravel => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 1.55, false, true);
        }
        RoutePattern::Twin => {
            draw_bitmap_parallel_routes(img, geom, color, thickness, thickness as f32);
        }
        RoutePattern::Tripod => {
            draw_bitmap_tripods(img, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Tick => {
            draw_bitmap_base_spine(img, geom, color, thickness, 0.28);
            draw_bitmap_ticks(
                img,
                geom,
                color,
                thickness,
                geom.unit * 4.5,
                thickness as f32 * 2.2,
            );
        }
        RoutePattern::Bridge => {
            draw_bitmap_strided_route(
                img,
                geom,
                dim_rgba(color, 0.72),
                stroke_px(thickness, 0.8),
                &[3.0, 2.0],
            );
            draw_bitmap_ticks(
                img,
                geom,
                color,
                thickness,
                geom.unit * 5.0,
                thickness as f32 * 1.8,
            );
        }
        RoutePattern::Patter => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 2.2, true, false);
        }
        RoutePattern::Quartet => {
            draw_bitmap_dot_clusters(img, geom, color, thickness, geom.unit * 8.0, 4);
        }
        RoutePattern::Railroad => {
            let offset = thickness as f32 * 1.25;
            draw_bitmap_parallel_routes(img, geom, color, stroke_px(thickness, 0.8), offset);
            draw_bitmap_ticks(
                img,
                geom,
                color,
                stroke_px(thickness, 0.75),
                geom.unit * 5.5,
                offset * 1.15,
            );
        }
        RoutePattern::DoubleTap => {
            draw_bitmap_double_taps(img, geom, color, thickness, geom.unit * 7.0);
        }
        RoutePattern::Pebble => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 2.6, false, true);
        }
        RoutePattern::Whisper => {
            draw_bitmap_disc_trail(
                img,
                geom,
                dim_rgba(color, 0.78),
                thickness,
                geom.unit * 7.0,
                false,
                false,
            );
        }
        RoutePattern::March => {
            draw_bitmap_chevrons(img, geom, color, thickness, geom.unit * 5.5);
        }
    }
}

#[derive(Clone, Copy)]
struct BitmapRouteGeom {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    ux: f32,
    uy: f32,
    nx: f32,
    ny: f32,
    total: f32,
    unit: f32,
}

impl BitmapRouteGeom {
    fn new(x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32) -> Option<Self> {
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let total = dx.hypot(dy);
        if total <= 0.0 {
            return None;
        }
        let ux = dx / total;
        let uy = dy / total;
        Some(Self {
            x0,
            y0,
            x1,
            y1,
            ux,
            uy,
            nx: -uy,
            ny: ux,
            total,
            unit: (thickness as f32).max(2.0),
        })
    }

    fn at(self, t: f32, offset: f32) -> (i32, i32) {
        let t = t.clamp(0.0, self.total);
        (
            self.nx
                .mul_add(offset, self.ux.mul_add(t, self.x0 as f32))
                .round() as i32,
            self.ny
                .mul_add(offset, self.uy.mul_add(t, self.y0 as f32))
                .round() as i32,
        )
    }
}

/// FIX.txt §3: closed-form iteration count for `t = start; while t < total {
/// t += spacing }` loops, replacing accumulated float drift with a single
/// stable computation. Returns 0 for non-positive spacing or non-finite
/// results so caller `for i in 0..n_steps` becomes a no-op.
#[inline]
fn float_loop_steps(total: f32, start: f32, spacing: f32) -> i32 {
    if !(spacing > 0.0) {
        return 0;
    }
    let n = ((total - start) / spacing).ceil();
    if n.is_finite() && n > 0.0 {
        n as i32
    } else {
        0
    }
}

// FIX.txt §3: variable per-iteration stride + `.min(total)` clamp; the
// closed-form `((total-start)/step).ceil()` rewrite does not apply here
// because `step` changes each loop.
#[allow(clippy::while_float)]
fn draw_bitmap_strided_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    strides: &[f32],
) {
    if strides.is_empty() {
        draw_line_thick(img, geom.x0, geom.y0, geom.x1, geom.y1, color, thickness);
        return;
    }
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < geom.total {
        let stride = strides[idx % strides.len()];
        let seg = stride * geom.unit;
        let next_t = (t + seg).min(geom.total);
        if idx.is_multiple_of(2) {
            let (sx, sy) = geom.at(t, 0.0);
            let (ex, ey) = geom.at(next_t, 0.0);
            if stride <= 1.5 {
                let mx = (sx + ex) / 2;
                let my = (sy + ey) / 2;
                let r = ((thickness as f32) * 0.6).round() as i32;
                fill_circle(img, mx, my, r.max(1), color);
            } else {
                draw_line_thick(img, sx, sy, ex, ey, color, thickness);
            }
        }
        t = next_t;
        idx += 1;
    }
}

fn draw_bitmap_parallel_routes(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    offset: f32,
) {
    for side in [-offset, offset] {
        let (sx, sy) = geom.at(0.0, side);
        let (ex, ey) = geom.at(geom.total, side);
        draw_line_thick(img, sx, sy, ex, ey, color, thickness);
    }
}

fn draw_bitmap_base_spine(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    dim: f32,
) {
    draw_line_thick(
        img,
        geom.x0,
        geom.y0,
        geom.x1,
        geom.y1,
        dim_rgba(color, dim),
        stroke_px(thickness, 0.7),
    );
}

fn draw_bitmap_ticks(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    half_len: f32,
) {
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let (mx, my) = geom.at(t, 0.0);
        let sx = geom.nx.mul_add(-half_len, mx as f32).round() as i32;
        let sy = geom.ny.mul_add(-half_len, my as f32).round() as i32;
        let ex = geom.nx.mul_add(half_len, mx as f32).round() as i32;
        let ey = geom.ny.mul_add(half_len, my as f32).round() as i32;
        draw_line_thick(img, sx, sy, ex, ey, color, stroke_px(thickness, 0.75));
    }
}

fn draw_bitmap_jagged_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = (geom.x0, geom.y0);
    let start = spacing;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    let mut sign = 1.0;
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let next = geom.at(t, amplitude * sign);
        draw_line_thick(img, prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        sign = -sign;
    }
    draw_line_thick(img, prev.0, prev.1, geom.x1, geom.y1, color, thickness);
}

fn draw_bitmap_zigzag_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = geom.at(0.0, -amplitude);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    let mut sign = 1.0;
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let next = geom.at(t, amplitude * sign);
        draw_line_thick(img, prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        sign = -sign;
    }
    let end = geom.at(geom.total, -amplitude * sign);
    draw_line_thick(img, prev.0, prev.1, end.0, end.1, color, thickness);
}

fn draw_bitmap_disc_trail(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    hollow: bool,
    alternating: bool,
) {
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let radius = if alternating && (i as usize).is_multiple_of(2) {
            thickness as f32 * 0.85
        } else {
            thickness as f32 * 0.55
        }
        .round() as i32;
        let (mx, my) = geom.at(t, 0.0);
        if hollow {
            draw_circle(img, mx, my, radius.max(1), color);
        } else {
            fill_circle(img, mx, my, radius.max(1), color);
        }
    }
}

fn draw_bitmap_dot_clusters(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    count: usize,
) {
    let dot_gap = geom.unit * 1.25;
    let radius = ((thickness as f32) * 0.55).round() as i32;
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for step in 0..n_steps {
        let t = spacing.mul_add(step as f32, start);
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            let (mx, my) = geom.at(t + local, 0.0);
            fill_circle(img, mx, my, radius.max(1), color);
        }
    }
}

fn draw_bitmap_double_taps(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let pair_gap = geom.unit * 1.3;
    let half_len = thickness as f32 * 1.8;
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        for local in [-pair_gap * 0.5, pair_gap * 0.5] {
            let (mx, my) = geom.at(t + local, 0.0);
            let sx = geom.nx.mul_add(-half_len, mx as f32).round() as i32;
            let sy = geom.ny.mul_add(-half_len, my as f32).round() as i32;
            let ex = geom.nx.mul_add(half_len, mx as f32).round() as i32;
            let ey = geom.ny.mul_add(half_len, my as f32).round() as i32;
            draw_line_thick(img, sx, sy, ex, ey, color, stroke_px(thickness, 0.8));
        }
    }
}

fn draw_bitmap_chevrons(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let size = geom.unit * 1.8;
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let tip = geom.at(t + size * 0.35, 0.0);
        let back = geom.at(t - size * 0.35, 0.0);
        let left = (
            (geom.nx * size).mul_add(0.35, back.0 as f32).round() as i32,
            (geom.ny * size).mul_add(0.35, back.1 as f32).round() as i32,
        );
        let right = (
            (geom.nx * size).mul_add(-0.35, back.0 as f32).round() as i32,
            (geom.ny * size).mul_add(-0.35, back.1 as f32).round() as i32,
        );
        draw_line_thick(img, left.0, left.1, tip.0, tip.1, color, thickness);
        draw_line_thick(img, right.0, right.1, tip.0, tip.1, color, thickness);
    }
}

fn draw_bitmap_tripods(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let size = (geom.unit * 1.8).max(thickness as f32 * 2.5);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = geom.at(t, 0.0);
        let mid_p = (mid.0 as f32, mid.1 as f32);

        // Forward leg
        let fwd = geom.at(t + size * 0.4, 0.0);
        draw_line_thick(img, mid.0, mid.1, fwd.0, fwd.1, color, thickness);

        // Lateral legs
        let l_pos = (
            (geom.nx * size).mul_add(0.4, mid_p.0).round() as i32,
            (geom.ny * size).mul_add(0.4, mid_p.1).round() as i32,
        );
        let r_pos = (
            (geom.nx * size).mul_add(-0.4, mid_p.0).round() as i32,
            (geom.ny * size).mul_add(-0.4, mid_p.1).round() as i32,
        );
        draw_line_thick(img, mid.0, mid.1, l_pos.0, l_pos.1, color, thickness);
        draw_line_thick(img, mid.0, mid.1, r_pos.0, r_pos.1, color, thickness);
    }
}

fn draw_bitmap_bursts(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let radius = (thickness as f32 * 1.6).max(2.0);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(geom.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = geom.at(t, 0.0);
        let a = geom.at(t - radius, 0.0);
        let b = geom.at(t + radius, 0.0);
        let c = (
            geom.nx.mul_add(-radius, mid.0 as f32).round() as i32,
            geom.ny.mul_add(-radius, mid.1 as f32).round() as i32,
        );
        let d = (
            geom.nx.mul_add(radius, mid.0 as f32).round() as i32,
            geom.ny.mul_add(radius, mid.1 as f32).round() as i32,
        );
        draw_line_thick(img, a.0, a.1, b.0, b.1, color, stroke_px(thickness, 0.65));
        draw_line_thick(img, c.0, c.1, d.0, d.1, color, stroke_px(thickness, 0.65));
        fill_circle(img, mid.0, mid.1, stroke_px(thickness, 0.45), color);
    }
}
