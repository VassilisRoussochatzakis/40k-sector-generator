//! `AppConfig`: the parsed sectorforge.toml.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub project: ProjectConfig,
    pub inputs: InputConfig,
    pub generation: GenerationConfig,
    pub outputs: OutputConfig,
    /// Optional top-level alias for `[map_theme]` from §13 NEW2.md. It is
    /// folded into `outputs.bitmap.theme` by the project loader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_theme: Option<crate::map_theme::MapThemeConfig>,
    /// Optional analytics dashboard thresholds (§8 NEW.md). Defaults apply
    /// when the `[analyze]` table is omitted.
    #[serde(default)]
    pub analyze: crate::analytics::AnalyzeConfig,
    /// Optional default search configuration (§2 NEW.md). Constraints still
    /// come from a separate `wishes.toml`; this block only controls
    /// search budget / reporting defaults when the wishes file omits them.
    #[serde(default)]
    pub search: crate::search::SearchConfig,
    /// Optional default diff verbosity (§10 NEW.md). The CLI overrides
    /// these per-invocation.
    #[serde(default)]
    pub diff: crate::diff::DiffConfig,
    /// §1 NEW2.md: generated sector chronicle / historical timeline.
    #[serde(default)]
    pub history: crate::history::HistoryConfig,
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
    /// Directory containing `worlds.toml`.
    pub world_data_dir: String,
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
    /// §5 NEW2.md: optional `relations.toml` with kind/disposition rules,
    /// legacy stance pins, and public/secret attitude overrides.
    #[serde(default)]
    pub relations: Option<String>,
    /// §5 NEW.md: optional `regions.toml` with the condition catalogue and
    /// region-stage parameters.
    #[serde(default)]
    pub regions: Option<String>,
    /// §12 NEW.md: optional `economy.toml` with production/consumption tables.
    #[serde(default)]
    pub economy: Option<String>,
    /// §1 NEW2.md: optional `history.toml` with eras and event rules.
    #[serde(default)]
    pub history: Option<String>,
    /// §3 NEW.md: optional `personae.toml` with pools and manual entries.
    #[serde(default)]
    pub personae: Option<String>,
    /// §7 NEW2.md: optional `sites.toml` with manual entries.
    #[serde(default)]
    pub sites: Option<String>,
    /// §HK4 BUILDER_REQS: optional `hooks.toml` with `HooksConfig` knobs and
    /// `[[manual]]` entries that survive "Regenerate hooks".
    #[serde(default)]
    pub hooks: Option<String>,
    /// §M1..§M5 BUILDER_REQS: optional `missions.toml` with `MissionsConfig`
    /// knobs and `[[manual]]` mission seeds that survive "Auto-derive missions".
    #[serde(default)]
    pub missions: Option<String>,
    /// §PR1..§PR4 BUILDER_REQS: optional `prose.toml` with `ProseConfig` knobs
    /// and `overrides` (per-sector overview + per-system text replacements)
    /// that survive "Regenerate prose".
    #[serde(default)]
    pub prose: Option<String>,
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
    #[serde(default)]
    pub relations: RelationsGenerationConfig,
    /// §15 NEW2.md: when using constraint-based generation, the base seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_base_seed: Option<String>,
    /// §15 NEW2.md: when using constraint-based generation, the selected candidate index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_candidate_index: Option<u32>,
    /// §15 NEW2.md: hash of the constraints file used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_constraints_digest: Option<String>,
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
#[non_exhaustive]
pub enum PlacementMode {
    UniformGrid,
    WeightedGrid,
    Clustered,
}

enum_slug!(PlacementMode {
    UniformGrid => "uniform_grid",
    WeightedGrid => "weighted_grid",
    Clustered => "clustered",
});

impl core::fmt::Display for PlacementMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
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
#[non_exhaustive]
pub enum WorldSelectionMode {
    WeightedRows,
}

enum_slug!(WorldSelectionMode {
    WeightedRows => "weighted_rows",
});

