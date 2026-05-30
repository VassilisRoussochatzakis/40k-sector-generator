//! Size-only → fully-complete sector generation (RANDOM.md).
//!
//! Given nothing but a [`SectorSize`] (and an optional reproducibility seed),
//! synthesise an *entirely new, fully-randomised* project — a complete
//! `sectorforge.toml` in which every section is present and every overlay is
//! explicitly enabled — generate the sector, and run the five post-generation
//! derivations the orchestrator does **not** (`personae`, `sites`, `hooks`,
//! `missions`, `prose`). The result is a [`RandomReport`]: a sector with every
//! feature on, plus the loaded [`ProjectInput`] so the builder/CLI can export
//! and keep editing.
//!
//! # Determinism
//!
//! There is exactly one non-deterministic step — [`mint_seed`], which selects a
//! root seed from process entropy when the caller does not supply one. Every
//! rolled config knob is then derived from that seed through a dedicated
//! `"config"` RNG stage ([`crate::rng::stage_rng`]), and generation keeps using
//! its own stage keys. Re-running with the same seed reproduces the identical
//! config *and* sector, byte-for-byte. No `rand::thread_rng()` is introduced and
//! no RNG is seeded from outside the stage RNG (the determinism invariant in
//! CLAUDE.md): the mint is pre-generation entropy selection, not a generation
//! draw.
//!
//! # Strategy
//!
//! The content + overlay *data* (worlds, factions, names, regions, economy,
//! history, personae, sites, hooks, missions, prose) is reused verbatim from
//! the checked-in [`FULL_PRESET_ID`] preset — the single source of truth for
//! "every feature on". Only the `[generation]` shape and the wiring config are
//! synthesised here. See RANDOM.md §5 (Strategy S) and §6 (the `_full` preset).

use camino::Utf8Path;
use rand::Rng;

use crate::config::{
    AppConfig, BitmapConfig, GenerationConfig, HtmlConfig, InputConfig, OutputConfig, OutputFormat,
    PlacementConfig, PlacementMode, ProjectConfig, RelationsGenerationConfig,
    RouteGenerationConfig, WorldSelectionConfig, WorldSelectionMode,
};
use crate::errors::SectorError;
use crate::hooks::HooksReport;
use crate::input::ProjectInput;
use crate::missions::MissionsReport;
use crate::personae::PersonaeReport;
use crate::prose::ProseReport;
use crate::sector_model::GeneratedSector;
use crate::sites::SitesReport;

/// Id of the checked-in "everything on" preset that supplies the content +
/// overlay data tree. Hidden from the gallery (leading `_`) but scaffoldable.
pub const FULL_PRESET_ID: &str = "_full";

/// The one user input: how big the sector is. The grid dims map to a cell
/// count; `system_count` and every other structural knob are rolled from the
/// seed (RANDOM.md §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorSize {
    /// 6 × 8 = 48 cells.
    Small,
    /// 8 × 10 = 80 cells (≈ `_base`).
    Medium,
    /// 12 × 14 = 168 cells.
    Large,
    /// 16 × 20 = 320 cells.
    Huge,
    /// An explicit `width × height`.
    Custom { width: u32, height: u32 },
}

impl SectorSize {
    /// Grid dimensions `(width, height)` in hexes.
    #[must_use]
    pub fn dims(self) -> (u32, u32) {
        match self {
            SectorSize::Small => (6, 8),
            SectorSize::Medium => (8, 10),
            SectorSize::Large => (12, 14),
            SectorSize::Huge => (16, 20),
            SectorSize::Custom { width, height } => (width, height),
        }
    }

    /// Parse a CLI-style size token (`small`/`medium`/`large`/`huge`).
    /// `Custom` is expressed via explicit `--width`/`--height`, not a token.
    #[must_use]
    pub fn parse_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "huge" => Some(Self::Huge),
            _ => None,
        }
    }

    /// Stable lower-case slug for logging / titles.
    #[must_use]
    pub fn as_slug(self) -> &'static str {
        match self {
            SectorSize::Small => "small",
            SectorSize::Medium => "medium",
            SectorSize::Large => "large",
            SectorSize::Huge => "huge",
            SectorSize::Custom { .. } => "custom",
        }
    }
}

