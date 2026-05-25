//! Route line drawing: warp-route patterns + control glyphs.

use std::collections::HashMap;

use image::Rgba;

use crate::bitmap::RenderOptions;
use crate::faction_style::faction_style_rgb_by_id;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedRoute, GeneratedSector, RoutePattern};

use super::colors::{darken, dim, rgba_from_tuple, route_thickness, stability_color, stroke_px};
use super::geom::hex_center;
use super::primitives::{circle, line, rect};
use super::{star_radius_ratio, HEX_SIZE};

pub(super) fn draw_routes(s: &mut String, sector: &GeneratedSector, opts: &RenderOptions) {
    let mut centers: HashMap<&str, (f32, f32)> = HashMap::new();
    for sys in sector.systems.iter() {
        centers.insert(sys.id.as_str(), hex_center(sys.coord.q, sys.coord.r));
    }
    let star_r = HEX_SIZE * star_radius_ratio();
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
        let thickness = route_thickness(&opts.theme, route.stability);
        let pattern = route.pattern_with_salt(&sector.seed, opts.route_view_mode);
        draw_route_pattern(s, sx, sy, ex, ey, color, thickness, pattern);
        draw_route_control_glyph(s, sector, route, (sx, sy), (ex, ey), thickness, &opts.theme);
    }
}

fn shorten_to_star(a: (f32, f32), b: (f32, f32), star_r: f32) -> Option<((f32, f32), (f32, f32))> {
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

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_route_pattern(
    s: &mut String,
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
        RoutePattern::Solid => line(s, x0, y0, x1, y1, color, thickness, None),
        RoutePattern::Dashed | RoutePattern::DotDash | RoutePattern::Dotted => {
            strided(s, g, color, thickness, pattern.strides());
        }
        RoutePattern::Cracked => {
            jagged(s, g, color, thickness, g.unit * 3.0, thickness * 1.7);
        }
        RoutePattern::Ghost => {
            strided(s, g, dim(color, 0.62), thickness, pattern.strides());
        }
        RoutePattern::Burst => bursts(s, g, color, thickness, g.unit * 5.0),
        RoutePattern::Staccato => {
            zigzag(s, g, color, thickness, g.unit * 3.2, thickness * 1.8);
        }
        RoutePattern::Gravel => disc_trail(s, g, color, thickness, g.unit * 1.55, false, true),
        RoutePattern::Twin => parallel(s, g, color, thickness, thickness),
        RoutePattern::Tripod => tripods(s, g, color, thickness, g.unit * 5.0),
        RoutePattern::Tick => {
            spine(s, g, color, thickness, 0.28);
            ticks(s, g, color, thickness, g.unit * 4.5, thickness * 2.2);
        }
        RoutePattern::Bridge => {
            strided(
                s,
                g,
                dim(color, 0.72),
                stroke_px(thickness, 0.8),
                &[3.0, 2.0],
            );
            ticks(s, g, color, thickness, g.unit * 5.0, thickness * 1.8);
        }
        RoutePattern::Patter => disc_trail(s, g, color, thickness, g.unit * 2.2, true, false),
        RoutePattern::Quartet => dot_clusters(s, g, color, thickness, g.unit * 8.0, 4),
        RoutePattern::Railroad => {
            let offset = thickness * 1.25;
            parallel(s, g, color, stroke_px(thickness, 0.8), offset);
            ticks(
                s,
                g,
                color,
                stroke_px(thickness, 0.75),
                g.unit * 5.5,
                offset * 1.15,
            );
        }
        RoutePattern::DoubleTap => double_taps(s, g, color, thickness, g.unit * 7.0),
        RoutePattern::Pebble => disc_trail(s, g, color, thickness, g.unit * 2.6, false, true),
        RoutePattern::Whisper => disc_trail(
            s,
            g,
            dim(color, 0.78),
            thickness,
            g.unit * 7.0,
            false,
            false,
        ),
        RoutePattern::March => chevrons(s, g, color, thickness, g.unit * 5.5),
    }
}

/// FIX.txt §3: closed-form iteration count for `t = start; while t < total {
/// t += spacing }` loops, replacing accumulated float drift with a single
/// stable computation.
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
fn strided(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, strides: &[f32]) {
    if strides.is_empty() {
        line(s, g.x0, g.y0, g.x1, g.y1, color, thickness, None);
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
                circle(s, mx, my, thickness * 0.6, color, None, 0.0);
            } else {
                line(s, sx, sy, ex, ey, color, thickness, None);
            }
        }
        t = next_t;
        idx += 1;
    }
}

fn parallel(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, offset: f32) {
    for side in [-offset, offset] {
        let (sx, sy) = g.at(0.0, side);
        let (ex, ey) = g.at(g.total, side);
        line(s, sx, sy, ex, ey, color, thickness, None);
    }
}

fn spine(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, dim_factor: f32) {
    line(
        s,
        g.x0,
        g.y0,
        g.x1,
        g.y1,
        dim(color, dim_factor),
        stroke_px(thickness, 0.7),
        None,
    );
}

fn ticks(
    s: &mut String,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    half_len: f32,
) {
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let (mx, my) = g.at(t, 0.0);
        let sx = g.nx.mul_add(-half_len, mx);
        let sy = g.ny.mul_add(-half_len, my);
        let ex = g.nx.mul_add(half_len, mx);
        let ey = g.ny.mul_add(half_len, my);
        line(s, sx, sy, ex, ey, color, stroke_px(thickness, 0.75), None);
    }
}

fn jagged(
    s: &mut String,
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
        line(s, prev.0, prev.1, next.0, next.1, color, thickness, None);
        prev = next;
        sign = -sign;
    }
    line(s, prev.0, prev.1, g.x1, g.y1, color, thickness, None);
}

