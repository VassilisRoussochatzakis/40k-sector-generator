//! Color/style helpers for the SVG renderer — mostly re-export shims.
//!
//! Pass B / C / D (REFACTOR_PLAN.md Task 3) moved identical helpers
//! into [`super::super::render_core::colors`]. The `route_thickness`
//! wrapper kept here forwards to `route_thickness_f32` at the SVG's
//! native `HEX_SIZE` so the legend swatch code stays terse.

use crate::map_theme::MapTheme;
use crate::sector_model::RouteStability;

pub(super) use super::super::render_core::colors::{
    darken, rgba as rgba_from_tuple, short, stability_color, star_color, tint_against,
};

use super::super::render_core::colors::route_thickness_f32;
use super::HEX_SIZE;

pub(super) fn route_thickness(theme: &MapTheme, stability: RouteStability) -> f32 {
    route_thickness_f32(theme, stability, HEX_SIZE)
}
