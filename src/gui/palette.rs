//! Shared color palette + per-entity color helpers for the GUI.
//!
//! Ported from `bitmap.rs` / `system_map.rs` so the GUI matches the PNG export
//! aesthetic. Keep all colors here so future restyling is one-file.

use egui::{Color32, Pos2, Stroke};

use crate::sector_model::{GeneratedFaction, RoutePattern, RouteStability};

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

/// Renders a line from `a` to `b` using the given `pattern`. Solid patterns
/// emit a single `line_segment`; dashed/dotted patterns walk the segment and
/// stamp alternating on/off runs whose lengths scale with `thickness`. Short
/// "dot" runs render as filled discs so the dotted style stays visible at
/// thin strokes.
pub fn draw_route_line(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    thickness: f32,
    color: Color32,
    pattern: RoutePattern,
) {
    let strides = pattern.strides();
    if strides.is_empty() {
        painter.line_segment([a, b], Stroke::new(thickness, color));
        return;
    }
    let unit = thickness.max(2.0);
    let delta = b - a;
    let total = delta.length();
    if total <= 0.0 {
        return;
    }
    let dir = delta / total;
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < total {
        let stride = strides[idx % strides.len()];
        let seg = stride * unit;
        let next_t = (t + seg).min(total);
        if idx.is_multiple_of(2) {
            let p0 = a + dir * t;
            let p1 = a + dir * next_t;
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

pub fn stability_color(s: RouteStability) -> Color32 {
    match s {
        RouteStability::Stable => Color32::from_rgb(110, 210, 130),
        RouteStability::Unstable => Color32::from_rgb(240, 200, 90),
        RouteStability::Hazardous => Color32::from_rgb(235, 90, 90),
        RouteStability::Perilous => Color32::from_rgb(165, 100, 215),
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
// Core hue/glyph/border logic lives in [`crate::faction_style`] (no GUI deps);
// this wrapper just re-paints the RGB primitives as `egui::Color32`.

pub use crate::faction_style::FactionBorder;

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
    let rgb = crate::faction_style::faction_style_rgb_by_id(factions, id);
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
    let rgb = crate::faction_style::faction_style_rgb(kind, id, disposition);
    FactionStyle {
        fill: from_rgb(rgb.fill),
        accent: from_rgb(rgb.accent),
        glyph: rgb.glyph,
        border: rgb.border,
    }
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
