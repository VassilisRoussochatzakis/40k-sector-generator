#![forbid(unsafe_code)]

pub mod app_icon;
pub mod card;
pub mod design;
pub mod fonts;
pub mod heatmap;
pub mod info_panel;
pub mod jobs;
pub mod map_theme;
pub mod modal;
pub mod nav;
pub mod palette;
pub mod sector_view;
pub mod system_view;
pub mod theme;
pub mod ui_kit;
pub mod visual_tokens;
pub mod widgets;

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

#[cfg(test)]
mod tests {
    use super::human_bytes;

    #[test]
    fn human_bytes_integer_byte_branch_has_no_decimal() {
        // < 1024 uses the integer `{n} B` arm: space before `B`, no `.0`.
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
    }

    #[test]
    fn human_bytes_kib_boundary_uses_one_decimal() {
        // Exactly 1024 crosses into the KiB arm: 1024/1024 = 1.0.
        assert_eq!(human_bytes(1024), "1.0 KiB");
    }

    #[test]
    fn human_bytes_kib_upper_edge_rounds_to_1024() {
        // 1_048_575 < 1_048_576, so still KiB; 1_048_575 / 1024 = 1023.999…
        // formatted with `{:.1}` rounds up to "1024.0". This is the
        // integer-`B`-vs-`{:.1}` split the gap targets.
        assert_eq!(human_bytes(1_048_575), "1024.0 KiB");
    }

    #[test]
    fn human_bytes_mib_and_gib_boundaries() {
        assert_eq!(human_bytes(1_048_576), "1.0 MiB"); // 1024 * 1024
        assert_eq!(human_bytes(1_073_741_824), "1.0 GiB"); // 1024 * 1024 * 1024
    }
}
