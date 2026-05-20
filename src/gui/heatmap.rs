//! GUI wrapper around the pure heatmap scoring in [`crate::heatmap`].
//!
//! Just re-exports `HeatmapMode` and converts the RGB cells into `Color32`
//! cells so the egui sector view can paint them directly.

use std::collections::HashMap;

use egui::Color32;

use crate::sector_model::GeneratedSector;

pub use crate::heatmap::HeatmapMode;

/// One per-hex sample: tint colour plus a 0..=1 intensity.
#[derive(Debug, Clone, Copy)]
pub struct HeatCell {
    pub color: Color32,
    pub intensity: f32,
}

/// Computed heatmap values keyed by system id.
pub fn compute(
    sector: &GeneratedSector,
    mode: HeatmapMode,
) -> HashMap<crate::ids::SystemId, HeatCell> {
    crate::heatmap::compute_rgb(sector, mode)
        .into_iter()
        .map(|(k, c)| {
            (
                k,
                HeatCell {
                    color: Color32::from_rgb(c.rgb.0, c.rgb.1, c.rgb.2),
                    intensity: c.intensity,
                },
            )
        })
        .collect()
}
