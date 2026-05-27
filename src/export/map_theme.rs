//! Themeable bitmap-map rendering.
//!
//! Themes are presentation-only: they never feed back into generation, world
//! selection, routes, or any serialized sector facts.

use image::Rgba;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapThemeError {
    #[error("unknown map theme '{0}' (expected one of: {})", BUILTIN_THEME_NAMES.join(", "))]
    UnknownTheme(String),
    #[error("{field} has invalid color '{value}' (expected #RRGGBB or #RRGGBBAA)")]
    InvalidColor { field: String, value: String },
    #[error("{name} must be finite")]
    InvalidFactor { name: String },
    #[error("invalid map theme file: {0}")]
    InvalidFile(String),
}

pub const BUILTIN_THEME_NAMES: &[&str] = &[
    "gm_dark",
    "print_mono",
    "imperial_archive",
    "navis_tactical",
    "inquisition_redacted",
    "subsector_political",
];

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MapThemeConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub panel_background: Option<String>,
    #[serde(default)]
    pub hex_empty: Option<String>,
    #[serde(default)]
    pub hex_outline: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_dim: Option<String>,
    #[serde(default)]
    pub subsector_border: Option<String>,
    #[serde(default)]
    pub subsector_label: Option<String>,
    #[serde(default)]
    pub subsector_label_background: Option<String>,
    #[serde(default)]
    pub capital_marker: Option<String>,
    #[serde(default)]
    pub capital_outline: Option<String>,
    #[serde(default)]
    pub route_stable: Option<String>,
    #[serde(default)]
    pub route_unstable: Option<String>,
    #[serde(default)]
    pub route_hazardous: Option<String>,
    #[serde(default)]
    pub route_perilous: Option<String>,
    #[serde(default)]
    pub route_type: Option<String>,
    #[serde(default)]
    pub route_control_neutral: Option<String>,
    #[serde(default)]
    pub orbit_ring: Option<String>,
    #[serde(default)]
    pub show_subsector_borders: Option<bool>,
    #[serde(default)]
    pub route_line_mode: Option<RouteLineMode>,
    #[serde(default)]
    pub label_density: Option<LabelDensity>,
    #[serde(default)]
    pub legend: Option<LegendStyle>,
    #[serde(default)]
    pub symbol_set: Option<SymbolSet>,
    #[serde(default)]
    pub faction_tint_strength: Option<f32>,
    #[serde(default)]
    pub heatmap_tint_min: Option<f32>,
    #[serde(default)]
    pub heatmap_tint_range: Option<f32>,
    #[serde(default)]
    pub route_thickness: Option<f32>,
    #[serde(default)]
    pub region_tint_strength: Option<f32>,
}

