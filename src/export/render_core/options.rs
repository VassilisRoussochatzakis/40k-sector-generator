//! Backend-neutral [`RenderOptions`] — moved here from `bitmap` so that
//! `svg_export` no longer reaches *into* `bitmap` for it (the leaky
//! abstraction the plan called out).

use crate::map_theme::MapTheme;

/// Per-render options independent of the project config. Mirrors the relevant
/// bits of [`crate::config::BitmapConfig`] so callers (CLI, GUI export, tests)
/// can override without touching the project's TOML.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Tint each system's hex by the dominant faction (§8).
    pub faction_fill: bool,
    /// Overlay a heatmap tint per system (§10). `Off` disables it.
    pub heatmap: crate::heatmap::HeatmapMode,
    /// §13 NEW2.md: presentation-only map theme.
    pub theme: MapTheme,
    pub route_view_mode: crate::sector_model::RouteViewMode,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            faction_fill: true,
            heatmap: crate::heatmap::HeatmapMode::Off,
            theme: MapTheme::gm_dark(),
            route_view_mode: crate::sector_model::RouteViewMode::default(),
        }
    }
}
