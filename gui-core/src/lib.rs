#![forbid(unsafe_code)]

pub mod app_icon;
pub mod heatmap;
pub mod info_panel;
pub mod jobs;
pub mod map_theme;
pub mod nav;
pub mod palette;
pub mod sector_view;
pub mod system_view;
pub mod visual_tokens;

pub use nav::entity_link;

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
