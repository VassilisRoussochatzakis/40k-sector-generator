//! Behavioural CLI tests driving the built `sectorforge` binary as a
//! subprocess. Unlike `cli_smoke.rs` (which only proves each subcommand is
//! reachable + `--help` works), these assert real *behaviour*: exit codes,
//! stdout/stderr substrings, and which output files a run did or did not write.
//!
//! Covered:
//!   * `generate-all` — empty dir, 2/2 success, 1-bad+1-good partial failure,
//!     nonexistent dir (exit 74), only-direct-children counting.
//!   * `generate` — bad `--exclude`/`--formats` tokens → exit 78, no output.
//!   * `validate` / `validate-sector` — `--strict` warnings, error reports, and
//!     a corrupted `sector.json` (all → exit 1).
//!
//! Everything goes through the binary because the CLI runners
//! (`run_generate_all`, etc.) are declared inside the *binary* crate, not the
//! library, so they are not reachable from `tests/it`'s library link.

use std::path::Path;
use std::process::Command;

use camino::Utf8PathBuf;

/// Absolute path to the freshly built `sectorforge` binary.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_sectorforge")
}

/// The checked-in `examples/m42_project` — a valid project that validates with
/// 0 warnings and generates cleanly. Used as the cheapest valid fixture.
fn m42_source() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

/// Recursively copy a directory tree (no external dep; m42 has nested data
/// subdirs so this must recurse).
fn copy_dir_all(src: &Path, dst: &Path) {
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

/// Copy `examples/m42_project` into `<base>/<name>` and return its path. Any
/// pre-existing `out/` from the checked-in tree is removed so that the later
/// existence of `out/sector.json` proves *this* run wrote it.
fn copy_m42(base: &Path, name: &str) -> Utf8PathBuf {
    let dst = base.join(name);
    copy_dir_all(m42_source().as_std_path(), &dst);
    let _ = std::fs::remove_dir_all(dst.join("out"));
    Utf8PathBuf::from_path_buf(dst).expect("utf8 project path")
}

/// Replace `from` with `to` exactly once inside a copied project's
/// `sectorforge.toml`, asserting the substring was actually present.
fn edit_toml(project: &Utf8PathBuf, from: &str, to: &str) {
    let path = project.join("sectorforge.toml");
    let text = std::fs::read_to_string(&path).expect("read sectorforge.toml");
    assert!(
        text.contains(from),
        "fixture sectorforge.toml missing expected substring {from:?}"
    );
    let edited = text.replace(from, to);
    std::fs::write(&path, edited).expect("write sectorforge.toml");
}

/// Run the binary with `args`, returning `(exit_code, stdout, stderr)`.
fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `sectorforge {args:?}`: {e}"));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ── Gap 4: generate-all ────────────────────────────────────────────────────

#[test]
fn generate_all_empty_dir_succeeds_with_notice() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).expect("mk empty dir");

    let (code, _stdout, stderr) = run(&["generate-all", "--dir", empty.to_str().unwrap()]);

    assert_eq!(code, Some(0), "empty dir should succeed; stderr: {stderr}");
    assert!(
        stderr.contains("no sector projects (sectorforge.toml) found under "),
        "stderr missing empty-dir notice: {stderr}"
    );
}

#[test]
fn generate_all_two_valid_projects_report_two_of_two() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj_a = copy_m42(tmp.path(), "projA");
    let proj_b = copy_m42(tmp.path(), "projB");

    let (code, stdout, stderr) = run(&["generate-all", "--dir", tmp.path().to_str().unwrap()]);

    assert_eq!(code, Some(0), "both should generate; stderr: {stderr}");
    assert!(
        stdout.contains("generate-all: 2/2 project(s) generated"),
        "stdout missing 2/2 summary: {stdout}"
    );
    assert!(
        stderr.contains("generate-all: 2 project(s) under "),
        "stderr missing project count: {stderr}"
    );
    assert!(
        proj_a.join("out/sector.json").exists(),
        "projA out/sector.json was not written"
    );
    assert!(
        proj_b.join("out/sector.json").exists(),
        "projB out/sector.json was not written"
    );
}

