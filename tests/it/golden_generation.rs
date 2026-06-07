//! Integration tests: full project → generate → export → reload.

use std::{collections::BTreeMap, fs, sync::OnceLock};

use camino::Utf8PathBuf;
use sectorforge::{
    config::{OutputConfig, OutputFormat},
    sector_model::GenerationManifest,
    GeneratedSector, GeneratedSystem, ProjectInput,
};

#[test]
fn generate_fixture_project_succeeds() {
    let report = sectorforge::validate_project(fixture_input()).expect("validate");
    assert!(report.ok, "validation failed: {:?}", report.errors);

    let sector = fixture_sector();
    assert!(!sector.systems.is_empty());
    assert_eq!(sector.manifest.system_count, sector.systems.len());
    assert!(sector.manifest.world_count > 0);
}

#[test]
fn generate_same_seed_same_output() {
    let repeat = sectorforge::generate_sector(fixture_input().clone()).unwrap();
    let repeat_json = serde_json::to_string(&repeat).unwrap();
    assert_eq!(
        fixture_sector_json(),
        repeat_json,
        "byte-stable output expected for identical seed"
    );
}

#[test]
fn generate_different_seed_different_output() {
    let mut input = fixture_input().clone();
    input.config.generation.seed = "seed-B".to_string();
    let alternate = sectorforge::generate_sector(input).unwrap();
    let alternate_json = serde_json::to_string(&alternate).unwrap();
    assert_ne!(
        fixture_sector_json(),
        alternate_json,
        "different seeds should change the output"
    );
}

#[test]
fn validate_fixture_project_succeeds() {
    let report = sectorforge::validate_project(fixture_input()).unwrap();
    assert!(report.ok);
    assert!(report.world_workbook.usable_candidate_count > 0);
}

#[test]
fn inspect_worlds_reports_candidate_count() {
    let data_dir = fixture_world_data_dir();
    let stats = sectorforge::inspect_world_workbook(data_dir.as_str()).unwrap();
    assert!(stats.generator_rows > 0);
    assert!(stats.usable_candidates > 0);
    assert!(stats.key_table_counts.contains_key("star_colours"));
}

#[test]
fn export_writes_all_expected_files() {
    let output_cfg = text_export_config();
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::export_sector(sector, &output_cfg, &tmp_path).unwrap();
    assert!(tmp_path.join("sector.json").exists());
    assert!(tmp_path.join("sector.md").exists());
    assert!(tmp_path.join("manifest.json").exists());
    assert!(!tmp_path.join("systems").join("sys-0001.json").exists());
    assert_exported_sector_matches(&tmp_path, sector);
    assert_exported_manifest_matches(&tmp_path, sector);
}

#[test]
fn export_can_opt_in_to_per_system_json() {
    let output_cfg = json_export_config(true);
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();

    sectorforge::export_sector(sector, &output_cfg, &tmp_path).unwrap();

    assert!(tmp_path.join("sector.json").exists());
    assert!(tmp_path.join("systems").join("sys-0001.json").exists());
    assert_exported_sector_matches(&tmp_path, sector);
    assert_exported_system_matches(&tmp_path, &sector.systems[0]);
}

#[test]
fn export_removes_stale_per_system_json_when_disabled() {
    let output_cfg = json_export_config(false);
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let systems_dir = tmp_path.join("systems");
    fs::create_dir_all(&systems_dir).unwrap();
    let stale_json = systems_dir.join(format!("{}.json", sector.systems[0].id));
    let keep_png = systems_dir.join(format!("{}.png", sector.systems[0].id));
    fs::write(&stale_json, "{}").unwrap();
    fs::write(&keep_png, "not a real png").unwrap();

    sectorforge::export_sector(sector, &output_cfg, &tmp_path).unwrap();

    assert!(tmp_path.join("sector.json").exists());
    assert!(!stale_json.exists());
    assert!(keep_png.exists());
    assert_exported_sector_matches(&tmp_path, sector);
}

#[test]
fn manifest_records_seed_and_input_digests() {
    let sector = fixture_sector();
    assert_eq!(sector.manifest.seed.as_ref(), "m42-default-seed");
    assert!(sector
        .manifest
        .input_digests
        .contains_key("sectorforge.toml"));
    assert!(sector
        .manifest
        .input_digests
        .values()
        .all(|v| v.starts_with("blake3:")));
    let _: &BTreeMap<String, String> = &sector.manifest.input_digests;
}

