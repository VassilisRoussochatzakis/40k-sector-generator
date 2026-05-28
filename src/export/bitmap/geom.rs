//! Scaled geometry + hex math for the bitmap renderer.

use crate::map_theme::{LegendStyle, MapTheme};
use crate::sector_model::GeneratedSector;

/// Scaled geometry for the sector map. All pixel sizes are derived from
/// `scale` so callers can opt into higher resolution renders.
pub(crate) struct Geom {
    pub scale: i32,
    pub hex_size: f32,
    pub margin: i32,
    pub legend_width: i32,
    pub legend_pad: i32,
    pub line_h: i32,
    pub text_scale: i32,
    pub title_scale: i32,
}

impl Geom {
    pub(super) fn new(scale: u32, theme: &MapTheme) -> Self {
        let s = scale.max(1).min(i32::MAX as u32) as i32;
        let legend_width = match theme.legend {
            LegendStyle::Hidden => 0,
            LegendStyle::Compact => 220 * s,
            LegendStyle::Full => 280 * s,
        };
        Self {
            scale: s,
            hex_size: 26.0 * s as f32,
            margin: 28 * s,
            legend_width,
            legend_pad: 16 * s,
            line_h: 18 * s,
            text_scale: s,
            title_scale: 2 * s,
        }
    }
}

/// Map pixel bounds matching the GUI's `sector_view` layout. Includes the
/// bottom label band so system-name text fits under each hex.
pub(super) struct MapBounds {
    pub w: i32,
    pub h: i32,
}

pub(super) fn map_bounds(sector: &GeneratedSector, g: &Geom) -> MapBounds {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    // Pointy-top odd-r offset layout: odd rows shift right by half a step,
    // so the bounding rect is `width * horiz_step` wide plus a half-step
    // when height > 1 to cover the staggered odd rows.
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = (g.margin as f32).mul_add(2.0, horiz_step * (sector.width as f32 + odd_shift)) as i32;
    let label_band = (g.hex_size * 0.55) as i32;
    let h = 2.0f32.mul_add(
        g.hex_size,
        (g.margin as f32).mul_add(2.0, (sector.height.saturating_sub(1)) as f32 * vert_step),
    ) as i32
        + label_band;
    MapBounds { w, h }
}

pub(super) fn hex_center(q: i32, r: i32, g: &Geom) -> (i32, i32) {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = horiz_step.mul_add(q as f32 + row_shift, g.margin as f32) + horiz_step / 2.0;
    let y = g.margin as f32 + vert_step * r as f32 + g.hex_size;
    (x.round() as i32, y.round() as i32)
}

/// Axis-aligned rect in pixel space. Used for collision tests when placing
/// subsector labels.
#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub(super) fn intersects(self, o: Rect) -> bool {
        self.x0 < o.x1 && self.x1 > o.x0 && self.y0 < o.y1 && self.y1 > o.y0
    }
}