/// Everything a "fully complete" sector needs: the generated sector plus the
/// five post-generation derivations and the loaded project (catalogs + config)
/// so the result is immediately exportable and editable.
#[derive(Debug, Clone)]
pub struct RandomReport {
    pub sector: GeneratedSector,
    pub personae: PersonaeReport,
    pub sites: SitesReport,
    pub hooks: HooksReport,
    pub missions: MissionsReport,
    pub prose: ProseReport,
    /// The loaded project — catalogs + the synthesised config. The builder
    /// re-opens `dest` from disk instead; the CLI uses this to export.
    pub input: ProjectInput,
    /// The root seed actually used (minted if the caller passed `None`), echoed
    /// for reproducibility.
    pub seed: String,
}

/// Region overlay knobs scaled to the grid (kept out of [`AppConfig`] because
/// regions live in their own data file, patched separately).
#[derive(Debug, Clone, Copy)]
struct RegionKnobs {
    count: u32,
    mean_size: u32,
}

/// Mint a fresh root seed from process entropy. This is the **only**
/// non-deterministic step in the whole path (RANDOM.md §5.1). Output is a
/// 16-char hex string. Avoids `rand::thread_rng()` to honour the determinism
/// invariant — entropy comes from wall-clock nanos, the pid, and an ASLR-tinged
/// stack address, folded through blake3.
#[must_use]
pub fn mint_seed() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = u128::from(std::process::id());
    let stack_marker = (&nanos as *const u128) as usize;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"sectorforge:random-seed-mint:");
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&pid.to_le_bytes());
    hasher.update(&stack_marker.to_le_bytes());
    let hex = hasher.finalize().to_hex();
    hex[..16].to_string()
}

/// Build the complete, fully-randomised [`AppConfig`] for `(seed, size)`.
/// Exposed for the determinism tests — same inputs ⇒ identical config.
#[must_use]
pub fn build_random_config(seed: &str, size: SectorSize) -> AppConfig {
    let mut rng = crate::rng::stage_rng(seed, "config", "");
    build_random_config_inner(seed, size, &mut rng).0
}

