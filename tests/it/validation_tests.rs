//! Validation behavior in adverse cases.

use camino::Utf8PathBuf;

#[test]
fn missing_world_data_fails_validation() {
    // Build a temp project that points at a nonexistent world-data dir.
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    std::fs::write(
        root.join("sectorforge.toml"),
        r#"
[project]
id = "x"
title = "x"

[inputs]
world_data_dir = "missing_dir"

[generation]
seed = "s"
sector_width = 4
sector_height = 4
system_count = 2
min_worlds_per_system = 1
max_worlds_per_system = 2

[outputs]
directory = "out"
formats = ["json"]
"#,
    )
    .unwrap();
    let result = sectorforge::load_project(&root);
    assert!(
        result.is_err(),
        "load should fail when world data dir is missing"
    );
}

#[test]
fn no_factions_is_ok() {
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();
    input.factions.clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
}

fn manifest_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}
