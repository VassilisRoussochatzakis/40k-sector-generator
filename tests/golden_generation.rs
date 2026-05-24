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
    sectorforge::export_sector(&sector, &output_cfg, &tmp_path).unwrap();
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

    sectorforge::export_sector(&sector, &output_cfg, &tmp_path).unwrap();

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

    sectorforge::export_sector(&sector, &output_cfg, &tmp_path).unwrap();

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
