//! Visual tokens for the sector map renderer.
//!
//! `RenderMapTheme` is the single source of truth for every colour and sizing
//! constant that [`crate::sector_view::SectorView`] paints. Apps either pass a
//! customised theme or use [`RenderMapTheme::default`]; the viewer, the
//! editor's MAP panel, and the builder's MAP tab all read the same theme, so
//! adding a new map element is a one-place change.
//!
//! Named `RenderMapTheme` to disambiguate from
//! [`sectorforge::map_theme::MapTheme`], which is the data-layer
//! representation parsed from user TOML and consumed by PNG/SVG exporters.
//! This type is the rendering-layer counterpart used by the egui paint code.
//!
//! Sizing is expressed as [`ScaledSize`] — a `(multiplier, min_px)` pair the
//! painter resolves with `hex_size * mul`, floored at `min_px` so icons stay
//! readable when the user zooms out.

use egui::Color32;

use crate::palette;
use crate::visual_tokens::MapRegionOverlay;

#[derive(Clone, Copy, Debug)]
pub struct ScaledSize {
    pub mul: f32,
    pub min: f32,
}

impl ScaledSize {
    pub const fn new(mul: f32, min: f32) -> Self {
        Self { mul, min }
    }

    pub fn px(self, hex_size: f32) -> f32 {
        (hex_size * self.mul).max(self.min)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderMapTheme {
    // -- core colours -------------------------------------------------------
    pub bg: Color32,
    pub hex_empty: Color32,
    pub hex_outline: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub selection: Color32,
    pub path_highlight: Color32,
    pub path_waypoint: Color32,
    // -- overlay colours ---------------------------------------------------
    pub subsector_border_color: Color32,
    pub subsector_label: Color32,
    pub subsector_highlight: Color32,
    pub subsector_label_bg: Color32,
    pub capital_marker_fill: Color32,
    pub capital_marker_outline: Color32,
    pub region_label: Color32,
    pub region_label_bg: Color32,
    pub region_label_outline: Color32,
    pub pinned_outline: Color32,
    pub multi_select_outline: Color32,
    pub rect_select_tint: Color32,
    pub pending_route_preview: Color32,
    // -- region condition palette ------------------------------------------
    pub region_warp_storm: Color32,
    pub region_turbulence: Color32,
    pub region_calm_corridor: Color32,
    pub region_blackout: Color32,
    pub region_anomaly: Color32,
    pub region_necropolis_drift: Color32,
    pub region_beacon_chain: Color32,
    pub region_empyric_bleed: Color32,
    // -- sizing tokens (multiplier · hex_size, floored at min) -------------
    pub star_radius_mul: f32,
    pub route_thickness: ScaledSize,
    pub subsector_border_thickness: ScaledSize,
    pub capital_marker_radius: ScaledSize,
    pub pip_font: ScaledSize,
    pub pip_disc_radius: ScaledSize,
    pub system_label_font: ScaledSize,
    pub subsector_label_font: ScaledSize,
    pub region_label_font: ScaledSize,
    // -- glow alphas (0..=255) on premultiplied glow colours ----------------
    pub path_glow_alpha: u8,
    pub selection_glow_alpha: u8,
    // -- ring radius bumps (added to hex_size) -----------------------------
    pub pinned_ring_bump: f32,
    pub selection_ring_bump: f32,
    pub waypoint_ring_bump: f32,
    // -- ring stroke widths ------------------------------------------------
    pub pinned_ring_thickness: f32,
    pub selection_ring_thickness: f32,
    pub multi_select_ring_thickness: f32,
    pub waypoint_ring_thickness: f32,
    // -- route overlay thickness multipliers (× route_thickness) -----------
    pub path_glow_mul: f32,
    pub path_core_mul: f32,
    pub selection_glow_mul: f32,
    pub selection_core_mul: f32,
    pub pending_route_preview_mul: f32,
    pub pending_route_preview_min: f32,
}

impl RenderMapTheme {
    pub fn region_color(&self, kind: MapRegionOverlay) -> Color32 {
        match kind {
            MapRegionOverlay::WarpStorm => self.region_warp_storm,
            MapRegionOverlay::Turbulence => self.region_turbulence,
            MapRegionOverlay::CalmCorridor => self.region_calm_corridor,
            MapRegionOverlay::Blackout => self.region_blackout,
            MapRegionOverlay::Anomaly => self.region_anomaly,
            MapRegionOverlay::NecropolisDrift => self.region_necropolis_drift,
            MapRegionOverlay::BeaconChain => self.region_beacon_chain,
            MapRegionOverlay::EmpyricBleed => self.region_empyric_bleed,
        }
    }
}

impl Default for RenderMapTheme {
    fn default() -> Self {
        Self {
            bg: palette::BG,
            hex_empty: palette::HEX_EMPTY,
            hex_outline: palette::HEX_OUTLINE,
            text: palette::TEXT,
            text_dim: palette::TEXT_DIM,
            selection: palette::SELECTION,
            path_highlight: palette::PATH_HIGHLIGHT,
            path_waypoint: palette::PATH_WAYPOINT,

            subsector_border_color: Color32::from_rgb(160, 160, 160),
            subsector_label: Color32::from_rgb(230, 195, 120),
            subsector_highlight: Color32::from_rgba_premultiplied(40, 40, 44, 70),
            subsector_label_bg: Color32::from_rgba_unmultiplied(20, 16, 28, 210),
            capital_marker_fill: Color32::from_rgb(255, 220, 100),
            capital_marker_outline: Color32::from_rgb(60, 40, 10),
            region_label: Color32::from_rgb(178, 174, 196),
            region_label_bg: Color32::from_rgba_premultiplied(16, 14, 22, 190),
            region_label_outline: Color32::from_rgba_premultiplied(42, 40, 53, 90),
            pinned_outline: Color32::from_rgb(255, 120, 90),
            multi_select_outline: Color32::from_rgb(180, 200, 255),
            rect_select_tint: Color32::from_rgba_unmultiplied(255, 240, 120, 30),
            pending_route_preview: Color32::from_rgb(255, 220, 120),

            region_warp_storm: Color32::from_rgb(170, 60, 180),
            region_turbulence: Color32::from_rgb(140, 100, 200),
            region_calm_corridor: Color32::from_rgb(90, 200, 180),
            region_blackout: Color32::from_rgb(60, 60, 80),
            region_anomaly: Color32::from_rgb(220, 160, 60),
            region_necropolis_drift: Color32::from_rgb(100, 130, 140),
            region_beacon_chain: Color32::from_rgb(230, 210, 100),
            region_empyric_bleed: Color32::from_rgb(190, 70, 160),

            star_radius_mul: 0.2016,
            route_thickness: ScaledSize::new(0.08, 2.0),
            subsector_border_thickness: ScaledSize::new(0.10, 2.5),
            capital_marker_radius: ScaledSize::new(0.15, 3.5),
            pip_font: ScaledSize::new(0.36, 11.0),
            pip_disc_radius: ScaledSize::new(0.22, 8.0),
            system_label_font: ScaledSize::new(0.28, 9.0),
            subsector_label_font: ScaledSize::new(0.36, 11.0),
            region_label_font: ScaledSize::new(0.31, 10.0),

            path_glow_alpha: 70,
            selection_glow_alpha: 75,

            pinned_ring_bump: 5.0,
            selection_ring_bump: 2.0,
            waypoint_ring_bump: 4.0,

            pinned_ring_thickness: 1.5,
            selection_ring_thickness: 2.5,
            multi_select_ring_thickness: 2.0,
            waypoint_ring_thickness: 2.5,

            path_glow_mul: 3.2,
            path_core_mul: 1.8,
            selection_glow_mul: 3.6,
            selection_core_mul: 1.9,
            pending_route_preview_mul: 1.4,
            pending_route_preview_min: 2.5,
        }
    }
}
