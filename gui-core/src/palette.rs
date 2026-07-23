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

// ── §SPRUCE D7 — semantic status colors (success / warning / danger / info) ───
//
// The defect: panels reached for ad-hoc `Color32::from_rgb(...)` amber / red /
// green triples for "unsaved", validation, health pips, and badges — ~50 of them
// scattered across ~25 panels — and the warm amber collided with the Grimdark
// *brass accent*, so a clean validation line read like a warning (SPRUCE §1 D7).
//
// This is the single semantic-color source. Like [`ChromeColors`] it is a
// process-wide snapshot the custom painters / panels read without threading a
// `&Ui`, swapped by [`crate::theme::Theme::apply`]. Two sets only — `DARK` and
// `LIGHT` — keyed on the preset's `dark` flag rather than per-preset: these
// encode *meaning*, not brand, so a red error reads red under every dark preset,
// and keeping them theme-independent guarantees the warning hue never coincides
// with a preset's own accent (the D7 failure). `neutral` is deliberately absent —
// the muted/"clean" state is [`chrome_text_dim`], already theme-aware.
#[derive(Clone, Copy, Debug)]
pub struct StatusColors {
    /// Clean / ok / pass — a measured green.
    pub success: Color32,
    /// Warning — an orange pushed clear of any brass/gold accent (SPRUCE D7).
    pub warning: Color32,
    /// Error / destructive — a desaturated red.
    pub danger: Color32,
    /// Neutral notice / informational highlight — a muted blue.
    pub info: Color32,
}

impl StatusColors {
    /// Status colors tuned for the seven dark presets (SPRUCE §3.1 Grimdark row).
    pub const DARK: StatusColors = StatusColors {
        success: Color32::from_rgb(0x7F, 0xB0, 0x69),
        warning: Color32::from_rgb(0xE0, 0x91, 0x3A),
        danger: Color32::from_rgb(0xD2, 0x60, 0x3F),
        info: Color32::from_rgb(0x5E, 0x8C, 0xA8),
    };
    /// Status colors darkened + saturated to read on the `Light` parchment panel.
    /// `danger` is pushed vermilion so it stays distinct from the crimson accent.
    pub const LIGHT: StatusColors = StatusColors {
        success: Color32::from_rgb(0x33, 0x70, 0x33),
        warning: Color32::from_rgb(0xB0, 0x66, 0x12),
        danger: Color32::from_rgb(0xC2, 0x46, 0x2A),
        info: Color32::from_rgb(0x2C, 0x5A, 0x82),
    };
}

static STATUS: std::sync::RwLock<StatusColors> = std::sync::RwLock::new(StatusColors::DARK);

/// Install the active status-color set (called by [`crate::theme::Theme::apply`]).
pub fn set_status(colors: StatusColors) {
    if let Ok(mut guard) = STATUS.write() {
        *guard = colors;
    }
}

/// Snapshot of the active status colors.
#[must_use]
pub fn status() -> StatusColors {
    STATUS.read().map_or(StatusColors::DARK, |c| *c)
}

/// Active **success** color (clean / ok / pass). Theme-aware.
#[must_use]
pub fn success() -> Color32 {
    status().success
}

/// Active **warning** color — distinct from any preset accent. Theme-aware.
#[must_use]
pub fn warning() -> Color32 {
    status().warning
}

/// Active **danger** color (error / destructive). Theme-aware.
#[must_use]
pub fn danger() -> Color32 {
    status().danger
}

/// Active **info** color (neutral notice). Theme-aware.
#[must_use]
pub fn info() -> Color32 {
    status().info
}

/// Color a validation summary by state (SPRUCE D7): muted when clean, warning
/// amber for warnings only, danger red once any error fires. The muted/clean
/// case is [`chrome_text_dim`] so a `0 err / 0 warn` line never reads as amber.
#[must_use]
pub fn validation_color(errors: usize, warnings: usize) -> Color32 {
    if errors > 0 {
        danger()
    } else if warnings > 0 {
        warning()
    } else {
        chrome_text_dim()
    }
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

impl RouteControlKind {
    /// Every control category, in the order the route-control legend lists
    /// them. Single source of truth for the legend (see
    /// [`crate::info_panel`]); iterate this rather than re-spelling the set.
    pub const ALL: &'static [RouteControlKind] = &[
        RouteControlKind::Patrol,
        RouteControlKind::Toll,
        RouteControlKind::Interdiction,
        RouteControlKind::Piracy,
    ];

    /// Uppercase display label used in the legend.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            RouteControlKind::Patrol => "PATROL",
            RouteControlKind::Toll => "TOLL",
            RouteControlKind::Interdiction => "INTERDICTION",
            RouteControlKind::Piracy => "PIRACY",
        }
    }
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

