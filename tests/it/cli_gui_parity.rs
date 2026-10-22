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

use crate::shared::{fixture_dir, run_cli_ok, tempdir_utf8};

#[test]
fn cli_and_library_produce_identical_sector_json() {
    let project = fixture_dir();

    let (_tmp, out_dir) = tempdir_utf8();

    // 1. CLI path
    run_cli_ok(&[
        "generate",
        "--project",
        project.as_str(),
        "--out",
        out_dir.as_str(),
        "--allow-warnings",
    ]);

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
}

/// TF-T-11: `validate` parity — the CLI's `--json` output should match the
/// in-process `validate_project` report byte-for-byte.
#[test]
fn cli_and_library_validate_produce_identical_report() {
    let project = fixture_dir();

    let cli_out = run_cli_ok(&["validate", "--project", project.as_str(), "--json"]);
    let cli_value: serde_json::Value =
        serde_json::from_str(&cli_out).expect("parse CLI validation JSON");

    let project_input = sectorforge::load_project(&project).expect("load_project");
    let lib_report = sectorforge::validate_project(&project_input).expect("validate_project");
    let lib_value: serde_json::Value = serde_json::to_value(&lib_report).expect("ser lib report");

    assert_eq!(
        cli_value, lib_value,
        "CLI and library validate paths must produce identical reports"
    );
}

/// TF-T-11: `analyze` parity — the CLI's `--json` output (from the same
/// sector that `generate_sector` would produce) should match a fresh
/// in-process `analyze` of that sector.
#[test]
fn cli_and_library_analyze_produce_identical_json() {
    let project = fixture_dir();

    let cli_out = run_cli_ok(&["analyze", "--project", project.as_str(), "--json"]);
    let cli_analysis: sectorforge::analytics::SectorAnalysis =
        serde_json::from_str(&cli_out).expect("parse CLI analyze JSON");

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
