//! Hex/map geometry shared across svg_export submodules.

use crate::sector_model::GeneratedSector;

use super::HEX_SIZE;

const MARGIN: f32 = 28.0;

#[derive(Clone, Copy)]
pub(super) struct MapBounds {
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) fn map_bounds(sector: &GeneratedSector) -> MapBounds {
    let horiz_step = HEX_SIZE * 3f32.sqrt();
    let vert_step = HEX_SIZE * 1.5;
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = MARGIN.mul_add(2.0, horiz_step * (sector.width as f32 + odd_shift));
    let label_band = HEX_SIZE * 0.55;
    let h = 2.0f32.mul_add(
        HEX_SIZE,
        MARGIN.mul_add(2.0, sector.height.saturating_sub(1) as f32 * vert_step),
    ) + label_band;
    MapBounds { w, h }
}

pub(super) fn hex_center(q: i32, r: i32) -> (f32, f32) {
    let horiz_step = HEX_SIZE * 3f32.sqrt();
    let vert_step = HEX_SIZE * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = horiz_step.mul_add(q as f32 + row_shift, MARGIN) + horiz_step / 2.0;
    let y = MARGIN + vert_step * r as f32 + HEX_SIZE;
    (x, y)
}
