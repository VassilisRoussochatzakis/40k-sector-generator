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
use crate::regions::RegionConditionKind;
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

pub(super) fn draw_hex_grid(
    s: &mut String,
    sector: &GeneratedSector,
    heat_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    let region_colours = compute_region_colours(sector);
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