impl core::fmt::Display for WorldSelectionMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

fn default_selection_mode() -> WorldSelectionMode {
    WorldSelectionMode::WeightedRows
}

fn default_bias() -> f64 {
    1.25
}

/// Target stability mix for **public** warp routes (`StableWarpLane` /
/// `ChartedPassage` / `SecretPassage`; hidden lanes keep their own per-type
/// stability). When present on [`RouteGenerationConfig`], a final generation
/// stage re-buckets the public route graph so the fraction of routes in each
/// [`crate::sector_model::RouteStability`] tier approaches these weights —
/// *regardless* of how many warp-storm regions the sector rolled.
///
/// Weights are relative (they need not sum to `1.0`; they are normalised) and
/// negatives are clamped to `0`. The re-bucketing is a monotonic function of
/// the pre-rebalance danger ordering, so a shorter route is never made less
/// safe than a longer one carrying the same hazards (the "short is safer than
/// long" invariant is preserved by construction).
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct StabilityTargets {
    #[serde(default)]
    pub stable: f64,
    #[serde(default)]
    pub unstable: f64,
    #[serde(default)]
    pub hazardous: f64,
    #[serde(default)]
    pub perilous: f64,
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
    /// Optional target stability mix for public routes. `None` (default)
    /// preserves the legacy early perilous-cap behaviour and byte-identical
    /// golden output; `Some` swaps that cap for a quantile re-bucketing that
    /// guarantees the configured fraction of Stable / Unstable / Hazardous /
    /// Perilous lanes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability_targets: Option<StabilityTargets>,
}

impl Default for RouteGenerationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_route_distance: default_max_route_distance(),
            route_density: default_route_density(),
            ensure_connected_graph: true,
            stability_targets: None,
        }
    }
}

fn default_max_route_distance() -> u32 {
    4
}

fn default_route_density() -> f64 {
    0.30
}

/// `[generation.relations]` — controls how the inter-faction diplomacy
/// matrix is sized at generation time. The full canonical faction catalogue
/// on the bundled data set is ~1000 entries, which yields C(n,2) ≈ 500k
/// pairs and tens of MB of JSON. Filtering by minimum world presence keeps
/// the matrix scoped to factions that meaningfully appear in the sector.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelationsGenerationConfig {
    /// Minimum number of worlds a faction must occupy to appear in the
    /// `relations` matrix. `1` (default) emits a pair for every faction with
    /// at least one world presence anywhere in the sector. Set to `2` or
    /// higher to drop incidental single-world cameos and shrink the matrix
    /// quadratically.
    #[serde(default = "default_min_world_presence")]
    pub min_world_presence: usize,
}

impl Default for RelationsGenerationConfig {
    fn default() -> Self {
        Self {
            min_world_presence: default_min_world_presence(),
        }
    }
}

fn default_min_world_presence() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    pub directory: String,
    pub formats: Vec<OutputFormat>,
    #[serde(default = "default_true")]
    pub pretty_json: bool,
    #[serde(default)]
    pub write_per_system_files: bool,
    #[serde(default = "default_true")]
    pub write_manifest: bool,
    #[serde(default)]
    pub write_diagnostics: bool,
    #[serde(default)]
    pub bitmap: BitmapConfig,
    /// §11 NEW.md: optional self-contained interactive HTML export. Default
    /// is disabled even when `OutputFormat::Html` is listed (toggle is the
    /// `formats` list); the sub-table only carries theme + redaction options.
    #[serde(default)]
    pub html: HtmlConfig,
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
    /// §8: tint each system's hex by the dominant faction's `FactionStyle.fill`.
    #[serde(default = "default_true")]
    pub faction_fill: bool,
    /// §10: heatmap mode applied to the sector PNG. `Off` keeps the plain map.
    /// When non-`Off`, `Control` mode still uses `faction_fill` colours; other
    /// modes override the tint.
    #[serde(default)]
    pub heatmap: crate::heatmap::HeatmapMode,
    /// §13 NEW2.md: built-in or inline/custom map theme. Presentation only;
    /// generation output is unchanged.
    #[serde(default)]
    pub theme: crate::map_theme::MapThemeConfig,
    /// Optional path to a TOML map theme file, relative to the project root.
    /// Its digest is recorded in `manifest.input_digests`.
    #[serde(default)]
    pub theme_file: Option<String>,
}

