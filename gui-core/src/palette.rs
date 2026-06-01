//! Shared color palette + per-entity color helpers for the GUI.
//!
//! Ported from `bitmap.rs` / `system_map.rs` so the GUI matches the PNG export
//! aesthetic. Keep all colors here so future restyling is one-file.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use sectorforge::sector_model::{GeneratedFaction, GeneratedRoute, RoutePattern, RouteStability};

pub const BG: Color32 = Color32::from_rgb(14, 12, 20);
pub const PANEL_BG: Color32 = Color32::from_rgb(22, 18, 30);
pub const HEX_EMPTY: Color32 = Color32::from_rgb(28, 26, 38);
pub const HEX_OUTLINE: Color32 = Color32::from_rgb(60, 55, 78);
pub const TEXT: Color32 = Color32::from_rgb(232, 228, 240);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 145, 165);
pub const ORBIT_RING: Color32 = Color32::from_rgb(55, 50, 72);
pub const SELECTION: Color32 = Color32::from_rgb(255, 240, 120);
pub const PATH_HIGHLIGHT: Color32 = Color32::from_rgb(120, 220, 255);
pub const PATH_WAYPOINT: Color32 = Color32::from_rgb(255, 200, 90);

/// Theme-driven *chrome* colors — panel/window backgrounds and UI text. Unlike
/// the consts above (which the map painters and `map_theme` use for the
/// always-dark canvas + semantic overlays), these flip with the active
/// [`crate::theme::Theme`] so the surrounding UI can go light. The GUI reads
/// them through [`chrome_bg`] / [`chrome_panel`] / [`chrome_text`] /
/// [`chrome_text_dim`]; [`crate::theme::Theme::apply`] pushes a new set via
/// [`set_chrome`]. The default matches the dark consts, so nothing shifts until
/// a theme is applied.
#[derive(Clone, Copy, Debug)]
pub struct ChromeColors {
    /// Darkest chrome backdrop (was [`BG`]).
    pub bg: Color32,
    /// Panel/surface fill (was [`PANEL_BG`]).
    pub panel: Color32,
    /// Primary UI text (was [`TEXT`]).
    pub text: Color32,
    /// Secondary / label UI text (was [`TEXT_DIM`]).
    pub text_dim: Color32,
}

impl ChromeColors {
    /// The original dark palette — the process-wide default until a theme runs.
    pub const DARK: ChromeColors = ChromeColors {
        bg: BG,
        panel: PANEL_BG,
        text: TEXT,
        text_dim: TEXT_DIM,
    };
}

static CHROME: std::sync::RwLock<ChromeColors> = std::sync::RwLock::new(ChromeColors::DARK);

/// Install a new chrome color set (called by [`crate::theme::Theme::apply`]).
pub fn set_chrome(colors: ChromeColors) {
    if let Ok(mut guard) = CHROME.write() {
        *guard = colors;
    }
}

/// Snapshot of the active chrome colors.
#[must_use]
pub fn chrome() -> ChromeColors {
    CHROME.read().map_or(ChromeColors::DARK, |c| *c)
}

/// Active darkest chrome backdrop. Theme-aware replacement for [`BG`] at UI
/// (non-map) call sites.
#[must_use]
pub fn chrome_bg() -> Color32 {
    chrome().bg
}

/// Active panel/surface fill. Theme-aware replacement for [`PANEL_BG`] at UI
/// (non-map) call sites.
#[must_use]
pub fn chrome_panel() -> Color32 {
    chrome().panel
}

/// Active primary UI text color. Theme-aware replacement for [`TEXT`] at UI
/// (non-map) call sites.
#[must_use]
pub fn chrome_text() -> Color32 {
    chrome().text
}

/// Active secondary UI text color. Theme-aware replacement for [`TEXT_DIM`] at
/// UI (non-map) call sites.
#[must_use]
pub fn chrome_text_dim() -> Color32 {
    chrome().text_dim
}

pub fn star_color(code: &str) -> Color32 {
    match code.trim().to_ascii_uppercase().as_str() {
        "O" => Color32::from_rgb(255, 150, 70),
        "B" => Color32::from_rgb(180, 210, 255),
        "A" => Color32::from_rgb(255, 200, 90),
        "F" => Color32::from_rgb(220, 90, 200),
        "G" => Color32::from_rgb(110, 210, 130),
        "K" => Color32::from_rgb(200, 190, 130),
        "M" => Color32::from_rgb(200, 60, 70),
        _ => Color32::from_rgb(180, 180, 180),
    }
}

