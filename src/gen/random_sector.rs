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
    PlacementConfig, ProjectConfig, RelationsGenerationConfig,
    RouteGenerationConfig, WorldSelectionConfig,
};
use crate::errors::SectorError;
use crate::hooks::HooksReport;
use crate::input::ProjectInput;
use crate::missions::MissionsReport;
use crate::personae::PersonaeReport;
use crate::prose::ProseReport;
use crate::sector_model::GeneratedSector;
use crate::sites::SitesReport;
use crate::SectorProgress;

/// Id of the checked-in "everything on" preset that supplies the content +
/// overlay data tree. Hidden from the gallery (leading `_`) but scaffoldable.
pub const FULL_PRESET_ID: &str = "_full";

/// Upper bound (inclusive) on either custom grid dimension. The largest grid
/// the generator is expected to handle is `MAX_CUSTOM_DIM × MAX_CUSTOM_DIM`
/// (RANDOM.md §7.4). The GUI clamps its width/height fields to this and the CLI
/// rejects anything larger; the value is shared so all three entry points agree.
pub const MAX_CUSTOM_DIM: u32 = 80;

/// The one user input: how big the sector is. **Every sector is square**
/// (`width == height`), so a size is fully described by a single side length
/// (RANDOM.md §5.2 / the square-sector invariant). The side length maps to a
/// cell count; `system_count` and every other structural knob are rolled from
/// the seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorSize {
    /// 8 × 8 = 64 cells.
    Small,
    /// 16 × 16 = 256 cells.
    Medium,
    /// 32 × 32 = 1024 cells.
    Large,
    /// 48 × 48 = 2304 cells.
    Vast,
    /// 64 × 64 = 4096 cells.
    Massive,
    /// 80 × 80 = 6400 cells — the maximum supported grid ([`MAX_CUSTOM_DIM`]).
    Huge,
    /// An explicit square `dim × dim`.
    Custom { dim: u32 },
}

impl SectorSize {
    /// Grid dimensions `(width, height)` in hexes. Always square, so both
    /// elements are equal — see [`Self::dim`].
    #[must_use]
    pub fn dims(self) -> (u32, u32) {
        let d = self.dim();
        (d, d)
    }

    /// The square side length in hexes. Sectors are square, so one dimension
    /// fully describes the grid.
    #[must_use]
    pub fn dim(self) -> u32 {
        match self {
            SectorSize::Small => 8,
            SectorSize::Medium => 16,
            SectorSize::Large => 32,
            SectorSize::Vast => 48,
            SectorSize::Massive => 64,
            SectorSize::Huge => 80,
            SectorSize::Custom { dim } => dim,
        }
    }

    /// Parse a CLI-style size token
    /// (`small`/`medium`/`large`/`vast`/`massive`/`huge`). `Custom` is
    /// expressed via an explicit square `--width`/`--height`, not a token.
    #[must_use]
    pub fn parse_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "vast" => Some(Self::Vast),
            "massive" => Some(Self::Massive),
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
            SectorSize::Vast => "vast",
            SectorSize::Massive => "massive",
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

/// A coarse, ordered phase of the random-sector pipeline. The phases run in
/// declaration order; [`generate_random_sector_with_progress`] emits each one
/// as it begins, so the builder/CLI can drive a progress bar without knowing
/// the internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomPhase {
    /// Scaffolding the project bundle from the `_full` preset.
    Scaffolding,
    /// Rolling the config, patching regions, loading + validating the project.
    Configuring,
    /// Generating the sector itself (the long pole on big grids). Subdivided by
    /// the underlying [`SectorProgress`] stages.
    Generating,
    /// Deriving the personae overlay.
    Personae,
    /// Deriving the sites overlay.
    Sites,
    /// Deriving the adventure hooks overlay.
    Hooks,
    /// Deriving the missions overlay.
    Missions,
    /// Deriving the prose / gazetteer overlay.
    Prose,
}

impl RandomPhase {
    /// All phases in pipeline order.
    pub const ALL: [RandomPhase; 8] = [
        RandomPhase::Scaffolding,
        RandomPhase::Configuring,
        RandomPhase::Generating,
        RandomPhase::Personae,
        RandomPhase::Sites,
        RandomPhase::Hooks,
        RandomPhase::Missions,
        RandomPhase::Prose,
    ];

