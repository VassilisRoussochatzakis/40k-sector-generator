//! AppConfig: the parsed sectorforge.toml.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub project: ProjectConfig,
    pub inputs: InputConfig,
    pub generation: GenerationConfig,
    pub outputs: OutputConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputConfig {
    pub world_workbook: String,
    #[serde(default)]
    pub system_names: Option<String>,
    #[serde(default)]
    pub world_names: Option<String>,
    #[serde(default)]
    pub factions: Option<String>,
    #[serde(default)]
    pub route_rules: Option<String>,
    #[serde(default)]
    pub generation_profiles: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerationConfig {
    pub seed: String,
    pub sector_width: u32,
    pub sector_height: u32,
    #[serde(default)]
    pub subsector_width: Option<u32>,
    #[serde(default)]
    pub subsector_height: Option<u32>,
    pub system_count: usize,
    pub min_worlds_per_system: usize,
    pub max_worlds_per_system: usize,
    #[serde(default = "default_true")]
    pub allow_empty_hexes: bool,
    #[serde(default = "default_feature_count")]
    pub world_feature_count: usize,
    #[serde(default = "default_true")]
    pub strict_world_rows: bool,
    #[serde(default)]
    pub placement: PlacementConfig,
    #[serde(default)]
    pub world_selection: WorldSelectionConfig,
    #[serde(default)]
    pub routes: RouteGenerationConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlacementConfig {
    #[serde(default = "default_placement_mode")]
    pub mode: PlacementMode,
    #[serde(default)]
    pub cluster_bias: f64,
    #[serde(default = "default_min_dist")]
    pub minimum_system_distance: u32,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            mode: default_placement_mode(),
            cluster_bias: 0.0,
            minimum_system_distance: default_min_dist(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    UniformGrid,
    WeightedGrid,
    Clustered,
}

fn default_placement_mode() -> PlacementMode {
    PlacementMode::UniformGrid
}

fn default_min_dist() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldSelectionConfig {
    #[serde(default = "default_selection_mode")]
    pub mode: WorldSelectionMode,
    #[serde(default = "default_true")]
    pub require_complete_rows: bool,
    #[serde(default)]
    pub allow_partial_rows: bool,
    #[serde(default = "default_bias")]
    pub same_star_colour_bias: f64,
    #[serde(default)]
    pub strict_same_star_colour: bool,
    #[serde(default)]
    pub avoid_duplicate_world_type_in_system: bool,
}

impl Default for WorldSelectionConfig {
    fn default() -> Self {
        Self {
            mode: default_selection_mode(),
            require_complete_rows: true,
            allow_partial_rows: false,
            same_star_colour_bias: default_bias(),
            strict_same_star_colour: false,
            avoid_duplicate_world_type_in_system: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldSelectionMode {
    WeightedRows,
}

fn default_selection_mode() -> WorldSelectionMode {
    WorldSelectionMode::WeightedRows
}

fn default_bias() -> f64 {
    1.25
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteGenerationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_route_distance")]
    pub max_route_distance: u32,
    #[serde(default = "default_route_density")]
    pub route_density: f64,
    #[serde(default = "default_true")]
    pub ensure_connected_graph: bool,
}

impl Default for RouteGenerationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_route_distance: default_max_route_distance(),
            route_density: default_route_density(),
            ensure_connected_graph: true,
        }
    }
}

fn default_max_route_distance() -> u32 {
    4
}

fn default_route_density() -> f64 {
    0.30
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    pub directory: String,
    pub formats: Vec<OutputFormat>,
    #[serde(default = "default_true")]
    pub pretty_json: bool,
    #[serde(default = "default_true")]
    pub write_per_system_files: bool,
    #[serde(default = "default_true")]
    pub write_manifest: bool,
    #[serde(default)]
    pub write_diagnostics: bool,
    #[serde(default)]
    pub bitmap: BitmapConfig,
}

/// Image render options for the bitmap exporters.
///
/// `sector_scale` and `system_scale` are integer multipliers over the base
/// design size. The base sector map is ~720x460 with an 8x10 grid; at
/// `sector_scale = 5` it lands around 3600x2300, i.e. ~4K. Valid range 1..=8.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitmapConfig {
    #[serde(default = "default_scale")]
    pub sector_scale: u32,
    #[serde(default = "default_scale")]
    pub system_scale: u32,
    #[serde(default = "default_true")]
    pub render_systems: bool,
}

impl Default for BitmapConfig {
    fn default() -> Self {
        Self {
            sector_scale: default_scale(),
            system_scale: default_scale(),
            render_systems: true,
        }
    }
}

fn default_scale() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
    Markdown,
    Csv,
    /// PNG hex map with legend.
    Bitmap,
    /// Windows .bmp hex map with legend.
    Bmp,
}

fn default_true() -> bool {
    true
}

fn default_feature_count() -> usize {
    3
}
