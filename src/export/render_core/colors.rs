//! Shared color helpers used by both `bitmap` and `svg_export`.
//!
//! Every function here is `f32`-based at its arithmetic core and returns
//! `Rgba<u8>` so both backends consume the same types. Backend-specific
//! quantization (e.g. `f32 → i32` for the bitmap's thickness math) lives in
//! a small wrapper in each backend.

use image::Rgba;

use crate::map_theme::{MapTheme, RouteLineMode};
use crate::sector_model::RouteStability;

/// §8 spectral colour mapping. Identical between backends.
pub(crate) fn star_color(code: &str) -> Rgba<u8> {
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

/// Route stability → themed colour. Identical between backends.
pub(crate) fn stability_color(theme: &MapTheme, s: RouteStability) -> Rgba<u8> {
    match s {
        RouteStability::Stable => theme.route_stable,
        RouteStability::Unstable => theme.route_unstable,
        RouteStability::Hazardous => theme.route_hazardous,
        RouteStability::Perilous => theme.route_perilous,
    }
}

/// Blend `from` toward `to` by `t`, mirroring the live egui renderer's
/// `gui_core::sector_view::blend_heat`. A floor (`0.20`) keeps any non-zero
/// `t` visibly tinted and a cap (`0.85`) preserves a hint of the base tone, so
/// the exported heatmap / region tints match the on-screen map exactly. `t`
/// is clamped into `0..=1` first.
pub(crate) fn blend_heat(from: Rgba<u8>, to: Rgba<u8>, t: f32) -> Rgba<u8> {
    let t = if t > 0.0 {
        0.65f32.mul_add(t.min(1.0), 0.20).min(0.85)
    } else {
        0.0
    };
    let mix = |a: u8, b: u8| f32::from(a).mul_add(1.0 - t, f32::from(b) * t).round() as u8;
    Rgba([
        mix(from.0[0], to.0[0]),
        mix(from.0[1], to.0[1]),
        mix(from.0[2], to.0[2]),
        255,
    ])
}

/// Mix `c` toward `base` by `(1 - amount)`. `amount = 1.0` returns `c`
/// unchanged; `amount = 0.0` returns `base`. Identical between backends.
pub(crate) fn tint_against(c: Rgba<u8>, amount: f32, base: Rgba<u8>) -> Rgba<u8> {
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

/// Scale every RGB channel by `(1 - amount).clamp(0, 1)`. Preserves alpha.
pub(crate) fn darken(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let scale = (1.0 - amount).clamp(0.0, 1.0);
    Rgba([
        (f32::from(c.0[0]) * scale) as u8,
        (f32::from(c.0[1]) * scale) as u8,
        (f32::from(c.0[2]) * scale) as u8,
        c.0[3],
    ])
}

/// Multiply every RGB channel by `factor` (clamped to `[0, 255]`).
/// Preserves alpha.
pub(crate) fn dim(c: Rgba<u8>, factor: f32) -> Rgba<u8> {
    Rgba([
        ((c.0[0] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[1] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((c.0[2] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        c.0[3],
    ])
}

/// Promote an `(r, g, b)` triple to an opaque `Rgba`.
pub(crate) fn rgba(t: (u8, u8, u8)) -> Rgba<u8> {
    Rgba([t.0, t.1, t.2, 255])
}

/// Truncate `s` to at most `max` characters, appending `.` when truncated.
/// Identical between backends.
pub(crate) fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

/// Backend-neutral route thickness in `f32`. Bitmap callers round to `i32`
/// at the primitive-call boundary; SVG uses the `f32` directly.
pub(crate) fn route_thickness_f32(
    theme: &MapTheme,
    stability: RouteStability,
    hex_size: f32,
) -> f32 {
    let base = (hex_size * 0.08).max(2.0) * theme.route_thickness;
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
