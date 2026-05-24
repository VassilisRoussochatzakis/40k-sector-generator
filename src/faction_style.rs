//! Pure-data faction style (§8). No GUI dependencies — usable by both the egui
//! viewport and the PNG bitmap exporters.
//!
//! Hue is keyed by `kind`, jittered ±25° by a djb2 hash of `id`. Saturation has
//! a smaller ±0.10 jitter. `disposition` selects a [`FactionBorder`] used by
//! both renderers to style the territory outline.

use crate::sector_model::GeneratedFaction;

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

/// RGB-flavoured `FactionStyle`. The GUI re-wraps `fill` / `accent` as
/// `egui::Color32`; the bitmap renderer wraps them as `image::Rgba<u8>`.
#[derive(Debug, Clone, Copy)]
pub struct FactionStyleRgb {
    pub fill: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub glyph: char,
    pub border: FactionBorder,
}

/// Stable 32-bit djb2 hash. Used for deterministic id-keyed hue rotation;
/// portability across builds is required so we do not rely on
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

/// Deterministic style for a faction.
#[must_use]
pub fn faction_style_rgb(kind: &str, id: &str, disposition: &str) -> FactionStyleRgb {
    let (h, s, v) = kind_base_hue(kind);
    let hash = djb2(id);
    let hue_shift = (((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 50.0;
    let sat_shift = (((hash >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 0.20;
    let h2 = h + hue_shift;
    let s2 = (s + sat_shift).clamp(0.15, 1.0);
    let fill = hsv_to_rgb(h2, s2, v);
    let accent = hsv_to_rgb(h2, (s2 * 1.2).clamp(0.0, 1.0), (v * 0.55).clamp(0.0, 1.0));
    FactionStyleRgb {
        fill,
        accent,
        glyph: glyph_for_kind(kind, hash),
        border: border_for(disposition),
    }
}

/// Parse a `#RRGGBB` (or `RRGGBB`) hex literal into an `(u8, u8, u8)` triple.
#[must_use]
pub fn parse_hex_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Render an `(u8, u8, u8)` triple as `#RRGGBB`.
#[must_use]
pub fn rgb_to_hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2)
}

/// Parse a border-style spelling. `"clean"`, `"jagged"`, `"dotted"`, `"thin"`.
#[must_use]
pub fn parse_border(s: &str) -> Option<FactionBorder> {
    match s.trim().to_ascii_lowercase().as_str() {
        "clean" => Some(FactionBorder::Clean),
        "jagged" => Some(FactionBorder::Jagged),
        "dotted" => Some(FactionBorder::Dotted),
        "thin" => Some(FactionBorder::Thin),
        _ => None,
    }
}

/// §F2: derive a style honoring per-faction `FactionDef` overrides before
/// falling back to the kind/id deterministic palette.
#[must_use]
pub fn faction_style_rgb_with_overrides(
    kind: &str,
    id: &str,
    disposition: &str,
    fill_override: Option<&str>,
    accent_override: Option<&str>,
    glyph_override: Option<&str>,
    border_override: Option<&str>,
) -> FactionStyleRgb {
    let mut base = faction_style_rgb(kind, id, disposition);
    if let Some(rgb) = fill_override.and_then(parse_hex_rgb) {
        base.fill = rgb;
    }
    if let Some(rgb) = accent_override.and_then(parse_hex_rgb) {
        base.accent = rgb;
    }
    if let Some(g) = glyph_override.and_then(|s| s.chars().next()) {
        base.glyph = g;
    }
    if let Some(b) = border_override.and_then(parse_border) {
        base.border = b;
    }
    base
}

/// Resolve a style from a sector's faction list by id; falls back to a neutral
/// grey when the id is unknown.
#[must_use]
pub fn faction_style_rgb_by_id(factions: &[GeneratedFaction], id: &str) -> FactionStyleRgb {
    if let Some(f) = factions.iter().find(|f| f.id == id) {
        return faction_style_rgb(&f.kind, &f.id, &f.disposition);
    }
    FactionStyleRgb {
        fill: (150, 150, 160),
        accent: (70, 70, 80),
        glyph: '?',
        border: FactionBorder::Thin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let rgb = (0x12, 0xAB, 0xFF);
        let s = rgb_to_hex(rgb);
        assert_eq!(s, "#12ABFF");
        assert_eq!(parse_hex_rgb(&s), Some(rgb));
        assert_eq!(parse_hex_rgb("12abff"), Some(rgb));
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert!(parse_hex_rgb("").is_none());
        assert!(parse_hex_rgb("#12").is_none());
        assert!(parse_hex_rgb("#XYZAAA").is_none());
    }

    #[test]
    fn border_parser_covers_all_variants() {
        assert!(matches!(parse_border("clean"), Some(FactionBorder::Clean)));
        assert!(matches!(
            parse_border("JAGGED"),
            Some(FactionBorder::Jagged)
        ));
        assert!(matches!(
            parse_border("dotted"),
            Some(FactionBorder::Dotted)
        ));
        assert!(matches!(parse_border("thin"), Some(FactionBorder::Thin)));
        assert!(parse_border("nope").is_none());
    }

    #[test]
    fn overrides_replace_derived_fields() {
        let derived = faction_style_rgb("imperial", "f1", "lawful");
        let custom = faction_style_rgb_with_overrides(
            "imperial",
            "f1",
            "lawful",
            Some("#112233"),
            Some("#445566"),
            Some("X"),
            Some("jagged"),
        );
        assert_eq!(custom.fill, (0x11, 0x22, 0x33));
        assert_eq!(custom.accent, (0x44, 0x55, 0x66));
        assert_eq!(custom.glyph, 'X');
        assert!(matches!(custom.border, FactionBorder::Jagged));
        assert_ne!(custom.fill, derived.fill);
    }

    #[test]
    fn overrides_none_leaves_derived_intact() {
        let derived = faction_style_rgb("ork", "ork_warband_alpha", "hostile");
        let same = faction_style_rgb_with_overrides(
            "ork",
            "ork_warband_alpha",
            "hostile",
            None,
            None,
            None,
            None,
        );
        assert_eq!(derived.fill, same.fill);
        assert_eq!(derived.glyph, same.glyph);
    }
}