/// Renders a route from `a` to `b` using the given `pattern`. Most patterns
/// draw geometric motifs (rails, ladders, ticks, chevrons, bursts) instead of
/// only changing dash rhythm, so route classes remain distinct on dense maps.
pub fn draw_route_line(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    thickness: f32,
    color: Color32,
    pattern: RoutePattern,
) {
    let Some(geom) = RouteGeom::new(a, b, thickness) else {
        return;
    };
    match pattern {
        RoutePattern::Solid => {
            painter.line_segment([a, b], Stroke::new(thickness, color));
        }
        RoutePattern::Dashed | RoutePattern::DotDash | RoutePattern::Dotted => {
            draw_strided_route(painter, geom, color, thickness, pattern.strides());
        }
        RoutePattern::Cracked => {
            draw_jagged_route(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 3.0,
                thickness * 1.7,
            );
        }
        RoutePattern::Ghost => {
            draw_strided_route(
                painter,
                geom,
                fade(color, 0.45),
                thickness,
                pattern.strides(),
            );
        }
        RoutePattern::Burst => {
            draw_bursts(painter, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Staccato => {
            draw_zigzag_route(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 3.2,
                thickness * 1.8,
            );
        }
        RoutePattern::Gravel => {
            draw_disc_trail(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 1.55,
                false,
                true,
            );
        }
        RoutePattern::Twin => {
            draw_parallel_routes(painter, geom, color, thickness, thickness * 1.0);
        }
        RoutePattern::Tripod => {
            draw_tripods(painter, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Tick => {
            draw_base_spine(painter, geom, color, thickness, 0.28);
            draw_ticks(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 4.5,
                thickness * 2.2,
            );
        }
        RoutePattern::Bridge => {
            draw_strided_route(
                painter,
                geom,
                fade(color, 0.55),
                thickness * 0.8,
                &[3.0, 2.0],
            );
            draw_ticks(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 5.0,
                thickness * 1.8,
            );
        }
        RoutePattern::Patter => {
            draw_disc_trail(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 2.2,
                true,
                false,
            );
        }
        RoutePattern::Quartet => {
            draw_dot_clusters(painter, geom, color, thickness, geom.unit * 8.0, 4);
        }
        RoutePattern::Railroad => {
            let offset = thickness * 1.25;
            draw_parallel_routes(painter, geom, color, thickness * 0.8, offset);
            draw_ticks(
                painter,
                geom,
                color,
                thickness * 0.75,
                geom.unit * 5.5,
                offset * 1.15,
            );
        }
        RoutePattern::DoubleTap => {
            draw_double_taps(painter, geom, color, thickness, geom.unit * 7.0);
        }
        RoutePattern::Pebble => {
            draw_disc_trail(
                painter,
                geom,
                color,
                thickness,
                geom.unit * 2.6,
                false,
                true,
            );
        }
        RoutePattern::Whisper => {
            draw_disc_trail(
                painter,
                geom,
                fade(color, 0.7),
                thickness,
                geom.unit * 7.0,
                false,
                false,
            );
        }
        RoutePattern::March => {
            draw_chevrons(painter, geom, color, thickness, geom.unit * 5.5);
        }
        _ => {
            painter.line_segment([a, b], Stroke::new(thickness, color));
        }
    }
}

/// §3 route control glyph categories. Mirrors `bitmap::ControlKind` so the
/// in-app sector view shows the same midpoint shapes (Patrol disc, Toll
/// square, Interdiction crossbar, Piracy X) as the PNG export.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum RouteControlKind {
    Patrol,
    Toll,
    Interdiction,
    Piracy,
}

const ROUTE_CONTROL_MIN_SCORE: f32 = 40.0;

#[must_use]
pub fn top_route_control(route: &GeneratedRoute) -> Option<(String, RouteControlKind, f32)> {
    let mut best: Option<(&str, RouteControlKind, f32)> = None;
    for c in &route.controls {
        for (kind, score) in [
            (RouteControlKind::Interdiction, c.interdiction),
            (RouteControlKind::Patrol, c.patrol),
            (RouteControlKind::Piracy, c.piracy),
            (RouteControlKind::Toll, c.toll),
        ] {
            if best.map(|(_, _, s)| score > s).unwrap_or(true) {
                best = Some((c.faction_id.as_str(), kind, score));
            }
        }
    }
    best.map(|(id, k, s)| (id.to_string(), k, s))
}

/// At the midpoint of a route, draw a shape for the single strongest
/// `RouteControl` category when its score is at least
/// [`ROUTE_CONTROL_MIN_SCORE`]. Color comes from the controlling faction's
/// fill so the reader can identify who asserts along the route.
pub fn draw_route_control_glyph(
    painter: &egui::Painter,
    factions: &[GeneratedFaction],
    route: &GeneratedRoute,
    a: Pos2,
    b: Pos2,
    thickness: f32,
) {
    let Some((faction_id, kind, score)) = top_route_control(route) else {
        return;
    };
    if score < ROUTE_CONTROL_MIN_SCORE {
        return;
    }
    let color = faction_style_by_id(factions, &faction_id).fill;
    let dark = darken(color, 0.5);
    let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let size = (thickness * 3.0).max(6.0);
    let stroke_w = thickness.max(2.0);
    let outline = Stroke::new(1.0, dark);
    match kind {
        RouteControlKind::Patrol => {
            let r = size * 0.5;
            painter.circle_filled(mid, r, color);
            painter.circle_stroke(mid, r, outline);
        }
        RouteControlKind::Toll => {
            let half = size * 0.5;
            let rect =
                egui::Rect::from_min_size(Pos2::new(mid.x - half, mid.y - half), Vec2::splat(size));
            painter.rect_filled(rect, 0.0, color);
            painter.rect_stroke(rect, 0.0, outline);
        }
        RouteControlKind::Interdiction => {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size;
            let p0 = Pos2::new(mid.x - px * half, mid.y - py * half);
            let p1 = Pos2::new(mid.x + px * half, mid.y + py * half);
            painter.line_segment([p0, p1], Stroke::new(stroke_w, color));
        }
        RouteControlKind::Piracy => {
            let half = size * 0.5;
            let stroke = Stroke::new(stroke_w, color);
            painter.line_segment(
                [
                    Pos2::new(mid.x - half, mid.y - half),
                    Pos2::new(mid.x + half, mid.y + half),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(mid.x - half, mid.y + half),
                    Pos2::new(mid.x + half, mid.y - half),
                ],
                stroke,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct RouteGeom {
    a: Pos2,
    b: Pos2,
    dir: Vec2,
    normal: Vec2,
    total: f32,
    unit: f32,
}

impl RouteGeom {
    fn new(a: Pos2, b: Pos2, thickness: f32) -> Option<Self> {
        let delta = b - a;
        let total = delta.length();
        if total <= 0.0 {
            return None;
        }
        let dir = delta / total;
        Some(Self {
            a,
            b,
            dir,
            normal: Vec2::new(-dir.y, dir.x),
            total,
            unit: thickness.max(2.0),
        })
    }

    fn at(self, t: f32, offset: f32) -> Pos2 {
        self.a + self.dir * t.clamp(0.0, self.total) + self.normal * offset
    }
}

fn draw_strided_route(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    strides: &[f32],
) {
    if strides.is_empty() {
        painter.line_segment([geom.a, geom.b], Stroke::new(thickness, color));
        return;
    }
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < geom.total {
        let stride = strides[idx % strides.len()];
        let seg = stride * geom.unit;
        let next_t = (t + seg).min(geom.total);
        if idx.is_multiple_of(2) {
            let p0 = geom.at(t, 0.0);
            let p1 = geom.at(next_t, 0.0);
            if stride <= 1.5 {
                let mid = p0 + (p1 - p0) * 0.5;
                painter.circle_filled(mid, thickness * 0.6, color);
            } else {
                painter.line_segment([p0, p1], Stroke::new(thickness, color));
            }
        }
        t = next_t;
        idx += 1;
    }
}

fn draw_parallel_routes(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    offset: f32,
) {
    for side in [-offset, offset] {
        painter.line_segment(
            [geom.at(0.0, side), geom.at(geom.total, side)],
            Stroke::new(thickness, color),
        );
    }
}

fn draw_base_spine(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    alpha: f32,
) {
    painter.line_segment(
        [geom.a, geom.b],
        Stroke::new((thickness * 0.7).max(1.0), fade(color, alpha)),
    );
}

fn draw_ticks(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
    half_len: f32,
) {
    let mut t = spacing * 0.5;
    while t < geom.total {
        let mid = geom.at(t, 0.0);
        painter.line_segment(
            [mid - geom.normal * half_len, mid + geom.normal * half_len],
            Stroke::new((thickness * 0.75).max(1.0), color),
        );
        t += spacing;
    }
}

fn draw_jagged_route(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = geom.a;
    let mut t = spacing;
    let mut sign = 1.0;
    while t < geom.total {
        let next = geom.at(t, amplitude * sign);
        painter.line_segment([prev, next], Stroke::new(thickness, color));
        prev = next;
        t += spacing;
        sign = -sign;
    }
    painter.line_segment([prev, geom.b], Stroke::new(thickness, color));
}

fn draw_zigzag_route(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = geom.at(0.0, -amplitude);
    let mut t = spacing * 0.5;
    let mut sign = 1.0;
    while t < geom.total {
        let next = geom.at(t, amplitude * sign);
        painter.line_segment([prev, next], Stroke::new(thickness, color));
        prev = next;
        t += spacing;
        sign = -sign;
    }
    painter.line_segment(
        [prev, geom.at(geom.total, -amplitude * sign)],
        Stroke::new(thickness, color),
    );
}

fn draw_disc_trail(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
    hollow: bool,
    alternating: bool,
) {
    let mut t = spacing * 0.5;
    let mut i = 0usize;
    while t < geom.total {
        let radius = if alternating && i.is_multiple_of(2) {
            thickness * 0.85
        } else {
            thickness * 0.55
        }
        .max(1.0);
        let mid = geom.at(t, 0.0);
        if hollow {
            painter.circle_stroke(mid, radius, Stroke::new((thickness * 0.45).max(1.0), color));
        } else {
            painter.circle_filled(mid, radius, color);
        }
        t += spacing;
        i += 1;
    }
}

fn draw_dot_clusters(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
    count: usize,
) {
    let dot_gap = geom.unit * 1.25;
    let radius = (thickness * 0.55).max(1.0);
    let mut t = spacing * 0.5;
    while t < geom.total {
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            painter.circle_filled(geom.at(t + local, 0.0), radius, color);
        }
        t += spacing;
    }
}

fn draw_double_taps(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
) {
    let pair_gap = geom.unit * 1.3;
    let half = thickness * 1.8;
    let mut t = spacing * 0.5;
    while t < geom.total {
        for local in [-pair_gap * 0.5, pair_gap * 0.5] {
            let mid = geom.at(t + local, 0.0);
            painter.line_segment(
                [mid - geom.normal * half, mid + geom.normal * half],
                Stroke::new((thickness * 0.8).max(1.0), color),
            );
        }
        t += spacing;
    }
}

fn draw_chevrons(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
) {
    let size = geom.unit * 1.8;
    let mut t = spacing * 0.5;
    while t < geom.total {
        let tip = geom.at(t + size * 0.35, 0.0);
        let back = geom.at(t - size * 0.35, 0.0);
        painter.line_segment(
            [back + geom.normal * size * 0.35, tip],
            Stroke::new(thickness, color),
        );
        painter.line_segment(
            [back - geom.normal * size * 0.35, tip],
            Stroke::new(thickness, color),
        );
        t += spacing;
    }
}

fn draw_tripods(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
) {
    let size = (geom.unit * 1.8).max(thickness * 2.5);
    let mut t = spacing * 0.5;
    let stroke = Stroke::new(thickness, color);
    while t < geom.total {
        let mid = geom.at(t, 0.0);
        // Forward "leg"
        painter.line_segment([mid, geom.at(t + size * 0.4, 0.0)], stroke);
        // Lateral "legs" (creating the T/fork)
        painter.line_segment([mid, mid + geom.normal * size * 0.4], stroke);
        painter.line_segment([mid, mid - geom.normal * size * 0.4], stroke);
        t += spacing;
    }
}

fn draw_bursts(
    painter: &egui::Painter,
    geom: RouteGeom,
    color: Color32,
    thickness: f32,
    spacing: f32,
) {
    let radius = (thickness * 1.6).max(2.0);
    let mut t = spacing * 0.5;
    while t < geom.total {
        let mid = geom.at(t, 0.0);
        painter.line_segment(
            [mid - geom.dir * radius, mid + geom.dir * radius],
            Stroke::new((thickness * 0.65).max(1.0), color),
        );
        painter.line_segment(
            [mid - geom.normal * radius, mid + geom.normal * radius],
            Stroke::new((thickness * 0.65).max(1.0), color),
        );
        painter.circle_filled(mid, (thickness * 0.45).max(1.0), color);
        t += spacing;
    }
}

fn fade(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        ((color.a() as f32) * alpha).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn stability_color(s: RouteStability) -> Color32 {
    match s {
        RouteStability::Stable => Color32::from_rgb(110, 210, 130),
        RouteStability::Unstable => Color32::from_rgb(240, 200, 90),
        RouteStability::Hazardous => Color32::from_rgb(235, 90, 90),
        RouteStability::Perilous => Color32::from_rgb(165, 100, 215),
        _ => Color32::from_rgb(150, 150, 150),
    }
}

pub fn world_type_color(t: &str) -> Color32 {
    match t {
        "AgriWorld" => Color32::from_rgb(120, 200, 110),
        "Asteroid" => Color32::from_rgb(150, 145, 130),
        "BastionWorld" => Color32::from_rgb(170, 175, 185),
        "DeathWorld" => Color32::from_rgb(200, 60, 70),
        "DeadWorld" => Color32::from_rgb(90, 85, 95),
        "ExtractiveColony" => Color32::from_rgb(200, 130, 70),
        "FeralWorld" => Color32::from_rgb(100, 150, 90),
        "FeudalWorld" => Color32::from_rgb(180, 130, 70),
        "ForgeWorld" => Color32::from_rgb(220, 110, 50),
        "FrontierWorld" => Color32::from_rgb(210, 190, 130),
        "HiveWorld" => Color32::from_rgb(200, 80, 200),
        "ImperialWorld" => Color32::from_rgb(230, 220, 90),
        "IndustrialWorld" => Color32::from_rgb(180, 110, 60),
        "Orbital" => Color32::from_rgb(110, 200, 230),
        "PenalWorld" => Color32::from_rgb(130, 60, 60),
        "PlanetaryDump" => Color32::from_rgb(130, 130, 80),
        "PlanetaryMonument" => Color32::from_rgb(230, 200, 90),
        "PleasureWorld" => Color32::from_rgb(240, 150, 200),
        "ResearchStation" => Color32::from_rgb(130, 170, 230),
        "ShrineWorld" => Color32::from_rgb(230, 220, 200),
        "TombWorld" => Color32::from_rgb(200, 200, 210),
        "WarpLostWorld" => Color32::from_rgb(140, 90, 200),
        "Worldship" => Color32::from_rgb(80, 100, 180),
        "XenosWorld" => Color32::from_rgb(120, 220, 120),
        _ => Color32::from_rgb(180, 180, 180),
    }
}

pub fn darken(c: Color32, amount: f32) -> Color32 {
    let s = (1.0 - amount).clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (f32::from(c.r()) * s) as u8,
        (f32::from(c.g()) * s) as u8,
        (f32::from(c.b()) * s) as u8,
        c.a(),
    )
}

pub fn tint(c: Color32, amount: f32) -> Color32 {
    let mix = |v: u8, base: u8| {
        let f = f32::from(v) * amount + f32::from(base) * (1.0 - amount);
        f.round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(
        mix(c.r(), HEX_EMPTY.r()),
        mix(c.g(), HEX_EMPTY.g()),
        mix(c.b(), HEX_EMPTY.b()),
    )
}

pub fn contrast_text(c: Color32) -> Color32 {
    let luma = 0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b());
    if luma > 140.0 {
        Color32::from_rgb(20, 20, 30)
    } else {
        Color32::from_rgb(240, 240, 245)
    }
}

// ── Per-faction style (§8) ───────────────────────────────────────────────────
//
// Core hue/glyph/border logic lives in [`sectorforge::faction_style`] (no GUI deps);
// this wrapper just re-paints the RGB primitives as `egui::Color32`.

pub use sectorforge::faction_style::FactionBorder;

#[derive(Debug, Clone, Copy)]
pub struct FactionStyle {
    pub fill: Color32,
    pub accent: Color32,
    pub glyph: char,
    pub border: FactionBorder,
}

fn from_rgb(t: (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(t.0, t.1, t.2)
}

/// Resolve a `FactionStyle` from a sector's faction list by id.
#[must_use]
pub fn faction_style_by_id(factions: &[GeneratedFaction], id: &str) -> FactionStyle {
    let rgb = sectorforge::faction_style::faction_style_rgb_by_id(factions, id);
    FactionStyle {
        fill: from_rgb(rgb.fill),
        accent: from_rgb(rgb.accent),
        glyph: rgb.glyph,
        border: rgb.border,
    }
}

/// Build a deterministic visual style for a faction.
#[must_use]
pub fn faction_style(kind: &str, id: &str, disposition: &str) -> FactionStyle {
    let rgb = sectorforge::faction_style::faction_style_rgb(kind, id, disposition);
    FactionStyle {
        fill: from_rgb(rgb.fill),
        accent: from_rgb(rgb.accent),
        glyph: rgb.glyph,
        border: rgb.border,
    }
}

pub fn draw_faction_chip(ui: &mut Ui, style: FactionStyle) {
    draw_faction_chip_sized(ui, style, Vec2::new(20.0, 20.0), 3.0, 13.0);
}

pub fn draw_faction_chip_sized(
    ui: &mut Ui,
    style: FactionStyle,
    size: Vec2,
    rounding: f32,
    font_size: f32,
) {
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let bg = match style.border {
        FactionBorder::Dotted => {
            Color32::from_rgba_unmultiplied(style.fill.r(), style.fill.g(), style.fill.b(), 150)
        }
        _ => style.fill,
    };
    painter.rect_filled(rect, rounding, bg);
    match style.border {
        FactionBorder::Clean => {
            painter.rect_stroke(rect, rounding, Stroke::new(1.2, style.accent));
        }
        FactionBorder::Jagged => {
            painter.rect_stroke(rect, rounding, Stroke::new(2.4, style.accent));
        }
        FactionBorder::Dotted => {
            painter.rect_stroke(rect, rounding, Stroke::new(1.0, PANEL_BG));
        }
        FactionBorder::Thin => {
            painter.rect_stroke(rect, rounding, Stroke::new(0.8, style.accent));
        }
        _ => {
            painter.rect_stroke(rect, rounding, Stroke::new(1.0, style.accent));
        }
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        style.glyph.to_string(),
        FontId::monospace(font_size),
        contrast_text(style.fill),
    );
}

pub fn draw_faction_style_rgb_preview(
    ui: &mut Ui,
    style: sectorforge::faction_style::FactionStyleRgb,
) {
    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(96.0, 32.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let fill = from_rgb(style.fill);
    let accent = from_rgb(style.accent);
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(rect, 4.0, Stroke::new(2.0, accent));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        style.glyph.to_string(),
        FontId::monospace(16.0),
        Color32::BLACK,
    );
}

pub fn draw_fraction_bar(
    ui: &mut Ui,
    size: Vec2,
    fraction: f32,
    fill: Color32,
    bg: Color32,
    outline: Stroke,
    rounding: f32,
) {
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rounding, bg);
    let fill_width = (fraction.clamp(0.0, 1.0) * size.x).max(1.0);
    let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_width, size.y));
    painter.rect_filled(fill_rect, rounding, fill);
    painter.rect_stroke(rect, rounding, outline);
}

pub fn paint_rect_filled(ui: &Ui, clip_rect: Rect, rect: Rect, rounding: f32, fill: Color32) {
    ui.painter_at(clip_rect).rect_filled(rect, rounding, fill);
}

pub fn paint_rect_stroke(ui: &Ui, clip_rect: Rect, rect: Rect, rounding: f32, stroke: Stroke) {
    ui.painter_at(clip_rect).rect_stroke(rect, rounding, stroke);
}

pub fn paint_text(
    ui: &Ui,
    clip_rect: Rect,
    pos: Pos2,
    align: Align2,
    text: impl ToString,
    font_id: FontId,
    color: Color32,
) {
    ui.painter_at(clip_rect)
        .text(pos, align, text, font_id, color);
}

pub fn paint_circle_filled(ui: &Ui, clip_rect: Rect, center: Pos2, radius: f32, fill: Color32) {
    ui.painter_at(clip_rect).circle_filled(center, radius, fill);
}

pub fn paint_circle_stroke(ui: &Ui, clip_rect: Rect, center: Pos2, radius: f32, stroke: Stroke) {
    ui.painter_at(clip_rect)
        .circle_stroke(center, radius, stroke);
}

pub fn draw_route_line_clipped(
    ui: &Ui,
    clip_rect: Rect,
    a: Pos2,
    b: Pos2,
    thickness: f32,
    color: Color32,
    pattern: RoutePattern,
) {
    let painter = ui.painter_at(clip_rect);
    draw_route_line(&painter, a, b, thickness, color, pattern);
}

pub const STAR_LEGEND: &[(&str, &str)] = &[
    ("O", "ORANGE DWARF"),
    ("B", "BLUE-WHITE"),
    ("A", "AMBER"),
    ("F", "FUCHSIA"),
    ("G", "GREEN"),
    ("K", "KHAKI"),
    ("M", "MAROON"),
];
