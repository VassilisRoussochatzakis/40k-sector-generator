//! Color/style helpers for the SVG renderer.
//!
//! Pass B (REFACTOR_PLAN.md Task 3): identical helpers moved into
//! [`super::super::render_core::colors`]. This file keeps only the
//! `f32`-native wrappers that match the SVG primitive layer's expected
//! types.

use crate::map_theme::MapTheme;
use crate::sector_model::RouteStability;

pub(super) use super::super::render_core::colors::{
    darken, dim, rgba as rgba_from_tuple, short, stability_color, star_color, tint_against,
};

use super::super::render_core::colors::route_thickness_f32;
use super::HEX_SIZE;

pub(super) fn route_thickness(theme: &MapTheme, stability: RouteStability) -> f32 {
    route_thickness_f32(theme, stability, HEX_SIZE)
}

pub(super) fn stroke_px(thickness: f32, factor: f32) -> f32 {
    (thickness * factor).max(1.0)
}
