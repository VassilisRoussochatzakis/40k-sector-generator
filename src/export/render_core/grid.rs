//! Shared hex-grid fill + subsector-border drawing.

use std::collections::HashMap;

use image::Rgba;

use super::canvas::Canvas;
use crate::map_theme::MapTheme;
use crate::sector_model::{offset_r_neighbors, GeneratedSector};
use crate::subsectors::Subsector;

pub(crate) fn hex_vertices(cx: f32, cy: f32, size: f32) -> [(f32, f32); 6] {
    let mut out = [(0.0f32, 0.0f32); 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * 60.0f32.mul_add(i as f32, -30.0);
        *slot = (size.mul_add(angle.cos(), cx), size.mul_add(angle.sin(), cy));
    }
    out
}

pub(crate) fn draw_hex_grid<C: Canvas>(
    canvas: &mut C,
    sector: &GeneratedSector,
    sys_tints: &HashMap<(i32, i32), Rgba<u8>>,
    region_tints: &HashMap<(i32, i32), Rgba<u8>>,
    theme: &MapTheme,
    hex_size: f32,
    hex_center_fn: impl Fn(i32, i32) -> (f32, f32),
) {
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center_fn(q, r);
            let base = region_tints
                .get(&(q, r))
                .copied()
                .unwrap_or(theme.hex_empty);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(base);
            let pts = hex_vertices(cx, cy, hex_size);
            canvas.polygon(&pts, fill, Some(theme.hex_outline), 1.0);
        }
    }
}

pub(crate) fn draw_subsector_borders<C: Canvas>(
    canvas: &mut C,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
    hex_size: f32,
    hex_center_fn: impl Fn(i32, i32) -> (f32, f32),
) {
    let mut owner: HashMap<(i32, i32), &str> = HashMap::new();
    for s in subsectors {
        for &(q, r) in &s.hex_cells {
            owner.insert((q as i32, r as i32), s.id.as_ref());
        }
    }
    if owner.is_empty() {
        return;
    }
    let border_thick = (hex_size * 0.10).max(2.5);
    let dot_radius = canvas.quantize((border_thick * 0.8).max(2.0));
    let spacing = dot_radius * 2.5;
    for r in 0..sector.height as i32 {
        let deltas = offset_r_neighbors(r);
        for q in 0..sector.width as i32 {
            let Some(here_id) = owner.get(&(q, r)).copied() else {
                continue;
            };
            let (cx, cy) = hex_center_fn(q, r);
            let v = hex_vertices(cx, cy, hex_size);
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
                    canvas.circle(mx, my, dot_radius, theme.subsector_border, None, 0.0);
                }
            }
        }
    }
}
