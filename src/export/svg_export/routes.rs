//! SVG route entry: thin wrapper around the shared route drawing in
//! [`crate::export::render_core::routes`].

use image::Rgba;

use crate::export::render_core::canvas::Canvas;
use crate::export::render_core::routes::top_route_control;
pub(super) use crate::export::render_core::routes::ControlKind;
use crate::sector_model::RoutePattern;

/// Legend helper: draw a single route-pattern segment via `SvgCanvas`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_route_pattern(
    s: &mut String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgba<u8>,
    thickness: f32,
    pattern: RoutePattern,
) {
    let mut canvas = SvgCanvas::new(s);
    crate::export::render_core::routes::draw_route_pattern(
        &mut canvas,
        x0,
        y0,
        x1,
        y1,
        color,
        thickness,
        pattern,
    );
}
use crate::export::render_core::RenderOptions;
use crate::faction_style::faction_style_rgb_by_id;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedRoute, GeneratedSector};

use super::canvas::SvgCanvas;
use super::colors::{darken, rgba_from_tuple};
use super::geom::hex_center;
use super::HEX_SIZE;

pub(super) fn draw_routes(s: &mut String, sector: &GeneratedSector, opts: &RenderOptions) {
    let mut canvas = SvgCanvas::new(s);
    crate::export::render_core::routes::draw_routes(
        &mut canvas,
        sector,
        opts,
        HEX_SIZE,
        hex_center,
        |canvas, route, a, b, thickness| {
            draw_route_control_glyph(canvas, sector, route, a, b, thickness, &opts.theme);
        },
    );
}

fn draw_route_control_glyph(
    canvas: &mut SvgCanvas<'_>,
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
        canvas.line(
            mx - half,
            my - half,
            mx + half,
            my + half,
            color,
            thickness.max(2.0),
        );
        canvas.line(
            mx - half,
            my + half,
            mx + half,
            my - half,
            color,
            thickness.max(2.0),
        );
        return;
    }
    match kind {
        ControlKind::Interdiction => {
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let len = dx.hypot(dy).max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size;
            let x0 = mx - px * half;
            let y0 = my - py * half;
            let x1 = mx + px * half;
            let y1 = my + py * half;
            canvas.line(x0, y0, x1, y1, color, thickness.max(2.0));
            canvas.line(x0, y0, x1, y1, dark, 1.0);
        }
        ControlKind::Patrol => {
            canvas.circle(mx, my, size * 0.5, color, Some(dark), 1.0);
        }
        ControlKind::Toll => {
            let half = size * 0.5;
            canvas.rect(mx - half, my - half, size, size, color, Some(dark));
        }
        ControlKind::Piracy => {
            let half = size * 0.5;
            canvas.line(
                mx - half,
                my - half,
                mx + half,
                my + half,
                color,
                thickness.max(2.0),
            );
            canvas.line(
                mx - half,
                my + half,
                mx + half,
                my - half,
                color,
                thickness.max(2.0),
            );
        }
    }
}
