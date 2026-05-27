//! Shared route-line drawing for both bitmap (PNG) and SVG backends.
//!
//! Pre-Pass-C/D, this code was duplicated across `bitmap/routes.rs` and
//! `svg_export/routes.rs` with the only real difference being `i32` vs
//! `f32` coordinates. Both flavours now live here, generic over
//! [`super::canvas::Canvas`] — bitmap rounds at the primitive boundary
//! inside its `Canvas` impl.
//!
//! [`draw_route_control_glyph`] stays backend-specific because the bitmap
//! midpoint computation uses `i32` floor division
//! (`(a + b) / 2`) which doesn't round-trip through `f32` arithmetic
//! without changing PNG bytes.

use std::collections::HashMap;

use image::Rgba;

use super::canvas::Canvas;
use super::colors::{dim, route_thickness_f32, stability_color};
use super::options::RenderOptions;
use crate::sector_model::{GeneratedRoute, GeneratedSector, RoutePattern};

#[must_use]
pub(crate) fn star_radius_ratio() -> f32 {
    0.2016
}

/// Backend-neutral entry: draw every route in `sector`. The caller's
/// `Canvas` impl owns coordinate quantisation. `hex_size` is the same
/// world-space hex radius both backends use for spacing.
///
/// The per-route midpoint glyph is left to the caller (see
/// `bitmap::routes::draw_route_control_glyph` / `svg_export::routes`)
/// because its `i32` floor-division math diverges from `f32` rounding.
pub(crate) fn draw_routes<C: Canvas, F>(
    canvas: &mut C,
    sector: &GeneratedSector,
    opts: &RenderOptions,
    hex_size: f32,
    hex_center_fn: impl Fn(i32, i32) -> (f32, f32),
    mut draw_glyph: F,
) where
    F: FnMut(&mut C, &GeneratedRoute, (f32, f32), (f32, f32), f32),
{
    let mut centers: HashMap<&str, (f32, f32)> = HashMap::new();
    for sys in sector.systems.iter() {
        centers.insert(sys.id.as_str(), hex_center_fn(sys.coord.q, sys.coord.r));
    }
    let star_r = hex_size * star_radius_ratio();
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
        let thickness_raw = route_thickness_f32(&opts.theme, route.stability, hex_size);
        let thickness = canvas.quantize(thickness_raw).max(1.0);
        let pattern = route.pattern_with_salt(&sector.seed, opts.route_view_mode);
        draw_route_pattern(canvas, sx, sy, ex, ey, color, thickness, pattern);
        draw_glyph(canvas, route, (sx, sy), (ex, ey), thickness);
    }
}

pub(crate) fn shorten_to_star(
    a: (f32, f32),
    b: (f32, f32),
    star_r: f32,
) -> Option<((f32, f32), (f32, f32))> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    if len <= star_r * 2.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    Some((
        (ux.mul_add(star_r, a.0), uy.mul_add(star_r, a.1)),
        (ux.mul_add(-star_r, b.0), uy.mul_add(-star_r, b.1)),
    ))
}

#[derive(Clone, Copy)]
struct RouteGeom {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    ux: f32,
    uy: f32,
    nx: f32,
    ny: f32,
    total: f32,
    unit: f32,
}

impl RouteGeom {
    fn new(x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32) -> Option<Self> {
        let dx = x1 - x0;
        let dy = y1 - y0;
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
            unit: thickness.max(2.0),
        })
    }

    fn at(self, t: f32, offset: f32) -> (f32, f32) {
        let t = t.clamp(0.0, self.total);
        (
            self.nx.mul_add(offset, self.ux.mul_add(t, self.x0)),
            self.ny.mul_add(offset, self.uy.mul_add(t, self.y0)),
        )
    }
}

/// FIX.txt §3: closed-form iteration count for `t = start; while t < total {
/// t += spacing }` loops, replacing accumulated float drift with a single
/// stable computation.
#[inline]
fn float_loop_steps(total: f32, start: f32, spacing: f32) -> i32 {
    if spacing <= 0.0 || spacing.is_nan() {
        return 0;
    }
    let n = ((total - start) / spacing).ceil();
    if n.is_finite() && n > 0.0 {
        n as i32
    } else {
        0
    }
}

