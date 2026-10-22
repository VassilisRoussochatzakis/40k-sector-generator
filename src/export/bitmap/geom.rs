//! Scaled geometry + hex math for the bitmap renderer.

use crate::export::render_core::{geom, legend::legend_width};
use crate::map_theme::MapTheme;
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
        Self {
            scale: s,
            hex_size: 26.0 * s as f32,
            margin: 28 * s,
            legend_width: legend_width(theme.legend) as i32 * s,
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

// ponytail: truncating body + label band once matches the old per-part truncs only
// below ~2^21 px tall (f32 ulp); valid configs stay under ~320k. Split if that changes.
pub(super) fn map_bounds(sector: &GeneratedSector, g: &Geom) -> MapBounds {
    let (w, h) = geom::map_size(sector, g.hex_size, g.margin as f32);
    let (w, h) = (w as i32, h as i32);
    MapBounds { w, h }
}

pub(super) fn hex_center(q: i32, r: i32, g: &Geom) -> (i32, i32) {
    let (x, y) = geom::hex_center(q, r, g.hex_size, g.margin as f32);
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
