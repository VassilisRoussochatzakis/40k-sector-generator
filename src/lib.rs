//! sectorforge — deterministic Warhammer 40k star sector generator.
//!
//! `worlds.rs` owns the canonical world taxonomy and Excel parsing.
//! Everything in this crate builds a sector-scale layer around it:
//! candidate pools, deterministic placement, systems, worlds, routes,
//! factions, validation, and export.

pub mod worlds;

pub mod config;
pub mod errors;
pub mod export;
pub mod factions;
pub mod generation;
pub mod ids;
pub mod input;
pub mod names;
pub mod rng;
pub mod routes;
pub mod sector_model;
pub mod taxonomy;
pub mod validation;
pub mod world_pool;

pub use config::AppConfig;
pub use errors::SectorError;
pub use input::ProjectInput;
pub use sector_model::GeneratedSector;
pub use validation::ValidationReport;

pub const GENERATOR_NAME: &str = "sectorforge";
pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

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

/// Write generated sector to disk according to the project's output config.
pub fn export_sector(
    sector: &GeneratedSector,
    output_config: &config::OutputConfig,
    output_dir: impl AsRef<Utf8Path>,
) -> Result<(), SectorError> {
    export::export_all(sector, output_config, output_dir.as_ref())
}

/// Inspect-worlds: load and summarize a workbook for the CLI.
pub fn inspect_world_workbook(path: &str) -> Result<world_pool::WorkbookStats, SectorError> {
    world_pool::inspect_workbook(path)
}