fn zigzag(
    s: &mut String,
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
        line(s, prev.0, prev.1, next.0, next.1, color, thickness, None);
        prev = next;
        sign = -sign;
    }
    let end = g.at(g.total, -amplitude * sign);
    line(s, prev.0, prev.1, end.0, end.1, color, thickness, None);
}

fn disc_trail(
    s: &mut String,
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
        let radius = if alternating && i & 1 == 0 {
            thickness * 0.85
        } else {
            thickness * 0.55
        };
        let (mx, my) = g.at(t, 0.0);
        if hollow {
            circle(
                s,
                mx,
                my,
                radius.max(1.0),
                Rgba([0, 0, 0, 0]),
                Some(color),
                1.0,
            );
        } else {
            circle(s, mx, my, radius.max(1.0), color, None, 0.0);
        }
    }
}

fn dot_clusters(
    s: &mut String,
    g: RouteGeom,
    color: Rgba<u8>,
    thickness: f32,
    spacing: f32,
    count: usize,
) {
    let dot_gap = g.unit * 1.25;
    let radius = thickness * 0.55;
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for step in 0..n_steps {
        let t = spacing.mul_add(step as f32, start);
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            let (mx, my) = g.at(t + local, 0.0);
            circle(s, mx, my, radius.max(1.0), color, None, 0.0);
        }
    }
}

fn double_taps(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let pair_gap = g.unit * 1.3;
    let half_len = thickness * 1.8;
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
            line(s, sx, sy, ex, ey, color, stroke_px(thickness, 0.8), None);
        }
    }
}

fn chevrons(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
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
        line(s, left.0, left.1, tip.0, tip.1, color, thickness, None);
        line(s, right.0, right.1, tip.0, tip.1, color, thickness, None);
    }
}

fn tripods(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let size = (g.unit * 1.8).max(thickness * 2.5);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = g.at(t, 0.0);
        let fwd = g.at(t + size * 0.4, 0.0);
        line(s, mid.0, mid.1, fwd.0, fwd.1, color, thickness, None);
        let l_pos = (
            (g.nx * size).mul_add(0.4, mid.0),
            (g.ny * size).mul_add(0.4, mid.1),
        );
        let r_pos = (
            (g.nx * size).mul_add(-0.4, mid.0),
            (g.ny * size).mul_add(-0.4, mid.1),
        );
        line(s, mid.0, mid.1, l_pos.0, l_pos.1, color, thickness, None);
        line(s, mid.0, mid.1, r_pos.0, r_pos.1, color, thickness, None);
    }
}

fn bursts(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let radius = (thickness * 1.6).max(2.0);
    let start = spacing * 0.5;
    let n_steps = float_loop_steps(g.total, start, spacing);
    for i in 0..n_steps {
        let t = spacing.mul_add(i as f32, start);
        let mid = g.at(t, 0.0);
        let a = g.at(t - radius, 0.0);
        let b = g.at(t + radius, 0.0);
        let c = (g.nx.mul_add(-radius, mid.0), g.ny.mul_add(-radius, mid.1));
        let d = (g.nx.mul_add(radius, mid.0), g.ny.mul_add(radius, mid.1));
        line(
            s,
            a.0,
            a.1,
            b.0,
            b.1,
            color,
            stroke_px(thickness, 0.65),
            None,
        );
        line(
            s,
            c.0,
            c.1,
            d.0,
            d.1,
            color,
            stroke_px(thickness, 0.65),
            None,
        );
        circle(
            s,
            mid.0,
            mid.1,
            stroke_px(thickness, 0.45),
            color,
            None,
            0.0,
        );
    }
}

#[derive(Clone, Copy)]
pub(super) enum ControlKind {
    Patrol,
    Toll,
    Interdiction,
    Piracy,
}

fn top_route_control(route: &GeneratedRoute) -> Option<(String, ControlKind, f32)> {
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

fn draw_route_control_glyph(
    s: &mut String,
    sector: &GeneratedSector,
    route: &GeneratedRoute,
    a: (f32, f32),
    b: (f32, f32),
    thickness: f32,
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
        rgba_from_tuple(style.fill)
    };
    let dark = darken(color, 0.5);
    let mx = (a.0 + b.0) * 0.5;
    let my = (a.1 + b.1) * 0.5;
    let size = (thickness * 3.0).max(6.0);
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        let half = size;
        line(
            s,
            mx - half,
            my - half,
            mx + half,
            my + half,
            color,
            thickness.max(2.0),
            None,
        );
        line(
            s,
            mx - half,
            my + half,
            mx + half,
            my - half,
            color,
            thickness.max(2.0),
            None,
        );
        return;
    }
    match kind {
        ControlKind::Interdiction => {
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let len = dx.hypot(dy).max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size;
            let x0 = mx - px * half;
            let y0 = my - py * half;
            let x1 = mx + px * half;
            let y1 = my + py * half;
            line(s, x0, y0, x1, y1, color, thickness.max(2.0), None);
            line(s, x0, y0, x1, y1, dark, 1.0, None);
        }
        ControlKind::Patrol => {
            circle(s, mx, my, size * 0.5, color, Some(dark), 1.0);
        }
        ControlKind::Toll => {
            let half = size * 0.5;
            rect(s, mx - half, my - half, size, size, color, Some(dark));
        }
        ControlKind::Piracy => {
            let half = size * 0.5;
            line(
                s,
                mx - half,
                my - half,
                mx + half,
                my + half,
                color,
                thickness.max(2.0),
                None,
            );
            line(
                s,
                mx - half,
                my + half,
                mx + half,
                my - half,
                color,
                thickness.max(2.0),
                None,
            );
        }
    }
}
