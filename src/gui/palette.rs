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
// Each faction gets a deterministic style derived from `kind`, `id`, and
// `disposition`. `kind` selects a base hue family (Imperial = gold, Mechanicus
// = orange, Chaos = magenta, etc.); `id` rotates hue and shifts saturation so
// two factions of the same kind read as distinct; `disposition` controls
// border behaviour (clean / jagged / dotted).

/// Border behaviour driven by faction disposition (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionBorder {
    /// Crisp solid line — `lawful`, `insular`.
    Clean,
    /// Jagged / wide stroke — `hostile`, `zealous`.
    Jagged,
    /// Low-opacity dotted — `secretive`.
    Dotted,
    /// Default thin line.
    Thin,
}

#[derive(Debug, Clone, Copy)]
pub struct FactionStyle {
    pub fill: Color32,
    pub accent: Color32,
    pub glyph: char,
    pub border: FactionBorder,
}

/// Stable 32-bit djb2 hash of a string. Used for deterministic id-keyed hue
/// rotation; portability across builds is required so we do not rely on
/// `DefaultHasher`.
fn djb2(s: &str) -> u32 {
    let mut h: u32 = 5381;
    for b in s.as_bytes() {
        h = h.wrapping_mul(33).wrapping_add(u32::from(*b));
    }
    h
}

fn kind_base_hue(kind: &str) -> (f32, f32, f32) {
    // (hue 0..360, saturation 0..1, value 0..1)
    match kind {
        "imperial" => (48.0, 0.70, 0.88),
        "adepta_sororitas" => (350.0, 0.55, 0.85),
        "inquisition" => (340.0, 0.30, 0.55),
        "adeptus_astartes" => (210.0, 0.60, 0.78),
        "imperial_guard" => (90.0, 0.45, 0.65),
        "imperial_knight" => (35.0, 0.55, 0.80),
        "collegia_titanica" => (20.0, 0.65, 0.78),
        "deathwatch" | "grey_knights" | "talons_of_the_emperor" => (220.0, 0.20, 0.45),
        "mechanicus" => (18.0, 0.75, 0.80),
        "dark_mechanicum" => (15.0, 0.85, 0.45),
        "chaos_space_marine" => (320.0, 0.70, 0.65),
        "chaos_knight" => (305.0, 0.65, 0.55),
        "traitor_guard" => (300.0, 0.45, 0.55),
        "traitor_titan_legion" => (290.0, 0.55, 0.55),
        "daemon" => (280.0, 0.80, 0.55),
        "cult" => (260.0, 0.50, 0.55),
        "tau" => (200.0, 0.55, 0.82),
        "aeldari" => (170.0, 0.55, 0.75),
        "drukhari" => (335.0, 0.70, 0.55),
        "harlequin" => (155.0, 0.60, 0.85),
        "leagues_of_votann" => (200.0, 0.35, 0.65),
        "ork" => (110.0, 0.75, 0.55),
        "tyranid" => (75.0, 0.60, 0.50),
        "necron" => (135.0, 0.55, 0.65),
        "minor_xenos" | "xenos" => (140.0, 0.40, 0.65),
        "merchant" => (45.0, 0.60, 0.80),
        "criminal" => (15.0, 0.40, 0.55),
        "rebel" => (10.0, 0.55, 0.70),
        "genestealer_cult" => (155.0, 0.70, 0.55),
        _ => (210.0, 0.30, 0.60),
    }
}

fn glyph_for_kind(kind: &str, salt: u32) -> char {
    let pool: &[char] = match kind {
        "imperial" | "imperial_guard" => &['I', 'V', 'X', 'Y'],
        "adepta_sororitas" => &['S', 'T'],
        "inquisition" => &['Q'],
        "adeptus_astartes" | "deathwatch" | "grey_knights" | "talons_of_the_emperor" => {
            &['M', 'A', 'W']
        }
        "imperial_knight" | "chaos_knight" => &['K'],
        "collegia_titanica" | "traitor_titan_legion" => &['T'],
        "mechanicus" => &['G', 'H', 'O'],
        "dark_mechanicum" => &['D', 'P'],
        "chaos_space_marine" | "traitor_guard" => &['C', 'R', 'F'],
        "daemon" => &['Z'],
        "cult" | "genestealer_cult" => &['U', 'N'],
        "tau" => &['L', 'E'],
        "aeldari" => &['A', 'E'],
        "drukhari" => &['B'],
        "harlequin" => &['J'],
        "leagues_of_votann" => &['V', 'N'],
        "ork" => &['O', 'X'],
        "tyranid" => &['Y'],
        "necron" => &['N'],
        "merchant" => &['$', '&'],
        "criminal" => &['?', '!'],
        "rebel" => &['R'],
        _ => &['*'],
    };
    pool[(salt as usize) % pool.len()]
}

fn border_for(disposition: &str) -> FactionBorder {
    match disposition {
        "lawful" | "insular" => FactionBorder::Clean,
        "hostile" | "zealous" => FactionBorder::Jagged,
        "secretive" => FactionBorder::Dotted,
        _ => FactionBorder::Thin,
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = v * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as i32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_u(r1), to_u(g1), to_u(b1))
}

/// Resolve a `FactionStyle` from a sector's faction list by id. Returns a
/// neutral fallback style when the id is unknown.
#[must_use]
pub fn faction_style_by_id(factions: &[GeneratedFaction], id: &str) -> FactionStyle {
    if let Some(f) = factions.iter().find(|f| f.id == id) {
        return faction_style(&f.kind, &f.id, &f.disposition);
    }
    FactionStyle {
        fill: Color32::from_rgb(150, 150, 160),
        accent: Color32::from_rgb(70, 70, 80),
        glyph: '?',
        border: FactionBorder::Thin,
    }
}

/// Build a deterministic visual style for a faction.
#[must_use]
pub fn faction_style(kind: &str, id: &str, disposition: &str) -> FactionStyle {
    let (h, s, v) = kind_base_hue(kind);
    let hash = djb2(id);
    // ±25° hue jitter so two factions of the same kind read as distinct without
    // crossing into the next kind's band.
    let hue_shift = (((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 50.0;
    // ±0.10 saturation jitter.
    let sat_shift = (((hash >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 0.20;
    let h2 = h + hue_shift;
    let s2 = (s + sat_shift).clamp(0.15, 1.0);
    let (r, g, b) = hsv_to_rgb(h2, s2, v);
    let fill = Color32::from_rgb(r, g, b);
    let (ar, ag, ab) = hsv_to_rgb(h2, (s2 * 1.2).clamp(0.0, 1.0), (v * 0.55).clamp(0.0, 1.0));
    let accent = Color32::from_rgb(ar, ag, ab);
    FactionStyle {
        fill,
        accent,
        glyph: glyph_for_kind(kind, hash),
        border: border_for(disposition),
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
