//! SVG export: vector counterpart to [`crate::bitmap`].
//!
//! Mirrors the PNG renderer's layout and visual language but emits SVG
//! primitives (`<polygon>`, `<circle>`, `<line>`, `<rect>`, `<path>`,
//! `<text>`) so the output stays editable in vector tools and crisp at
//! arbitrary zoom. Layout uses the same scaled `Geom` as the bitmap
//! renderer with `scale=1` — SVG resolves the physical size via its
//! `width`/`height` attributes, so there is no separate resolution knob.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use camino::Utf8Path;
use image::Rgba;

use crate::bitmap::RenderOptions;
use crate::errors::SectorError;
use crate::faction_style::faction_style_rgb_by_id;
use crate::heatmap::{self, HeatCellRgb, HeatmapMode};
use crate::map_theme::{LabelDensity, LegendStyle, MapTheme, SymbolSet};
use crate::sector_model::{
    offset_r_neighbors, GeneratedRoute, GeneratedSector, GeneratedSystem, RoutePattern,
    RouteStability, RouteType,
};
use crate::subsectors::Subsector;

const HEX_SIZE: f32 = 26.0;
const MARGIN: f32 = 28.0;
const LEGEND_PAD: f32 = 16.0;
const LINE_H: f32 = 18.0;
const TITLE_FONT: f32 = 18.0;
const BODY_FONT: f32 = 11.0;
const SYS_LABEL_FONT: f32 = 10.0;
const SUB_LABEL_FONT: f32 = 13.0;
const REGION_LABEL_FONT: f32 = 11.0;
const PIP_FONT: f32 = 9.0;

fn legend_width(theme: &MapTheme) -> f32 {
    match theme.legend {
        LegendStyle::Hidden => 0.0,
        LegendStyle::Compact => 220.0,
        LegendStyle::Full => 280.0,
    }
}

fn star_radius_ratio() -> f32 {
    0.2016
}

/// Render an SVG document for `sector`. Pure — does no I/O.
#[must_use]
pub fn render_sector_svg(
    sector: &GeneratedSector,
    subsectors: Option<&[Subsector]>,
    opts: &RenderOptions,
) -> String {
    let bounds = map_bounds(sector);
    let total_w = bounds.w + legend_width(&opts.theme);
    let legend_h = legend_height(sector, opts);
    let total_h = bounds.h.max(legend_h);

    let mut s = String::with_capacity(64 * 1024);
    let _ = write!(
        s,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="monospace">"#,
        w = total_w.round() as i32,
        h = total_h.round() as i32,
    );

    // Background.
    rect(&mut s, 0.0, 0.0, total_w, total_h, opts.theme.bg, None);

    let subs = subsectors.unwrap_or(&[]);
    let draw_subsectors = !subs.is_empty() && opts.theme.show_subsector_borders;

    let heat = if matches!(opts.heatmap, HeatmapMode::Off) {
        HashMap::new()
    } else {
        heatmap::compute_rgb(sector, opts.heatmap)
    };
    let sys_tints = compute_system_tints(sector, opts, &heat);

    draw_hex_grid(&mut s, sector, &sys_tints, &opts.theme);
    if draw_subsectors {
        draw_subsector_borders(&mut s, sector, subs, &opts.theme);
    }
    draw_routes(&mut s, sector, opts);
    draw_region_labels(&mut s, sector, &opts.theme, bounds);
    draw_systems(&mut s, sector, subs, opts);
    if draw_subsectors && !matches!(opts.theme.label_density, LabelDensity::None) {
        draw_subsector_labels(&mut s, sector, subs, &opts.theme, bounds);
    }
    draw_system_labels(&mut s, sector, subs, opts);

    if !matches!(opts.theme.legend, LegendStyle::Hidden) {
        rect(
            &mut s,
            bounds.w,
            0.0,
            legend_width(&opts.theme),
            total_h,
            opts.theme.panel_bg,
            None,
        );
        draw_legend(&mut s, sector, bounds.w, opts);
    }

    s.push_str("</svg>\n");
    s
}

/// Write an SVG document to `path`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_svg_to(
    sector: &GeneratedSector,
    path: &Utf8Path,
    subsectors: Option<&[Subsector]>,
) -> Result<(), SectorError> {
    write_sector_svg_to_with(sector, path, subsectors, &RenderOptions::default())
}

/// Variant of [`write_sector_svg_to`] taking explicit [`RenderOptions`].
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_svg_to_with(
    sector: &GeneratedSector,
    path: &Utf8Path,
    subsectors: Option<&[Subsector]>,
    opts: &RenderOptions,
) -> Result<(), SectorError> {
    let body = render_sector_svg(sector, subsectors, opts);
    std::fs::write(path.as_std_path(), body).map_err(|e| SectorError::io(path.as_str(), e))
}

// ── Geometry ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct MapBounds {
    w: f32,
    h: f32,
}

fn map_bounds(sector: &GeneratedSector) -> MapBounds {
    let horiz_step = HEX_SIZE * 3f32.sqrt();
    let vert_step = HEX_SIZE * 1.5;
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = MARGIN * 2.0 + horiz_step * (sector.width as f32 + odd_shift);
    let label_band = HEX_SIZE * 0.55;
    let h = MARGIN * 2.0
        + sector.height.saturating_sub(1) as f32 * vert_step
        + 2.0 * HEX_SIZE
        + label_band;
    MapBounds { w, h }
}