#[inline]
fn stroke_factor<C: Canvas>(canvas: &C, thickness: f32, factor: f32) -> f32 {
    canvas.quantize(thickness * factor).max(1.0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_route_pattern<C: Canvas>(
    canvas: &mut C,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgba<u8>,
    thickness: f32,
    pattern: RoutePattern,
) {
    let Some(g) = RouteGeom::new(x0, y0, x1, y1, thickness) else {
        return;
    };
    match pattern {
        RoutePattern::Solid => canvas.line(x0, y0, x1, y1, color, thickness),
        RoutePattern::Dashed | RoutePattern::DotDash | RoutePattern::Dotted => {
            strided(canvas, g, color, thickness, pattern.strides());
        }
        RoutePattern::Cracked => {
            jagged(canvas, g, color, thickness, g.unit * 3.0, thickness * 1.7);
        }
        RoutePattern::Ghost => {
            strided(canvas, g, dim(color, 0.62), thickness, pattern.strides());
        }
        RoutePattern::Burst => bursts(canvas, g, color, thickness, g.unit * 5.0),
        RoutePattern::Staccato => {
            zigzag(canvas, g, color, thickness, g.unit * 3.2, thickness * 1.8);
        }
        RoutePattern::Gravel => disc_trail(canvas, g, color, thickness, g.unit * 1.55, false, true),
        RoutePattern::Twin => parallel(canvas, g, color, thickness, thickness),
        RoutePattern::Tripod => tripods(canvas, g, color, thickness, g.unit * 5.0),
        RoutePattern::Tick => {
            spine(canvas, g, color, thickness, 0.28);
            ticks(canvas, g, color, thickness, g.unit * 4.5, thickness * 2.2);
        }
        RoutePattern::Bridge => {
            strided(
                canvas,
                g,
                dim(color, 0.72),
                stroke_factor(canvas, thickness, 0.8),
                &[3.0, 2.0],
            );
            ticks(canvas, g, color, thickness, g.unit * 5.0, thickness * 1.8);
        }
        RoutePattern::Patter => disc_trail(canvas, g, color, thickness, g.unit * 2.2, true, false),
        RoutePattern::Quartet => dot_clusters(canvas, g, color, thickness, g.unit * 8.0, 4),
        RoutePattern::Railroad => {
            let offset = thickness * 1.25;
            parallel(
                canvas,
                g,
                color,
                stroke_factor(canvas, thickness, 0.8),
                offset,
            );
            ticks(
                canvas,
                g,
                color,
                stroke_factor(canvas, thickness, 0.75),
                g.unit * 5.5,
                offset * 1.15,
            );
        }
        RoutePattern::DoubleTap => double_taps(canvas, g, color, thickness, g.unit * 7.0),
        RoutePattern::Pebble => disc_trail(canvas, g, color, thickness, g.unit * 2.6, false, true),
        RoutePattern::Whisper => disc_trail(
            canvas,
            g,
            dim(color, 0.78),
            thickness,
            g.unit * 7.0,
            false,
            false,
        ),
        RoutePattern::March => chevrons(canvas, g, color, thickness, g.unit * 5.5),
    }
}

// FIX.txt §3: variable per-iteration stride + `.min(total)` clamp; the
// closed-form `((total-start)/step).ceil()` rewrite does not apply here
// because `step` changes each loop.
#[allow(clippy::while_float)]
fn strided<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    strides: &[f32],
) {
    if strides.is_empty() {
        canvas.line(g.x0, g.y0, g.x1, g.y1, color, thickness);
        return;
    }
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < g.total {
        let stride = strides[idx % strides.len()];
        let seg = stride * g.unit;
        let next_t = (t + seg).min(g.total);
        if idx & 1 == 0 {
            let (sx, sy) = g.at(t, 0.0);
            let (ex, ey) = g.at(next_t, 0.0);
            if stride <= 1.5 {
                let mx = (sx + ex) * 0.5;
                let my = (sy + ey) * 0.5;
                canvas.circle(mx, my, thickness * 0.6, color, None, 0.0);
            } else {
                canvas.line(sx, sy, ex, ey, color, thickness);
            }
        }
        t = next_t;
        idx += 1;
    }
}

fn parallel<C: Canvas>(canvas: &mut C, g: RouteGeom, color: Rgba<u8>, thickness: f32, offset: f32) {
    for side in [-offset, offset] {
        let (sx, sy) = g.at(0.0, side);
        let (ex, ey) = g.at(g.total, side);
        canvas.line(sx, sy, ex, ey, color, thickness);
    }
}

fn spine<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    dim_factor: f32,
) {
    let t = stroke_factor(canvas, thickness, 0.7);
    canvas.line(g.x0, g.y0, g.x1, g.y1, dim(color, dim_factor), t);
}

fn ticks<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    half_len: f32,
) {
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    let t_thick = stroke_factor(canvas, thickness, 0.75);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let (mx, my) = g.at(t, 0.0);
        let sx = g.nx.mul_add(-half_len, mx);
        let sy = g.ny.mul_add(-half_len, my);
        let ex = g.nx.mul_add(half_len, mx);
        let ey = g.ny.mul_add(half_len, my);
        canvas.line(sx, sy, ex, ey, color, t_thick);
    }
}

