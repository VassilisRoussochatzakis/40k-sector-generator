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
    // §TF-P-1: catalogs live behind `Arc`; mutating a single field requires
    // either `Arc::make_mut` (clones-on-write) or rebuilding the `Arc`. The
    // test owns the sole reference at this point, so `make_mut` is cheap.
    std::sync::Arc::make_mut(&mut input.catalogs)
        .factions
        .clear();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(report.ok);
}

#[test]
fn majority_excluded_rows_is_severe_error() {
    // Regression guard: a column-truncated workbook (most rows missing fields)
    // must fail loudly, not silently generate from the surviving fraction.
    // See WB_EXCLUDED_ROWS_SEVERE in src/validate/validation.rs.
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();

    let cat = std::sync::Arc::make_mut(&mut input.catalogs);
    let valid = cat.world_rows[0].clone();
    cat.world_rows.clear();
    cat.world_rows.push(valid.clone()); // 1 usable row
    for _ in 0..4 {
        // Drop the first required field so the row is excluded by the pool
        // builder's `first_missing_field` check.
        let mut stub = valid.clone();
        stub.star_colour = None;
        cat.world_rows.push(stub);
    }

    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok, "4 excluded vs 1 usable must fail validation");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.code == "WB_EXCLUDED_ROWS_SEVERE"),
        "expected WB_EXCLUDED_ROWS_SEVERE, got {:?}",
        report.errors
    );
}

#[test]
fn oversized_sector_dims_fail_validation() {
    // Regression guard: a hand-edited `sectorforge.toml` with absurd dimensions
    // must be rejected before generation allocates a multi-gigabyte cell grid.
    // See GEN_SECTOR_TOO_LARGE / MAX_SECTOR_DIM in src/validate/validation.rs.
    let project = manifest_dir().join("examples/m42_project");
    let mut input = sectorforge::load_project(project).unwrap();
    input.config.generation.sector_width = 2000;
    input.config.generation.sector_height = 2000;
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(!report.ok, "2000x2000 must fail validation");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.code == "GEN_SECTOR_TOO_LARGE"),
        "expected GEN_SECTOR_TOO_LARGE, got {:?}",
        report.errors
    );
}

fn manifest_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}