fn hex_center(q: i32, r: i32) -> (f32, f32) {
    let horiz_step = HEX_SIZE * 3f32.sqrt();
    let vert_step = HEX_SIZE * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = MARGIN + horiz_step * (q as f32 + row_shift) + horiz_step / 2.0;
    let y = MARGIN + vert_step * r as f32 + HEX_SIZE;
    (x, y)
}

fn hex_vertices(cx: f32, cy: f32, size: f32) -> [(f32, f32); 6] {
    let mut out = [(0.0f32, 0.0f32); 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 30.0);
        *slot = (cx + size * angle.cos(), cy + size * angle.sin());
    }
    out
}

// ── Hex grid + subsector + region tints ─────────────────────────────────────

fn compute_system_tints(
    sector: &GeneratedSector,
    opts: &RenderOptions,
    heat: &HashMap<crate::ids::SystemId, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for sys in sector.systems.iter() {
        let key = (sys.coord.q, sys.coord.r);
        if !matches!(opts.heatmap, HeatmapMode::Off) {
            if let Some(cell) = heat.get(&sys.id) {
                let strength =
                    opts.theme.heatmap_tint_min + cell.intensity * opts.theme.heatmap_tint_range;
                let color = rgba_from_tuple(cell.rgb);
                out.insert(key, tint_against(color, strength, opts.theme.hex_empty));
                continue;
            }
        }
        if opts.faction_fill {
            if let Some(dom) = sys.control.dominant.as_deref() {
                let style = faction_style_rgb_by_id(&sector.factions, dom);
                out.insert(
                    key,
                    tint_against(
                        rgba_from_tuple(style.fill),
                        opts.theme.faction_tint_strength,
                        opts.theme.hex_empty,
                    ),
                );
            }
        }
    }
    out
}

fn compute_region_tints(
    sector: &GeneratedSector,
    theme: &MapTheme,
) -> HashMap<(i32, i32), Rgba<u8>> {
    use crate::regions::RegionConditionKind;
    let mut out = HashMap::new();
    for region in sector.regions.iter() {
        let base = match region.kind {
            RegionConditionKind::WarpStorm => Rgba([120, 60, 180, 255]),
            RegionConditionKind::Turbulence => Rgba([110, 100, 160, 255]),
            RegionConditionKind::CalmCorridor => Rgba([80, 160, 170, 255]),
            RegionConditionKind::Blackout => Rgba([60, 60, 70, 255]),
            RegionConditionKind::Anomaly => Rgba([180, 130, 100, 255]),
            RegionConditionKind::NecropolisDrift => Rgba([100, 120, 130, 255]),
            RegionConditionKind::BeaconChain => Rgba([190, 180, 110, 255]),
            RegionConditionKind::EmpyricBleed => Rgba([150, 80, 140, 255]),
        };
        let tinted = tint_against(base, theme.region_tint_strength, theme.hex_empty);
        for h in &region.hexes {
            out.insert((h.q, h.r), tinted);
        }
    }
    out
}

fn draw_hex_grid(
    s: &mut String,
    sector: &GeneratedSector,
    sys_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    let region_tints = compute_region_tints(sector, theme);
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r);
            let base = region_tints
                .get(&(q, r))
                .copied()
                .unwrap_or(theme.hex_empty);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(base);
            polygon(
                s,
                &hex_vertices(cx, cy, HEX_SIZE),
                fill,
                Some(theme.hex_outline),
                1.0,
            );
        }
    }
}

fn draw_subsector_borders(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
) {
    let mut owner: HashMap<(i32, i32), &str> = HashMap::new();
    for sub in subsectors {
        for &(q, r) in &sub.hex_cells {
            owner.insert((q as i32, r as i32), sub.id.as_ref());
        }
    }
    if owner.is_empty() {
        return;
    }
    let border_thick = (HEX_SIZE * 0.10).max(2.5);
    let dot_radius = (border_thick * 0.8).max(2.0);
    let spacing = dot_radius * 2.5;
    for r in 0..sector.height as i32 {
        let deltas = offset_r_neighbors(r);
        for q in 0..sector.width as i32 {
            let Some(here_id) = owner.get(&(q, r)).copied() else {
                continue;
            };
            let (cx, cy) = hex_center(q, r);
            let v = hex_vertices(cx, cy, HEX_SIZE);
            for (i, (dq, dr)) in deltas.iter().enumerate() {
                let other = owner.get(&(q + dq, r + dr)).copied();
                let differs = match other {
                    Some(id) => id != here_id,
                    None => true,
                };
                if !differs {
                    continue;
                }
                let a = v[i];
                let b = v[(i + 1) % 6];
                let edge_len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let segments = (edge_len / spacing).ceil().max(1.0) as usize;
                for j in 0..=segments {
                    let t = j as f32 / segments as f32;
                    let mx = a.0 + (b.0 - a.0) * t;
                    let my = a.1 + (b.1 - a.1) * t;
                    circle(s, mx, my, dot_radius, theme.subsector_border, None, 0.0);
                }
            }
        }
    }
}

// ── Routes ──────────────────────────────────────────────────────────────────

