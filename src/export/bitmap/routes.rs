//! Bitmap route entry: thin wrapper around the shared route drawing in
//! [`crate::export::render_core::routes`].
//!
//! Only the midpoint control-glyph stays here, because its `i32`
//! floor-division math (`(a + b) / 2`) doesn't round-trip through `f32`
//! arithmetic without changing PNG bytes.

use image::Rgba;

use crate::export::render_core::routes::top_route_control;
use crate::faction_style::faction_style_rgb_by_id;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedRoute, GeneratedSector};

use super::canvas::BitmapCanvas;
use super::colors::darken;
use super::geom::{hex_center, Geom};
use super::RenderOptions;

pub(crate) use crate::export::render_core::routes::{star_radius_ratio, ControlKind};

/// Legend swatch helper: draws a single route-pattern segment at integer
/// pixel coordinates. Wraps the shared `f32` entry through `BitmapCanvas`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_route_pattern_legend(
    img: &mut image::RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgba<u8>,
    thickness: i32,
    pattern: crate::sector_model::RoutePattern,
) {
    let mut canvas = BitmapCanvas::new(img);
    crate::export::render_core::routes::draw_route_pattern(
        &mut canvas,
        x0 as f32,
        y0 as f32,
        x1 as f32,
        y1 as f32,
        color,
        thickness as f32,
        pattern,
    );
}

pub(super) fn draw_routes(
    img: &mut image::RgbaImage,
    sector: &GeneratedSector,
    g: &Geom,
    opts: &RenderOptions,
) {
    let mut canvas = BitmapCanvas::new(img);
    crate::export::render_core::routes::draw_routes(
        &mut canvas,
        sector,
        opts,
        g.hex_size,
        |q, r| {
            let (x, y) = hex_center(q, r, g);
            (x as f32, y as f32)
        },
        |canvas, route, a, b, thickness| {
            draw_route_control_glyph(canvas, sector, route, a, b, thickness, &opts.theme);
        },
    );
}

/// §3: at the midpoint of a route, draw a symbol for the single strongest
/// `RouteControl` category (patrol / toll / interdiction / piracy) when its
/// score is ≥ 40. Bitmap-specific: uses i32 floor-division on the midpoint
/// so PNG bytes match the pre-refactor renderer.
fn draw_route_control_glyph(
    canvas: &mut BitmapCanvas<'_>,
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
        Rgba([style.fill.0, style.fill.1, style.fill.2, 255])
    };
    let dark = darken(color, 0.5);
    // Mirror the pre-refactor bitmap behaviour: the line endpoints were
    // rounded i32s, the midpoint was `(a + b) / 2` with i32 floor division.
    let ax = a.0.round() as i32;
    let ay = a.1.round() as i32;
    let bx = b.0.round() as i32;
    let by = b.1.round() as i32;
    let thick_i = thickness.round().max(1.0) as i32;
    let mx = (ax + bx) / 2;
    let my = (ay + by) / 2;
    let size = (thick_i * 3).max(6);
    let img = &mut *canvas.img;
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        let half = size;
        super::primitives::draw_line_thick(
            img,
            mx - half,
            my - half,
            mx + half,
            my + half,
            color,
            thick_i.max(2),
        );
        super::primitives::draw_line_thick(
            img,
            mx - half,
            my + half,
            mx + half,
            my - half,
            color,
            thick_i.max(2),
        );
        return;
    }
    match kind {
        ControlKind::Interdiction => {
            let dx = (bx - ax) as f32;
            let dy = (by - ay) as f32;
            let len = dx.hypot(dy).max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size as f32;
            let x0 = (mx as f32 - px * half) as i32;
            let y0 = (my as f32 - py * half) as i32;
            let x1 = (mx as f32 + px * half) as i32;
            let y1 = (my as f32 + py * half) as i32;
            super::primitives::draw_line_thick(img, x0, y0, x1, y1, color, thick_i.max(2));
            super::primitives::draw_line_thick(img, x0, y0, x1, y1, dark, 1);
        }
        ControlKind::Patrol => {
            super::primitives::fill_circle(img, mx, my, size / 2, color);
            super::primitives::draw_circle(img, mx, my, size / 2, dark);
        }
        ControlKind::Toll => {
            let half = size / 2;
            super::primitives::fill_rect(img, mx - half, my - half, size, size, color);
            super::primitives::draw_rect_outline(img, mx - half, my - half, size, size, dark);
        }
        ControlKind::Piracy => {
            let half = size / 2;
            super::primitives::draw_line_thick(
                img,
                mx - half,
                my - half,
                mx + half,
                my + half,
                color,
                thick_i.max(2),
            );
            super::primitives::draw_line_thick(
                img,
                mx - half,
                my + half,
                mx + half,
                my - half,
                color,
                thick_i.max(2),
            );
        }
    }
}
