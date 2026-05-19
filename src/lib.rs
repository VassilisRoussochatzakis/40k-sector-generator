//! sectorforge — deterministic Warhammer 40k star sector generator.
//!
//! [`worlds`] owns the canonical world taxonomy and CSV parsing.
//! Everything in this crate builds a sector-scale layer around it:
//! candidate pools, deterministic placement, systems, worlds, routes,
//! factions, validation, and export.
//!
//! # Quick start
//!
//! ```no_run
//! use camino::Utf8PathBuf;
//!
//! let project_dir = Utf8PathBuf::from("examples/m42_project");
//! let input = sectorforge::load_project(&project_dir)?;
//! let report = sectorforge::validate_project(&input)?;
//! assert!(report.ok);
//!
//! let output_cfg = input.config.outputs.clone();
//! let sector = sectorforge::generate_sector(input)?;
//! sectorforge::export_sector(&sector, &output_cfg, "out")?;
//! # Ok::<(), sectorforge::SectorError>(())
//! ```

pub mod worlds;

pub mod analytics;
pub mod archetypes;
pub mod bitmap;
pub mod config;
pub mod conflict;
pub mod control;
pub mod diff;
pub mod economy;
pub mod errors;
pub mod export;
pub mod faction_style;
pub mod factions;
pub mod generation;
pub mod gui;
pub mod heatmap;
pub mod hidden_routes;
pub mod history;
pub mod hooks;
pub mod html_export;
pub mod ids;
pub mod importance;
pub mod influence_field;
pub mod input;
pub mod intel;
pub mod invariants;
pub mod names;
pub mod orbital_assets;
pub mod personae;
pub mod power_projection;
pub mod presets;
pub mod prose;
pub mod regions;
pub mod relations;
pub mod render;
pub mod rng;
pub mod route_control;
pub mod routes;
pub mod search;
pub mod sector_model;
pub mod sector_save;
pub mod segmentum;
pub mod stability;
pub mod subsectors;
pub mod surface_region;
pub mod system_map;
pub mod taxonomy;
pub mod validation;
pub mod world_ecs;
pub mod world_pool;

pub use config::AppConfig;
pub use conflict::{advance_sector, ConflictState, HYSTERESIS_TICKS};
pub use economy::{
    load_economy_file, EconomyConfig, EconomyFile, EconomyReport, ResourceVector, RouteEconomy,
    SystemEconomy, WorldEconomy,
};
pub use errors::SectorError;
pub use input::ProjectInput;
pub use invariants::{InvariantReport, InvariantViolation};
pub use regions::{load_regions_file, RegionConditionKind, RegionsConfig, RegionsFile, WarpRegion};
pub use relations::{
    load_relations_file, FactionRelation, RelationsConfig, RelationsFile, RelationsMatrix,
    RelationsReport, Stance,
};
pub use sector_model::{GeneratedSector, GeneratedSystem, HexCoord};
pub use sector_save::{merge as merge_sector_save, split as split_sector_save, SectorSave};
pub use segmentum::{
    compose as compose_segmentum, load_segmentum_file, BorderOrientation, ChildEntry, FactionMode,
    InterSectorLink, Segmentum, SegmentumChild, SegmentumConfig, SegmentumFile, SegmentumManifest,
    StitchConfig,
};
pub use subsectors::{
    build_subsectors, ControlDenominator, Subsector, SubsectorBuildError, SubsectorConfig,
};
pub use validation::{ValidationIssue, ValidationReport};
pub use world_ecs::{build as build_entity_world, EntityWorld};

pub const GENERATOR_NAME: &str = "sectorforge";
pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::collections::BTreeSet;
use std::fs;

use camino::Utf8Path;

/// Load a project directory (`sectorforge.toml` + every input it references)
/// into memory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the project root or any referenced file
/// cannot be read, [`SectorError::ConfigParse`] if `sectorforge.toml` or a
/// TOML data file fails to parse, and [`SectorError::WorldDataLoad`] if
/// `data/worlds/{key,generator}.csv` are malformed.
///
/// # Examples
///
/// ```no_run
/// let project = sectorforge::load_project("examples/m42_project")?;
/// assert_eq!(project.config.project.id, "m42-sector");
/// # Ok::<(), sectorforge::SectorError>(())
/// ```
pub fn load_project(path: impl AsRef<Utf8Path>) -> Result<ProjectInput, SectorError> {
    input::load_project(path.as_ref())
}

