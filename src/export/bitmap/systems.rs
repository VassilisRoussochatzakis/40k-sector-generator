//! Star disks, world-count pips, and subsector capital markers.

use image::{Rgba, RgbaImage};

use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::colors::{darken, star_color};
use super::geom::{hex_center, Geom};
use super::primitives::{
    draw_circle, draw_line, draw_line_thick, draw_rect_outline, draw_text, fill_circle,
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
    let star_r = (g.hex_size * star_radius_ratio()) as i32;
    let pip_scale = pip_text_scale(g);
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        if let Some(star) = &sys.star {
            let fill = star_color(&star.colour_code);
            // Star disk (no tinted hex fill; matches GUI sector view).
            fill_circle(img, cx, cy, star_r, fill);
            draw_circle(img, cx, cy, star_r, darken(fill, 0.55));
        } else {
            // Special location: draw a small grey square or diamond.
            let r = star_r * 3 / 4;
            fill_rect(
                img,
                cx - r,
                cy - r,
                2 * r,
                2 * r,
                Rgba([140, 140, 150, 255]),
            );
        }

        // Subsector capital marker: gold diamond above the star.
        if subsectors
            .iter()
            .any(|s| s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(img, cx, cy, g.hex_size, &opts.theme);
        }

        // World-count pip on the lower-right of the hex.
        let pip = sector.get_worlds_for_system(sys).len();
        if pip > 0 {
            let label = format!("{pip}");
            let (tw, th) = text_size(&label, pip_scale);
            let tx = cx + (g.hex_size * 0.55) as i32 - tw;
            let ty = cy + (g.hex_size * 0.55) as i32 - th;
            draw_text(img, tx, ty, &label, opts.theme.text, pip_scale);
        }
    }
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
