//! System glyphs, world-count pips, and subsector capital markers.
//!
//! Glyph shapes mirror the live egui renderer
//! (`sectorforge_gui_core::sector_view::draw_system_glyph`) so the exported PNG
//! shows the same symbols as the builder / viewer map. Classification is the
//! shared [`crate::sector_model::SystemGlyph`].

use image::{Rgba, RgbaImage};

use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedSector, SystemGlyph};
use crate::subsectors::Subsector;

use super::colors::{darken, star_color};
use super::geom::{hex_center, Geom};
use super::primitives::{
    draw_circle, draw_line, draw_line_thick, draw_rect_outline, draw_ring, draw_text, fill_circle,
    fill_polygon, fill_rect, text_size, GLYPH_H,
};
use super::routes::star_radius_ratio;
use super::RenderOptions;

pub(super) fn pip_text_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.34) / GLYPH_H as f32).round() as i32).max(1)
}

pub(super) fn draw_systems(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    opts: &RenderOptions,
) {
    let star_r = g.hex_size * star_radius_ratio();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);

        // Marker colour: star spectral colour, else the dim glyph stroke colour
        // (matches the live renderer's `fill` choice).
        let fill = match &sys.star {
            Some(star) => star_color(&star.colour_code),
            None => opts.theme.text_dim,
        };

        draw_system_glyph(
            img,
            cx,
            cy,
            star_r,
            SystemGlyph::from_system(sys),
            fill,
            &opts.theme,
        );

        // Subsector capital marker: gold diamond above the star.
        if subsectors
            .iter()
            .any(|s| s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(img, cx, cy, g.hex_size, &opts.theme);
        }

        // World-count pip on the top-right of the hex.
        let pip = sys.worlds.len();
        if pip > 0 {
            draw_world_pip(img, cx, cy, pip, fill, g, &opts.theme);
        }
    }
}

/// Round a stroke width to an integer pixel thickness (>= 1).
fn stroke_px(thickness: f32) -> i32 {
    (thickness.round() as i32).max(1)
}

/// Draw a closed polyline through `pts` at the given pixel thickness.
fn outline(img: &mut RgbaImage, pts: &[(i32, i32)], color: Rgba<u8>, thickness: i32) {
    for i in 0..pts.len() {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % pts.len()];
        if thickness <= 1 {
            draw_line(img, ax, ay, bx, by, color);
        } else {
            draw_line_thick(img, ax, ay, bx, by, color, thickness);
        }
    }
}

/// Draw the per-system map glyph. One arm per [`SystemGlyph`]; mirrors the
/// shapes painted by `gui-core`'s `draw_system_glyph` with `radius == star_r`.
fn draw_system_glyph(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    radius: f32,
    glyph: SystemGlyph,
    fill: Rgba<u8>,
    theme: &MapTheme,
) {
    let dim = theme.text_dim;
    let ri = |v: f32| v.round() as i32;
    let cxf = cx as f32;
    let cyf = cy as f32;
    match glyph {
        SystemGlyph::Star => {
            fill_circle(img, cx, cy, ri(radius), fill);
            draw_ring(img, cx, cy, ri(radius), stroke_px(1.5), darken(fill, 0.55));
        }
        SystemGlyph::UncataloguedStar | SystemGlyph::SpecialLocation => {
            let r = radius * 0.8;
            let pts = [
                (cx, ri(cyf - r)),
                (ri(cxf + r), cy),
                (cx, ri(cyf + r)),
                (ri(cxf - r), cy),
            ];
            outline(img, &pts, dim, stroke_px(1.5));
        }
        SystemGlyph::BlackHole => {
            fill_circle(img, cx, cy, ri(radius * 0.55), Rgba([0, 0, 0, 255]));
            draw_ring(img, cx, cy, ri(radius * 0.9), stroke_px(2.0), dim);
            draw_ring(
                img,
                cx,
                cy,
                ri(radius * 1.1),
                stroke_px(1.0),
                darken(dim, 0.4),
            );
        }
        SystemGlyph::WarpAnomaly => {
            let r = radius * 0.95;
            let h = r * 0.866;
            let up = [
                (cx, ri(cyf - r)),
                (ri(cxf + h), ri(cyf + r * 0.5)),
                (ri(cxf - h), ri(cyf + r * 0.5)),
            ];
            let dn = [
                (cx, ri(cyf + r)),
                (ri(cxf + h), ri(cyf - r * 0.5)),
                (ri(cxf - h), ri(cyf - r * 0.5)),
            ];
            outline(img, &up, dim, stroke_px(1.3));
            outline(img, &dn, dim, stroke_px(1.3));
        }
        SystemGlyph::SpaceStation => {
            let r = radius * 0.65;
            let sq = [
                (ri(cxf - r), ri(cyf - r)),
                (ri(cxf + r), ri(cyf - r)),
                (ri(cxf + r), ri(cyf + r)),
                (ri(cxf - r), ri(cyf + r)),
            ];
            outline(img, &sq, dim, stroke_px(1.5));
            let arm = ri(r * 1.3);
            draw_line(img, cx - arm, cy, cx + arm, cy, dim);
            draw_line(img, cx, cy - arm, cx, cy + arm, dim);
        }
    }
}

/// World-count pip: a small backed disc on the hex's top-right corner with a
/// centred count. Mirrors the live renderer's pip (disc + dark ring + number).
fn draw_world_pip(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    pip: usize,
    fill: Rgba<u8>,
    g: &Geom,
    theme: &MapTheme,
) {
    let disc_r = (g.hex_size * 0.22).round() as i32;
    let px = cx + (g.hex_size * 0.55).round() as i32;
    let py = cy - (g.hex_size * 0.55).round() as i32;
    fill_circle(img, px, py, disc_r, theme.bg);
    let ring_w = ((disc_r as f32 * 0.25).clamp(0.5, 1.2).round() as i32).max(1);
    draw_ring(img, px, py, disc_r, ring_w, darken(fill, 0.4));
    let scale = pip_text_scale(g);
    let label = pip.to_string();
    let (tw, th) = text_size(&label, scale);
    draw_text(img, px - tw / 2, py - th / 2, &label, theme.text, scale);
}

fn draw_capital_marker(img: &mut RgbaImage, cx: i32, cy: i32, hex_size: f32, theme: &MapTheme) {
    let r = ((hex_size * 0.15).max(3.5)).round() as i32;
    let dy = -((hex_size * 0.55).round() as i32);
    let center_y = cy + dy;
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        fill_rect(
            img,
            cx - r,
            center_y - r / 2,
            r * 2,
            r,
            theme.capital_marker,
        );
        draw_rect_outline(
            img,
            cx - r,
            center_y - r / 2,
            r * 2,
            r,
            theme.capital_outline,
        );
        return;
    }
    if matches!(theme.symbol_set, SymbolSet::Tactical) {
        draw_line_thick(
            img,
            cx - r,
            center_y,
            cx + r,
            center_y,
            theme.capital_marker,
            2,
        );
        draw_line_thick(
            img,
            cx,
            center_y - r,
            cx,
            center_y + r,
            theme.capital_marker,
            2,
        );
        draw_circle(img, cx, center_y, r, theme.capital_outline);
        return;
    }
    let pts = [
        (cx, center_y - r),
        (cx + r, center_y),
        (cx, center_y + r),
        (cx - r, center_y),
    ];
    fill_polygon(img, &pts, theme.capital_marker);
    for i in 0..4 {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % 4];
        draw_line(img, ax, ay, bx, by, theme.capital_outline);
    }
}