fn draw_routes(s: &mut String, sector: &GeneratedSector, opts: &RenderOptions) {
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
    let len = (dx * dx + dy * dy).sqrt();
    if len <= star_r * 2.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    Some((
        (a.0 + ux * star_r, a.1 + uy * star_r),
        (b.0 - ux * star_r, b.1 - uy * star_r),
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
        let total = (dx * dx + dy * dy).sqrt();
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
            self.x0 + self.ux * t + self.nx * offset,
            self.y0 + self.uy * t + self.ny * offset,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_route_pattern(
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
    let mut t = spacing * 0.5;
    while t < g.total {
        let (mx, my) = g.at(t, 0.0);
        let sx = mx - g.nx * half_len;
        let sy = my - g.ny * half_len;
        let ex = mx + g.nx * half_len;
        let ey = my + g.ny * half_len;
        line(s, sx, sy, ex, ey, color, stroke_px(thickness, 0.75), None);
        t += spacing;
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
    let mut t = spacing;
    let mut sign = 1.0_f32;
    while t < g.total {
        let next = g.at(t, amplitude * sign);
        line(s, prev.0, prev.1, next.0, next.1, color, thickness, None);
        prev = next;
        t += spacing;
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
    let mut t = spacing * 0.5;
    let mut sign = 1.0_f32;
    while t < g.total {
        let next = g.at(t, amplitude * sign);
        line(s, prev.0, prev.1, next.0, next.1, color, thickness, None);
        prev = next;
        t += spacing;
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
    let mut t = spacing * 0.5;
    let mut i = 0usize;
    while t < g.total {
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
        t += spacing;
        i += 1;
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
    let mut t = spacing * 0.5;
    while t < g.total {
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            let (mx, my) = g.at(t + local, 0.0);
            circle(s, mx, my, radius.max(1.0), color, None, 0.0);
        }
        t += spacing;
    }
}

fn double_taps(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let pair_gap = g.unit * 1.3;
    let half_len = thickness * 1.8;
    let mut t = spacing * 0.5;
    while t < g.total {
        for local in [-pair_gap * 0.5, pair_gap * 0.5] {
            let (mx, my) = g.at(t + local, 0.0);
            let sx = mx - g.nx * half_len;
            let sy = my - g.ny * half_len;
            let ex = mx + g.nx * half_len;
            let ey = my + g.ny * half_len;
            line(s, sx, sy, ex, ey, color, stroke_px(thickness, 0.8), None);
        }
        t += spacing;
    }
}

fn chevrons(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let size = g.unit * 1.8;
    let mut t = spacing * 0.5;
    while t < g.total {
        let tip = g.at(t + size * 0.35, 0.0);
        let back = g.at(t - size * 0.35, 0.0);
        let left = (back.0 + g.nx * size * 0.35, back.1 + g.ny * size * 0.35);
        let right = (back.0 - g.nx * size * 0.35, back.1 - g.ny * size * 0.35);
        line(s, left.0, left.1, tip.0, tip.1, color, thickness, None);
        line(s, right.0, right.1, tip.0, tip.1, color, thickness, None);
        t += spacing;
    }
}

fn tripods(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let size = (g.unit * 1.8).max(thickness * 2.5);
    let mut t = spacing * 0.5;
    while t < g.total {
        let mid = g.at(t, 0.0);
        let fwd = g.at(t + size * 0.4, 0.0);
        line(s, mid.0, mid.1, fwd.0, fwd.1, color, thickness, None);
        let l_pos = (mid.0 + g.nx * size * 0.4, mid.1 + g.ny * size * 0.4);
        let r_pos = (mid.0 - g.nx * size * 0.4, mid.1 - g.ny * size * 0.4);
        line(s, mid.0, mid.1, l_pos.0, l_pos.1, color, thickness, None);
        line(s, mid.0, mid.1, r_pos.0, r_pos.1, color, thickness, None);
        t += spacing;
    }
}

fn bursts(s: &mut String, g: RouteGeom, color: Rgba<u8>, thickness: f32, spacing: f32) {
    let radius = (thickness * 1.6).max(2.0);
    let mut t = spacing * 0.5;
    while t < g.total {
        let mid = g.at(t, 0.0);
        let a = g.at(t - radius, 0.0);
        let b = g.at(t + radius, 0.0);
        let c = (mid.0 - g.nx * radius, mid.1 - g.ny * radius);
        let d = (mid.0 + g.nx * radius, mid.1 + g.ny * radius);
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
        t += spacing;
    }
}

#[derive(Clone, Copy)]
enum ControlKind {
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
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
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

// ── Systems ─────────────────────────────────────────────────────────────────

fn draw_systems(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    opts: &RenderOptions,
) {
    let star_r = HEX_SIZE * star_radius_ratio();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        if let Some(star) = &sys.star {
            let fill = star_color(&star.colour_code);
            circle(s, cx, cy, star_r, fill, Some(darken(fill, 0.55)), 1.0);
        } else {
            let r = star_r * 3.0 / 4.0;
            rect(
                s,
                cx - r,
                cy - r,
                r * 2.0,
                r * 2.0,
                Rgba([140, 140, 150, 255]),
                None,
            );
        }

        if subsectors
            .iter()
            .any(|sub| sub.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(s, cx, cy, &opts.theme);
        }

        let pip = sector.get_worlds_for_system(sys).len();
        if pip > 0 {
            let label = format!("{pip}");
            let tx = cx + HEX_SIZE * 0.55;
            let ty = cy + HEX_SIZE * 0.55;
            text(s, tx, ty, &label, opts.theme.text, PIP_FONT, "end");
        }
    }
}

fn draw_capital_marker(s: &mut String, cx: f32, cy: f32, theme: &MapTheme) {
    let r = (HEX_SIZE * 0.15).max(3.5);
    let dy = -HEX_SIZE * 0.55;
    let cy_marker = cy + dy;
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        rect(
            s,
            cx - r,
            cy_marker - r * 0.5,
            r * 2.0,
            r,
            theme.capital_marker,
            Some(theme.capital_outline),
        );
        return;
    }
    if matches!(theme.symbol_set, SymbolSet::Tactical) {
        line(
            s,
            cx - r,
            cy_marker,
            cx + r,
            cy_marker,
            theme.capital_marker,
            2.0,
            None,
        );
        line(
            s,
            cx,
            cy_marker - r,
            cx,
            cy_marker + r,
            theme.capital_marker,
            2.0,
            None,
        );
        circle(
            s,
            cx,
            cy_marker,
            r,
            Rgba([0, 0, 0, 0]),
            Some(theme.capital_outline),
            1.0,
        );
        return;
    }
    let pts = [
        (cx, cy_marker - r),
        (cx + r, cy_marker),
        (cx, cy_marker + r),
        (cx - r, cy_marker),
    ];
    polygon(
        s,
        &pts,
        theme.capital_marker,
        Some(theme.capital_outline),
        1.0,
    );
}

// ── Labels ──────────────────────────────────────────────────────────────────

fn draw_region_labels(
    s: &mut String,
    sector: &GeneratedSector,
    theme: &MapTheme,
    bounds: MapBounds,
) {
    if sector.regions.is_empty() || matches!(theme.label_density, LabelDensity::None) {
        return;
    }
    let bg = tint_against(theme.panel_bg, 0.70, theme.hex_empty);
    let outline = darken(theme.text_dim, 0.45);
    for region in sector.regions.iter() {
        let Some((cx, cy)) = region_label_anchor(region) else {
            continue;
        };
        let label = short(&region.name.to_ascii_uppercase(), 18);
        let pad_x = 6.0;
        let pad_y = 3.0;
        let tw = label.chars().count() as f32 * REGION_LABEL_FONT * 0.6;
        let bw = tw + pad_x * 2.0;
        let bh = REGION_LABEL_FONT + pad_y * 2.0;
        let x = (cx - bw * 0.5).clamp(0.0, (bounds.w - bw).max(0.0));
        let y = (cy - bh * 0.5).clamp(0.0, (bounds.h - bh).max(0.0));
        rect(s, x, y, bw, bh, bg, Some(outline));
        text(
            s,
            x + bw * 0.5,
            y + bh * 0.5 + REGION_LABEL_FONT * 0.35,
            &label,
            theme.text_dim,
            REGION_LABEL_FONT,
            "middle",
        );
    }
}

fn region_label_anchor(region: &crate::regions::WarpRegion) -> Option<(f32, f32)> {
    if region.hexes.is_empty() {
        return None;
    }
    let mut sx = 0.0_f32;
    let mut sy = 0.0_f32;
    for h in &region.hexes {
        let (cx, cy) = hex_center(h.q, h.r);
        sx += cx;
        sy += cy;
    }
    let n = region.hexes.len() as f32;
    Some((sx / n, sy / n))
}

fn draw_system_labels(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    opts: &RenderOptions,
) {
    if matches!(opts.theme.label_density, LabelDensity::None) {
        return;
    }
    let star_r = HEX_SIZE * star_radius_ratio();
    for sys in sector.systems.iter() {
        if !system_label_visible(sys, subsectors, &opts.theme, sector) {
            continue;
        }
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        let label = sys.name.to_ascii_uppercase();
        let ty = cy + star_r + 3.0 + SYS_LABEL_FONT;
        let tw = label.chars().count() as f32 * SYS_LABEL_FONT * 0.6;
        let pad_x = 3.0;
        let pad_y = 1.0;
        rect(
            s,
            cx - tw * 0.5 - pad_x,
            ty - SYS_LABEL_FONT - pad_y,
            tw + pad_x * 2.0,
            SYS_LABEL_FONT + pad_y * 2.0,
            opts.theme.bg,
            None,
        );
        text(
            s,
            cx,
            ty,
            &label,
            opts.theme.text_dim,
            SYS_LABEL_FONT,
            "middle",
        );
    }
}

fn system_label_visible(
    sys: &GeneratedSystem,
    subsectors: &[Subsector],
    theme: &MapTheme,
    sector: &GeneratedSector,
) -> bool {
    match theme.label_density {
        LabelDensity::All => true,
        LabelDensity::None => false,
        LabelDensity::ImportantOnly => {
            sector.get_worlds_for_system(sys).len() >= 4
                || !sys.primary_factions.is_empty()
                || subsectors.iter().any(|sub| {
                    sub.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str())
                })
        }
    }
}

fn draw_subsector_labels(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
    bounds: MapBounds,
) {
    let line_gap = 2.0;
    let pad_x = 6.0;
    let pad_y = 2.0;

    let sys_pad_x = 3.0;
    let sys_pad_y = 1.0;
    let hex_half_w = HEX_SIZE * 3f32.sqrt() / 2.0;
    let hex_half_h = HEX_SIZE;
    let star_r = HEX_SIZE * star_radius_ratio();

    let mut obstacles: Vec<Rect> = Vec::with_capacity(sector.systems.len() * 2);
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        obstacles.push(Rect {
            x0: cx - hex_half_w,
            y0: cy - hex_half_h,
            x1: cx + hex_half_w,
            y1: cy + hex_half_h,
        });
        let name = sys.name.to_ascii_uppercase();
        let tw = name.chars().count() as f32 * SYS_LABEL_FONT * 0.6;
        let ty = cy + star_r + 3.0 + SYS_LABEL_FONT;
        obstacles.push(Rect {
            x0: cx - tw * 0.5 - sys_pad_x,
            y0: ty - SYS_LABEL_FONT - sys_pad_y,
            x1: cx + tw * 0.5 + sys_pad_x,
            y1: ty + sys_pad_y,
        });
    }

    let mut placed: Vec<Rect> = Vec::with_capacity(subsectors.len());
    for sub in subsectors {
        if sub.system_ids.is_empty() || sub.hex_cells.is_empty() {
            continue;
        }
        let cells: HashSet<(i32, i32)> = sub
            .hex_cells
            .iter()
            .map(|&(q, r)| (q as i32, r as i32))
            .collect();

        let top = "SUBSECTOR";
        let bot_owned = sub
            .name
            .strip_prefix("Subsector ")
            .unwrap_or_else(|| sub.name.as_ref())
            .to_ascii_uppercase();

        let tw_top = top.chars().count() as f32 * SUB_LABEL_FONT * 0.6;
        let tw_bot = bot_owned.chars().count() as f32 * SUB_LABEL_FONT * 0.6;
        let block_w = tw_top.max(tw_bot);
        let block_h = SUB_LABEL_FONT * 2.0 + line_gap;

        let mut sx = 0.0_f32;
        let mut sy = 0.0_f32;
        for &(q, r) in &sub.hex_cells {
            let (cx, cy) = hex_center(q as i32, r as i32);
            sx += cx;
            sy += cy;
        }
        let n = sub.hex_cells.len() as f32;
        let cen_x = sx / n;
        let cen_y = sy / n;

        let sys_cells: HashSet<(i32, i32)> = sector
            .systems
            .iter()
            .map(|sys| (sys.coord.q, sys.coord.r))
            .collect();
        let mut cands: Vec<(i32, i32, f32)> = sub
            .hex_cells
            .iter()
            .filter(|&&(q, r)| !sys_cells.contains(&(q as i32, r as i32)))
            .map(|&(q, r)| {
                let (cx, cy) = hex_center(q as i32, r as i32);
                let dx = cx - cen_x;
                let dy = cy - cen_y;
                (q as i32, r as i32, dx * dx + dy * dy)
            })
            .collect();
        cands.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        let try_place = |q: i32, r: i32, above: bool, placed: &[Rect]| -> Option<(f32, f32)> {
            let nbrs = offset_r_neighbors(r);
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
            let (cx, cy) = hex_center(q, r);
            let block_top = if above {
                cy - HEX_SIZE - block_h - 2.0
            } else {
                cy + HEX_SIZE + 2.0
            };
            let block_min_x = cx - block_w * 0.5;
            let rect_b = Rect {
                x0: block_min_x - pad_x,
                y0: block_top - pad_y,
                x1: block_min_x + block_w + pad_x,
                y1: block_top + block_h + pad_y,
            };
            if rect_b.x0 < 0.0 || rect_b.y0 < 0.0 || rect_b.x1 > bounds.w || rect_b.y1 > bounds.h {
                return None;
            }
            for o in obstacles.iter().chain(placed.iter()) {
                if rect_b.intersects(*o) {
                    return None;
                }
            }
            Some((block_min_x, block_top))
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

        let (block_min_x, block_top_y) = chosen.unwrap_or_else(|| {
            let &(q0, r0) = sub
                .hex_cells
                .iter()
                .min_by(|&&(q1, r1), &&(q2, r2)| {
                    let (cx1, cy1) = hex_center(q1 as i32, r1 as i32);
                    let (cx2, cy2) = hex_center(q2 as i32, r2 as i32);
                    let d1 = (cx1 - cen_x).powi(2) + (cy1 - cen_y).powi(2);
                    let d2 = (cx2 - cen_x).powi(2) + (cy2 - cen_y).powi(2);
                    d1.partial_cmp(&d2).unwrap_or(std::cmp::Ordering::Equal)
                })
                .expect("non-empty");
            let (cx, cy) = hex_center(q0 as i32, r0 as i32);
            let bt = cy - HEX_SIZE - block_h - 2.0;
            let bmx = (cx - block_w * 0.5)
                .max(pad_x)
                .min(bounds.w - block_w - pad_x);
            let bty = bt.max(pad_y).min(bounds.h - block_h - pad_y);
            (bmx, bty)
        });

        rect(
            s,
            block_min_x - pad_x,
            block_top_y - pad_y,
            block_w + pad_x * 2.0,
            block_h + pad_y * 2.0,
            theme.subsector_label_bg,
            None,
        );
        text(
            s,
            block_min_x + block_w * 0.5,
            block_top_y + SUB_LABEL_FONT,
            top,
            theme.subsector_label,
            SUB_LABEL_FONT,
            "middle",
        );
        text(
            s,
            block_min_x + block_w * 0.5,
            block_top_y + SUB_LABEL_FONT * 2.0 + line_gap,
            &bot_owned,
            theme.subsector_label,
            SUB_LABEL_FONT,
            "middle",
        );

        placed.push(Rect {
            x0: block_min_x - pad_x,
            y0: block_top_y - pad_y,
            x1: block_min_x + block_w + pad_x,
            y1: block_top_y + block_h + pad_y,
        });
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Rect {
    fn intersects(self, o: Rect) -> bool {
        self.x0 < o.x1 && self.x1 > o.x0 && self.y0 < o.y1 && self.y1 > o.y0
    }
}

// ── Legend ──────────────────────────────────────────────────────────────────

fn legend_height(sector: &GeneratedSector, opts: &RenderOptions) -> f32 {
    if matches!(opts.theme.legend, LegendStyle::Hidden) {
        return 0.0;
    }
    let heatmap_lines: i32 = if matches!(opts.heatmap, HeatmapMode::Off) {
        0
    } else {
        2
    };
    if matches!(opts.theme.legend, LegendStyle::Compact) {
        let lines = 4 + 1 + 5 + 1 + factions_visible(sector) as i32 + heatmap_lines;
        return LEGEND_PAD * 2.0 + lines as f32 * LINE_H;
    }
    let route_type_rows = match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => RouteType::ALL.len() as i32,
        crate::sector_model::RouteViewMode::TopLevel => {
            crate::sector_model::RouteKind::ALL.len() as i32
        }
    };
    let route_control_lines: i32 = if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        5
    } else {
        0
    };
    let lines = 4
        + 1
        + route_type_rows
        + 1
        + 1
        + 4
        + 1
        + route_control_lines
        + 1
        + factions_visible(sector) as i32
        + heatmap_lines;
    LEGEND_PAD * 2.0 + lines as f32 * LINE_H
}

fn factions_visible(sector: &GeneratedSector) -> usize {
    if sector.factions.is_empty() {
        return 0;
    }
    let buckets = crate::importance::compute_display_buckets(
        sector,
        crate::importance::DEFAULT_MINOR_FRACTION,
        crate::importance::DEFAULT_DISPLAY_CAP,
    );
    1 + buckets.len()
}

fn draw_legend(s: &mut String, sector: &GeneratedSector, map_w: f32, opts: &RenderOptions) {
    let x0 = map_w + LEGEND_PAD;
    let mut y = LEGEND_PAD + TITLE_FONT;
    let theme = &opts.theme;

    text(
        s,
        x0,
        y,
        &format!("SECTOR: {}", sector.id.to_uppercase()),
        theme.text,
        TITLE_FONT,
        "start",
    );
    y += LINE_H + 4.0;
    text(
        s,
        x0,
        y,
        &format!("SEED: {}", short(&sector.seed, 20)),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H - 4.0;
    text(
        s,
        x0,
        y,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.all_worlds().count(),
        ),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H + 4.0;
    text(
        s,
        x0,
        y,
        &format!("THEME: {}", short(&theme.name.to_uppercase(), 20)),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H;

    if matches!(theme.legend, LegendStyle::Compact) {
        y += 4.0;
        draw_compact_legend_body(s, sector, x0, y, opts);
        return;
    }

    // ROUTE TYPE
    text(s, x0, y, "ROUTE TYPE", theme.text, BODY_FONT, "start");
    y += LINE_H;
    match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => {
            for rt in RouteType::ALL {
                draw_route_pattern(
                    s,
                    x0,
                    y - BODY_FONT * 0.3,
                    x0 + 30.0,
                    y - BODY_FONT * 0.3,
                    theme.route_type,
                    3.0,
                    rt.pattern(opts.route_view_mode),
                );
                text(s, x0 + 38.0, y, rt.label(), theme.text, BODY_FONT, "start");
                y += LINE_H;
            }
        }
        crate::sector_model::RouteViewMode::TopLevel => {
            for kind in crate::sector_model::RouteKind::ALL {
                draw_route_pattern(
                    s,
                    x0,
                    y - BODY_FONT * 0.3,
                    x0 + 30.0,
                    y - BODY_FONT * 0.3,
                    theme.route_type,
                    3.0,
                    kind.patterns()[0],
                );
                text(
                    s,
                    x0 + 38.0,
                    y,
                    kind.label(),
                    theme.text,
                    BODY_FONT,
                    "start",
                );
                y += LINE_H;
            }
        }
    }
    y += 4.0;

    // ROUTE STABILITY
    text(s, x0, y, "ROUTE STABILITY", theme.text, BODY_FONT, "start");
    y += LINE_H;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Perilous, "PERILOUS"),
    ] {
        let color = stability_color(theme, stab);
        line(
            s,
            x0,
            y - BODY_FONT * 0.3,
            x0 + 22.0,
            y - BODY_FONT * 0.3,
            color,
            3.0,
            None,
        );
        text(s, x0 + 30.0, y, name, theme.text, BODY_FONT, "start");
        y += LINE_H;
    }
    y += 4.0;

    // ROUTE CONTROL
    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        text(s, x0, y, "ROUTE CONTROL", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let glyph_cx = x0 + 8.0;
        let glyph_size = 10.0;
        let half = glyph_size * 0.5;
        let neutral = theme.route_control_neutral;
        for (name, kind) in [
            ("PATROL", ControlKind::Patrol),
            ("TOLL", ControlKind::Toll),
            ("INTERDICTION", ControlKind::Interdiction),
            ("PIRACY", ControlKind::Piracy),
        ] {
            let cy_y = y - BODY_FONT * 0.3;
            match kind {
                ControlKind::Patrol => {
                    circle(
                        s,
                        glyph_cx,
                        cy_y,
                        half,
                        neutral,
                        Some(darken(neutral, 0.5)),
                        1.0,
                    );
                }
                ControlKind::Toll => {
                    rect(
                        s,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        neutral,
                        Some(darken(neutral, 0.5)),
                    );
                }
                ControlKind::Interdiction => {
                    line(
                        s,
                        glyph_cx,
                        cy_y - half,
                        glyph_cx,
                        cy_y + half,
                        neutral,
                        2.0,
                        None,
                    );
                }
                ControlKind::Piracy => {
                    line(
                        s,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_cx + half,
                        cy_y + half,
                        neutral,
                        2.0,
                        None,
                    );
                    line(
                        s,
                        glyph_cx - half,
                        cy_y + half,
                        glyph_cx + half,
                        cy_y - half,
                        neutral,
                        2.0,
                        None,
                    );
                }
            }
            text(s, x0 + 22.0, y, name, theme.text, BODY_FONT, "start");
            y += LINE_H;
        }
        y += 4.0;
    }

    // FACTIONS
    if !sector.factions.is_empty() {
        text(s, x0, y, "FACTIONS", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            crate::importance::DEFAULT_MINOR_FRACTION,
            crate::importance::DEFAULT_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, world_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    world_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, *world_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    world_count,
                    ..
                } => (
                    label.to_uppercase(),
                    *system_count,
                    *world_count,
                    (140, 140, 150),
                ),
            };
            let swatch_color = rgba_from_tuple(swatch_rgb);
            rect(
                s,
                x0,
                y - swatch + 2.0,
                swatch,
                swatch,
                swatch_color,
                Some(darken(swatch_color, 0.5)),
            );
            text(
                s,
                x0 + swatch + 8.0,
                y,
                &format!("{} ({} SYS, {} W)", short(&label, 16), sys_n, world_n),
                theme.text_dim,
                BODY_FONT,
                "start",
            );
            y += LINE_H;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4.0;
        text(s, x0, y, "HEATMAP", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        rect(
            s,
            x0,
            y - swatch + 2.0,
            swatch,
            swatch,
            chip,
            Some(darken(chip, 0.5)),
        );
        text(
            s,
            x0 + swatch + 8.0,
            y,
            opts.heatmap.label(),
            theme.text,
            BODY_FONT,
            "start",
        );
    }
}

fn draw_compact_legend_body(
    s: &mut String,
    sector: &GeneratedSector,
    x0: f32,
    mut y: f32,
    opts: &RenderOptions,
) {
    let theme = &opts.theme;
    text(s, x0, y, "ROUTES", theme.text, BODY_FONT, "start");
    y += LINE_H;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARD"),
        (RouteStability::Perilous, "PERIL"),
    ] {
        let color = stability_color(theme, stab);
        line(
            s,
            x0,
            y - BODY_FONT * 0.3,
            x0 + 22.0,
            y - BODY_FONT * 0.3,
            color,
            route_thickness(theme, stab),
            None,
        );
        text(s, x0 + 30.0, y, name, theme.text, BODY_FONT, "start");
        y += LINE_H;
    }
    y += 4.0;
    if !sector.factions.is_empty() {
        text(s, x0, y, "FACTIONS", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            crate::importance::DEFAULT_MINOR_FRACTION,
            crate::importance::DEFAULT_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    ..
                } => (label.to_uppercase(), *system_count, (140, 140, 150)),
            };
            let swatch_color = rgba_from_tuple(swatch_rgb);
            rect(
                s,
                x0,
                y - swatch + 2.0,
                swatch,
                swatch,
                swatch_color,
                Some(darken(swatch_color, 0.5)),
            );
            text(
                s,
                x0 + swatch + 8.0,
                y,
                &format!("{} ({} SYS)", short(&label, 14), sys_n),
                theme.text_dim,
                BODY_FONT,
                "start",
            );
            y += LINE_H;
        }
    }
    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4.0;
        text(s, x0, y, "HEATMAP", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        rect(
            s,
            x0,
            y - swatch + 2.0,
            swatch,
            swatch,
            chip,
            Some(darken(chip, 0.5)),
        );
        text(
            s,
            x0 + swatch + 8.0,
            y,
            opts.heatmap.label(),
            theme.text,
            BODY_FONT,
            "start",
        );
    }
}

