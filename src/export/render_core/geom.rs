//! Pointy-top odd-r hex layout (`f32` px) shared by both backends: SVG uses it
//! as-is, bitmap rounds centres and truncates bounds.

use crate::sector_model::GeneratedSector;

/// Map `(w, h)` matching the GUI's `sector_view` layout: odd rows shift right
/// half a step, and `h` includes the bottom label band for system names.
pub(crate) fn map_size(sector: &GeneratedSector, hex_size: f32, margin: f32) -> (f32, f32) {
    let horiz_step = hex_size * 3f32.sqrt();
    let vert_step = hex_size * 1.5;
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = margin.mul_add(2.0, horiz_step * (sector.width as f32 + odd_shift));
    let h = 2.0f32.mul_add(
        hex_size,
        margin.mul_add(2.0, sector.height.saturating_sub(1) as f32 * vert_step),
    ) + hex_size * 0.55;
    (w, h)
}

pub(crate) fn hex_center(q: i32, r: i32, hex_size: f32, margin: f32) -> (f32, f32) {
    let horiz_step = hex_size * 3f32.sqrt();
    let vert_step = hex_size * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = horiz_step.mul_add(q as f32 + row_shift, margin) + horiz_step / 2.0;
    (x, margin + vert_step * r as f32 + hex_size)
}
