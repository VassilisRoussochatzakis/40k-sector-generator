//! PNG export: renders the sector as a PNG hex map with a legend.
//!
//! Pure pixel work — no external font files. A small 5x7 monospace bitmap
//! font lives in [`primitives`]. Hex layout is pointy-top with odd-r offset
//! rows, matching the ASCII map in `export.rs`.
//!
//! All sizes are derived from a `Geom` struct built from an integer scale
//! factor so the same renderer can produce a small thumbnail or a ~4K poster.
//!
//! The renderer is split into focused submodules:
//! [`geom`], [`colors`], [`grid`], [`routes`], [`systems`], [`labels`],
//! [`regions`], [`legend`]. This `mod.rs` keeps the public API surface
//! (`write_bitmap*`, `render_sector_image`, `encode_png_bytes`,
//! `RenderOptions`) and the top-level `render()` orchestrator.

use camino::Utf8Path;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use crate::errors::SectorError;
use crate::map_theme::{LabelDensity, LegendStyle};
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

use geom::{map_bounds, Geom, MapBounds};

pub(crate) use colors::{darken, short, star_color, tint_against};
// These cross the `bitmap` module boundary (used by `system_map.rs`);
// submodule callers reach `primitives` directly via `super::primitives::*`.
pub(crate) use primitives::{
    draw_circle, draw_line, draw_rect_outline, draw_ring, draw_text, fill_circle, fill_rect,
    text_size,
};

/// Per-render options independent of the project config. Backwards-compat
/// re-export: the type now lives in [`super::render_core::options`] (Task 3
/// Pass B) so both `bitmap` and `svg_export` can share it without one
/// reaching into the other.
pub use super::render_core::RenderOptions;

/// Save an RGBA image as PNG using fast (low-CPU) deflate. Lossless.
pub(crate) fn save_png_fast(img: &RgbaImage, path: &Utf8Path) -> Result<(), SectorError> {
    let file = std::fs::File::create(path.as_std_path())
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    let writer = std::io::BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::NoFilter);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    Ok(())
}

/// Render the sector PNG to `output_dir/sector.png` with the given
/// [`RenderOptions`] (faction fill + heatmap mode).
pub fn write_bitmap_with(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> Result<(), SectorError> {
    let path = output_dir.join("sector.png");
    let img = render(sector, scale, subsectors, opts);
    save_png_fast(&img, &path)
}

/// Render the sector PNG to an explicit file path (caller chooses the name),
/// with explicit [`RenderOptions`].
pub fn write_sector_png_to_with(
    sector: &GeneratedSector,
    path: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> Result<(), SectorError> {
    let img = render(sector, scale, subsectors, opts);
    save_png_fast(&img, path)
}

/// docs/OPTIMIZE.txt G1: pure rasterisation (no PNG encoding, no disk I/O). Returns
/// the in-memory RGBA image so benches and golden tests can isolate
/// rasterisation cost from PNG encode cost.
#[must_use]
pub fn render_sector_image(
    sector: &GeneratedSector,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> RgbaImage {
    render(sector, scale, subsectors, opts)
}

/// docs/OPTIMIZE.txt G1: encode an in-memory RGBA image to PNG bytes. Same encoder
/// settings as [`write_bitmap_with`] (fast deflate, no filter). Bench /
/// golden-test only — production callers should use [`write_bitmap_with`] /
/// [`write_sector_png_to_with`] which stream straight to disk.
///
/// # Errors
///
/// Returns [`SectorError::ExportFailed`] if the encoder rejects the buffer.
pub fn encode_png_bytes(img: &RgbaImage) -> Result<Vec<u8>, SectorError> {
    let mut buf: Vec<u8> = Vec::with_capacity(img.as_raw().len() / 4);
    let encoder =
        PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, FilterType::NoFilter);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| SectorError::export("<memory>", e.to_string()))?;
    Ok(buf)
}

fn render(
    sector: &GeneratedSector,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> RgbaImage {
    let g = Geom::new(scale, &opts.theme);
    let MapBounds { w: map_w, h: map_h } = map_bounds(sector, &g);

    let legend_h = legend::legend_height(sector, &g, &opts);
    let total_w = map_w.saturating_add(g.legend_width);
    let total_h = map_h.max(legend_h);

    let mut img =
        RgbaImage::from_pixel(total_w.max(0) as u32, total_h.max(0) as u32, opts.theme.bg);

    let subs = subsectors.unwrap_or(&[]);
    let draw_subsectors = !subs.is_empty() && opts.theme.show_subsector_borders;

    // Per-system heatmap overlay tint (§10). No faction fill — the live map
    // never tints hexes by faction, and the export now matches it.
    let heat = grid::compute_heatmap(sector, &opts);
    let heat_tints = grid::compute_heat_tints(sector, &opts, &heat);

    grid::draw_hex_grid(&mut img, sector, &g, &heat_tints, &opts.theme);
    if draw_subsectors {
        grid::draw_subsector_borders(&mut img, sector, subs, &g, &opts.theme);
    }
    routes::draw_routes(&mut img, sector, &g, &opts);
    regions::draw_region_labels(&mut img, sector, &g, &opts.theme);
    systems::draw_systems(&mut img, sector, subs, &g, &opts);
    if draw_subsectors && !matches!(opts.theme.label_density, LabelDensity::None) {
        labels::draw_subsector_labels(&mut img, sector, subs, &g, &opts.theme);
    }
    labels::draw_system_labels(&mut img, sector, subs, &g, &opts);

    // Legend painted last so any overflow from the map gets clipped behind it.
    if !matches!(opts.theme.legend, LegendStyle::Hidden) {
        fill_rect(
            &mut img,
            map_w,
            0,
            g.legend_width,
            total_h,
            opts.theme.panel_bg,
        );
        legend::draw_legend(&mut img, sector, map_w, &g, &opts);
    }

    img
}

#[cfg(test)]
mod tests;
