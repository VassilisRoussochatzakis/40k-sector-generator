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
use sectorforge::sector_model::RouteStability;

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
    // -- route stability palette (§35 T4 — theme-driven) -------------------
    pub route_stable: Color32,
    pub route_unstable: Color32,
    pub route_hazardous: Color32,
    pub route_perilous: Color32,
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
    /// §35 T4: route colour for a stability tier, theme-driven. Mirrors
    /// [`crate::palette::stability_color`] for the default theme.
    pub fn route_color(&self, stability: RouteStability) -> Color32 {
        match stability {
            RouteStability::Stable => self.route_stable,
            RouteStability::Unstable => self.route_unstable,
            RouteStability::Hazardous => self.route_hazardous,
            RouteStability::Perilous => self.route_perilous,
            _ => self.route_unstable,
        }
    }

    /// §35 T1/T4: derive an egui render theme from a resolved data-layer
    /// [`sectorforge::map_theme::MapTheme`] (the kind parsed from a builtin
    /// name or a custom `map_theme.toml`). Colours the data theme does not
    /// carry (selection, path glow, region palette, sizing) keep their
    /// [`RenderMapTheme::default`] values so the live map stays legible.
    #[must_use]
    pub fn from_map_theme(mt: &sectorforge::map_theme::MapTheme) -> Self {
        // `mt.*` are `image::Rgba<u8>` whose `.0` is `[r, g, b, a]`; take the
        // array so gui-core needn't name the `image` crate directly.
        fn c([r, g, b, a]: [u8; 4]) -> Color32 {
            Color32::from_rgba_unmultiplied(r, g, b, a)
        }
        let base = Self::default();
        Self {
            bg: c(mt.bg.0),
            hex_empty: c(mt.hex_empty.0),
            hex_outline: c(mt.hex_outline.0),
            text: c(mt.text.0),
            text_dim: c(mt.text_dim.0),
            subsector_border_color: c(mt.subsector_border.0),
            subsector_label: c(mt.subsector_label.0),
            subsector_label_bg: c(mt.subsector_label_bg.0),
            capital_marker_fill: c(mt.capital_marker.0),
            capital_marker_outline: c(mt.capital_outline.0),
            route_stable: c(mt.route_stable.0),
            route_unstable: c(mt.route_unstable.0),
            route_hazardous: c(mt.route_hazardous.0),
            route_perilous: c(mt.route_perilous.0),
            // Scale the per-hex route thickness by the theme's multiplier so
            // print_mono thins and navis_tactical thickens the live lanes too.
            route_thickness: ScaledSize::new(
                base.route_thickness.mul * mt.route_thickness,
                base.route_thickness.min,
            ),
            ..base
        }
    }

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

            // Mirrors `palette::stability_color` so the default render theme is
            // visually identical to the pre-§35 hard-coded route colours.
            route_stable: Color32::from_rgb(110, 210, 130),
            route_unstable: Color32::from_rgb(240, 200, 90),
            route_hazardous: Color32::from_rgb(235, 90, 90),
            route_perilous: Color32::from_rgb(165, 100, 215),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_route_colours_match_palette_stability_color() {
        let t = RenderMapTheme::default();
        assert_eq!(
            t.route_color(RouteStability::Stable),
            crate::palette::stability_color(RouteStability::Stable)
        );
        assert_eq!(
            t.route_color(RouteStability::Perilous),
            crate::palette::stability_color(RouteStability::Perilous)
        );
    }

    #[test]
    fn from_map_theme_carries_data_route_colours() {
        // navis_tactical has distinct route colours and a thicker route mult.
        let mt = sectorforge::map_theme::resolve_map_theme(
            &sectorforge::map_theme::MapThemeConfig::named("navis_tactical"),
        )
        .unwrap();
        let rt = RenderMapTheme::from_map_theme(&mt);
        let [r, g, b, a] = mt.route_hazardous.0;
        assert_eq!(
            rt.route_color(RouteStability::Hazardous),
            Color32::from_rgba_unmultiplied(r, g, b, a)
        );
        // thickness multiplier > 1.0 widens the default per-hex thickness.
        assert!(rt.route_thickness.mul > RenderMapTheme::default().route_thickness.mul);
    }

    // ── Gap 5: from_map_theme `..base` preservation ───────────────────────────
    // Fields the data layer carries (`bg`, route colours, …) come from the
    // resolved `MapTheme`; everything else (selection, path glow, the region
    // overlay palette, `star_radius_mul`, …) is preserved from
    // `RenderMapTheme::default()` via the `..base` struct-update tail.
    #[test]
    fn from_map_theme_carries_bg_from_data() {
        // print_mono's bg is pure white (255,255,255) — unmistakably different
        // from the dark default bg, so this proves `bg` is *carried*, not kept.
        let mt = sectorforge::map_theme::resolve_map_theme(
            &sectorforge::map_theme::MapThemeConfig::named("print_mono"),
        )
        .unwrap();
        let rt = RenderMapTheme::from_map_theme(&mt);
        let base = RenderMapTheme::default();

        let [r, g, b, a] = mt.bg.0;
        assert_eq!(rt.bg, Color32::from_rgba_unmultiplied(r, g, b, a));
        // print_mono bg is opaque white.
        assert_eq!(rt.bg, Color32::from_rgb(255, 255, 255));
        // …and it differs from the preserved default, proving it was carried.
        assert_ne!(rt.bg, base.bg);
    }

    #[test]
    fn from_map_theme_preserves_non_data_fields_from_base() {
        let mt = sectorforge::map_theme::resolve_map_theme(
            &sectorforge::map_theme::MapThemeConfig::named("print_mono"),
        )
        .unwrap();
        let rt = RenderMapTheme::from_map_theme(&mt);
        let base = RenderMapTheme::default();

        // Selection / path-highlight glow colours are not carried by MapTheme;
        // they fall through `..base`.
        assert_eq!(rt.selection, base.selection);
        assert_eq!(rt.path_highlight, base.path_highlight);
        assert_eq!(rt.path_waypoint, base.path_waypoint);
        // The entire region-overlay palette is preserved (spot-check both ends).
        assert_eq!(rt.region_warp_storm, base.region_warp_storm);
        assert_eq!(rt.region_empyric_bleed, base.region_empyric_bleed);
        // A preserved scalar sizing token.
        assert_eq!(rt.star_radius_mul, base.star_radius_mul);
        assert_eq!(rt.selection_glow_alpha, base.selection_glow_alpha);
    }

    // ── Gap 11: route_color / region_color / ScaledSize::px ───────────────────
    #[test]
    fn route_color_maps_each_tier_to_its_field() {
        let t = RenderMapTheme::default();
        assert_eq!(t.route_color(RouteStability::Stable), t.route_stable);
        assert_eq!(t.route_color(RouteStability::Unstable), t.route_unstable);
        assert_eq!(t.route_color(RouteStability::Hazardous), t.route_hazardous);
        assert_eq!(t.route_color(RouteStability::Perilous), t.route_perilous);
        // Concrete default values (mirror `palette::stability_color`).
        assert_eq!(t.route_color(RouteStability::Stable), Color32::from_rgb(110, 210, 130));
        assert_eq!(t.route_color(RouteStability::Unstable), Color32::from_rgb(240, 200, 90));
        assert_eq!(t.route_color(RouteStability::Hazardous), Color32::from_rgb(235, 90, 90));
        assert_eq!(t.route_color(RouteStability::Perilous), Color32::from_rgb(165, 100, 215));
        // `RouteStability` is `#[non_exhaustive]`, so the `_ => route_unstable`
        // arm is a future-proofing fallback: it cannot be exercised here because
        // no fifth variant is constructible, but document the mapping it uses.
        assert_eq!(t.route_color(RouteStability::Unstable), t.route_unstable);
    }

    #[test]
    fn region_color_maps_each_overlay_distinctly() {
        use crate::visual_tokens::MapRegionOverlay;
        let t = RenderMapTheme::default();
        assert_eq!(t.region_color(MapRegionOverlay::WarpStorm), Color32::from_rgb(170, 60, 180));
        assert_eq!(t.region_color(MapRegionOverlay::Turbulence), Color32::from_rgb(140, 100, 200));
        assert_eq!(t.region_color(MapRegionOverlay::CalmCorridor), Color32::from_rgb(90, 200, 180));
        assert_eq!(t.region_color(MapRegionOverlay::Blackout), Color32::from_rgb(60, 60, 80));
        assert_eq!(t.region_color(MapRegionOverlay::Anomaly), Color32::from_rgb(220, 160, 60));
        assert_eq!(
            t.region_color(MapRegionOverlay::NecropolisDrift),
            Color32::from_rgb(100, 130, 140)
        );
        assert_eq!(t.region_color(MapRegionOverlay::BeaconChain), Color32::from_rgb(230, 210, 100));
        assert_eq!(t.region_color(MapRegionOverlay::EmpyricBleed), Color32::from_rgb(190, 70, 160));

        // All eight overlay colours are pairwise distinct.
        let all = [
            t.region_color(MapRegionOverlay::WarpStorm),
            t.region_color(MapRegionOverlay::Turbulence),
            t.region_color(MapRegionOverlay::CalmCorridor),
            t.region_color(MapRegionOverlay::Blackout),
            t.region_color(MapRegionOverlay::Anomaly),
            t.region_color(MapRegionOverlay::NecropolisDrift),
            t.region_color(MapRegionOverlay::BeaconChain),
            t.region_color(MapRegionOverlay::EmpyricBleed),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "overlay colours {i} and {j} collide");
            }
        }
    }

    #[test]
    fn scaled_size_px_applies_min_floor() {
        // px = max(hex_size * mul, min).
        assert_eq!(ScaledSize::new(0.5, 4.0).px(20.0), 10.0); // 10.0 > 4.0 floor
        assert_eq!(ScaledSize::new(0.5, 4.0).px(2.0), 4.0); // 1.0 < 4.0 -> floor
        // Default route thickness: mul 0.08, min 2.0.
        let rt = RenderMapTheme::default().route_thickness;
        assert_eq!(rt.px(100.0), 8.0); // max(8.0, 2.0)
        assert_eq!(rt.px(10.0), 2.0); // max(0.8, 2.0) -> floor
    }
}
