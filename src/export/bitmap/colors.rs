//! Color helpers for the bitmap renderer — mostly re-export shims.
//!
//! Pass B / C / D (REFACTOR_PLAN.md Task 3) moved identical helpers
//! (`star_color`, `stability_color`, `tint_against`, `darken`, `dim`,
//! `short`, `rgba`) into [`super::super::render_core::colors`]. The
//! `route_thickness` wrapper kept here returns an `i32` because the
//! bitmap legend swatch code wants pre-rounded pixels.

use crate::map_theme::MapTheme;
use crate::sector_model::RouteStability;

pub(crate) use super::super::render_core::colors::{
    darken, rgba, short, stability_color, star_color, tint_against,
};

use super::super::render_core::colors::route_thickness_f32;
use super::geom::Geom;

/// `i32`-quantised route thickness for the bitmap legend swatches.
pub(super) fn route_thickness(theme: &MapTheme, stability: RouteStability, g: &Geom) -> i32 {
    route_thickness_f32(theme, stability, g.hex_size)
        .round()
        .max(1.0) as i32
}
