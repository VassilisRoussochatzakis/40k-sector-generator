//! docs/OPTIMIZE.txt G2 (rust_sectorforge_existing_app_optimization_prompt_v4 §3A):
//! prove the CLI binary and the in-process library path (used by the GUI) emit
//! the same sector for the same project + seed.
//!
//! The GUI calls `sectorforge::generate_sector` directly — there is no
//! GUI-only generation code path — so parity is verified by:
//!   1. invoking the `sectorforge` CLI binary against the bundled
//!      `examples/m42_project` fixture,
//!   2. reading the CLI's `sector.json` back through
//!      [`sectorforge::load_sector_json`],
//!   3. running `generate_sector` in-process on the same project,
//!   4. asserting structural equality on a stable JSON canonicalisation.
//!
//! If the test ever fails it means the CLI grew an output-affecting code path
//! the GUI / library does not exercise (or vice-versa), which is a correctness
//! regression per §1 of the optimisation spec.

use std::process::Command;

use camino::Utf8PathBuf;

use crate::shared::{bin, fixture_dir};

#[test]
fn cli_and_library_produce_identical_sector_json() {
    let project = fixture_dir();

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8 tmp");

    // 1. CLI path
    let status = Command::new(bin())
        .args([
            "generate",
            "--project",
            project.as_str(),
            "--out",
            out_dir.as_str(),
            "--allow-warnings",
        ])
        .status()
        .expect("spawn sectorforge CLI");
    assert!(status.success(), "CLI generate exited non-zero: {status:?}");

    let cli_sector =
        sectorforge::load_sector_json(out_dir.join("sector.json")).expect("load CLI sector.json");

    // 2. Library / GUI path
    let project_input = sectorforge::load_project(&project).expect("load_project");
    let lib_sector = sectorforge::generate_sector(project_input).expect("generate_sector");

    // 3. Compare canonical JSON. `to_string` (not pretty) is deterministic in
    // serde_json regardless of whitespace strategy, so any structural drift
    // surfaces here.
    let cli_json = serde_json::to_string(&cli_sector).expect("serialise cli sector");
    let lib_json = serde_json::to_string(&lib_sector).expect("serialise lib sector");
    assert_eq!(
        cli_json, lib_json,
        "CLI and library generation paths must produce byte-identical sector JSON \
         for the same project + seed (project = {project})"
    );

    // 4. Spot-check a few invariants in case JSON equality ever short-circuits
    // (e.g. NaN comparisons). These are cheap and document the contract.
    assert_eq!(
        cli_sector.seed.as_ref(),
        lib_sector.seed.as_ref(),
        "seed mismatch"
    );
    assert_eq!(
        cli_sector.systems.len(),
        lib_sector.systems.len(),
        "system count mismatch"
    );
    assert_eq!(
        cli_sector.routes.len(),
        lib_sector.routes.len(),
        "route count mismatch"
    );
    assert_eq!(
        cli_sector.manifest.input_digests, lib_sector.manifest.input_digests,
        "input digests must match — same project ⇒ same digests"
    );
}

/// TF-T-11: `validate` parity — the CLI's `--json` output should match the
/// in-process `validate_project` report byte-for-byte.
#[test]
fn cli_and_library_validate_produce_identical_report() {
    let project = fixture_dir();

    let cli_out = Command::new(bin())
        .args(["validate", "--project", project.as_str(), "--json"])
        .output()
        .expect("spawn sectorforge CLI validate");
    assert!(
        cli_out.status.success(),
        "CLI validate exited non-zero: {:?}\nstderr: {}",
        cli_out.status,
        String::from_utf8_lossy(&cli_out.stderr)
    );
    let cli_value: serde_json::Value =
        serde_json::from_slice(&cli_out.stdout).expect("parse CLI validation JSON");

    let project_input = sectorforge::load_project(&project).expect("load_project");
    let lib_report = sectorforge::validate_project(&project_input).expect("validate_project");
    let lib_value: serde_json::Value = serde_json::to_value(&lib_report).expect("ser lib report");

    assert_eq!(
        cli_value, lib_value,
        "CLI and library validate paths must produce identical reports"
    );
    assert_eq!(
        cli_value.get("ok").and_then(|v| v.as_bool()),
        Some(lib_report.ok),
        "ok flag mismatch"
    );
}

/// TF-T-11: `analyze` parity — the CLI's `--json` output (from the same
/// sector that `generate_sector` would produce) should match a fresh
/// in-process `analyze` of that sector.
#[test]
fn cli_and_library_analyze_produce_identical_json() {
    let project = fixture_dir();

    let cli_out = Command::new(bin())
        .args(["analyze", "--project", project.as_str(), "--json"])
        .output()
        .expect("spawn sectorforge CLI analyze");
    assert!(
        cli_out.status.success(),
        "CLI analyze exited non-zero: {:?}\nstderr: {}",
        cli_out.status,
        String::from_utf8_lossy(&cli_out.stderr)
    );
    let cli_analysis: sectorforge::analytics::SectorAnalysis =
        serde_json::from_slice(&cli_out.stdout).expect("parse CLI analyze JSON");

    let project_input = sectorforge::load_project(&project).expect("load_project");
    let lib_sector = sectorforge::generate_sector(project_input).expect("generate_sector");
    let lib_analysis = sectorforge::analytics::analyze(&lib_sector);

    let cli_json = serde_json::to_string(&cli_analysis).expect("ser cli analysis");
    let lib_json = serde_json::to_string(&lib_analysis).expect("ser lib analysis");
    assert_eq!(
        cli_json, lib_json,
        "CLI and library analyze paths must produce byte-identical JSON"
    );
}
