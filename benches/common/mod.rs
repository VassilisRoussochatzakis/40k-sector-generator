//! Shared prelude for the root bench targets. A subdirectory module, so cargo
//! doesn't auto-register it as a bench target; each bench file does
//! `mod common;`.

// Every bench target compiles its own copy and uses only part of it.
#![allow(dead_code)]

use camino::Utf8PathBuf;
use sectorforge::{generate_sector, load_project, GeneratedSector, ProjectInput};

/// Square scales — (side, system_count) for tiny / normal / large — labelled
/// `{side}x{side}_{count}`.
pub const SCALES: &[(u32, usize)] = &[(10, 24), (20, 96), (30, 200)];

/// `examples/m42_project` as checked in. Its seed is fixed, so every run
/// builds the same sectors.
pub fn load() -> ProjectInput {
    load_project(Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project"))
        .expect("load m42_project")
}

/// m42 on a square `side`×`side` grid holding `count` systems.
pub fn project(side: u32, count: usize) -> ProjectInput {
    let mut input = load();
    let generation = &mut input.config.generation;
    generation.sector_width = side;
    generation.sector_height = side;
    generation.system_count = count;
    input
}

/// The sector [`project`] generates — build it outside the timed loop so
/// generation cost stays out of the measurement.
pub fn fixture(side: u32, count: usize) -> GeneratedSector {
    generate_sector(project(side, count)).expect("generate fixture")
}