/// Run validation against an already-loaded project. Never panics.
///
/// # Errors
///
/// Currently infallible — the result is wrapped in `Result` to leave room for
/// future fatal-validation cases. Inspect [`ValidationReport`] for issues.
///
/// # Examples
///
/// ```no_run
/// let project = sectorforge::load_project("examples/m42_project")?;
/// let report = sectorforge::validate_project(&project)?;
/// assert!(report.errors.is_empty());
/// # Ok::<(), sectorforge::SectorError>(())
/// ```
pub fn validate_project(project: &ProjectInput) -> Result<ValidationReport, SectorError> {
    Ok(validation::validate(project))
}

/// Deterministic top-level sector generation. Pure — does not touch disk.
///
/// # Errors
///
/// Returns [`SectorError::NoWorldCandidates`] if the project's world pool is
/// empty, [`SectorError::WeightedSelectionFailed`] if a weighted RNG draw
/// cannot complete, and [`SectorError::InvalidConfig`] if generation
/// parameters are inconsistent (e.g. `system_count` exceeds grid cells).
///
/// # Examples
///
/// ```no_run
/// let project = sectorforge::load_project("examples/m42_project")?;
/// let sector = sectorforge::generate_sector(project)?;
/// assert_eq!(sector.systems.len(), 24);
/// # Ok::<(), sectorforge::SectorError>(())
/// ```
pub fn generate_sector(project: ProjectInput) -> Result<GeneratedSector, SectorError> {
    generation::generate(project)
}

/// Spec §11.11: post-generation invariants check on an already-built sector.
///
/// Never fails — returns an [`InvariantReport`] whose `violations` vector is
/// empty on a clean sector.
///
/// # Examples
///
/// ```no_run
/// let project = sectorforge::load_project("examples/m42_project")?;
/// let sector = sectorforge::generate_sector(project)?;
/// let report = sectorforge::validate_sector(&sector);
/// assert!(report.violations.is_empty());
/// # Ok::<(), sectorforge::SectorError>(())
/// ```
#[must_use]
pub fn validate_sector(sector: &GeneratedSector) -> InvariantReport {
    invariants::check_sector(sector)
}

/// Spec §10: generate one fully populated system standalone, using a project's
/// catalogs/config. Routes and full sector context are not produced. Factions
/// are assigned to the single system using the same rules as sector
/// generation.
///
/// # Errors
///
/// Returns [`SectorError::InvalidConfig`] when `index == 0`,
/// [`SectorError::NoWorldCandidates`] when the world pool is empty, and
/// propagates any error from the underlying generator.
///
/// # Examples
///
/// ```no_run
/// use sectorforge::HexCoord;
/// let project = sectorforge::load_project("examples/m42_project")?;
/// let sys = sectorforge::generate_system_standalone(project, 1, HexCoord { q: 0, r: 0 })?;
/// assert_eq!(sys.id, "sys-0001");
/// # Ok::<(), sectorforge::SectorError>(())
/// ```
pub fn generate_system_standalone(
    project: ProjectInput,
    index: usize,
    coord: HexCoord,
) -> Result<GeneratedSystem, SectorError> {
    if index == 0 {
        return Err(SectorError::InvalidConfig(
            "system index must be >= 1".to_string(),
        ));
    }
    let pool = world_pool::build_pool(
        &project.world_rows,
        &project.world_tables,
        &project.config.generation.world_selection,
    );
    if pool.candidates.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    let mut sys = generation::build_system(
        &project.config,
        &pool,
        &project.names,
        index,
        coord,
        &mut used_names,
    )?;
    let mut single = [sys.clone()];
    generation::assign_factions_for_systems(
        &mut single,
        &project.factions,
        &project.config.generation.seed,
        &sys.id,
    );
    sys = single[0].clone();
    Ok(sys)
}

/// Spec §12: deterministic Markdown overview for a generated sector.
#[must_use]
pub fn render_sector_markdown(sector: &GeneratedSector) -> String {
    render::render_sector_markdown(sector)
}

/// Standalone single-system Markdown snippet.
#[must_use]
pub fn render_system_markdown(system: &GeneratedSystem) -> String {
    render::render_system_markdown(system)
}

/// Load a previously generated sector from a JSON file. Round-trips through
/// `serde_json` (spec §11.11 requires this).
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be read and
/// [`SectorError::ConfigParse`] if the JSON does not match the
/// [`GeneratedSector`] schema.
pub fn load_sector_json(path: impl AsRef<Utf8Path>) -> Result<GeneratedSector, SectorError> {
    let p = path.as_ref();
    let text = fs::read_to_string(p).map_err(|e| SectorError::io(p.as_str(), e))?;
    serde_json::from_str(&text)
        .map_err(|e| SectorError::config_parse(p.as_str(), format!("invalid sector json: {e}")))
}

