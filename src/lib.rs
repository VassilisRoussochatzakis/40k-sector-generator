//! sectorforge — deterministic Warhammer 40k star sector generator.
//!
//! `worlds.rs` owns the canonical world taxonomy and Excel parsing.
//! Everything in this crate builds a sector-scale layer around it:
//! candidate pools, deterministic placement, systems, worlds, routes,
//! factions, validation, and export.

pub mod worlds;

pub mod bitmap;
pub mod config;
pub mod gui;
pub mod errors;
pub mod export;
pub mod factions;
pub mod generation;
pub mod ids;
pub mod input;
pub mod invariants;
pub mod names;
pub mod render;
pub mod rng;
pub mod routes;
pub mod sector_model;
pub mod system_map;
pub mod taxonomy;
pub mod validation;
pub mod world_pool;

pub use config::AppConfig;
pub use errors::SectorError;
pub use input::ProjectInput;
pub use invariants::{InvariantReport, InvariantViolation};
pub use sector_model::{GeneratedSector, GeneratedSystem, HexCoord};
pub use validation::{ValidationIssue, ValidationReport};

pub const GENERATOR_NAME: &str = "sectorforge";
pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::collections::BTreeSet;
use std::fs;

use camino::Utf8Path;

/// Load a project directory (config + all referenced input files) into memory.
pub fn load_project(path: impl AsRef<Utf8Path>) -> Result<ProjectInput, SectorError> {
    input::load_project(path.as_ref())
}

/// Run validation against an already-loaded project. Never panics.
pub fn validate_project(project: &ProjectInput) -> Result<ValidationReport, SectorError> {
    Ok(validation::validate(project))
}

/// Deterministic top-level sector generation. Pure — does not touch disk.
pub fn generate_sector(project: ProjectInput) -> Result<GeneratedSector, SectorError> {
    generation::generate(project)
}

/// Spec §11.11: post-generation invariants check on an already-built sector.
pub fn validate_sector(sector: &GeneratedSector) -> InvariantReport {
    invariants::check_sector(sector)
}

/// Spec §10: generate one fully populated system standalone, using a project's
/// catalogs/config. Routes and full sector context are not produced. Factions
/// are assigned to the single system using the same rules as sector
/// generation.
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
pub fn render_sector_markdown(sector: &GeneratedSector) -> String {
    render::render_sector_markdown(sector)
}

/// Standalone single-system Markdown snippet.
pub fn render_system_markdown(system: &GeneratedSystem) -> String {
    render::render_system_markdown(system)
}

/// Load a previously generated sector from a JSON file. Round-trips through
/// `serde_json` (spec §11.11 requires this).
pub fn load_sector_json(path: impl AsRef<Utf8Path>) -> Result<GeneratedSector, SectorError> {
    let p = path.as_ref();
    let text = fs::read_to_string(p).map_err(|e| SectorError::io(p.as_str(), e))?;
    serde_json::from_str(&text)
        .map_err(|e| SectorError::config_parse(p.as_str(), format!("invalid sector json: {e}")))
}

/// Spec §15: deterministic writers for the generated sector / standalone system.
pub fn write_sector_json(
    path: impl AsRef<Utf8Path>,
    sector: &GeneratedSector,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = serde_json::to_string_pretty(sector)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

pub fn write_system_json(
    path: impl AsRef<Utf8Path>,
    system: &GeneratedSystem,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = serde_json::to_string_pretty(system)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

pub fn write_sector_markdown(
    path: impl AsRef<Utf8Path>,
    sector: &GeneratedSector,
) -> Result<(), SectorError> {
    let p = path.as_ref();
    let text = render::render_sector_markdown(sector);
    fs::write(p, text).map_err(|e| SectorError::io(p.as_str(), e))
}

/// Write generated sector to disk according to the project's output config.
pub fn export_sector(
    sector: &GeneratedSector,
    output_config: &config::OutputConfig,
    output_dir: impl AsRef<Utf8Path>,
) -> Result<(), SectorError> {
    export::export_all(sector, output_config, output_dir.as_ref())
}

/// Inspect-worlds: load and summarize a world-data dir for the CLI.
pub fn inspect_world_workbook(path: &str) -> Result<world_pool::WorkbookStats, SectorError> {
    world_pool::inspect_workbook(path)
}
