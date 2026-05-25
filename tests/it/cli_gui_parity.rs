//! OPTIMIZE.txt G2 (rust_sectorforge_existing_app_optimization_prompt_v4 §3A):
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

#[test]
fn cli_and_library_produce_identical_sector_json() {
    let bin = env!("CARGO_BIN_EXE_sectorforge");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project = Utf8PathBuf::from(manifest_dir).join("examples/m42_project");

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8 tmp");

    // 1. CLI path
    let status = Command::new(bin)
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