/// Spec §15: deterministic writers for the generated sector / standalone system.
///
/// # Errors
///
/// Returns [`SectorError::ExportFailed`] if the sector cannot be serialised to
/// JSON, and [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_json(
    path: impl AsRef<Utf8Path>,
    sector: &GeneratedSector,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = serde_json::to_string_pretty(sector)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

/// Write a standalone [`GeneratedSystem`] to disk as pretty JSON.
///
/// # Errors
///
/// Same as [`write_sector_json`].
pub fn write_system_json(
    path: impl AsRef<Utf8Path>,
    system: &GeneratedSystem,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = serde_json::to_string_pretty(system)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

/// Spec §13 NEXT: write a [`SectorSave`] (IDs-only runtime state) to JSON.
///
/// # Errors
///
/// Returns [`SectorError::ExportFailed`] when serialisation fails and
/// [`SectorError::Io`] when the file cannot be written.
pub fn write_sector_save(path: impl AsRef<Utf8Path>, save: &SectorSave) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = serde_json::to_string_pretty(save)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

/// Spec §13 NEXT: load a [`SectorSave`] from disk.
///
/// # Errors
///
/// Returns [`SectorError::Io`] when the file cannot be read and
/// [`SectorError::ConfigParse`] when the JSON does not match the
/// [`SectorSave`] schema.
pub fn load_sector_save(path: impl AsRef<Utf8Path>) -> Result<SectorSave, SectorError> {
    let p = path.as_ref();
    let text = fs::read_to_string(p).map_err(|e| SectorError::io(p.as_str(), e))?;
    serde_json::from_str(&text)
        .map_err(|e| SectorError::config_parse(p.as_str(), format!("invalid sector save: {e}")))
}

/// Render and write the Markdown overview for a sector.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if the file cannot be written.
pub fn write_sector_markdown(
    path: impl AsRef<Utf8Path>,
    sector: &GeneratedSector,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = render::render_sector_markdown(sector);
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

/// Write generated sector to disk according to the project's output config.
///
/// # Errors
///
/// Propagates any error from the JSON/Markdown/CSV/bitmap writers — typically
/// [`SectorError::Io`] or [`SectorError::ExportFailed`].
pub fn export_sector(
    sector: &GeneratedSector,
    output_config: &config::OutputConfig,
    output_dir: impl AsRef<Utf8Path>,
) -> Result<(), SectorError> {
    export::export_all(sector, output_config, output_dir.as_ref())
}

/// §11 NEW.md: write a self-contained interactive HTML sector view to
/// `output_dir/sector.html`. Returns the resulting path.
///
/// # Errors
///
/// [`SectorError::Io`] / [`SectorError::ExportFailed`] from the writer.
pub fn write_interactive_html(
    sector: &GeneratedSector,
    output_dir: impl AsRef<Utf8Path>,
    cfg: &config::HtmlConfig,
) -> Result<camino::Utf8PathBuf, SectorError> {
    html_export::write_html(sector, output_dir.as_ref(), cfg)
}

/// §8 NEW.md: read-only sector analytics dashboard.
#[must_use]
pub fn analyze_sector(sector: &GeneratedSector) -> analytics::SectorAnalysis {
    analytics::analyze(sector)
}

/// §8 NEW.md: analytics with explicit threshold config (overrides defaults).
#[must_use]
pub fn analyze_sector_with(
    sector: &GeneratedSector,
    cfg: &analytics::AnalyzeConfig,
) -> analytics::SectorAnalysis {
    analytics::analyze_with(sector, cfg)
}

/// §8 NEW.md: deterministic Markdown render of an analysis.
#[must_use]
pub fn render_analysis_markdown(analysis: &analytics::SectorAnalysis) -> String {
    analytics::render_markdown(analysis)
}

/// §8 NEW.md: write `analysis.md` and `analysis.json` next to each other.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written, and
/// [`SectorError::ExportFailed`] if the analysis cannot be serialised.
pub fn write_analysis(
    output_dir: impl AsRef<Utf8Path>,
    analysis: &analytics::SectorAnalysis,
) -> Result<(), SectorError> {
    let dir = output_dir.as_ref();
    fs::create_dir_all(dir).map_err(|e| SectorError::io(dir.as_str(), e))?;
    let md_path = dir.join("analysis.md");
    let md = analytics::render_markdown(analysis);
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = dir.join("analysis.json");
    let json = serde_json::to_string_pretty(analysis)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    Ok(())
}

/// §2 NEW.md: constraint-directed deterministic seed search. Runs up to
/// `wishes.search.budget` candidates derived from `wishes.search.base_seed`
/// (defaulting to the project's configured seed) and returns the first
/// candidate that satisfies every constraint, or a near-miss report.
///
/// The project is used as a template only; the generator is not modified.
/// Same project + same wishes ⇒ same winning seed.
///
/// # Errors
///
/// Propagates any [`SectorError`] raised by `generate_sector` for an
/// individual candidate. Preflight problems (unknown faction ids etc.) are
/// reported inside the returned [`search::SearchOutcome`].
pub fn run_seed_search(
    project: &ProjectInput,
    wishes: &search::WishesFile,
) -> Result<search::SearchOutcome, SectorError> {
    search::run_search(project, wishes)
}

/// §2 NEW.md: write `search.md` + `search.json` into a directory.
///
/// # Errors
///
/// Same as [`search::write_search_outcome`].
pub fn write_search_outcome(
    output_dir: impl AsRef<Utf8Path>,
    outcome: &search::SearchOutcome,
) -> Result<(), SectorError> {
    search::write_search_outcome(output_dir.as_ref(), outcome)
}

/// §10 NEW.md: pure structural diff between two sectors. Renames map to
/// modifications, not delete+add, because entity ids are stable.
#[must_use]
pub fn diff_sectors(before: &GeneratedSector, after: &GeneratedSector) -> diff::SectorDiff {
    diff::diff_sectors(before, after)
}

/// §10 NEW.md: as [`diff_sectors`] but with explicit verbosity/threshold config.
#[must_use]
pub fn diff_sectors_with(
    before: &GeneratedSector,
    after: &GeneratedSector,
    cfg: &diff::DiffConfig,
) -> diff::SectorDiff {
    diff::diff_sectors_with(before, after, cfg)
}

/// §10 NEW.md: Markdown render of a sector diff.
#[must_use]
pub fn render_diff_markdown(d: &diff::SectorDiff) -> String {
    diff::render_markdown(d)
}

/// §10 NEW.md: write `diff.md` + `diff.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written and
/// [`SectorError::ExportFailed`] if the diff cannot be serialised.
pub fn write_diff(
    output_dir: impl AsRef<Utf8Path>,
    d: &diff::SectorDiff,
) -> Result<(), SectorError> {
    diff::write_diff(output_dir.as_ref(), d)
}

/// §1 NEW.md: deterministic chronicle of in-universe events explaining
/// the present sector configuration.
#[must_use]
pub fn derive_history(sector: &GeneratedSector) -> history::HistoryReport {
    history::derive(sector)
}

/// §1 NEW.md: chronicle with explicit config.
#[must_use]
pub fn derive_history_with(
    sector: &GeneratedSector,
    cfg: &history::HistoryConfig,
) -> history::HistoryReport {
    history::derive_with(sector, cfg)
}

/// §1 NEW.md: write `history.md` + `history.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_history(
    output_dir: impl AsRef<Utf8Path>,
    report: &history::HistoryReport,
    cfg: &history::HistoryConfig,
) -> Result<(), SectorError> {
    history::write_report(output_dir.as_ref(), report, cfg)
}

/// §3 NEW.md: deterministic dramatis personae overlay.
#[must_use]
pub fn derive_personae(sector: &GeneratedSector) -> personae::PersonaeReport {
    personae::derive(sector)
}

/// §3 NEW.md: personae with explicit config.
#[must_use]
pub fn derive_personae_with(
    sector: &GeneratedSector,
    cfg: &personae::PersonaeConfig,
) -> personae::PersonaeReport {
    personae::derive_with(sector, cfg)
}

/// §3 NEW.md: write `personae.md` + `personae.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_personae(
    output_dir: impl AsRef<Utf8Path>,
    report: &personae::PersonaeReport,
) -> Result<(), SectorError> {
    personae::write_report(output_dir.as_ref(), report)
}