fn jagged<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = (g.x0, g.y0);
    let start = spacing;
    let n_steps = float_loop_steps(g.total, start, spacing);
    let mut sign = 1.0_f32;
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let next = g.at(t, amplitude * sign);
        canvas.line(prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        sign = -sign;
    }
    canvas.line(prev.0, prev.1, g.x1, g.y1, color, thickness);
}

fn zigzag<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = g.at(0.0, -amplitude);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    let mut sign = 1.0_f32;
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let next = g.at(t, amplitude * sign);
        canvas.line(prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        sign = -sign;
    }
    let end = g.at(g.total, -amplitude * sign);
    canvas.line(prev.0, prev.1, end.0, end.1, color, thickness);
}

fn disc_trail<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    hollow: bool,
    alternating: bool,
) {
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let radius_raw = if alternating && i & 1 == 0 {
            thickness * 0.85
        } else {
            thickness * 0.55
        };
        let radius = canvas.quantize(radius_raw).max(1.0);
        let (mx, my) = g.at(t, 0.0);
        if hollow {
            canvas.circle(mx, my, radius, Rgba([0, 0, 0, 0]), Some(color), 1.0);
        } else {
            canvas.circle(mx, my, radius, color, None, 0.0);
        }
    }
}

fn dot_clusters<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    count: usize,
) {
    let dot_gap = g.unit * 1.25;
    let radius = canvas.quantize(thickness * 0.55).max(1.0);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for step in 0..n_steps {
        let t = spacing.mul_add(step as f32, start);
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            let (mx, my) = g.at(t + local, 0.0);
            canvas.circle(mx, my, radius, color, None, 0.0);
        }
    }
}

fn double_taps<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
) {
    let pair_gap = g.unit * 1.3;
    let half_len = thickness * 1.8;
    let t_thick = stroke_factor(canvas, thickness, 0.8);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        for local in [-pair_gap * 0.5, pair_gap * 0.5] {
            let (mx, my) = g.at(t + local, 0.0);
            let sx = g.nx.mul_add(-half_len, mx);
            let sy = g.ny.mul_add(-half_len, my);
            let ex = g.nx.mul_add(half_len, mx);
            let ey = g.ny.mul_add(half_len, my);
            canvas.line(sx, sy, ex, ey, color, t_thick);
        }
    }
}

fn chevrons<C: Canvas>(
    canvas: &mut C,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
) {
    let size = g.unit * 1.8;
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let tip = g.at(t + size * 0.35, 0.0);
        let back = g.at(t - size * 0.35, 0.0);
        let left = (
            (g.nx * size).mul_add(0.35, back.0),
            (g.ny * size).mul_add(0.35, back.1),
        );
        let right = (
            (g.nx * size).mul_add(-0.35, back.0),
            (g.ny * size).mul_add(-0.35, back.1),
        );
        canvas.line(left.0, left.1, tip.0, tip.1, color, thickness);
        canvas.line(right.0, right.1, tip.0, tip.1, color, thickness);
    }
}

fn tripods<C: Canvas>(canvas: &mut C, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let size = (g.unit * 1.8).max(thickness * 2.5);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = g.at(t, 0.0);
        let fwd = g.at(t + size * 0.4, 0.0);
        canvas.line(mid.0, mid.1, fwd.0, fwd.1, color, thickness);
        let l_pos = (
            (g.nx * size).mul_add(0.4, mid.0),
            (g.ny * size).mul_add(0.4, mid.1),
        );
        let r_pos = (
            (g.nx * size).mul_add(-0.4, mid.0),
            (g.ny * size).mul_add(-0.4, mid.1),
        );
        canvas.line(mid.0, mid.1, l_pos.0, l_pos.1, color, thickness);
        canvas.line(mid.0, mid.1, r_pos.0, r_pos.1, color, thickness);
    }
}

fn bursts<C: Canvas>(canvas: &mut C, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let radius = (thickness * 1.6).max(2.0);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    let cross_thick = stroke_factor(canvas, thickness, 0.65);
    let centre_r = canvas.quantize(thickness * 0.45).max(1.0);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = g.at(t, 0.0);
        let a = g.at(t - radius, 0.0);
        let b = g.at(t + radius, 0.0);
        let c = (g.nx.mul_add(-radius, mid.0), g.ny.mul_add(-radius, mid.1));
        let d = (g.nx.mul_add(radius, mid.0), g.ny.mul_add(radius, mid.1));
        canvas.line(a.0, a.1, b.0, b.1, color, cross_thick);
        canvas.line(c.0, c.1, d.0, d.1, color, cross_thick);
        canvas.circle(mid.0, mid.1, centre_r, color, None, 0.0);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ControlKind {
    Patrol,
    Toll,
    Interdiction,
    Piracy,
}

#[must_use]
pub(crate) fn top_route_control(route: &GeneratedRoute) -> Option<(String, ControlKind, f32)> {
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
    best.map(|(id, k, sc)| (id.to_string(), k, sc))
}