// ── Style helpers ──────────────────────────────────────────────────────────

fn star_color(code: &str) -> Rgba<u8> {
    match code.trim().to_ascii_uppercase().as_str() {
        "O" => Rgba([255, 150, 70, 255]),
        "B" => Rgba([180, 210, 255, 255]),
        "A" => Rgba([255, 200, 90, 255]),
        "F" => Rgba([220, 90, 200, 255]),
        "G" => Rgba([110, 210, 130, 255]),
        "K" => Rgba([200, 190, 130, 255]),
        "M" => Rgba([200, 60, 70, 255]),
        _ => Rgba([180, 180, 180, 255]),
    }
}

fn stability_color(theme: &MapTheme, s: RouteStability) -> Rgba<u8> {
    match s {
        RouteStability::Stable => theme.route_stable,
        RouteStability::Unstable => theme.route_unstable,
        RouteStability::Hazardous => theme.route_hazardous,
        RouteStability::Perilous => theme.route_perilous,
    }
}

fn route_thickness(theme: &MapTheme, stability: RouteStability) -> f32 {
    use crate::map_theme::RouteLineMode;
    let base = (HEX_SIZE * 0.08).max(2.0) * theme.route_thickness;
    let mode = if matches!(theme.route_line_mode, RouteLineMode::HazardWeighted) {
        match stability {
            RouteStability::Stable => 0.9,
            RouteStability::Unstable => 1.05,
            RouteStability::Hazardous => 1.25,
            RouteStability::Perilous => 1.45,
        }
    } else {
        1.0
    };
    (base * mode).max(1.0)
}

