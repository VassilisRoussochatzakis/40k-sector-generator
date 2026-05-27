//! Hex-grid fill + subsector borders + system/region tint computation.

use std::collections::HashMap;

use image::Rgba;

use crate::export::render_core::RenderOptions;
use crate::faction_style::faction_style_rgb_by_id;
use crate::heatmap::{HeatCellRgb, HeatmapMode};
use crate::map_theme::MapTheme;
use crate::sector_model::{offset_r_neighbors, GeneratedSector};
use crate::subsectors::Subsector;

use super::colors::{rgba_from_tuple, tint_against};
use super::geom::{hex_center, hex_vertices};
use super::primitives::{circle, polygon};
use super::HEX_SIZE;

pub(super) fn compute_system_tints(
    sector: &GeneratedSector,
    opts: &RenderOptions,
    heat: &HashMap<crate::ids::SystemId, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for sys in sector.systems.iter() {
        let key = (sys.coord.q, sys.coord.r);
        if !matches!(opts.heatmap, HeatmapMode::Off) {
            if let Some(cell) = heat.get(&sys.id) {
                let strength = cell
                    .intensity
                    .mul_add(opts.theme.heatmap_tint_range, opts.theme.heatmap_tint_min);
                let color = rgba_from_tuple(cell.rgb);
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
                        rgba_from_tuple(style.fill),
                        opts.theme.faction_tint_strength,
                        opts.theme.hex_empty,
                    ),
                );
            }
        }
    }
    out
}

fn compute_region_tints(
    sector: &GeneratedSector,
    theme: &MapTheme,
) -> HashMap<(i32, i32), Rgba<u8>> {
    use crate::regions::RegionConditionKind;
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

pub(super) fn draw_hex_grid(
    s: &mut String,
    sector: &GeneratedSector,
    sys_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
) {
    let region_tints = compute_region_tints(sector, theme);
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r);
            let base = region_tints
                .get(&(q, r))
                .copied()
                .unwrap_or(theme.hex_empty);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(base);
            polygon(
                s,
                &hex_vertices(cx, cy, HEX_SIZE),
                fill,
                Some(theme.hex_outline),
                1.0,
            );
        }
    }
}

pub(super) fn draw_subsector_borders(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
) {
    let mut owner: HashMap<(i32, i32), &str> = HashMap::new();
    for sub in subsectors {
        for &(q, r) in &sub.hex_cells {
            owner.insert((q as i32, r as i32), sub.id.as_ref());
        }
    }
    if owner.is_empty() {
        return;
    }
    let border_thick = (HEX_SIZE * 0.10).max(2.5);
    let dot_radius = (border_thick * 0.8).max(2.0);
    let spacing = dot_radius * 2.5;
    for r in 0..sector.height as i32 {
        let deltas = offset_r_neighbors(r);
        for q in 0..sector.width as i32 {
            let Some(here_id) = owner.get(&(q, r)).copied() else {
                continue;
            };
            let (cx, cy) = hex_center(q, r);
            let v = hex_vertices(cx, cy, HEX_SIZE);
            for (i, (dq, dr)) in deltas.iter().enumerate() {
                let other = owner.get(&(q + dq, r + dr)).copied();
                let differs = match other {
                    Some(id) => id != here_id,
                    None => true,
                };
                if !differs {
                    continue;
                }
                let a = v[i];
                let b = v[(i + 1) % 6];
                let edge_len = (b.0 - a.0).hypot(b.1 - a.1);
                let segments = (edge_len / spacing).ceil().max(1.0) as usize;
                for j in 0..=segments {
                    let t = j as f32 / segments as f32;
                    let mx = (b.0 - a.0).mul_add(t, a.0);
                    let my = (b.1 - a.1).mul_add(t, a.1);
                    circle(s, mx, my, dot_radius, theme.subsector_border, None, 0.0);
                }
            }
        }
    }
}
