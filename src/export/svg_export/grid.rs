//! Backend-specific grid geometry; tint and grid drawing logic lives in render_core.

use std::collections::HashMap;

use image::Rgba;

pub(super) use crate::export::render_core::grid::compute_heat_tints;
use crate::map_theme::MapTheme;
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::canvas::SvgCanvas;
use super::geom::hex_center;
use super::HEX_SIZE;

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