/// Data-viz palette — intentionally **not** [`warning`]/[`danger`]. Route
/// stability encodes a domain-lore tier painted on the map canvas, not a UI
/// status; these values are fixed regardless of the active chrome theme. The
/// numeric overlap with [`StatusColors`] amber/red is coincidental, not a
/// coupling — see AREA_F F12.
pub fn stability_color(s: RouteStability) -> Color32 {
    match s {
        RouteStability::Stable => Color32::from_rgb(110, 210, 130),
        RouteStability::Unstable => Color32::from_rgb(240, 200, 90),
        RouteStability::Hazardous => Color32::from_rgb(235, 90, 90),
        RouteStability::Perilous => Color32::from_rgb(165, 100, 215),
        _ => Color32::from_rgb(150, 150, 150),
    }
}

/// Thin wrapper over [`sectorforge::export::system_map::world_type_color`]
/// (PONYTAIL #24 — the two tables were byte-identical) converting its
/// `image::Rgba<u8>` to `egui::Color32`.
pub fn world_type_color(t: &str) -> Color32 {
    let image::Rgba([r, g, b, a]) = sectorforge::export::system_map::world_type_color(t);
    Color32::from_rgba_premultiplied(r, g, b, a)
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
    let rounding = 3.0;
    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
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
        FontId::monospace(13.0),
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

/// A compact faction colour swatch (fill + accent outline + glyph), sized for
/// inline use in list rows. Mirrors [`draw_faction_style_rgb_preview`] at a
/// smaller scale so a roster can show each faction's map colour at a glance.
pub fn draw_faction_swatch(ui: &mut Ui, style: sectorforge::faction_style::FactionStyleRgb) {
    let side = ui.spacing().interact_size.y.min(16.0);
    let (rect, _resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, from_rgb(style.fill));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, from_rgb(style.accent)));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        style.glyph.to_string(),
        FontId::monospace(side * 0.7),
        contrast_text(from_rgb(style.fill)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::route_control::RouteControl;
    // `GeneratedRoute` and `RouteStability` already arrive via `use super::*;`
    // (re-exported from this module's own top-level `use`).
    use sectorforge::sector_model::{GeneratedSector, HexCoord, RouteType};

    // Build a `GeneratedRoute` carrying `controls`. `#[cfg(test)]` may bypass the
    // command bus to assemble a fixture; the route is fully square (8×8 sector).
    fn route_with_controls(controls: Vec<RouteControl>) -> GeneratedRoute {
        let mut s = GeneratedSector::empty("t", "T", "seed", 8, 8);
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let mut r = s.routes.into_iter().next().unwrap();
        r.controls = controls;
        r
    }

    // ── Gap 1: top_route_control ──────────────────────────────────────────────
    // Probes kinds in order [Interdiction, Patrol, Piracy, Toll] with a STRICT
    // `>` update, so ties resolve to the first-probed kind. Returns the raw max
    // score with NO 40-threshold filtering (that floor lives in the glyph fn).
    // `RouteControlKind` has no `PartialEq` — assert variants via `matches!`.

    #[test]
    fn top_route_control_empty_is_none() {
        let r = route_with_controls(vec![]);
        assert!(top_route_control(&r).is_none());
    }

    #[test]
    fn top_route_control_single_returns_faction_kind_score() {
        let r = route_with_controls(vec![RouteControl {
            faction_id: "fac-1".into(),
            patrol: 70.0,
            ..Default::default()
        }]);
        let (id, kind, score) = top_route_control(&r).expect("one control yields a result");
        assert_eq!(id, "fac-1");
        assert!(matches!(kind, RouteControlKind::Patrol));
        assert_eq!(score, 70.0);
    }

    #[test]
    fn top_route_control_tie_prefers_first_probed_kind() {
        // Interdiction and Patrol both at 50: Interdiction is probed first and
        // the update is strict `>`, so Interdiction holds.
        let r = route_with_controls(vec![RouteControl {
            faction_id: "f".into(),
            interdiction: 50.0,
            patrol: 50.0,
            ..Default::default()
        }]);
        let (_, kind, score) = top_route_control(&r).expect("tie still yields a result");
        assert!(matches!(kind, RouteControlKind::Interdiction));
        assert_eq!(score, 50.0);
    }

    #[test]
    fn top_route_control_cross_kind_max_wins() {
        let r = route_with_controls(vec![RouteControl {
            faction_id: "f".into(),
            piracy: 90.0,
            toll: 10.0,
            ..Default::default()
        }]);
        let (_, kind, score) = top_route_control(&r).expect("result");
        assert!(matches!(kind, RouteControlKind::Piracy));
        assert_eq!(score, 90.0);
    }

    #[test]
    fn top_route_control_returns_raw_score_below_floor() {
        // Score 5.0 is below ROUTE_CONTROL_MIN_SCORE (40); top_route_control
        // does NOT apply that floor, so it still returns Some with score 5.0.
        let r = route_with_controls(vec![RouteControl {
            faction_id: "f".into(),
            patrol: 5.0,
            ..Default::default()
        }]);
        let (_, kind, score) = top_route_control(&r).expect("no floor inside top_route_control");
        assert!(matches!(kind, RouteControlKind::Patrol));
        assert_eq!(score, 5.0);
    }

    #[test]
    fn top_route_control_global_max_across_controls() {
        // Two factions; the global maximum (faction "b", interdiction 80) wins.
        let r = route_with_controls(vec![
            RouteControl {
                faction_id: "a".into(),
                patrol: 30.0,
                ..Default::default()
            },
            RouteControl {
                faction_id: "b".into(),
                interdiction: 80.0,
                ..Default::default()
            },
        ]);
        let (id, kind, score) = top_route_control(&r).expect("result");
        assert_eq!(id, "b");
        assert!(matches!(kind, RouteControlKind::Interdiction));
        assert_eq!(score, 80.0);
    }

    // ── Gap 2: validation_color (errors mask warnings) ────────────────────────
    // status/chrome colours are process-global statics another test could swap;
    // assert against the live helpers so the branch logic is proven independent
    // of any concurrently-installed theme. (Default DARK literals noted inline.)
    #[test]
    fn validation_color_clean_is_dim() {
        // (0,0) -> chrome_text_dim() (default TEXT_DIM = rgb(150,145,165)).
        assert_eq!(validation_color(0, 0), chrome_text_dim());
    }

    #[test]
    fn validation_color_warnings_only_is_warning() {
        // (0,3) -> warning() (default StatusColors::DARK.warning = rgb(224,145,58)).
        assert_eq!(validation_color(0, 3), warning());
    }

    #[test]
    fn validation_color_errors_take_precedence() {
        // (2,0) -> danger() (default StatusColors::DARK.danger = rgb(210,96,63)).
        assert_eq!(validation_color(2, 0), danger());
        // (2,5) -> danger() as well: any error masks warnings.
        assert_eq!(validation_color(2, 5), danger());
    }

    // ── Gap 10: star_color (trim+upper tolerant) vs world_type_color (exact) ──
    #[test]
    fn star_color_is_case_and_whitespace_tolerant() {
        // lowercase folds to "O".
        assert_eq!(star_color("o"), Color32::from_rgb(255, 150, 70));
        // surrounding whitespace trimmed, then uppercased to "M".
        assert_eq!(star_color("  m  "), Color32::from_rgb(200, 60, 70));
    }

    #[test]
    fn star_color_unknown_is_grey_fallback() {
        assert_eq!(star_color("Z"), Color32::from_rgb(180, 180, 180));
    }

    #[test]
    fn world_type_color_is_exact_case_sensitive() {
        assert_eq!(
            world_type_color("ForgeWorld"),
            Color32::from_rgb(220, 110, 50)
        );
        assert_eq!(
            world_type_color("AgriWorld"),
            Color32::from_rgb(120, 200, 110)
        );
        // Case-sensitive miss -> grey (the asymmetry vs star_color).
        assert_eq!(
            world_type_color("forgeworld"),
            Color32::from_rgb(180, 180, 180)
        );
        assert_eq!(
            world_type_color("Nonsense"),
            Color32::from_rgb(180, 180, 180)
        );
    }
}
