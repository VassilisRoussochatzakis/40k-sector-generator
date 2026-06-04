#![forbid(unsafe_code)]

pub mod app_icon;
pub mod card;
pub mod design;
pub mod heatmap;
pub mod info_panel;
pub mod jobs;
pub mod map_theme;
pub mod nav;
pub mod palette;
pub mod sector_view;
pub mod system_view;
pub mod theme;
pub mod ui_kit;
pub mod visual_tokens;

pub use nav::entity_link;

/// Encode an egui screenshot ([`egui::ColorImage`], RGBA8) to a PNG on disk.
///
/// Dev-tooling only: this backs the builder's `--screenshot` capture mode, the
/// autonomous visual-feedback loop a Claude session uses to *look at* a UI
/// change instead of beautifying blind (see `BEAUTY.md` §0). Not wired into any
/// shipping export path, so it has no bearing on the golden-tested writers.
///
/// # Errors
/// Returns an error if the image dimensions are degenerate or the PNG write
/// fails.
pub fn save_color_image_png(
    path: &std::path::Path,
    color_image: &egui::ColorImage,
) -> std::io::Result<()> {
    let [w, h] = color_image.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for px in &color_image.pixels {
        buf.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "screenshot has bad dimensions",
        )
    })?;
    img.save_with_format(path, image::ImageFormat::Png)
        .map_err(std::io::Error::other)
}

/// Human-readable byte size (binary units), shared by the GUI export status
/// lines — e.g. the live `sector.json` byte counter during a bundle export.
#[must_use]
pub fn human_bytes(n: u64) -> String {
    const UNIT: f64 = 1024.0;
    let b = n as f64;
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", b / UNIT / UNIT / UNIT)
    } else if n >= 1024 * 1024 {
        format!("{:.1} MiB", b / UNIT / UNIT)
    } else if n >= 1024 {
        format!("{:.1} KiB", b / UNIT)
    } else {
        format!("{n} B")
    }
}