#[test]
fn generate_all_partial_failure_reports_one_of_two_and_fails() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj_a = copy_m42(tmp.path(), "projA"); // good
    let proj_b = copy_m42(tmp.path(), "projB"); // broken below

    // Point projB at a missing world_data_dir so load_project errors out.
    edit_toml(
        &proj_b,
        "world_data_dir = \"data/worlds\"",
        "world_data_dir = \"data/DOES_NOT_EXIST\"",
    );

    let (code, stdout, stderr) = run(&["generate-all", "--dir", tmp.path().to_str().unwrap()]);

    // One project survived, one failed → ExitCode::FAILURE (1 on Unix).
    assert_eq!(code, Some(1), "partial failure should exit 1; stderr: {stderr}");
    assert!(
        stdout.contains("generate-all: 1/2 project(s) generated"),
        "stdout missing 1/2 summary: {stdout}"
    );
    assert!(
        stderr.contains("FAILED"),
        "stderr missing per-project FAILED line: {stderr}"
    );
    assert!(
        stderr.contains("failed:"),
        "stderr missing trailing failed summary: {stderr}"
    );
    assert!(
        proj_a.join("out/sector.json").exists(),
        "survivor projA should have written output"
    );
    assert!(
        !proj_b.join("out/sector.json").exists(),
        "failed projB must not have written output"
    );
}

#[test]
fn generate_all_nonexistent_dir_is_io_error() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let missing = tmp.path().join("does_not_exist");
    // Intentionally not created.

    let (code, _stdout, stderr) = run(&["generate-all", "--dir", missing.to_str().unwrap()]);

    // read_dir on a missing dir -> SectorError::io -> from_error -> 74.
    assert_eq!(code, Some(74), "missing dir should map to 74; stderr: {stderr}");
    assert!(
        stderr.contains("error: I/O error at "),
        "stderr missing I/O error line: {stderr}"
    );
}

#[test]
fn generate_all_counts_only_direct_children() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj_a = copy_m42(tmp.path(), "projA");

    // A nested project under projA must be invisible to the (non-recursive)
    // walker, which only scans immediate subdirectories of --dir.
    let nested = proj_a.join("sub");
    std::fs::create_dir_all(nested.as_std_path()).expect("mk nested dir");
    std::fs::write(nested.join("sectorforge.toml").as_std_path(), b"# decoy\n")
        .expect("write nested toml");

    let (code, stdout, stderr) = run(&["generate-all", "--dir", tmp.path().to_str().unwrap()]);

    assert_eq!(code, Some(0), "single valid child should succeed; stderr: {stderr}");
    assert!(
        stdout.contains("generate-all: 1/1 project(s) generated"),
        "nested project should be ignored (expected 1/1): {stdout}"
    );
}

// ── Gap 6: generate with bad format flags ──────────────────────────────────

#[test]
fn generate_exclude_json_is_rejected_and_writes_nothing() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj = copy_m42(tmp.path(), "proj");
    let genout = tmp.path().join("genout");

    let (code, _stdout, stderr) = run(&[
        "generate",
        "--project",
        proj.as_str(),
        "--out",
        genout.to_str().unwrap(),
        "--exclude",
        "json",
    ]);

    // InvalidConfig -> 78, fired in resolve_formats before any export.
    assert_eq!(code, Some(78), "excluding json should exit 78; stderr: {stderr}");
    assert!(
        stderr.contains("json cannot be excluded"),
        "stderr missing json-exclusion message: {stderr}"
    );
    assert!(
        !genout.join("sector.json").exists(),
        "no output should be written when --exclude json errors"
    );
}

#[test]
fn generate_unknown_format_token_is_rejected() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj = copy_m42(tmp.path(), "proj");
    let genout = tmp.path().join("genout2");

    let (code, _stdout, stderr) = run(&[
        "generate",
        "--project",
        proj.as_str(),
        "--out",
        genout.to_str().unwrap(),
        "--formats",
        "bogus",
    ]);

    assert_eq!(code, Some(78), "bad --formats token should exit 78; stderr: {stderr}");
    assert!(
        stderr.contains("unknown --formats token 'bogus'"),
        "stderr missing unknown-token message: {stderr}"
    );
}

