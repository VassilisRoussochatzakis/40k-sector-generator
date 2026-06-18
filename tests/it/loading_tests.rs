//! Project-loading guards: `read_relative` path-escape security (driven via
//! `load_project`) and the large `big_test` / `big_sparse_test` example parse.

use camino::Utf8PathBuf;
use sectorforge::{load_project, ProjectInput, SectorError};

use crate::shared::{copy_dir_all, fixture_dir};

/// Copy the canonical m42 fixture into a fresh tempdir, inject a chosen
/// `theme_file` value under the existing `[outputs.bitmap]` table, and return
/// the load result. `theme_file` is read by `read_relative` (input.rs) *before*
/// the worlds load, so an escaping value trips the guard first.
fn load_m42_with_bitmap_theme_file(
    tmp: &tempfile::TempDir,
    theme_file_value: &str,
) -> Result<ProjectInput, SectorError> {
    let dest = Utf8PathBuf::from_path_buf(tmp.path().join("proj")).expect("utf8 tempdir path");
    copy_dir_all(fixture_dir().as_std_path(), dest.as_std_path());

    let toml_path = dest.join("sectorforge.toml");
    let original = std::fs::read_to_string(toml_path.as_std_path()).unwrap();
    // m42 already carries `[outputs.bitmap]` (verified). Inject the field right
    // after that header so it lands inside the bitmap table, not a later one.
    let injected = original.replace(
        "[outputs.bitmap]\n",
        &format!("[outputs.bitmap]\ntheme_file = \"{theme_file_value}\"\n"),
    );
    assert_ne!(
        injected, original,
        "expected to find [outputs.bitmap] header to inject into"
    );
    std::fs::write(toml_path.as_std_path(), injected).unwrap();

    load_project(&dest)
}

#[test]
fn read_relative_rejects_parent_dir_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load_m42_with_bitmap_theme_file(&tmp, "../escape.toml")
        .expect_err("a `..` theme_file path must be rejected by the escape guard");
    assert!(
        matches!(err, SectorError::ConfigParse { .. }),
        "expected ConfigParse, got: {err:?}"
    );
    let msg = format!("{err}");
    assert!(
        msg.contains("escapes project root"),
        "error message should name the escape: {msg}"
    );
    assert!(
        msg.contains("../escape.toml"),
        "error message should echo the offending path: {msg}"
    );
}

#[test]
fn read_relative_rejects_absolute_path_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load_m42_with_bitmap_theme_file(&tmp, "/abs/escape.toml")
        .expect_err("an absolute theme_file path must be rejected by the escape guard");
    assert!(
        matches!(err, SectorError::ConfigParse { .. }),
        "expected ConfigParse, got: {err:?}"
    );
    let msg = format!("{err}");
    assert!(
        msg.contains("escapes project root"),
        "error message should name the escape: {msg}"
    );
}

#[test]
fn read_relative_allows_benign_nested_paths() {
    // The unmodified m42 fixture loads every `[inputs]` file via nested relative
    // paths (e.g. `data/names/system_names.toml`) — none contain `..` and none
    // are absolute, so they pass the guard. A clean load is the positive arm.
    assert!(
        load_project(fixture_dir()).is_ok(),
        "benign nested relative input paths must load cleanly"
    );
}

fn example_dir(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

#[test]
fn big_examples_load_and_are_square() {
    for name in ["big_test", "big_sparse_test"] {
        let input = load_project(example_dir(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let g = &input.config.generation;
        assert_eq!(g.sector_width, g.sector_height, "{name}: not square");
        assert!(g.system_count > 0, "{name}: zero systems");
        assert!(
            !input.catalogs.world_rows.is_empty(),
            "{name}: empty world rows"
        );
        assert!(
            !input.catalogs.factions.is_empty(),
            "{name}: empty factions"
        );
    }
}

#[test]
fn big_examples_have_expected_dims() {
    // Stronger, concrete assertions on the checked-in example configs.
    let big = load_project(example_dir("big_test")).unwrap();
    assert_eq!(big.config.generation.sector_width, 32);
    assert_eq!(big.config.generation.sector_height, 32);
    assert_eq!(big.config.generation.system_count, 200);

    let sparse = load_project(example_dir("big_sparse_test")).unwrap();
    assert_eq!(sparse.config.generation.sector_width, 32);
    assert_eq!(sparse.config.generation.sector_height, 32);
    assert_eq!(sparse.config.generation.system_count, 80);
}