    /// Zero-based index of this phase in [`Self::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }

    /// Human-readable label for the progress popup.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            RandomPhase::Scaffolding => "Preparing project",
            RandomPhase::Configuring => "Rolling configuration",
            RandomPhase::Generating => "Generating sector",
            RandomPhase::Personae => "Deriving personae",
            RandomPhase::Sites => "Deriving sites",
            RandomPhase::Hooks => "Deriving hooks",
            RandomPhase::Missions => "Deriving missions",
            RandomPhase::Prose => "Writing gazetteer",
        }
    }
}

/// A progress event emitted while a random sector is being generated. `phase`
/// is the current coarse pipeline phase; `fraction` is the overall completion
/// in `[0.0, 1.0]` (already blended with intra-phase progress for the long
/// generation phase); `detail` is an optional sub-step note.
#[derive(Debug, Clone)]
pub struct RandomProgress {
    pub phase: RandomPhase,
    pub fraction: f32,
    pub detail: Option<String>,
}

impl RandomProgress {
    /// One-line label for the popup: the phase label, plus any sub-step detail.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{} — {detail}", self.phase.label()),
            None => self.phase.label().to_string(),
        }
    }
}

/// Map an inner [`SectorProgress`] event onto `(intra, label)` for the
/// Generating phase: `intra` is `0.0..=1.0` within that phase's slice (the
/// per-system build loop dominates the cost on large grids), `label` is a short
/// sub-step note. Returns `None` for the many fine-grained sub-events so the bar
/// advances only on coarse milestones and never jumps backwards.
fn generation_substep(sp: &SectorProgress) -> Option<(f32, &'static str)> {
    Some(match sp {
        SectorProgress::WorldPoolBuilt { .. } => (0.03, "building world pool"),
        SectorProgress::SystemsPlaced { .. } => (0.06, "placing systems"),
        SectorProgress::SystemBuilt { current, total, .. } => {
            let frac = if *total == 0 {
                0.0
            } else {
                *current as f32 / *total as f32
            };
            (0.1 + frac * 0.6, "building systems")
        }
        SectorProgress::RoutesGenerated { .. } => (0.78, "generating routes"),
        SectorProgress::RegionEffectsApplied { .. } => (0.85, "applying region effects"),
        SectorProgress::ManifestBuilt { .. } => (0.9, "building manifest"),
        SectorProgress::ChronicleComplete { .. } => (0.96, "deriving chronicle"),
        SectorProgress::Complete { .. } => (1.0, "sector complete"),
        _ => return None,
    })
}

/// Overall fraction for a phase that is `intra` (`0.0..=1.0`) of the way through
/// its own slice. Phases each own an equal `1/ALL` slice of the bar.
fn phase_fraction(phase: RandomPhase, intra: f32) -> f32 {
    let span = 1.0 / RandomPhase::ALL.len() as f32;
    (phase.index() as f32 * span + intra.clamp(0.0, 1.0) * span).clamp(0.0, 1.0)
}