fn rgba_from_tuple(t: (u8, u8, u8)) -> Rgba<u8> {
    Rgba([t.0, t.1, t.2, 255])
}

fn tint_against(c: Rgba<u8>, amount: f32, base: Rgba<u8>) -> Rgba<u8> {
    let mix = |v: u8, base: u8| {
        let f = f32::from(v) * amount + f32::from(base) * (1.0 - amount);
        f.round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        mix(c.0[0], base.0[0]),
        mix(c.0[1], base.0[1]),
        mix(c.0[2], base.0[2]),
        255,
    ])
}

fn darken(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let scale = (1.0 - amount).clamp(0.0, 1.0);
    Rgba([
        (f32::from(c.0[0]) * scale) as u8,
        (f32::from(c.0[1]) * scale) as u8,
        (f32::from(c.0[2]) * scale) as u8,
        c.0[3],
    ])
}

fn dim(c: Rgba<u8>, factor: f32) -> Rgba<u8> {
    Rgba([
        ((c.0[0] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[1] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[2] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        c.0[3],
    ])
}

fn stroke_px(thickness: f32, factor: f32) -> f32 {
    (thickness * factor).max(1.0)
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

// ── SVG primitive emitters ─────────────────────────────────────────────────

fn color_hex(c: Rgba<u8>) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2])
}

