//! Shared application window icon for sectorforge GUI binaries.

use std::sync::Arc;

use egui::IconData;
use image::imageops::FilterType;

const ICON_PNG: &[u8] = include_bytes!("../../sectorpic.png");

/// Decode the bundled app icon into an [`IconData`] suitable for
/// `egui::ViewportBuilder::with_icon`. Returns `None` if decoding fails.
pub fn load_app_icon() -> Option<Arc<IconData>> {
    let img = image::load_from_memory(ICON_PNG).ok()?;
    let resized = img.resize_exact(256, 256, FilterType::Lanczos3).to_rgba8();
    let (width, height) = resized.dimensions();
    Some(Arc::new(IconData {
        rgba: resized.into_raw(),
        width,
        height,
    }))
}