impl Default for BitmapConfig {
    fn default() -> Self {
        Self {
            sector_scale: default_scale(),
            system_scale: default_scale(),
            render_systems: true,
            faction_fill: true,
            heatmap: crate::heatmap::HeatmapMode::Off,
            theme: crate::map_theme::MapThemeConfig::default(),
            theme_file: None,
        }
    }
}

fn default_scale() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OutputFormat {
    Json,
    Markdown,
    /// PNG hex map with legend.
    Bitmap,
    /// Vector hex map (sector.svg).
    Svg,
    /// §11 NEW.md: self-contained interactive HTML map.
    Html,
}

impl OutputFormat {
    /// Parse a CLI-style format token (`json`, `markdown`, `md`, `png`,
    /// `bitmap`, `svg`, `html`). Case-insensitive.
    pub fn parse_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "markdown" | "md" => Some(Self::Markdown),
            "png" | "bitmap" => Some(Self::Bitmap),
            "svg" => Some(Self::Svg),
            "html" => Some(Self::Html),
            _ => None,
        }
    }
}

enum_slug!(OutputFormat {
    Json => "json",
    Markdown => "markdown",
    Bitmap => "bitmap",
    Svg => "svg",
    Html => "html",
});

impl core::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

/// §11 NEW.md: interactive HTML exporter knobs. Theme picks the palette;
/// `player_edition` runs the existing intel redaction helper over the sector
/// before inlining so Hidden-tier presences and GM-only fields are stripped.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HtmlConfig {
    #[serde(default = "default_html_theme")]
    pub theme: HtmlTheme,
    /// When set, restrict the inlined sector to what the named observer
    /// faction id can see, using the same `min_intel_confidence` cutoff as
    /// the redaction helper. Personae, hooks, and intel records are stripped
    /// alongside hidden presences. `None` = full GM edition.
    #[serde(default)]
    pub player_observer: Option<String>,
    /// Intel-confidence cutoff (0..=100) for `player_observer`. Hidden
    /// faction presences with `vis * observer.vis / 100 < min` are dropped.
    #[serde(default = "default_player_min_conf")]
    pub player_min_confidence: u8,
    /// Warn (stderr) above this byte size; does not block the write.
    #[serde(default = "default_html_size_warn")]
    pub size_warn_bytes: u64,
    /// Use compact (non-pretty) JSON inline. Defaults to true — pretty JSON
    /// would roughly double the file size for no runtime benefit.
    #[serde(default = "default_html_compact")]
    pub compact_json: bool,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            theme: default_html_theme(),
            player_observer: None,
            player_min_confidence: default_player_min_conf(),
            size_warn_bytes: default_html_size_warn(),
            compact_json: default_html_compact(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HtmlTheme {
    /// Dark background, matches the GUI / PNG palette.
    Dark,
    /// Light cream background, sepia tints.
    Parchment,
    /// Cool blue-tinted greyscale, monitor-glow aesthetic.
    Hololithic,
}

enum_slug!(HtmlTheme {
    Dark => "dark",
    Parchment => "parchment",
    Hololithic => "hololithic",
});

impl core::fmt::Display for HtmlTheme {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

fn default_html_theme() -> HtmlTheme {
    HtmlTheme::Dark
}

fn default_player_min_conf() -> u8 {
    30
}

fn default_html_size_warn() -> u64 {
    8 * 1024 * 1024
}

fn default_html_compact() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_feature_count() -> usize {
    3
}