#[test]
fn route_ids_sort_with_lower_system_first() {
    let sector = fixture_sector();
    for r in &sector.routes {
        assert!(
            r.from_system_id <= r.to_system_id,
            "route from <= to: {r:?}"
        );
    }
}

// GAP 171: when `min_worlds_per_system == max_worlds_per_system`,
// `generate_worlds_for_system` skips `gen_range` and uses the bound directly,
// so every placed system gets exactly that many worlds. Driven through the
// public `generate_sector` entry point because the fn and its `WorldGenParams`
// are `pub(super)` (not reachable from the integration suite). The m42 fixture
// is already square (10×10) with `allow_empty_hexes = true`; we clone it and
// override only min/max so the shared memoized fixture stays untouched.
#[test]
fn worlds_per_system_min_equals_max_is_exact() {
    let mut input = fixture_input().clone();
    input.config.generation.min_worlds_per_system = 3;
    input.config.generation.max_worlds_per_system = 3;
    let sector = sectorforge::generate_sector(input).expect("generate");
    assert!(!sector.systems.is_empty(), "fixture should place systems");
    for sys in &sector.systems {
        assert_eq!(
            sys.worlds.len(),
            3,
            "system {} has {} worlds; min==max==3 must yield exactly 3",
            sys.id,
            sys.worlds.len()
        );
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn fixture_input() -> &'static ProjectInput {
    static INPUT: OnceLock<ProjectInput> = OnceLock::new();
    INPUT.get_or_init(|| sectorforge::load_project(fixture_project()).expect("load_project"))
}

fn fixture_sector() -> &'static GeneratedSector {
    static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
    SECTOR.get_or_init(|| {
        let sector = sectorforge::generate_sector(fixture_input().clone()).expect("generate");
        let report = sectorforge::validate_sector(&sector);
        assert!(
            report.ok,
            "generated fixture violates invariants: {:?}",
            report.violations
        );
        sector
    })
}

fn fixture_sector_json() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(|| serde_json::to_string(fixture_sector()).expect("serialize fixture sector"))
        .as_str()
}

fn json_export_config(write_per_system_files: bool) -> OutputConfig {
    let mut cfg = fixture_input().config.outputs.clone();
    cfg.formats = vec![OutputFormat::Json];
    cfg.write_manifest = false;
    cfg.write_per_system_files = write_per_system_files;
    cfg.bitmap.render_systems = false;
    cfg
}

fn text_export_config() -> OutputConfig {
    let mut cfg = json_export_config(false);
    cfg.formats.push(OutputFormat::Markdown);
    cfg.write_manifest = true;
    cfg
}

fn single_format_config(format: OutputFormat) -> OutputConfig {
    // Inherits render_systems=false so the bitmap arm stays a single map.
    let mut cfg = json_export_config(false);
    cfg.formats = vec![format];
    cfg.write_manifest = false;
    cfg
}

// G4: the Bitmap and Html arms of `export_sector` dispatch were never exercised
// by the integration suite (only Json + Markdown). Smoke-test that each writes
// its artifact; existence + non-empty only, no content pin.
#[test]
fn export_writes_bitmap_png() {
    let cfg = single_format_config(OutputFormat::Bitmap);
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::export_sector(sector, &cfg, &tmp_path).unwrap();
    let png = tmp_path.join("sector.png");
    assert!(png.exists());
    assert!(fs::metadata(&png).unwrap().len() > 0);
}

#[test]
fn export_writes_interactive_html() {
    let cfg = single_format_config(OutputFormat::Html);
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::export_sector(sector, &cfg, &tmp_path).unwrap();
    let html = tmp_path.join("sector.html");
    assert!(html.exists());
    assert!(fs::metadata(&html).unwrap().len() > 0);
}

fn assert_exported_sector_matches(output_dir: &Utf8PathBuf, expected: &GeneratedSector) {
    let exported = sectorforge::load_sector_json(output_dir.join("sector.json")).unwrap();
    assert_eq!(exported.seed.as_ref(), expected.seed.as_ref());
    assert_eq!(exported.systems.len(), expected.systems.len());
    assert_eq!(exported.routes.len(), expected.routes.len());
    assert_eq!(exported.manifest.world_count, expected.manifest.world_count);
}

