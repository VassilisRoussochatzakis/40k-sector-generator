//! Hex-grid fill + subsector borders + per-system tints.
//!
//! Tint computation stays here (it joins backend-agnostic theme math with
//! `Rgba` types that both backends already share). The hex polygon walk
//! and subsector border drawing call into [`crate::export::render_core::grid`].

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::heatmap::{self, HeatCellRgb, HeatmapMode};
use crate::map_theme::MapTheme;
use crate::regions::RegionConditionKind;
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
    let region_colours = compute_region_colours(sector);
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

/// Raw region condition colour per hex (un-blended). The blend onto the hex
/// base happens in `render_core::grid::draw_hex_grid` via `blend_heat`, so
/// these values match `gui_core::map_theme::RenderMapTheme`'s region palette
/// exactly.
fn compute_region_colours(sector: &GeneratedSector) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for region in sector.regions.iter() {
        let colour = region_colour(region.kind);
        for h in &region.hexes {
            out.insert((h.q, h.r), colour);
        }
    }
    out
}

/// Region condition → overlay colour. Matches the live renderer's
/// `RenderMapTheme` region palette.
fn region_colour(kind: RegionConditionKind) -> Rgba<u8> {
    match kind {
        RegionConditionKind::WarpStorm => Rgba([170, 60, 180, 255]),
        RegionConditionKind::Turbulence => Rgba([140, 100, 200, 255]),
        RegionConditionKind::CalmCorridor => Rgba([90, 200, 180, 255]),
        RegionConditionKind::Blackout => Rgba([60, 60, 80, 255]),
        RegionConditionKind::Anomaly => Rgba([220, 160, 60, 255]),
        RegionConditionKind::NecropolisDrift => Rgba([100, 130, 140, 255]),
        RegionConditionKind::BeaconChain => Rgba([230, 210, 100, 255]),
        RegionConditionKind::EmpyricBleed => Rgba([190, 70, 160, 255]),
    }
}
