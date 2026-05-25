//! Color/style helpers shared across svg_export submodules: star fills,
//! route stability colors/thicknesses, RGBA mixing, dim/darken, string
//! truncation.

use image::Rgba;

use crate::map_theme::MapTheme;
use crate::sector_model::RouteStability;

use super::HEX_SIZE;

pub(super) fn star_color(code: &str) -> Rgba<u8> {
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

pub(super) fn stability_color(theme: &MapTheme, s: RouteStability) -> Rgba<u8> {
    match s {
        RouteStability::Stable => theme.route_stable,
        RouteStability::Unstable => theme.route_unstable,
        RouteStability::Hazardous => theme.route_hazardous,
        RouteStability::Perilous => theme.route_perilous,
    }
}

pub(super) fn route_thickness(theme: &MapTheme, stability: RouteStability) -> f32 {
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

pub(super) fn rgba_from_tuple(t: (u8, u8, u8)) -> Rgba<u8> {
    Rgba([t.0, t.1, t.2, 255])
}

pub(super) fn tint_against(c: Rgba<u8>, amount: f32, base: Rgba<u8>) -> Rgba<u8> {
    let mix = |v: u8, base: u8| {
        let f = f32::from(v).mul_add(amount, f32::from(base) * (1.0 - amount));
        f.round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        mix(c.0[0], base.0[0]),
        mix(c.0[1], base.0[1]),
        mix(c.0[2], base.0[2]),
        255,
    ])
}

pub(super) fn darken(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let scale = (1.0 - amount).clamp(0.0, 1.0);
    Rgba([
        (f32::from(c.0[0]) * scale) as u8,
        (f32::from(c.0[1]) * scale) as u8,
        (f32::from(c.0[2]) * scale) as u8,
        c.0[3],
    ])
}

pub(super) fn dim(c: Rgba<u8>, factor: f32) -> Rgba<u8> {
    Rgba([
        ((c.0[0] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[1] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[2] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        c.0[3],
    ])
}

pub(super) fn stroke_px(thickness: f32, factor: f32) -> f32 {
    (thickness * factor).max(1.0)
}

pub(super) fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}
