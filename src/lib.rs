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

pub mod bitmap;
pub mod config;
pub mod control;
pub mod errors;
pub mod export;
pub mod faction_style;
pub mod factions;
pub mod generation;
pub mod gui;
pub mod heatmap;
pub mod ids;
pub mod importance;
pub mod input;
pub mod invariants;
pub mod names;
pub mod render;
pub mod rng;
pub mod route_control;
pub mod routes;
pub mod sector_model;
pub mod stability;
pub mod subsectors;
pub mod system_map;
pub mod taxonomy;
pub mod validation;
pub mod world_pool;

pub use config::AppConfig;
pub use errors::SectorError;
pub use input::ProjectInput;
pub use invariants::{InvariantReport, InvariantViolation};
pub use sector_model::{GeneratedSector, GeneratedSystem, HexCoord};
pub use subsectors::{
    build_subsectors, ControlDenominator, Subsector, SubsectorBuildError, SubsectorConfig,
};
pub use validation::{ValidationIssue, ValidationReport};

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

/// Inspect-worlds: load and summarize a world-data dir for the CLI.
///
/// # Errors
///
/// Returns [`SectorError::WorldDataLoad`] if `key.csv`/`generator.csv` cannot
/// be parsed.
pub fn inspect_world_workbook(path: &str) -> Result<world_pool::WorkbookStats, SectorError> {
    world_pool::inspect_workbook(path)
}