// ── Gap 7: validate / validate-sector exit codes ───────────────────────────

#[test]
fn validate_strict_fails_only_with_warnings() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj = copy_m42(tmp.path(), "warnproj");

    // min_worlds_per_system = 0 fires the GEN_MIN_WORLDS_ZERO *warning* while
    // keeping the report ok (m42 itself produces 0 warnings).
    edit_toml(
        &proj,
        "min_worlds_per_system = 2",
        "min_worlds_per_system = 0",
    );

    // Non-strict: a warning alone is not blocking → success.
    let (code, stdout, stderr) = run(&["validate", "--project", proj.as_str()]);
    assert_eq!(
        code,
        Some(0),
        "warning without --strict should succeed; stderr: {stderr}"
    );
    assert!(
        stdout.contains("GEN_MIN_WORLDS_ZERO"),
        "fixture should actually warn (GEN_MIN_WORLDS_ZERO): {stdout}"
    );

    // Strict: the same warning is now blocking → exit 1.
    let (code, _stdout, stderr) = run(&["validate", "--project", proj.as_str(), "--strict"]);
    assert_eq!(
        code,
        Some(1),
        "--strict with a warning should exit 1; stderr: {stderr}"
    );
}

#[test]
fn validate_failing_report_exits_one() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj = copy_m42(tmp.path(), "bigproj");

    // 2000 > MAX_SECTOR_DIM (1024) fires GEN_SECTOR_TOO_LARGE (an error, not a
    // warning). Kept square (2000 x 2000) to honour the square invariant.
    edit_toml(&proj, "sector_width = 10", "sector_width = 2000");
    edit_toml(&proj, "sector_height = 10", "sector_height = 2000");

    let (code, _stdout, stderr) = run(&["validate", "--project", proj.as_str()]);
    assert_eq!(
        code,
        Some(1),
        "a failing validation report should exit 1; stderr: {stderr}"
    );
}

#[test]
fn validate_sector_invalid_json_exits_one() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let proj = copy_m42(tmp.path(), "proj");
    let vsout = tmp.path().join("vsout");

    // 1. Generate a valid sector.json.
    let (code, _stdout, stderr) = run(&[
        "generate",
        "--project",
        proj.as_str(),
        "--out",
        vsout.to_str().unwrap(),
        "--formats",
        "json",
    ]);
    assert_eq!(code, Some(0), "fixture generation failed; stderr: {stderr}");
    let json_path = vsout.join("sector.json");
    assert!(json_path.exists(), "generation did not write sector.json");

    // Control: the valid sector passes invariants → exit 0, "OK".
    let (code, stdout, stderr) =
        run(&["validate-sector", "--sector", json_path.to_str().unwrap()]);
    assert_eq!(code, Some(0), "valid sector should pass; stderr: {stderr}");
    assert!(
        stdout.contains("Sector invariants: OK"),
        "valid sector stdout missing OK line: {stdout}"
    );

    // 2. Corrupt manifest.system_count so it disagrees with systems.len() →
    //    MANIFEST_SYSTEM_COUNT_MISMATCH → report.ok == false.
    let text = std::fs::read_to_string(&json_path).expect("read sector.json");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("parse sector.json");
    value["manifest"]["system_count"] = serde_json::json!(99999);
    std::fs::write(
        &json_path,
        serde_json::to_string(&value).expect("reserialize"),
    )
    .expect("write corrupted sector.json");

    // run_validate_sector returns Ok(ExitCode::from(1)) directly (bypassing
    // from_error) when the report is not ok.
    let (code, stdout, stderr) =
        run(&["validate-sector", "--sector", json_path.to_str().unwrap()]);
    assert_eq!(
        code,
        Some(1),
        "corrupted sector should exit 1; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Sector invariants: FAILED")
            || stdout.contains("MANIFEST_SYSTEM_COUNT_MISMATCH"),
        "corrupted sector stdout missing failure marker: {stdout}"
    );
}
