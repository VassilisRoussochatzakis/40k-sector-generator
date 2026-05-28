//! §8 NEW.md + §9 NEW.md — analytics and presets integration tests.

use camino::Utf8PathBuf;

use crate::shared::fixture_sector;

fn presets_dir() -> Utf8PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(manifest_dir).join("presets")
}

#[test]
fn analyze_real_sector_produces_complete_report() {
    let sector = fixture_sector();
    let analysis = sectorforge::analyze_sector(sector);
    assert_eq!(analysis.sector_id, sector.id);
    assert_eq!(analysis.system_count, sector.systems.len());
    assert_eq!(analysis.route_count, sector.routes.len());
    assert!(
        analysis.faction_count > 0,
        "expected factions in m42 project"
    );
    assert!(
        analysis.connectivity.component_count >= 1,
        "expected at least one connected component"
    );
    let md = sectorforge::render_analysis_markdown(&analysis);
    assert!(md.starts_with("# Sector Analysis"));
    assert!(md.contains("Faction Balance"));
}

#[test]
fn analyze_writers_emit_both_files() {
    let sector = fixture_sector();
    let analysis = sectorforge::analyze_sector(sector);
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    sectorforge::write_analysis(&dir, &analysis).unwrap();
    assert!(dir.join("analysis.md").exists());
    assert!(dir.join("analysis.json").exists());
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("analysis.json").as_path()).unwrap(),
    )
    .unwrap();
    assert_eq!(json["sector_id"].as_str().unwrap(), sector.id.as_ref());
}

#[test]
fn analysis_is_deterministic_for_same_sector() {
    let sector = fixture_sector();
    let a = sectorforge::analyze_sector(sector);
    let b = sectorforge::analyze_sector(sector);
    let a_md = sectorforge::render_analysis_markdown(&a);
    let b_md = sectorforge::render_analysis_markdown(&b);
    assert_eq!(a_md, b_md);
    let a_json = serde_json::to_string_pretty(&a).unwrap();
    let b_json = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(a_json, b_json);
}

#[test]
fn list_presets_returns_starter_bundles() {
    let dir = presets_dir();
    if !dir.exists() {
        eprintln!("presets/ dir missing — skipping");
        return;
    }
    let entries = sectorforge::presets::list(&dir).unwrap();
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"m42-classic"));
    assert!(ids.contains(&"embattled-frontier"));
    assert!(ids.contains(&"dead-sector"));
    assert!(ids.contains(&"mercantile-crossroads"));
    // Internal base must not appear.
    assert!(!ids.contains(&"_base"));
}

#[test]
fn scaffold_starter_preset_produces_loadable_project() {
    let dir = presets_dir();
    if !dir.exists() {
        eprintln!("presets/ dir missing — skipping");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = Utf8PathBuf::from_path_buf(tmp.path().join("scaffolded")).unwrap();
    sectorforge::presets::scaffold(&dir, "embattled-frontier", &dest, Some("test-seed")).unwrap();
    // Loadable as a project.
    let input = sectorforge::load_project(&dest).unwrap();
    assert_eq!(input.config.generation.seed, "test-seed");
    // Generation succeeds at least up to validation.
    let report = sectorforge::validate_project(&input).unwrap();
    // Validation may produce warnings (workbook exclusions); only errors are
    // disqualifying for this smoke test.
    assert!(
        report.errors.is_empty(),
        "expected no validation errors, got: {:?}",
        report.errors
    );
}