/// Mint a fresh root seed from process entropy. This is the **only**
/// non-deterministic step in the whole path (RANDOM.md §5.1). Output is a
/// 16-char hex string. Avoids `rand::thread_rng()` to honour the determinism
/// invariant — entropy comes from wall-clock nanos, the pid, and a per-process
/// monotonic counter, folded through blake3. The counter guarantees distinct
/// seeds for back-to-back mints in the same process even when the wall clock
/// has not advanced (e.g. scripted loops with a coarse clock).
///
/// determinism: SEED MINT ONLY — never call inside a generation stage.
#[must_use]
pub fn mint_seed() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    // Per-process monotonic tie-breaker so two mints in the same nanosecond
    // still produce different seeds (coarse clocks collide under tight loops).
    static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = u128::from(std::process::id());
    let counter = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"sectorforge:random-seed-mint:");
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&pid.to_le_bytes());
    hasher.update(&counter.to_le_bytes());
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

    // Roll a placement style (0=uniform, 1=weighted, 2=clustered). The style
    // itself is no longer stored on the config; it only gates whether
    // `cluster_bias` gets a nonzero roll. The draw sequence here is fixed —
    // `gen_range(0..3u8)` then a conditional `gen_range(0.3..=0.7)` — and must
    // not change, or the `random` SectorSize path churns its goldens.
    let clustered = rng.gen_range(0..3u8) == 2;
    let cluster_bias = if clustered {
        rng.gen_range(0.3..=0.7)
    } else {
        0.0
    };

    let same_star_colour_bias = rng.gen_range(1.0..=1.4);
    let strict_same_star_colour = rng.gen_bool(0.15);
    let avoid_duplicate_world_type_in_system = rng.gen_bool(0.5);

    let max_route_distance = rng.gen_range(3..=6u32);
    let route_density = rng.gen_range(0.15..=0.40);

    // Bound relations/economy output on the largest grids (RANDOM.md §5.6):
    // raise the presence floor once the grid passes ~400 cells (Large and up,
    // plus big customs) so the presence/economy graph stays tractable.
    let min_world_presence = if cells > 400 { 2 } else { 1 };

    // Region overlay scaled to the grid: total region area ≈ 60% of the grid,
    // spread over at most `MAX_REGIONS` blobs. Small grids keep a modest blob
    // size floor (so they get a handful of small regions); once 60% coverage
    // would need more than `MAX_REGIONS` blobs the count caps and the
    // per-region size grows instead — so an 80×80 grid gets ~50 large regions,
    // not hundreds of tiny ones. Still clamped to ≤ ½ the cells for tiny grids.
    const MAX_REGIONS: u32 = 50;
    let coverage_cells = cells_f * 0.6;
    let base_size = rng.gen_range(4..=8u32);
    let region_count = ((coverage_cells / f64::from(base_size)).round() as u32)
        .clamp(1, MAX_REGIONS.min((cells / 2).max(1)));
    let mean_size = ((coverage_cells / f64::from(region_count)).round() as u32).max(base_size);

    let theme = crate::map_theme::BUILTIN_THEME_NAMES
        [rng.gen_range(0..crate::map_theme::BUILTIN_THEME_NAMES.len())];

    // ── Assemble the config ─────────────────────────────────────────────────
    let slug = seed_slug(seed);

    let generation = GenerationConfig {
        seed: seed.to_string(),
        sector_width: width,
        sector_height: height,
        system_count,
        min_worlds_per_system: min_worlds,
        max_worlds_per_system: max_worlds,
        allow_empty_hexes: true,
        world_feature_count,
        placement: PlacementConfig {
            cluster_bias,
            minimum_system_distance: 1,
        },
        world_selection: WorldSelectionConfig {
            same_star_colour_bias,
            strict_same_star_colour,
            avoid_duplicate_world_type_in_system,
        },
        routes: RouteGenerationConfig {
            enabled: true,
            max_route_distance,
            route_density,
            ensure_connected_graph: true,
            // Legacy default: the random scaffolder keeps the early perilous-cap
            // balancing. Projects opt into quantile rebalancing via their toml.
            stability_targets: None,
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
        schema_version: None,
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
    generate_random_sector_with_progress(size, seed, presets_dir, dest, &mut |_| {})
}

/// As [`generate_random_sector`], but scaffolds the data tree from an explicit
/// `baseline` preset id (no-op progress). See
/// [`generate_random_sector_from_with_progress`] for the baseline semantics.
///
/// # Errors
///
/// Identical to [`generate_random_sector_from_with_progress`].
pub fn generate_random_sector_from(
    size: SectorSize,
    seed: Option<String>,
    baseline: &str,
    presets_dir: &Utf8Path,
    dest: &Utf8Path,
) -> Result<RandomReport, SectorError> {
    generate_random_sector_from_with_progress(size, seed, baseline, presets_dir, dest, &mut |_| {})
}

/// As [`generate_random_sector`], but emits a [`RandomProgress`] event at the
/// start of every pipeline phase (and throughout the long generation phase) so
/// a GUI can animate a progress popup (RANDOM.md §7.4). The headless paths use
/// the no-op-callback [`generate_random_sector`] wrapper.
///
/// The callback is a side channel only — the generated artifacts are identical
/// to [`generate_random_sector`] for the same `(size, seed)` (the generation
/// itself runs through [`crate::generate_sector_with_progress`], whose no-op
/// form is exactly what [`crate::generate_sector`] already calls).
///
/// # Errors
///
/// Identical to [`generate_random_sector`].
pub fn generate_random_sector_with_progress(
    size: SectorSize,
    seed: Option<String>,
    presets_dir: &Utf8Path,
    dest: &Utf8Path,
    progress: &mut dyn FnMut(RandomProgress),
) -> Result<RandomReport, SectorError> {
    generate_random_sector_from_with_progress(
        size,
        seed,
        FULL_PRESET_ID,
        presets_dir,
        dest,
        progress,
    )
}

/// As [`generate_random_sector_with_progress`], but scaffolds the content +
/// overlay *data tree* from an explicit `baseline` preset id (e.g.
/// `"dead-sector"`) instead of [`FULL_PRESET_ID`].
///
/// The rolled `[generation]` config is byte-identical for the same
/// `(size, seed)` regardless of the baseline — only the scaffolded data tree
/// differs. So a baseline themes the sector's worlds, factions, relations,
/// regions, economy, history, personae, sites, hooks, missions, and prose,
/// while the layout (placement, density, routes, sizing) stays fully
/// randomised (RANDOM.md §6: pick a themed baseline, fully roll the rest).
///
/// `baseline` must name a preset under `presets_dir` whose *merged* data tree
/// (after any `inherits` overlay) carries every file the rolled config
/// references at the standard nested paths — see [`crate::presets::scaffold`].
/// The four checked-in themed presets (`m42-classic`, `embattled-frontier`,
/// `dead-sector`, `mercantile-crossroads`) and `_full` all satisfy this.
///
/// # Errors
///
/// Identical to [`generate_random_sector_with_progress`]; additionally surfaces
/// any scaffolding error if `baseline` is missing or incomplete.
pub fn generate_random_sector_from_with_progress(
    size: SectorSize,
    seed: Option<String>,
    baseline: &str,
    presets_dir: &Utf8Path,
    dest: &Utf8Path,
    progress: &mut dyn FnMut(RandomProgress),
) -> Result<RandomReport, SectorError> {
    let seed = seed.unwrap_or_else(mint_seed);

    // 1. Materialise the bundle (copies the baseline's data tree + a
    //    sectorforge.toml we immediately overwrite). scaffold requires `dest`
    //    not to exist.
    progress(RandomProgress {
        phase: RandomPhase::Scaffolding,
        fraction: phase_fraction(RandomPhase::Scaffolding, 0.0),
        detail: None,
    });
    crate::presets::scaffold(presets_dir, baseline, dest, None)?;

    // 2. Roll the complete config from the seed and write it over the
    //    scaffolded one — we own every field, so nothing falls to a default.
    progress(RandomProgress {
        phase: RandomPhase::Configuring,
        fraction: phase_fraction(RandomPhase::Configuring, 0.0),
        detail: None,
    });
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
    //    The inner SectorProgress fraction is blended into the Generating slice.
    let sector = crate::generate_sector_with_progress(input.clone(), |sp| {
        if let Some((intra, detail)) = generation_substep(&sp) {
            progress(RandomProgress {
                phase: RandomPhase::Generating,
                fraction: phase_fraction(RandomPhase::Generating, intra),
                detail: Some(detail.to_string()),
            });
        }
    })?;
    let _invariants = crate::validate_sector(&sector);

    // 6. Run the five post-generation derivations the orchestrator skips.
    progress(RandomProgress {
        phase: RandomPhase::Personae,
        fraction: phase_fraction(RandomPhase::Personae, 0.0),
        detail: None,
    });
    let personae = crate::derive_personae_with(&sector, &input.catalogs.personae);
    progress(RandomProgress {
        phase: RandomPhase::Sites,
        fraction: phase_fraction(RandomPhase::Sites, 0.0),
        detail: None,
    });
    let sites = crate::derive_sites_with(&sector, &input.catalogs.sites);
    progress(RandomProgress {
        phase: RandomPhase::Hooks,
        fraction: phase_fraction(RandomPhase::Hooks, 0.0),
        detail: None,
    });
    let hooks = crate::derive_hooks_with(&sector, &input.catalogs.hooks);
    progress(RandomProgress {
        phase: RandomPhase::Missions,
        fraction: phase_fraction(RandomPhase::Missions, 0.0),
        detail: None,
    });
    let missions = crate::derive_missions_with(&sector, &input.catalogs.missions);
    progress(RandomProgress {
        phase: RandomPhase::Prose,
        fraction: phase_fraction(RandomPhase::Prose, 0.0),
        detail: None,
    });
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
        assert_eq!(SectorSize::Small.dims(), (8, 8));
        assert_eq!(SectorSize::Medium.dims(), (16, 16));
        assert_eq!(SectorSize::Large.dims(), (32, 32));
        assert_eq!(SectorSize::Vast.dims(), (48, 48));
        assert_eq!(SectorSize::Massive.dims(), (64, 64));
        assert_eq!(SectorSize::Huge.dims(), (80, 80));
        assert_eq!(SectorSize::Custom { dim: 9 }.dims(), (9, 9));
    }

    #[test]
    fn every_size_is_square() {
        for size in [
            SectorSize::Small,
            SectorSize::Medium,
            SectorSize::Large,
            SectorSize::Vast,
            SectorSize::Massive,
            SectorSize::Huge,
            SectorSize::Custom { dim: 13 },
        ] {
            let (w, h) = size.dims();
            assert_eq!(w, h, "{} must be square", size.as_slug());
        }
    }

    #[test]
    fn parse_token_roundtrip() {
        for s in [
            SectorSize::Small,
            SectorSize::Medium,
            SectorSize::Large,
            SectorSize::Vast,
            SectorSize::Massive,
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
            SectorSize::Vast,
            SectorSize::Massive,
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
    fn phase_fraction_is_monotonic_and_bounded() {
        let mut last = -1.0_f32;
        for phase in RandomPhase::ALL {
            let f = phase_fraction(phase, 0.0);
            assert!(
                (0.0..=1.0).contains(&f),
                "{phase:?} fraction {f} out of range"
            );
            assert!(
                f >= last,
                "{phase:?} fraction {f} not monotonic after {last}"
            );
            last = f;
            // Intra-phase progress never escapes the phase's own slice.
            let end = phase_fraction(phase, 1.0);
            assert!(end <= 1.0 && end >= f, "{phase:?} intra-progress regressed");
        }
        assert_eq!(phase_fraction(RandomPhase::Scaffolding, 0.0), 0.0);
    }

    #[test]
    fn random_progress_label_includes_detail() {
        let p = RandomProgress {
            phase: RandomPhase::Generating,
            fraction: 0.3,
            detail: Some("Building routes".into()),
        };
        assert_eq!(p.label(), "Generating sector — Building routes");
        let p = RandomProgress {
            phase: RandomPhase::Personae,
            fraction: 0.5,
            detail: None,
        };
        assert_eq!(p.label(), "Deriving personae");
    }

    // GAP 172 (a): `SectorSize::dims()` on degenerate dims is pure (just
    // returns `(d, d)`), so it never multiplies, overflows, or panics. 0×0 is
    // square by construction.
    #[test]
    fn sector_size_degenerate_dims() {
        assert_eq!(SectorSize::Custom { dim: 1 }.dims(), (1, 1));
        assert_eq!(SectorSize::Custom { dim: 0 }.dims(), (0, 0));
    }

    // GAP 172 (b): `build_random_config` at the smallest *valid* Custom dim
    // (dim:2). Exercises the `cells.max(1)` / `div_ceil(4).max(1)` guards and
    // the `system_count` clamp without tripping the `clamp(4, cells)` panic
    // that dim:1 (cells=1) hits.
    #[test]
    fn build_random_config_smallest_grid_dim2() {
        let cfg = build_random_config("degenerate-dim", SectorSize::Custom { dim: 2 });
        assert_eq!(cfg.generation.sector_width, 2);
        assert_eq!(cfg.generation.sector_height, 2);
        // cells = 2*2 = 4; density*4 rounds to 1..2 then clamps up to the lower
        // bound 4 (== cells), so system_count is exactly 4 regardless of the
        // rolled density.
        assert_eq!(cfg.generation.system_count, 4);
        assert!(cfg.generation.max_worlds_per_system >= cfg.generation.min_worlds_per_system);
    }

    // GAP 172 (c): regression marker — `Custom{dim:1}` yields cells=1, so
    // `system_count` does `clamp(4, 1)`, and Rust's `i64::clamp` panics when
    // `min > max`. This pins the *current* behaviour: if the roller is ever
    // fixed to clamp the lower bound down to `cells`, this test must be updated.
    #[test]
    #[should_panic(expected = "min > max")]
    fn build_random_config_dim1_panics_today() {
        let _ = build_random_config("dim1", SectorSize::Custom { dim: 1 });
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