/// §7 NEW.md: adventure / plot-hook derivation.
#[must_use]
pub fn derive_hooks(sector: &GeneratedSector) -> hooks::HooksReport {
    hooks::derive(sector)
}

/// §7 NEW.md: hooks with explicit config.
#[must_use]
pub fn derive_hooks_with(sector: &GeneratedSector, cfg: &hooks::HooksConfig) -> hooks::HooksReport {
    hooks::derive_with(sector, cfg)
}

/// §7 NEW.md: write `hooks.md` + `hooks.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_hooks(
    output_dir: impl AsRef<Utf8Path>,
    report: &hooks::HooksReport,
    cfg: &hooks::HooksConfig,
) -> Result<(), SectorError> {
    hooks::write_report(output_dir.as_ref(), report, cfg)
}

/// §6 NEW.md: deterministic gazetteer prose generator.
#[must_use]
pub fn derive_prose(sector: &GeneratedSector) -> prose::ProseReport {
    prose::derive(sector)
}

/// §6 NEW.md: prose with explicit config.
#[must_use]
pub fn derive_prose_with(sector: &GeneratedSector, cfg: &prose::ProseConfig) -> prose::ProseReport {
    prose::derive_with(sector, cfg)
}

/// §6 NEW.md: write `gazetteer.md` + `gazetteer.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_prose(
    output_dir: impl AsRef<Utf8Path>,
    report: &prose::ProseReport,
) -> Result<(), SectorError> {
    prose::write_report(output_dir.as_ref(), report)
}

