//! Hex-grid fill: per-system tints, region tints, and the grid itself.

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::faction_style::faction_style_rgb_by_id;
use crate::heatmap::{self, HeatCellRgb, HeatmapMode};
use crate::map_theme::MapTheme;
use crate::regions::RegionConditionKind;
use crate::sector_model::GeneratedSector;

use super::colors::{rgba, tint_against};
use super::geom::{draw_hex, hex_center, Geom};
use super::RenderOptions;

/// One hex tint per (q, r) where the system has a dominant faction (§8) or a
/// heatmap intensity > 0 (§10). Empty hexes stay at the theme's base fill.
pub(super) fn compute_system_tints(
    sector: &GeneratedSector,
    opts: &RenderOptions,
    heat: &HashMap<crate::ids::SystemId, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for sys in sector.systems.iter() {
        let key = (sys.coord.q, sys.coord.r);
        // Heatmap overrides faction fill for non-Control modes. For Control
        // mode the underlying score already drives `faction_style.fill`, so
        // both paths agree.
        if !matches!(opts.heatmap, HeatmapMode::Off) {
            if let Some(cell) = heat.get(&sys.id) {
                let strength = cell
                    .intensity
                    .mul_add(opts.theme.heatmap_tint_range, opts.theme.heatmap_tint_min);
                let color = rgba(cell.rgb);
                out.insert(key, tint_against(color, strength, opts.theme.hex_empty));
                continue;
            }
        }
        if opts.faction_fill {
            if let Some(dom) = sys.control.dominant.as_deref() {
                let style = faction_style_rgb_by_id(&sector.factions, dom);
                out.insert(
                    key,
                    tint_against(
                        rgba(style.fill),
                        opts.theme.faction_tint_strength,
                        opts.theme.hex_empty,
                    ),
                );
            }
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
    sys_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    // §5 NEW.md: region tints underneath the system tint so the overlay reads
    // as background colour rather than overwriting faction fill.
    let region_tints = compute_region_tints(sector, theme);
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r, g);
            let base = region_tints
                .get(&(q, r))
                .copied()
                .unwrap_or(theme.hex_empty);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(base);
            draw_hex(img, cx, cy, g.hex_size, fill, theme.hex_outline);
        }
    }
}

fn compute_region_tints(
    sector: &GeneratedSector,
    theme: &MapTheme,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for region in sector.regions.iter() {
        let base = match region.kind {
            RegionConditionKind::WarpStorm => Rgba([120, 60, 180, 255]),
            RegionConditionKind::Turbulence => Rgba([110, 100, 160, 255]),
            RegionConditionKind::CalmCorridor => Rgba([80, 160, 170, 255]),
            RegionConditionKind::Blackout => Rgba([60, 60, 70, 255]),
            RegionConditionKind::Anomaly => Rgba([180, 130, 100, 255]),
            RegionConditionKind::NecropolisDrift => Rgba([100, 120, 130, 255]),
            RegionConditionKind::BeaconChain => Rgba([190, 180, 110, 255]),
            RegionConditionKind::EmpyricBleed => Rgba([150, 80, 140, 255]),
        };
        let tinted = tint_against(base, theme.region_tint_strength, theme.hex_empty);
        for h in &region.hexes {
            out.insert((h.q, h.r), tinted);
        }
    }
    out
}