fn assert_exported_manifest_matches(output_dir: &Utf8PathBuf, expected: &GeneratedSector) {
    let text = fs::read_to_string(output_dir.join("manifest.json")).unwrap();
    let manifest: GenerationManifest = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest.seed.as_ref(), expected.manifest.seed.as_ref());
    assert_eq!(manifest.system_count, expected.systems.len());
    assert_eq!(manifest.world_count, expected.manifest.world_count);
    assert_eq!(manifest.route_count, expected.routes.len());
    assert_eq!(manifest.input_digests, expected.manifest.input_digests);
}

fn assert_exported_system_matches(output_dir: &Utf8PathBuf, expected: &GeneratedSystem) {
    let path = output_dir
        .join("systems")
        .join(format!("{}.json", expected.id));
    let text = fs::read_to_string(path).unwrap();
    let system: GeneratedSystem = serde_json::from_str(&text).unwrap();
    assert_eq!(system.id.as_ref(), expected.id.as_ref());
    assert_eq!(system.worlds.len(), expected.worlds.len());
}

fn fixture_project() -> Utf8PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(manifest_dir).join("examples/m42_project")
}

fn fixture_world_data_dir() -> Utf8PathBuf {
    fixture_project().join("data/worlds")
}

// ── G2 content goldens (IMPROVEMENT_REVIEW G2 / G-S3) ────────────────────────
// Byte-level pin of the exported `sector.json` / `sector.md` for the fixed m42
// fixture + seed. Mirrors `golden_png.rs`, but commits the *full* blessed file
// (not just a blake3 hash) so a drift surfaces the exact renamed field / dropped
// markdown row under `git diff tests/goldens/`. This is the safety net the A–F
// god-file splits (serialisation / markdown rendering) are gated on.
//
// Bless / refresh after an intentional output change:
//   UPDATE_GOLDEN_JSON=1 UPDATE_GOLDEN_MD=1 cargo test --test it -- golden

fn goldens_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// Export the memoized fixture sector to a tempdir and return the raw
/// `(sector.json, sector.md)` bytes-as-text exactly as written to disk.
fn export_fixture_text() -> (String, String) {
    let sector = fixture_sector();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::export_sector(sector, &text_export_config(), &tmp_path).unwrap();
    let json = fs::read_to_string(tmp_path.join("sector.json")).unwrap();
    let md = fs::read_to_string(tmp_path.join("sector.md")).unwrap();
    (json, md)
}

/// Compare `actual` against the committed golden `file_name`. With the `env`
/// gate set, (re)writes the golden and returns. On drift, reports the first
/// differing line rather than dumping the whole multi-KB file.
fn assert_content_golden(file_name: &str, env: &str, actual: &str) {
    let path = goldens_dir().join(file_name);
    if std::env::var_os(env).is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("missing golden {file_name}; bless with `{env}=1 cargo test --test it -- golden`")
    });
    if expected != actual {
        let (line, exp_l, act_l) = first_line_diff(&expected, actual);
        panic!(
            "{file_name} drifted from the committed golden at line {line}:\n  \
             golden: {exp_l:?}\n  actual: {act_l:?}\n(golden {} bytes, actual {} \
             bytes). If intentional, rerun with `{env}=1 cargo test --test it -- \
             golden` and review `git diff tests/goldens/`.",
            expected.len(),
            actual.len(),
        );
    }
}

/// First line (1-based) at which two texts diverge, with both lines. Only
/// called once a byte-level difference is known to exist.
fn first_line_diff(expected: &str, actual: &str) -> (usize, String, String) {
    let mut e = expected.lines();
    let mut a = actual.lines();
    let mut n = 0;
    loop {
        n += 1;
        match (e.next(), a.next()) {
            (Some(el), Some(al)) if el == al => continue,
            (el, al) => {
                return (
                    n,
                    el.unwrap_or("<end of file>").to_string(),
                    al.unwrap_or("<end of file>").to_string(),
                );
            }
        }
    }
}

#[test]
fn sector_json_matches_committed_golden() {
    let (json, _md) = export_fixture_text();
    assert_content_golden("sector_m42_default.json", "UPDATE_GOLDEN_JSON", &json);
}

#[test]
fn sector_md_matches_committed_golden() {
    let (_json, md) = export_fixture_text();
    assert_content_golden("sector_m42_default.md", "UPDATE_GOLDEN_MD", &md);
}