/// §4 NEW.md: derive the inter-faction diplomacy matrix for a generated
/// sector. Pure read-only over `sector.factions` + co-occurrence in worlds.
#[must_use]
pub fn derive_relations(sector: &GeneratedSector) -> relations::RelationsMatrix {
    relations::derive(sector)
}

/// §4 NEW.md: derive the matrix with an explicit configuration (rules /
/// overrides loaded from `relations.toml`).
#[must_use]
pub fn derive_relations_with(
    sector: &GeneratedSector,
    cfg: &relations::RelationsConfig,
) -> relations::RelationsMatrix {
    relations::derive_with(sector, cfg)
}

/// §4 NEW.md: write `relations.md` + `relations.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_relations(
    output_dir: impl AsRef<Utf8Path>,
    report: &relations::RelationsReport,
) -> Result<(), SectorError> {
    relations::write_report(output_dir.as_ref(), report)
}

/// §5 NEW.md: build the regional warp-phenomena overlay for a sector grid.
/// `cfg.enabled = false` returns an empty list (the default).
#[must_use]
pub fn build_regions(
    seed: &str,
    width: u32,
    height: u32,
    cfg: &regions::RegionsConfig,
) -> Vec<regions::WarpRegion> {
    regions::build_regions(seed, width, height, cfg)
}

/// §5 NEW.md: write `regions.md` + `regions.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_regions(
    output_dir: impl AsRef<Utf8Path>,
    sector_id: &str,
    regs: &[regions::WarpRegion],
) -> Result<(), SectorError> {
    regions::write_report(output_dir.as_ref(), sector_id, regs)
}

/// §12 NEW.md: derive the per-world / per-system / sector economy snapshot.
#[must_use]
pub fn derive_economy(sector: &GeneratedSector) -> economy::EconomyReport {
    economy::derive(sector)
}

/// §12 NEW.md: derive with explicit config.
#[must_use]
pub fn derive_economy_with(
    sector: &GeneratedSector,
    cfg: &economy::EconomyConfig,
) -> economy::EconomyReport {
    economy::derive_with(sector, cfg)
}

/// §12 NEW.md: write `economy.md` + `economy.json` + `economy.csv` into a dir.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_economy(
    output_dir: impl AsRef<Utf8Path>,
    sector_id: &str,
    report: &economy::EconomyReport,
) -> Result<(), SectorError> {
    economy::write_report(output_dir.as_ref(), sector_id, report)
}

/// §14 NEW.md: write a composed segmentum's report files
/// (`segmentum.md`, `segmentum.json`, `super_manifest.json`) into a
/// directory. Per-child sector outputs are written by [`compose_segmentum`]
/// itself under `<output_dir>/children/<child_id>/`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_segmentum(
    output_dir: impl AsRef<Utf8Path>,
    seg: &segmentum::Segmentum,
) -> Result<(), SectorError> {
    segmentum::write_report(output_dir.as_ref(), seg)
}

/// Inspect-worlds: load and summarize a world-data dir for the CLI.
///
/// # Errors
///
/// Returns [`SectorError::WorldDataLoad`] if `key.csv`/`generator.csv` cannot
/// be parsed.
pub fn inspect_world_workbook(path: &str) -> Result<world_pool::WorkbookStats, SectorError> {
    world_pool::inspect_workbook(path)
}
