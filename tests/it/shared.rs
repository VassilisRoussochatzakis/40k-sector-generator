//! Shared test fixtures used across the integration suite.
//!
//! [`fixture_sector`] memoises a single canonical `m42_project` generation via
//! a process-wide `OnceLock`. Tests that exercise the same fixture seed reuse
//! the result instead of regenerating per-test (~20-25 redundant generations
//! saved per run).

use std::path::{Path, PathBuf};
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

/// Generate a fresh sector from `seed`, independent of the memoised
/// [`fixture_sector`], so a proptest genuinely exercises seed variance rather
/// than re-deriving on one cached sector. Shared by the `economy`/`relations`/
/// `personae`/`hooks` derivation suites (TEST-001 G1).
pub fn gen_sector(seed: &str) -> GeneratedSector {
    let mut input = load_project(fixture_dir()).expect("load fixture");
    input.config.generation.seed = seed.to_string();
    generate_sector(input).expect("generate fixture")
}

/// Recursively copy the directory tree at `src` into `dst` (both real
/// filesystem paths). Shared by the project-loading and CLI-behaviour suites,
/// both of which copy the nested m42 fixture into a tempdir.
pub fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in std::fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_all(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy file");
        }
    }
}

/// Self-blessing blake3 pin against `tests/goldens/<file_name>`. With `env` set
/// in the environment, (re)writes the golden and returns; otherwise asserts the
/// freshly-computed hash matches the committed one. Shared by every byte-stable
/// export golden (`golden_png`, `svg_export_tests`, `export_byte_goldens`).
pub fn assert_blake3_golden(file_name: &str, env: &str, bytes: &[u8]) {
    let hash = blake3::hash(bytes).to_hex().to_string();
    let pin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(file_name);
    if std::env::var_os(env).is_some() {
        std::fs::create_dir_all(pin.parent().unwrap()).unwrap();
        std::fs::write(&pin, format!("{hash}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&pin).unwrap_or_else(|_| {
        panic!("missing pinned hash {file_name}; bless with `{env}=1 cargo test --test it -- export_byte_goldens`")
    });
    assert_eq!(
        expected.trim(),
        hash,
        "{file_name} drifted from the pinned hash ({} bytes); if intentional, rerun with `{env}=1`",
        bytes.len(),
    );
}
