//! Hex-grid fill + subsector borders + per-system tints.
//!
//! Tint computation stays here (it joins backend-agnostic theme math with
//! `Rgba` types that both backends already share). The hex polygon walk
//! and subsector border drawing call into [`crate::export::render_core::grid`].

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::heatmap::{self, HeatCellRgb, HeatmapMode};
use crate::map_theme::MapTheme;
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use crate::export::render_core::colors::blend_heat;

use super::canvas::BitmapCanvas;
use super::colors::rgba;
use super::geom::{hex_center, Geom};
use super::RenderOptions;

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
                blend_heat(opts.theme.hex_empty, rgba(cell.rgb), cell.intensity),
            );
        }
    }
    out
}

pub(super) fn compute_heatmap(
    sector: &GeneratedSector,
    opts: &RenderOptions,
) -> HashMap<crate::ids::SystemId, HeatCellRgb> {
    if matches!(opts.heatmap, HeatmapMode::Off) {
        HashMap::new()
    } else {
        heatmap::compute_rgb(sector, opts.heatmap)
    }
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