impl MapThemeConfig {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::default()
        }
    }

    pub fn overlay(mut self, other: Self) -> Self {
        macro_rules! overlay_field {
            ($field:ident) => {
                if other.$field.is_some() {
                    self.$field = other.$field;
                }
            };
        }
        overlay_field!(name);
        overlay_field!(background);
        overlay_field!(panel_background);
        overlay_field!(hex_empty);
        overlay_field!(hex_outline);
        overlay_field!(text);
        overlay_field!(text_dim);
        overlay_field!(subsector_border);
        overlay_field!(subsector_label);
        overlay_field!(subsector_label_background);
        overlay_field!(capital_marker);
        overlay_field!(capital_outline);
        overlay_field!(route_stable);
        overlay_field!(route_unstable);
        overlay_field!(route_hazardous);
        overlay_field!(route_perilous);
        overlay_field!(route_type);
        overlay_field!(route_control_neutral);
        overlay_field!(orbit_ring);
        overlay_field!(show_subsector_borders);
        overlay_field!(route_line_mode);
        overlay_field!(label_density);
        overlay_field!(legend);
        overlay_field!(symbol_set);
        overlay_field!(faction_tint_strength);
        overlay_field!(heatmap_tint_min);
        overlay_field!(heatmap_tint_range);
        overlay_field!(route_thickness);
        overlay_field!(region_tint_strength);
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MapThemeFile {
    pub map_theme: MapThemeConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteLineMode {
    Standard,
    HazardWeighted,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LabelDensity {
    All,
    ImportantOnly,
    None,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LegendStyle {
    Full,
    Compact,
    Hidden,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolSet {
    Standard,
    Tactical,
    Redacted,
}

#[derive(Debug, Clone)]
pub struct MapTheme {
    pub name: String,
    pub bg: Rgba<u8>,
    pub panel_bg: Rgba<u8>,
    pub hex_empty: Rgba<u8>,
    pub hex_outline: Rgba<u8>,
    pub text: Rgba<u8>,
    pub text_dim: Rgba<u8>,
    pub subsector_border: Rgba<u8>,
    pub subsector_label: Rgba<u8>,
    pub subsector_label_bg: Rgba<u8>,
    pub capital_marker: Rgba<u8>,
    pub capital_outline: Rgba<u8>,
    pub route_stable: Rgba<u8>,
    pub route_unstable: Rgba<u8>,
    pub route_hazardous: Rgba<u8>,
    pub route_perilous: Rgba<u8>,
    pub route_type: Rgba<u8>,
    pub route_control_neutral: Rgba<u8>,
    pub orbit_ring: Rgba<u8>,
    pub show_subsector_borders: bool,
    pub route_line_mode: RouteLineMode,
    pub label_density: LabelDensity,
    pub legend: LegendStyle,
    pub symbol_set: SymbolSet,
    pub faction_tint_strength: f32,
    pub heatmap_tint_min: f32,
    pub heatmap_tint_range: f32,
    pub route_thickness: f32,
    pub region_tint_strength: f32,
}

impl Default for MapTheme {
    fn default() -> Self {
        Self::gm_dark()
    }
}

impl MapTheme {
    pub fn gm_dark() -> Self {
        Self {
            name: "gm_dark".into(),
            bg: rgb(14, 12, 20),
            panel_bg: rgb(22, 18, 30),
            hex_empty: rgb(28, 26, 38),
            hex_outline: rgb(60, 55, 78),
            text: rgb(232, 228, 240),
            text_dim: rgb(150, 145, 165),
            subsector_border: rgb(160, 160, 160),
            subsector_label: rgb(230, 195, 120),
            subsector_label_bg: rgb(20, 16, 28),
            capital_marker: rgb(255, 220, 100),
            capital_outline: rgb(60, 40, 10),
            route_stable: rgb(110, 210, 130),
            route_unstable: rgb(240, 200, 90),
            route_hazardous: rgb(235, 90, 90),
            route_perilous: rgb(165, 100, 215),
            route_type: rgb(232, 228, 240),
            route_control_neutral: rgb(170, 170, 180),
            orbit_ring: rgb(55, 50, 72),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::Standard,
            label_density: LabelDensity::All,
            legend: LegendStyle::Full,
            symbol_set: SymbolSet::Standard,
            faction_tint_strength: 0.35,
            heatmap_tint_min: 0.18,
            heatmap_tint_range: 0.45,
            route_thickness: 1.0,
            region_tint_strength: 0.35,
        }
    }

    fn print_mono() -> Self {
        Self {
            name: "print_mono".into(),
            bg: rgb(255, 255, 255),
            panel_bg: rgb(246, 246, 246),
            hex_empty: rgb(246, 246, 246),
            hex_outline: rgb(40, 40, 40),
            text: rgb(15, 15, 15),
            text_dim: rgb(70, 70, 70),
            subsector_border: rgb(20, 20, 20),
            subsector_label: rgb(10, 10, 10),
            subsector_label_bg: rgb(255, 255, 255),
            capital_marker: rgb(20, 20, 20),
            capital_outline: rgb(255, 255, 255),
            route_stable: rgb(20, 20, 20),
            route_unstable: rgb(85, 85, 85),
            route_hazardous: rgb(130, 130, 130),
            route_perilous: rgb(170, 170, 170),
            route_type: rgb(20, 20, 20),
            route_control_neutral: rgb(40, 40, 40),
            orbit_ring: rgb(180, 180, 180),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::HazardWeighted,
            label_density: LabelDensity::All,
            legend: LegendStyle::Full,
            symbol_set: SymbolSet::Standard,
            faction_tint_strength: 0.16,
            heatmap_tint_min: 0.12,
            heatmap_tint_range: 0.35,
            route_thickness: 0.9,
            region_tint_strength: 0.16,
        }
    }

    fn imperial_archive() -> Self {
        Self {
            name: "imperial_archive".into(),
            bg: rgb(225, 207, 169),
            panel_bg: rgb(211, 190, 146),
            hex_empty: rgb(232, 214, 176),
            hex_outline: rgb(120, 88, 50),
            text: rgb(43, 31, 20),
            text_dim: rgb(91, 69, 45),
            subsector_border: rgb(95, 58, 30),
            subsector_label: rgb(74, 37, 21),
            subsector_label_bg: rgb(221, 199, 154),
            capital_marker: rgb(132, 27, 22),
            capital_outline: rgb(52, 22, 15),
            route_stable: rgb(67, 102, 72),
            route_unstable: rgb(143, 95, 35),
            route_hazardous: rgb(143, 42, 34),
            route_perilous: rgb(92, 56, 112),
            route_type: rgb(43, 31, 20),
            route_control_neutral: rgb(70, 52, 38),
            orbit_ring: rgb(135, 112, 80),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::Standard,
            label_density: LabelDensity::All,
            legend: LegendStyle::Full,
            symbol_set: SymbolSet::Standard,
            faction_tint_strength: 0.24,
            heatmap_tint_min: 0.16,
            heatmap_tint_range: 0.38,
            route_thickness: 1.0,
            region_tint_strength: 0.20,
        }
    }

    fn navis_tactical() -> Self {
        Self {
            name: "navis_tactical".into(),
            bg: rgb(5, 7, 10),
            panel_bg: rgb(8, 20, 30),
            hex_empty: rgb(11, 25, 35),
            hex_outline: rgb(44, 100, 130),
            text: rgb(208, 236, 255),
            text_dim: rgb(118, 160, 185),
            subsector_border: rgb(97, 210, 245),
            subsector_label: rgb(146, 228, 255),
            subsector_label_bg: rgb(6, 16, 24),
            capital_marker: rgb(255, 235, 136),
            capital_outline: rgb(42, 55, 20),
            route_stable: rgb(98, 230, 230),
            route_unstable: rgb(240, 210, 90),
            route_hazardous: rgb(244, 106, 94),
            route_perilous: rgb(184, 120, 255),
            route_type: rgb(208, 236, 255),
            route_control_neutral: rgb(150, 210, 230),
            orbit_ring: rgb(47, 88, 110),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::HazardWeighted,
            label_density: LabelDensity::ImportantOnly,
            legend: LegendStyle::Compact,
            symbol_set: SymbolSet::Tactical,
            faction_tint_strength: 0.26,
            heatmap_tint_min: 0.18,
            heatmap_tint_range: 0.48,
            route_thickness: 1.35,
            region_tint_strength: 0.28,
        }
    }

    fn inquisition_redacted() -> Self {
        Self {
            name: "inquisition_redacted".into(),
            bg: rgb(7, 7, 8),
            panel_bg: rgb(25, 9, 11),
            hex_empty: rgb(17, 17, 18),
            hex_outline: rgb(86, 24, 28),
            text: rgb(244, 224, 224),
            text_dim: rgb(185, 74, 74),
            subsector_border: rgb(155, 24, 30),
            subsector_label: rgb(245, 132, 132),
            subsector_label_bg: rgb(18, 6, 8),
            capital_marker: rgb(220, 38, 38),
            capital_outline: rgb(40, 5, 5),
            route_stable: rgb(175, 175, 175),
            route_unstable: rgb(210, 110, 78),
            route_hazardous: rgb(230, 45, 45),
            route_perilous: rgb(160, 45, 90),
            route_type: rgb(244, 224, 224),
            route_control_neutral: rgb(210, 40, 40),
            orbit_ring: rgb(80, 24, 28),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::HazardWeighted,
            label_density: LabelDensity::ImportantOnly,
            legend: LegendStyle::Compact,
            symbol_set: SymbolSet::Redacted,
            faction_tint_strength: 0.22,
            heatmap_tint_min: 0.20,
            heatmap_tint_range: 0.50,
            route_thickness: 1.15,
            region_tint_strength: 0.18,
        }
    }

    fn subsector_political() -> Self {
        Self {
            name: "subsector_political".into(),
            bg: rgb(13, 15, 19),
            panel_bg: rgb(24, 26, 33),
            hex_empty: rgb(31, 34, 42),
            hex_outline: rgb(70, 77, 92),
            text: rgb(236, 234, 225),
            text_dim: rgb(160, 164, 172),
            subsector_border: rgb(246, 211, 110),
            subsector_label: rgb(255, 230, 150),
            subsector_label_bg: rgb(18, 20, 24),
            capital_marker: rgb(255, 231, 110),
            capital_outline: rgb(50, 39, 12),
            route_stable: rgb(125, 205, 135),
            route_unstable: rgb(236, 196, 83),
            route_hazardous: rgb(232, 91, 82),
            route_perilous: rgb(175, 112, 214),
            route_type: rgb(236, 234, 225),
            route_control_neutral: rgb(190, 190, 176),
            orbit_ring: rgb(64, 66, 78),
            show_subsector_borders: true,
            route_line_mode: RouteLineMode::Standard,
            label_density: LabelDensity::All,
            legend: LegendStyle::Full,
            symbol_set: SymbolSet::Tactical,
            faction_tint_strength: 0.50,
            heatmap_tint_min: 0.18,
            heatmap_tint_range: 0.42,
            route_thickness: 1.05,
            region_tint_strength: 0.24,
        }
    }

    fn builtin(name: &str) -> Option<Self> {
        match normalize_name(name).as_str() {
            "gm_dark" => Some(Self::gm_dark()),
            "print_mono" => Some(Self::print_mono()),
            "imperial_archive" => Some(Self::imperial_archive()),
            "navis_tactical" => Some(Self::navis_tactical()),
            "inquisition_redacted" => Some(Self::inquisition_redacted()),
            "subsector_political" => Some(Self::subsector_political()),
            _ => None,
        }
    }
}

pub fn resolve_map_theme(cfg: &MapThemeConfig) -> Result<MapTheme, MapThemeError> {
    let name = cfg.name.as_deref().unwrap_or("gm_dark");
    let mut theme =
        MapTheme::builtin(name).ok_or_else(|| MapThemeError::UnknownTheme(name.to_string()))?;

    macro_rules! color_override {
        ($cfg_field:ident, $theme_field:ident) => {
            if let Some(value) = &cfg.$cfg_field {
                theme.$theme_field = parse_color(value, stringify!($cfg_field))?;
            }
        };
    }
    color_override!(background, bg);
    color_override!(panel_background, panel_bg);
    color_override!(hex_empty, hex_empty);
    color_override!(hex_outline, hex_outline);
    color_override!(text, text);
    color_override!(text_dim, text_dim);
    color_override!(subsector_border, subsector_border);
    color_override!(subsector_label, subsector_label);
    color_override!(subsector_label_background, subsector_label_bg);
    color_override!(capital_marker, capital_marker);
    color_override!(capital_outline, capital_outline);
    color_override!(route_stable, route_stable);
    color_override!(route_unstable, route_unstable);
    color_override!(route_hazardous, route_hazardous);
    color_override!(route_perilous, route_perilous);
    color_override!(route_type, route_type);
    color_override!(route_control_neutral, route_control_neutral);
    color_override!(orbit_ring, orbit_ring);

    if let Some(v) = cfg.show_subsector_borders {
        theme.show_subsector_borders = v;
    }
    if let Some(v) = cfg.route_line_mode {
        theme.route_line_mode = v;
    }
    if let Some(v) = cfg.label_density {
        theme.label_density = v;
    }
    if let Some(v) = cfg.legend {
        theme.legend = v;
    }
    if let Some(v) = cfg.symbol_set {
        theme.symbol_set = v;
    }
    if let Some(v) = cfg.faction_tint_strength {
        theme.faction_tint_strength = factor("faction_tint_strength", v, 0.0, 1.0)?;
    }
    if let Some(v) = cfg.heatmap_tint_min {
        theme.heatmap_tint_min = factor("heatmap_tint_min", v, 0.0, 1.0)?;
    }
    if let Some(v) = cfg.heatmap_tint_range {
        theme.heatmap_tint_range = factor("heatmap_tint_range", v, 0.0, 1.0)?;
    }
    if let Some(v) = cfg.route_thickness {
        theme.route_thickness = factor("route_thickness", v, 0.4, 3.0)?;
    }
    if let Some(v) = cfg.region_tint_strength {
        theme.region_tint_strength = factor("region_tint_strength", v, 0.0, 1.0)?;
    }
    Ok(theme)
}

pub fn parse_map_theme_file(text: &str) -> Result<MapThemeConfig, MapThemeError> {
    match toml::from_str::<MapThemeFile>(text) {
        Ok(file) => Ok(file.map_theme),
        Err(with_table_err) => toml::from_str::<MapThemeConfig>(text).map_err(|bare_err| {
            MapThemeError::InvalidFile(format!(
                "{with_table_err}; also failed as bare theme: {bare_err}"
            ))
        }),
    }
}

pub fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

fn parse_color(s: &str, field: &str) -> Result<Rgba<u8>, MapThemeError> {
    let raw = s.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    let parse_pair = |idx: usize| -> Result<u8, MapThemeError> {
        u8::from_str_radix(&hex[idx..idx + 2], 16).map_err(|_| MapThemeError::InvalidColor {
            field: field.to_string(),
            value: s.to_string(),
        })
    };
    match hex.len() {
        6 => Ok(Rgba([parse_pair(0)?, parse_pair(2)?, parse_pair(4)?, 255])),
        8 => Ok(Rgba([
            parse_pair(0)?,
            parse_pair(2)?,
            parse_pair(4)?,
            parse_pair(6)?,
        ])),
        _ => Err(MapThemeError::InvalidColor {
            field: field.to_string(),
            value: s.to_string(),
        }),
    }
}

fn factor(name: &str, value: f32, min: f32, max: f32) -> Result<f32, MapThemeError> {
    if !value.is_finite() {
        return Err(MapThemeError::InvalidFactor {
            name: name.to_string(),
        });
    }
    Ok(value.clamp(min, max))
}

fn rgb(r: u8, g: u8, b: u8) -> Rgba<u8> {
    Rgba([r, g, b, 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_named_theme() {
        let cfg = MapThemeConfig::named("navis-tactical");
        let theme = resolve_map_theme(&cfg).unwrap();
        assert_eq!(theme.name, "navis_tactical");
        assert_eq!(theme.legend, LegendStyle::Compact);
        assert_eq!(theme.route_line_mode, RouteLineMode::HazardWeighted);
    }

    #[test]
    fn applies_color_overrides() {
        let cfg = MapThemeConfig {
            name: Some("print_mono".into()),
            background: Some("#010203".into()),
            ..Default::default()
        };
        let theme = resolve_map_theme(&cfg).unwrap();
        assert_eq!(theme.bg, Rgba([1, 2, 3, 255]));
    }

    #[test]
    fn parses_table_wrapped_theme_file() {
        let text = r##"
            [map_theme]
            name = "imperial_archive"
            route_thickness = 1.4
        "##;
        let cfg = parse_map_theme_file(text).unwrap();
        assert_eq!(cfg.name.as_deref(), Some("imperial_archive"));
        assert_eq!(cfg.route_thickness, Some(1.4));
    }
}
