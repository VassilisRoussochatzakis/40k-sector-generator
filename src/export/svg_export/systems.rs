//! System glyphs, subsector capital markers, world-count pips.
//!
//! Glyph shapes mirror the live egui renderer
//! (`sectorforge_gui_core::sector_view::draw_system_glyph`) so the exported SVG
//! shows the same symbols as the builder / viewer map. Classification is the
//! shared [`crate::sector_model::SystemGlyph`].

use image::Rgba;

use crate::export::render_core::RenderOptions;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::{GeneratedSector, SystemGlyph};
use crate::subsectors::Subsector;

use super::colors::{darken, star_color};
use super::geom::hex_center;
use super::primitives::{circle, line, polygon, rect, text};
use super::{star_radius_ratio, HEX_SIZE};

/// Fully transparent fill for hollow (outline-only) glyphs.
const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

pub(super) fn draw_systems(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    opts: &RenderOptions,
) {
    let star_r = HEX_SIZE * star_radius_ratio();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);

        // Marker colour: star spectral colour, else the dim glyph stroke colour
        // (matches the live renderer's `fill` choice).
        let fill = match &sys.star {
            Some(star) => star_color(&star.colour_code),
            None => opts.theme.text_dim,
        };

        draw_system_glyph(
            s,
            cx,
            cy,
            star_r,
            SystemGlyph::from_system(sys),
            fill,
            &opts.theme,
        );

        if subsectors
            .iter()
            .any(|sub| sub.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(s, cx, cy, &opts.theme);
        }

        let pip = sys.worlds.len();
        if pip > 0 {
            draw_world_pip(s, cx, cy, pip, fill, &opts.theme);
        }
    }
}

/// Draw the per-system map glyph. One arm per [`SystemGlyph`]; mirrors the
/// shapes painted by `gui-core`'s `draw_system_glyph` with `radius == star_r`.
fn draw_system_glyph(
    s: &mut String,
    cx: f32,
    cy: f32,
    radius: f32,
    glyph: SystemGlyph,
    fill: Rgba<u8>,
    theme: &MapTheme,
) {
    let dim = theme.text_dim;
    match glyph {
        SystemGlyph::Star => {
            circle(s, cx, cy, radius, fill, Some(darken(fill, 0.55)), 1.5);
        }
        SystemGlyph::UncataloguedStar | SystemGlyph::SpecialLocation => {
            let r = radius * 0.8;
            let pts = [(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)];
            polygon(s, &pts, TRANSPARENT, Some(dim), 1.5);
        }
        SystemGlyph::BlackHole => {
            circle(s, cx, cy, radius * 0.55, Rgba([0, 0, 0, 255]), None, 0.0);
            circle(s, cx, cy, radius * 0.9, TRANSPARENT, Some(dim), 2.0);
            circle(
                s,
                cx,
                cy,
                radius * 1.1,
                TRANSPARENT,
                Some(darken(dim, 0.4)),
                1.0,
            );
        }
        SystemGlyph::WarpAnomaly => {
            let r = radius * 0.95;
            let h = r * 0.866;
            let up = [(cx, cy - r), (cx + h, cy + r * 0.5), (cx - h, cy + r * 0.5)];
            let dn = [(cx, cy + r), (cx + h, cy - r * 0.5), (cx - h, cy - r * 0.5)];
            polygon(s, &up, TRANSPARENT, Some(dim), 1.3);
            polygon(s, &dn, TRANSPARENT, Some(dim), 1.3);
        }
        SystemGlyph::SpaceStation => {
            let r = radius * 0.65;
            let sq = [
                (cx - r, cy - r),
                (cx + r, cy - r),
                (cx + r, cy + r),
                (cx - r, cy + r),
            ];
            polygon(s, &sq, TRANSPARENT, Some(dim), 1.5);
            let arm = r * 1.3;
            line(s, cx - arm, cy, cx + arm, cy, dim, 1.0, None);
            line(s, cx, cy - arm, cx, cy + arm, dim, 1.0, None);
        }
    }
}

/// World-count pip: a small backed disc on the hex's top-right corner with a
/// centred count. Mirrors the live renderer's pip (disc + dark ring + number).
fn draw_world_pip(s: &mut String, cx: f32, cy: f32, pip: usize, fill: Rgba<u8>, theme: &MapTheme) {
    let disc_r = HEX_SIZE * 0.22;
    let px = HEX_SIZE.mul_add(0.55, cx);
    let py = (-HEX_SIZE).mul_add(0.55, cy);
    let font = HEX_SIZE * 0.36;
    circle(s, px, py, disc_r, theme.bg, None, 0.0);
    let ring_w = (disc_r * 0.25).clamp(0.5, 1.2);
    circle(
        s,
        px,
        py,
        disc_r,
        TRANSPARENT,
        Some(darken(fill, 0.4)),
        ring_w,
    );
    text(
        s,
        px,
        font.mul_add(0.35, py),
        &pip.to_string(),
        theme.text,
        font,
        "middle",
    );
}

fn draw_capital_marker(s: &mut String, cx: f32, cy: f32, theme: &MapTheme) {
    let r = (HEX_SIZE * 0.15).max(3.5);
    let dy = -HEX_SIZE * 0.55;
    let cy_marker = cy + dy;
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        rect(
            s,
            cx - r,
            cy_marker - r * 0.5,
            r * 2.0,
            r,
            theme.capital_marker,
            Some(theme.capital_outline),
        );
        return;
    }
    if matches!(theme.symbol_set, SymbolSet::Tactical) {
        line(
            s,
            cx - r,
            cy_marker,
            cx + r,
            cy_marker,
            theme.capital_marker,
            2.0,
            None,
        );
        line(
            s,
            cx,
            cy_marker - r,
            cx,
            cy_marker + r,
            theme.capital_marker,
            2.0,
            None,
        );
        circle(
            s,
            cx,
            cy_marker,
            r,
            Rgba([0, 0, 0, 0]),
            Some(theme.capital_outline),
            1.0,
        );
        return;
    }
    let pts = [
        (cx, cy_marker - r),
        (cx + r, cy_marker),
        (cx, cy_marker + r),
        (cx - r, cy_marker),
    ];
    polygon(
        s,
        &pts,
        theme.capital_marker,
        Some(theme.capital_outline),
        1.0,
    );
}
