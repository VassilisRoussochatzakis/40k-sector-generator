//! Color helpers for the bitmap renderer.
//!
//! Pass B (REFACTOR_PLAN.md Task 3): the genuinely-identical helpers
//! (`star_color`, `stability_color`, `tint_against`, `darken`, `dim`,
//! `short`, `rgba`) now live once in [`super::super::render_core::colors`].
//! This file keeps only the bitmap-specific `i32`-quantised wrappers
//! (`route_thickness` returning pixels, `stroke_px`) that the bitmap
//! primitive layer wants in pre-rounded form.

use crate::map_theme::MapTheme;
use crate::sector_model::RouteStability;

pub(crate) use super::super::render_core::colors::{
    darken, dim as dim_rgba, rgba, short, stability_color, star_color, tint_against,
};

use super::super::render_core::colors::route_thickness_f32;
use super::geom::Geom;

/// `i32`-quantised route thickness for the bitmap line primitives.
pub(super) fn route_thickness(theme: &MapTheme, stability: RouteStability, g: &Geom) -> i32 {
    route_thickness_f32(theme, stability, g.hex_size)
        .round()
        .max(1.0) as i32
}

/// `i32`-quantised stroke scaling for nested bitmap glyphs.
pub(super) fn stroke_px(thickness: i32, factor: f32) -> i32 {
    ((thickness as f32) * factor).round().max(1.0) as i32
}
