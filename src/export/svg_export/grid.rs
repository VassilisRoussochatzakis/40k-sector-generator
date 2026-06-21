//! Hex-grid fill + subsector borders + per-system tints (SVG backend).
//!
//! Tint computation stays here. Polygon + dot drawing call into
//! [`crate::export::render_core::grid`]. The fill pipeline mirrors the live
//! egui renderer: heatmap tint as the per-hex base, region colour blended on
//! top, and no faction fill.

use std::collections::HashMap;

use image::Rgba;

use crate::export::render_core::colors::blend_heat;
use crate::export::render_core::RenderOptions;
use crate::heatmap::{HeatCellRgb, HeatmapMode};
use crate::map_theme::MapTheme;
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::canvas::SvgCanvas;
use super::colors::rgba_from_tuple;
use super::geom::hex_center;
use super::HEX_SIZE;

/// Per-hex heatmap tint (no faction fill — the live map never tints by
/// faction). Mirrors the live renderer: `blend_heat(hex_empty, cell, intensity)`.
pub(super) fn compute_heat_tints(
    sector: &GeneratedSector,
    opts: &RenderOptions,
    heat: &HashMap<crate::ids::SystemId, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    if matches!(opts.heatmap, HeatmapMode::Off) {
        return out;
    }
    for sys in sector.systems.iter() {
        if let Some(cell) = heat.get(&sys.id) {
            let key = (sys.coord.q, sys.coord.r);
            out.insert(
                key,
                blend_heat(
                    opts.theme.hex_empty,
                    rgba_from_tuple(cell.rgb),
                    cell.intensity,
                ),
            );
        }
    }
    out
}

pub(super) fn draw_hex_grid(
    s: &mut String,
    sector: &GeneratedSector,
    heat_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    let region_colours = crate::export::render_core::grid::compute_region_colours(sector);
    let mut canvas = SvgCanvas::new(s);
    crate::export::render_core::grid::draw_hex_grid(
        &mut canvas,
        sector,
        heat_tints,
        &region_colours,
        theme,
        HEX_SIZE,
        hex_center,
    );
}

pub(super) fn draw_subsector_borders(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
) {
    let mut canvas = SvgCanvas::new(s);
    crate::export::render_core::grid::draw_subsector_borders(
        &mut canvas,
        sector,
        subsectors,
        theme,
        HEX_SIZE,
        hex_center,
    );
}
