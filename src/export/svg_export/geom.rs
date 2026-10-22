//! Hex/map geometry shared across svg_export submodules: the
//! `render_core::geom` layout at scale 1.

use crate::export::render_core::geom;
use crate::sector_model::GeneratedSector;

use super::HEX_SIZE;

const MARGIN: f32 = 28.0;

#[derive(Clone, Copy)]
pub(super) struct MapBounds {
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) fn map_bounds(sector: &GeneratedSector) -> MapBounds {
    let (w, h) = geom::map_size(sector, HEX_SIZE, MARGIN);
    MapBounds { w, h }
}

pub(super) fn hex_center(q: i32, r: i32) -> (f32, f32) {
    geom::hex_center(q, r, HEX_SIZE, MARGIN)
}