fn opacity(c: Rgba<u8>) -> f32 {
    f32::from(c.0[3]) / 255.0
}

fn rect(s: &mut String, x: f32, y: f32, w: f32, h: f32, fill: Rgba<u8>, stroke: Option<Rgba<u8>>) {
    let _ = write!(
        s,
        r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{f}" fill-opacity="{fo:.3}""#,
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}""#,
            color_hex(stk),
            opacity(stk)
        );
    }
    s.push_str("/>");
}

fn circle(
    s: &mut String,
    cx: f32,
    cy: f32,
    r: f32,
    fill: Rgba<u8>,
    stroke: Option<Rgba<u8>>,
    stroke_w: f32,
) {
    let _ = write!(
        s,
        r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="{f}" fill-opacity="{fo:.3}""#,
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{stroke_w:.2}""#,
            color_hex(stk),
            opacity(stk),
        );
    }
    s.push_str("/>");
}

fn polygon(
    s: &mut String,
    pts: &[(f32, f32)],
    fill: Rgba<u8>,
    stroke: Option<Rgba<u8>>,
    stroke_w: f32,
) {
    s.push_str("<polygon points=\"");
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{x:.2},{y:.2}");
    }
    let _ = write!(
        s,
        "\" fill=\"{f}\" fill-opacity=\"{fo:.3}\"",
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{stroke_w:.2}""#,
            color_hex(stk),
            opacity(stk),
        );
    }
    s.push_str("/>");
}

