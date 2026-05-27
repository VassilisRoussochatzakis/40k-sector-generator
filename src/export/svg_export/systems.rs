//! Star disks, capital markers, world-count pips.

use image::Rgba;

use crate::export::render_core::RenderOptions;
use crate::map_theme::{MapTheme, SymbolSet};
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::colors::{darken, star_color};
use super::geom::hex_center;
use super::primitives::{circle, line, polygon, rect, text};
use super::{star_radius_ratio, HEX_SIZE};

const PIP_FONT: f32 = 9.0;

pub(super) fn draw_systems(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    opts: &RenderOptions,
) {
    let star_r = HEX_SIZE * star_radius_ratio();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        if let Some(star) = &sys.star {
            let fill = star_color(&star.colour_code);
            circle(s, cx, cy, star_r, fill, Some(darken(fill, 0.55)), 1.0);
        } else {
            let r = star_r * 3.0 / 4.0;
            rect(
                s,
                cx - r,
                cy - r,
                r * 2.0,
                r * 2.0,
                Rgba([140, 140, 150, 255]),
                None,
            );
        }

        if subsectors
            .iter()
            .any(|sub| sub.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(s, cx, cy, &opts.theme);
        }

        let pip = sector.get_worlds_for_system(sys).len();
        if pip > 0 {
            let label = format!("{pip}");
            let tx = HEX_SIZE.mul_add(0.55, cx);
            let ty = HEX_SIZE.mul_add(0.55, cy);
            text(s, tx, ty, &label, opts.theme.text, PIP_FONT, "end");
        }
    }
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
