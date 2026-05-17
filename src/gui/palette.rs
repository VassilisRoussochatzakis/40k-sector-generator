//! Shared color palette + per-entity color helpers for the GUI.
//!
//! Ported from `bitmap.rs` / `system_map.rs` so the GUI matches the PNG export
//! aesthetic. Keep all colors here so future restyling is one-file.

use egui::Color32;

use crate::sector_model::RouteStability;

pub const BG: Color32 = Color32::from_rgb(14, 12, 20);
pub const PANEL_BG: Color32 = Color32::from_rgb(22, 18, 30);
pub const HEX_EMPTY: Color32 = Color32::from_rgb(28, 26, 38);
pub const HEX_OUTLINE: Color32 = Color32::from_rgb(60, 55, 78);
pub const TEXT: Color32 = Color32::from_rgb(232, 228, 240);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 145, 165);
pub const ROUTE_DIM: Color32 = Color32::from_rgb(90, 88, 110);
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

pub fn stability_color(s: RouteStability) -> Color32 {
    match s {
        RouteStability::Stable => Color32::from_rgb(110, 210, 130),
        RouteStability::Unstable => Color32::from_rgb(240, 200, 90),
        RouteStability::Hazardous => Color32::from_rgb(235, 90, 90),
        RouteStability::Lost => ROUTE_DIM,
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

pub const STAR_LEGEND: &[(&str, &str)] = &[
    ("O", "ORANGE DWARF"),
    ("B", "BLUE-WHITE"),
    ("A", "AMBER"),
    ("F", "FUCHSIA"),
    ("G", "GREEN"),
    ("K", "KHAKI"),
    ("M", "MAROON"),
];