fn build_random_config_inner(
    seed: &str,
    size: SectorSize,
    rng: &mut impl Rng,
) -> (AppConfig, RegionKnobs) {
    let (width, height) = size.dims();
    let cells = width.saturating_mul(height).max(1);
    let cells_f = f64::from(cells);

    // ── Structural knobs (rolled in a fixed order for determinism) ──────────
    let density: f64 = rng.gen_range(0.25..=0.40);
    let system_count = ((density * cells_f).round() as i64).clamp(4, i64::from(cells)) as usize;

    let min_worlds = rng.gen_range(1..=2usize);
    let max_worlds = rng.gen_range(4..=7usize).max(min_worlds);
    let world_feature_count = rng.gen_range(3..=5usize);

    let subsector_width = width.div_ceil(4).max(1);
    let subsector_height = height.div_ceil(4).max(1);

    let mode = match rng.gen_range(0..3u8) {
        0 => PlacementMode::UniformGrid,
        1 => PlacementMode::WeightedGrid,
        _ => PlacementMode::Clustered,
    };
    let cluster_bias = if mode == PlacementMode::Clustered {
        rng.gen_range(0.3..=0.7)
    } else {
        0.0
    };

    let same_star_colour_bias = rng.gen_range(1.0..=1.4);
    let strict_same_star_colour = rng.gen_bool(0.15);
    let avoid_duplicate_world_type_in_system = rng.gen_bool(0.5);

    let max_route_distance = rng.gen_range(3..=6u32);
    let route_density = rng.gen_range(0.15..=0.40);

    // Bound relations/economy output on the largest grids (RANDOM.md §5.6).
    let min_world_presence = if matches!(size, SectorSize::Huge) {
        2
    } else {
        1
    };

    // Region overlay scaled to the grid so tiny sectors don't overfill: total
    // region area ≈ 60% of the grid, count clamped to ≤ ½ the cells.
    let mean_size = rng.gen_range(4..=8u32);
    let region_count =
        ((cells_f * 0.6 / f64::from(mean_size)).round() as u32).clamp(1, (cells / 2).max(1));

    let theme = crate::map_theme::BUILTIN_THEME_NAMES
        [rng.gen_range(0..crate::map_theme::BUILTIN_THEME_NAMES.len())];

    // ── Assemble the config ─────────────────────────────────────────────────
    let slug = seed_slug(seed);

    let generation = GenerationConfig {
        seed: seed.to_string(),
        sector_width: width,
        sector_height: height,
        subsector_width: Some(subsector_width),
        subsector_height: Some(subsector_height),
        system_count,
        min_worlds_per_system: min_worlds,
        max_worlds_per_system: max_worlds,
        allow_empty_hexes: true,
        world_feature_count,
        strict_world_rows: true,
        placement: PlacementConfig {
            mode,
            cluster_bias,
            minimum_system_distance: 1,
        },
        world_selection: WorldSelectionConfig {
            mode: WorldSelectionMode::WeightedRows,
            require_complete_rows: true,
            allow_partial_rows: false,
            same_star_colour_bias,
            strict_same_star_colour,
            avoid_duplicate_world_type_in_system,
        },
        routes: RouteGenerationConfig {
            enabled: true,
            max_route_distance,
            route_density,
            ensure_connected_graph: true,
        },
        relations: RelationsGenerationConfig { min_world_presence },
        search_base_seed: None,
        search_candidate_index: None,
        search_constraints_digest: None,
    };

    let inputs = InputConfig {
        world_data_dir: "data/worlds".to_string(),
        system_names: Some("data/names/system_names.toml".to_string()),
        world_names: Some("data/names/world_names.toml".to_string()),
        factions: Some("data/factions/factions.toml".to_string()),
        route_rules: Some("data/routes/route_rules.toml".to_string()),
        generation_profiles: None,
        relations: Some("data/factions/relations.toml".to_string()),
        regions: Some("data/routes/regions.toml".to_string()),
        economy: Some("data/worlds/economy.toml".to_string()),
        history: Some("data/history.toml".to_string()),
        personae: Some("data/personae.toml".to_string()),
        sites: Some("data/sites.toml".to_string()),
        hooks: Some("data/hooks.toml".to_string()),
        missions: Some("data/missions.toml".to_string()),
        prose: Some("data/prose.toml".to_string()),
    };

    let outputs = OutputConfig {
        directory: "out".to_string(),
        formats: vec![
            OutputFormat::Json,
            OutputFormat::Markdown,
            OutputFormat::Bitmap,
            OutputFormat::Svg,
            OutputFormat::Html,
        ],
        pretty_json: true,
        write_per_system_files: false,
        write_manifest: true,
        write_diagnostics: false,
        bitmap: BitmapConfig::default(),
        html: HtmlConfig::default(),
    };

    let config = AppConfig {
        project: ProjectConfig {
            id: format!("random-{slug}"),
            title: format!("Random Sector {slug}"),
            description: Some("Procedurally generated random sector.".to_string()),
            version: Some("0.1.0".to_string()),
        },
        inputs,
        generation,
        outputs,
        map_theme: Some(crate::map_theme::MapThemeConfig::named(theme)),
        analyze: crate::analytics::AnalyzeConfig::default(),
        search: crate::search::SearchConfig::default(),
        diff: crate::diff::DiffConfig::default(),
        // history.toml (enabled) wins at load time; this is the in-config copy.
        history: crate::history::HistoryConfig::default(),
    };

    (
        config,
        RegionKnobs {
            count: region_count,
            mean_size,
        },
    )
}

/// Lower-case alphanumeric, ≤ 8 chars, of a seed — used for the project id /
/// title. Deterministic so the synthesised config is reproducible.
fn seed_slug(seed: &str) -> String {
    let s: String = seed
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if s.is_empty() {
        "sector".to_string()
    } else {
        s
    }
}

