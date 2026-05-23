//! Integration tests: full project → generate → export → reload.

use std::{collections::BTreeMap, fs};

use camino::Utf8PathBuf;

#[test]
fn generate_fixture_project_succeeds() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).expect("load_project");
    let report = sectorforge::validate_project(&input).expect("validate");
    assert!(report.ok, "validation failed: {:?}", report.errors);

    let sector = sectorforge::generate_sector(input).expect("generate");
    assert!(!sector.systems.is_empty());
    assert!(sector.manifest.world_count > 0);
}

#[test]
fn generate_same_seed_same_output() {
    let project = fixture_project();
    let a = sectorforge::generate_sector(sectorforge::load_project(&project).unwrap()).unwrap();
    let b = sectorforge::generate_sector(sectorforge::load_project(&project).unwrap()).unwrap();
    let a_json = serde_json::to_string(&a).unwrap();
    let b_json = serde_json::to_string(&b).unwrap();
    assert_eq!(
        a_json, b_json,
        "byte-stable output expected for identical seed"
    );
}

#[test]
fn generate_different_seed_different_output() {
    let project = fixture_project();
    let mut input1 = sectorforge::load_project(&project).unwrap();
    let mut input2 = sectorforge::load_project(&project).unwrap();
    input1.config.generation.seed = "seed-A".to_string();
    input2.config.generation.seed = "seed-B".to_string();
    let a = sectorforge::generate_sector(input1).unwrap();
    let b = sectorforge::generate_sector(input2).unwrap();
    assert_ne!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "different seeds should change the output"
    );
}

#[test]
fn validate_fixture_project_succeeds() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let report = sectorforge::validate_project(&input).unwrap();
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
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let output_cfg = input.config.outputs.clone();
    let sector = sectorforge::generate_sector(input).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::export_sector(&sector, &output_cfg, &tmp_path).unwrap();
    assert!(tmp_path.join("sector.json").exists());
    assert!(tmp_path.join("sector.md").exists());
    assert!(tmp_path.join("manifest.json").exists());
    assert!(!tmp_path.join("systems").join("sys-0001.json").exists());
}

#[test]
fn export_can_opt_in_to_per_system_json() {
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let mut output_cfg = input.config.outputs.clone();
    output_cfg.write_per_system_files = true;
    let sector = sectorforge::generate_sector(input).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();

    sectorforge::export_sector(&sector, &output_cfg, &tmp_path).unwrap();

    assert!(tmp_path.join("sector.json").exists());
    assert!(tmp_path.join("systems").join("sys-0001.json").exists());
}

#[test]
fn export_removes_stale_per_system_json_when_disabled() {
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let mut output_cfg = input.config.outputs.clone();
    output_cfg.formats = vec![sectorforge::config::OutputFormat::Json];
    output_cfg.write_manifest = false;
    output_cfg.write_per_system_files = false;
    let sector = sectorforge::generate_sector(input).unwrap();
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
}

#[test]
fn manifest_records_seed_and_input_digests() {
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
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
    let project = fixture_project();
    let input = sectorforge::load_project(&project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    for r in &sector.routes {
        assert!(
            r.from_system_id <= r.to_system_id,
            "route from <= to: {r:?}"
        );
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn fixture_project() -> Utf8PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(manifest_dir).join("examples/m42_project")
}

fn fixture_world_data_dir() -> Utf8PathBuf {
    fixture_project().join("data/worlds")
}
