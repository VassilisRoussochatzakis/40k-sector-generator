//! Shared test fixtures used across the integration suite.
//!
//! [`fixture_sector`] memoises a single canonical `m42_project` generation via
//! a process-wide `OnceLock`. Tests that exercise the same fixture seed reuse
//! the result instead of regenerating per-test (~20-25 redundant generations
//! saved per run).

use std::sync::OnceLock;

use camino::Utf8PathBuf;
use sectorforge::{generate_sector, load_project, GeneratedSector};

pub fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

pub fn fixture_sector() -> &'static GeneratedSector {
    static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
    SECTOR.get_or_init(|| {
        let input = load_project(fixture_dir()).expect("load fixture project");
        generate_sector(input).expect("generate fixture sector")
    })
}