/// Replace the `count` and `mean_size` knobs inside the `[regions]` table of a
/// `regions.toml`, leaving `enabled`, `apply_to_routes`, and the conditions
/// pool untouched. Mirrors the line-oriented patch style of
/// `presets::rewrite_seed` — no full re-serialisation, so the authored
/// conditions survive verbatim.
fn patch_regions_toml(text: &str, count: u32, mean_size: u32) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    // True only inside the `[regions]` table (not `[[regions.conditions]]`).
    let mut in_regions = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_regions = trimmed.starts_with("[regions]");
        }
        let indent = &line[..line.len() - trimmed.len()];
        if in_regions && trimmed.starts_with("count") && trimmed.contains('=') {
            out.push_str(indent);
            out.push_str(&format!("count = {count}\n"));
            continue;
        }
        if in_regions && trimmed.starts_with("mean_size") && trimmed.contains('=') {
            out.push_str(indent);
            out.push_str(&format!("mean_size = {mean_size}\n"));
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Synthesise, generate, and fully derive a random sector into `dest`.
///
/// `presets_dir` must contain the [`FULL_PRESET_ID`] preset (use
/// [`crate::presets::default_presets_dir`] for the standard location, or call
/// [`generate_random_sector_default`]). `dest` must **not** already exist — it
/// is created fresh, so callers headless of the builder typically pass a path
/// inside a [`tempfile::TempDir`].
///
/// # Errors
///
/// Propagates every error from scaffolding, loading, validation, and
/// generation. Returns [`SectorError::ValidationFailed`] if the synthesised
/// project fails pre-generation validation (should not happen for the rolled
/// ranges, but guards against a malformed preset bundle).
pub fn generate_random_sector(
    size: SectorSize,
    seed: Option<String>,
    presets_dir: &Utf8Path,
    dest: &Utf8Path,
) -> Result<RandomReport, SectorError> {
    let seed = seed.unwrap_or_else(mint_seed);

    // 1. Materialise the bundle (copies _full's data tree + a sectorforge.toml
    //    we immediately overwrite). scaffold requires `dest` not to exist.
    crate::presets::scaffold(presets_dir, FULL_PRESET_ID, dest, None)?;

    // 2. Roll the complete config from the seed and write it over the
    //    scaffolded one — we own every field, so nothing falls to a default.
    let mut cfg_rng = crate::rng::stage_rng(&seed, "config", "");
    let (config, regions) = build_random_config_inner(&seed, size, &mut cfg_rng);
    let toml_text = toml::to_string_pretty(&config).map_err(|e| {
        SectorError::InvalidConfig(format!("serialising random sectorforge.toml: {e}"))
    })?;
    let cfg_path = dest.join("sectorforge.toml");
    std::fs::write(&cfg_path, toml_text).map_err(|e| SectorError::io(cfg_path.as_str(), e))?;

    // 3. Scale the regions overlay to the grid (keeps `enabled = true`).
    let regions_path = dest.join("data/routes/regions.toml");
    let regions_text = std::fs::read_to_string(&regions_path)
        .map_err(|e| SectorError::io(regions_path.as_str(), e))?;
    let patched = patch_regions_toml(&regions_text, regions.count, regions.mean_size);
    std::fs::write(&regions_path, patched)
        .map_err(|e| SectorError::io(regions_path.as_str(), e))?;

    // 4. Load + validate the freshly written project.
    let input = crate::load_project(dest)?;
    let report = crate::validate_project(&input)?;
    if !report.ok {
        return Err(SectorError::ValidationFailed {
            error_count: report.errors.len(),
            warning_count: report.warnings.len(),
        });
    }

    // 5. Generate + invariant-check (invariants are advisory here; a hard
    //    failure would surface as an empty/incoherent sector the tests catch).
    let sector = crate::generate_sector(input.clone())?;
    let _invariants = crate::validate_sector(&sector);

    // 6. Run the five post-generation derivations the orchestrator skips.
    let personae = crate::derive_personae_with(&sector, &input.catalogs.personae);
    let sites = crate::derive_sites_with(&sector, &input.catalogs.sites);
    let hooks = crate::derive_hooks_with(&sector, &input.catalogs.hooks);
    let missions = crate::derive_missions_with(&sector, &input.catalogs.missions);
    let prose = crate::derive_prose_with(&sector, &input.catalogs.prose);

    Ok(RandomReport {
        sector,
        personae,
        sites,
        hooks,
        missions,
        prose,
        input,
        seed,
    })
}

/// Convenience over [`generate_random_sector`] that resolves the presets
/// directory via [`crate::presets::default_presets_dir`].
///
/// # Errors
///
/// Forwards every error from [`generate_random_sector`].
pub fn generate_random_sector_default(
    size: SectorSize,
    seed: Option<String>,
    dest: &Utf8Path,
) -> Result<RandomReport, SectorError> {
    let presets_dir = crate::presets::default_presets_dir();
    generate_random_sector(size, seed, &presets_dir, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dims_match_spec() {
        assert_eq!(SectorSize::Small.dims(), (6, 8));
        assert_eq!(SectorSize::Medium.dims(), (8, 10));
        assert_eq!(SectorSize::Large.dims(), (12, 14));
        assert_eq!(SectorSize::Huge.dims(), (16, 20));
        assert_eq!(
            SectorSize::Custom {
                width: 5,
                height: 9
            }
            .dims(),
            (5, 9)
        );
    }

    #[test]
    fn parse_token_roundtrip() {
        for s in [
            SectorSize::Small,
            SectorSize::Medium,
            SectorSize::Large,
            SectorSize::Huge,
        ] {
            assert_eq!(SectorSize::parse_token(s.as_slug()), Some(s));
        }
        assert_eq!(SectorSize::parse_token("nonsense"), None);
    }

    #[test]
    fn config_is_serialisable_and_complete() {
        let cfg = build_random_config("test-seed", SectorSize::Medium);
        let text = toml::to_string_pretty(&cfg).expect("config serialises to TOML");
        // Every overlay wired + enabled, every format on.
        for key in [
            "[inputs]",
            "regions = \"data/routes/regions.toml\"",
            "economy = \"data/worlds/economy.toml\"",
            "hooks = \"data/hooks.toml\"",
            "missions = \"data/missions.toml\"",
            "prose = \"data/prose.toml\"",
            "[generation]",
            "[outputs]",
        ] {
            assert!(
                text.contains(key),
                "synthesised config missing {key}\n{text}"
            );
        }
        // Round-trips back into an AppConfig.
        let _back: AppConfig = toml::from_str(&text).expect("config round-trips");
    }

    #[test]
    fn config_roll_is_deterministic() {
        let a = build_random_config("repro-1", SectorSize::Large);
        let b = build_random_config("repro-1", SectorSize::Large);
        let ta = toml::to_string_pretty(&a).unwrap();
        let tb = toml::to_string_pretty(&b).unwrap();
        assert_eq!(ta, tb, "same (seed, size) must produce identical config");
    }

    #[test]
    fn system_count_respects_bounds() {
        for size in [
            SectorSize::Small,
            SectorSize::Medium,
            SectorSize::Large,
            SectorSize::Huge,
        ] {
            let (w, h) = size.dims();
            let cells = (w * h) as usize;
            let cfg = build_random_config("bounds", size);
            let sc = cfg.generation.system_count;
            assert!(sc >= 4, "{}: system_count {sc} < 4", size.as_slug());
            assert!(
                sc <= cells,
                "{}: system_count {sc} > cells {cells}",
                size.as_slug()
            );
            assert!(cfg.generation.max_worlds_per_system >= cfg.generation.min_worlds_per_system);
        }
    }

    #[test]
    fn patch_regions_replaces_only_table_knobs() {
        let src = "[regions]\nenabled = true\ncount = 6\nmean_size = 9\napply_to_routes = true\n\n[[regions.conditions]]\nkind = \"warp_storm\"\nweight = 3.0\n";
        let out = patch_regions_toml(src, 3, 5);
        assert!(out.contains("count = 3"));
        assert!(out.contains("mean_size = 5"));
        assert!(out.contains("enabled = true"));
        assert!(out.contains("kind = \"warp_storm\""));
        assert!(!out.contains("count = 6"));
    }
}