#[allow(clippy::too_many_arguments)]
fn line(
    s: &mut String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgba<u8>,
    width: f32,
    dasharray: Option<&str>,
) {
    let _ = write!(
        s,
        r#"<line x1="{x0:.2}" y1="{y0:.2}" x2="{x1:.2}" y2="{y1:.2}" stroke="{c}" stroke-opacity="{o:.3}" stroke-width="{w:.2}" stroke-linecap="round""#,
        c = color_hex(color),
        o = opacity(color),
        w = width,
    );
    if let Some(da) = dasharray {
        let _ = write!(s, r#" stroke-dasharray="{da}""#);
    }
    s.push_str("/>");
}

fn text(s: &mut String, x: f32, y: f32, body: &str, color: Rgba<u8>, size: f32, anchor: &str) {
    let _ = write!(
        s,
        r#"<text x="{x:.2}" y="{y:.2}" fill="{c}" fill-opacity="{o:.3}" font-size="{size:.2}" text-anchor="{anchor}">"#,
        c = color_hex(color),
        o = opacity(color),
    );
    escape_xml_into(s, body);
    s.push_str("</text>");
}

fn escape_xml_into(out: &mut String, body: &str) {
    for c in body.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::*;
    use std::collections::BTreeMap;

    fn empty_manifest() -> GenerationManifest {
        GenerationManifest {
            project_id: "p".into(),
            generated_at_policy: "x".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: "abc".into(),
            seed_hash: "h".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: BTreeMap::new(),
            settings_digest: "s".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        }
    }

    fn sample_sector() -> GeneratedSector {
        let sys = GeneratedSystem {
            id: "s1".into(),
            index: 0,
            name: "Test".into(),
            coord: HexCoord { q: 1, r: 1 },
            kind: SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "O".into(),
                colour_name: "orange dwarf".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: Default::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        };
        GeneratedSector {
            id: "demo".into(),
            title: "Demo".into(),
            seed: "abc".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            width: 4,
            height: 4,
            systems: vec![sys],
            routes: vec![],
            factions: vec![],
            manifest: empty_manifest(),
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
            id_history: BTreeMap::new(),
        }
    }

    #[test]
    fn renders_well_formed_svg() {
        let sector = sample_sector();
        let svg = render_sector_svg(&sector, None, &RenderOptions::default());
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("polygon"));
    }
}
