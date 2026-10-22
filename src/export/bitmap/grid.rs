//! Bitmap grid wrappers: heat-cell lookup (`compute_heatmap`) and
//! `BitmapCanvas` adapters for the shared grid/border drawing; heat tints are
//! re-exported from `render_core::grid`.

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::heatmap::{self, HeatCellRgb};
use crate::map_theme::MapTheme;
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

pub(super) use crate::export::render_core::grid::compute_heat_tints;

use super::canvas::BitmapCanvas;
use super::geom::{hex_center, Geom};
use super::RenderOptions;

pub(super) fn compute_heatmap(
    sector: &GeneratedSector,
    opts: &RenderOptions,
) -> HashMap<crate::ids::SystemId, HeatCellRgb> {
    heatmap::compute_rgb(sector, opts.heatmap)
}

pub(super) fn draw_hex_grid(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    g: &Geom,
    heat_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    let region_colours = crate::export::render_core::grid::compute_region_colours(sector);
    let mut canvas = BitmapCanvas::new(img);
    crate::export::render_core::grid::draw_hex_grid(
        &mut canvas,
        sector,
        heat_tints,
        &region_colours,
        theme,
        g.hex_size,
        |q, r| {
            let (x, y) = hex_center(q, r, g);
            (x as f32, y as f32)
        },
    );
}

pub(super) fn draw_subsector_borders(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    theme: &MapTheme,
) {
    let mut canvas = BitmapCanvas::new(img);
    crate::export::render_core::grid::draw_subsector_borders(
        &mut canvas,
        sector,
        subsectors,
        theme,
        g.hex_size,
        |q, r| {
            let (x, y) = hex_center(q, r, g);
            (x as f32, y as f32)
        },
    );
}
