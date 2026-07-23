//! SVG export: vector counterpart to [`crate::bitmap`].
//!
//! Mirrors the PNG renderer's layout and visual language but emits SVG
//! primitives (`<polygon>`, `<circle>`, `<line>`, `<rect>`, `<path>`,
//! `<text>`) so the output stays editable in vector tools and crisp at
//! arbitrary zoom. Layout uses the same scaled `Geom` as the bitmap
//! renderer with `scale=1` — SVG resolves the physical size via its
//! `width`/`height` attributes, so there is no separate resolution knob.

use std::collections::HashMap;
use std::fmt::Write as _;

use camino::Utf8Path;

use crate::errors::SectorError;
use crate::export::render_core::RenderOptions;
use crate::heatmap::{self, HeatmapMode};
use crate::map_theme::{LabelDensity, LegendStyle, MapTheme};
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

mod canvas;
mod colors;
mod geom;
mod grid;
mod labels;
mod legend;
mod primitives;
mod regions;
mod routes;
mod systems;
#[cfg(test)]
mod tests;

use geom::map_bounds;
use grid::{compute_heat_tints, draw_hex_grid, draw_subsector_borders};
use labels::{draw_subsector_labels, draw_system_labels};
use legend::{draw_legend, legend_height};
use primitives::rect;
use regions::draw_region_labels;
use routes::draw_routes;
use systems::draw_systems;

pub(crate) const HEX_SIZE: f32 = 26.0;

pub(crate) use crate::export::render_core::routes::star_radius_ratio;

fn legend_width(theme: &MapTheme) -> f32 {
    match theme.legend {
        LegendStyle::Hidden => 0.0,
        LegendStyle::Compact => 220.0,
        LegendStyle::Full => 280.0,
    }
}

/// Render an SVG document for `sector`. Pure — does no I/O.
#[must_use]
pub fn render_sector_svg(
    sector: &GeneratedSector,
    subsectors: Option<&[Subsector]>,
    opts: &RenderOptions,
) -> String {
    let bounds = map_bounds(sector);
    let total_w = bounds.w + legend_width(&opts.theme);
    let legend_h = legend_height(sector, opts);
    let total_h = bounds.h.max(legend_h);

    let mut s = String::with_capacity(64 * 1024);
    let _ = write!(
        s,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="monospace">"#,
        w = total_w.round() as i32,
        h = total_h.round() as i32,
    );

    // Background.
    rect(&mut s, 0.0, 0.0, total_w, total_h, opts.theme.bg, None);

    let subs = subsectors.unwrap_or(&[]);
    let draw_subsectors = !subs.is_empty() && opts.theme.show_subsector_borders;

    let heat = if matches!(opts.heatmap, HeatmapMode::Off) {
        HashMap::new()
    } else {
        heatmap::compute_rgb(sector, opts.heatmap)
    };
    let heat_tints = compute_heat_tints(sector, opts, &heat);

    draw_hex_grid(&mut s, sector, &heat_tints, &opts.theme);
    if draw_subsectors {
        draw_subsector_borders(&mut s, sector, subs, &opts.theme);
    }
    draw_routes(&mut s, sector, opts);
    draw_region_labels(&mut s, sector, &opts.theme, bounds);
    draw_systems(&mut s, sector, subs, opts);
    if draw_subsectors && !matches!(opts.theme.label_density, LabelDensity::None) {
        draw_subsector_labels(&mut s, sector, subs, &opts.theme, bounds);
    }
    draw_system_labels(&mut s, sector, subs, opts);

    if !matches!(opts.theme.legend, LegendStyle::Hidden) {
        rect(
            &mut s,
            bounds.w,
            0.0,
            legend_width(&opts.theme),
            total_h,
            opts.theme.panel_bg,
            None,
        );
        draw_legend(&mut s, sector, bounds.w, opts);
    }

    s.push_str("</svg>\n");
    s
}

/// Write an SVG document to `path`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_svg_to(
    sector: &GeneratedSector,
    path: &Utf8Path,
    subsectors: Option<&[Subsector]>,
) -> Result<(), SectorError> {
    write_sector_svg_to_with(sector, path, subsectors, &RenderOptions::default())
}

/// Variant of [`write_sector_svg_to`] taking explicit [`RenderOptions`].
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_svg_to_with(
    sector: &GeneratedSector,
    path: &Utf8Path,
    subsectors: Option<&[Subsector]>,
    opts: &RenderOptions,
) -> Result<(), SectorError> {
    let body = render_sector_svg(sector, subsectors, opts);
    std::fs::write(path.as_std_path(), body).map_err(|e| SectorError::io(path.as_str(), e))
}
