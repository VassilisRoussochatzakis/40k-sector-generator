//! RANDOM.md — size-only → fully-complete random-sector integration tests.
//!
//! Covers the "no gotcha" completeness guard (every overlay + all five
//! post-generation derivations populated), determinism (same seed ⇒
//! byte-identical), the CLI `random` subcommand, and the `_full` preset.

use std::process::Command;

use camino::Utf8PathBuf;
use sectorforge::random_sector::{
    build_random_config, generate_random_sector, RandomReport, SectorSize,
};

fn presets_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("presets")
}

fn fresh_dest(tmp: &tempfile::TempDir, name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(tmp.path().join(name)).expect("utf8 tempdir path")
}

/// The completeness assertion shared by every size: a sector with *every*
/// feature on, and all five post-gen reports non-empty.
fn assert_complete(report: &RandomReport, size: SectorSize) {
    let tag = size.as_slug();
    let s = &report.sector;

    assert!(!s.systems.is_empty(), "{tag}: no systems");
    assert!(!s.factions.is_empty(), "{tag}: no factions");
    assert!(!s.routes.is_empty(), "{tag}: no routes");

    // Overlays — the exact things that ship off-by-default and that this
    // feature has to turn on.
    assert!(!s.regions.is_empty(), "{tag}: regions overlay empty");
    assert!(s.economy.enabled, "{tag}: economy overlay disabled");
    assert!(!s.economy.worlds.is_empty(), "{tag}: economy has no worlds");
    assert!(!s.chronicle.events.is_empty(), "{tag}: chronicle empty");

    // ensure_connected_graph = true ⇒ a single connected component.
    let analysis = sectorforge::analyze_sector(s);
    assert_eq!(
        analysis.connectivity.component_count, 1,
        "{tag}: route graph is not connected"
    );

    // Every placed system has worlds within the configured bounds.
    let min = report.input.config.generation.min_worlds_per_system;
    let max = report.input.config.generation.max_worlds_per_system;
    for (i, sys) in s.systems.iter().enumerate() {
        let n = sys.worlds.len();
        assert!(
            (min..=max).contains(&n),
            "{tag}: system #{i} has {n} worlds, expected {min}..={max}"
        );
    }

    // The five post-generation derivations the orchestrator does not run.
    assert!(!report.personae.personae.is_empty(), "{tag}: no personae");
    assert!(!report.sites.sites.is_empty(), "{tag}: no sites");
    assert!(!report.hooks.hooks.is_empty(), "{tag}: no hooks");
    assert!(!report.missions.missions.is_empty(), "{tag}: no missions");
    assert!(
        !report.prose.system_entries.is_empty(),
        "{tag}: no prose entries"
    );
}

#[test]
fn random_small_and_medium_are_complete() {
    let dir = presets_dir();
    if !dir.exists() {
        eprintln!("presets/ missing — skipping");
        return;
    }
    for (size, seed, name) in [
        (SectorSize::Small, "it-small", "s"),
        (SectorSize::Medium, "it-medium", "m"),
    ] {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = fresh_dest(&tmp, name);
        let report = generate_random_sector(size, Some(seed.to_string()), &dir, &dest).unwrap();
        assert_complete(&report, size);
    }
}

#[test]
fn random_custom_size_respects_dims() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = fresh_dest(&tmp, "custom");
    let size = SectorSize::Custom { dim: 9 };
    let report = generate_random_sector(size, Some("it-custom".to_string()), &dir, &dest).unwrap();
    assert_eq!(report.sector.width, 9);
    assert_eq!(report.sector.height, 9);
    assert_complete(&report, size);
}

#[test]
#[ignore = "perf: Huge generation is slow; run explicitly like the segmentum test"]
fn random_huge_is_complete() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = fresh_dest(&tmp, "huge");
    let report =
        generate_random_sector(SectorSize::Huge, Some("it-huge".to_string()), &dir, &dest).unwrap();
    assert_complete(&report, SectorSize::Huge);
}

#[test]
#[ignore = "perf: 80x80 = 6400 cells; the max supported grid, run explicitly"]
fn random_max_80x80_completes() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = fresh_dest(&tmp, "max80");
    let size = SectorSize::Custom { dim: 80 };
    let report = generate_random_sector(size, Some("it-80x80".to_string()), &dir, &dest).unwrap();
    assert_eq!(report.sector.width, 80);
    assert_eq!(report.sector.height, 80);
    // The big grid still satisfies every completeness guarantee.
    assert_complete(&report, size);
}

#[test]
fn random_is_byte_deterministic() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let a = generate_random_sector(
        SectorSize::Small,
        Some("repro".to_string()),
        &dir,
        &fresh_dest(&tmp, "a"),
    )
    .unwrap();
    let b = generate_random_sector(
        SectorSize::Small,
        Some("repro".to_string()),
        &dir,
        &fresh_dest(&tmp, "b"),
    )
    .unwrap();
    let ja = serde_json::to_string(&a.sector).unwrap();
    let jb = serde_json::to_string(&b.sector).unwrap();
    assert_eq!(
        ja, jb,
        "same (size, seed) must yield a byte-identical sector"
    );

    // The synthesised config reproduces too.
    let ca = toml::to_string_pretty(&build_random_config("repro", SectorSize::Small)).unwrap();
    let cb = toml::to_string_pretty(&build_random_config("repro", SectorSize::Small)).unwrap();
    assert_eq!(
        ca, cb,
        "same (seed, size) must yield an identical sectorforge.toml"
    );
}

#[test]
fn full_preset_loads_validates_and_enables_overlays() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dest = fresh_dest(&tmp, "full");
    sectorforge::presets::scaffold(&dir, "_full", &dest, Some("preset-test")).unwrap();

    let input = sectorforge::load_project(&dest).unwrap();
    let report = sectorforge::validate_project(&input).unwrap();
    assert!(
        report.errors.is_empty(),
        "_full has validation errors: {:?}",
        report.errors
    );

    // Overlays must be enabled in the loaded catalogs (not silently off).
    assert!(
        input.catalogs.regions.enabled,
        "_full regions must be enabled"
    );
    assert!(
        input.catalogs.economy.enabled,
        "_full economy must be enabled"
    );
    assert!(
        input.catalogs.history.enabled,
        "_full history must be enabled"
    );

    // Every [inputs] file resolves on disk.
    for rel in [
        "data/worlds/worlds.toml",
        "data/names/system_names.toml",
        "data/names/world_names.toml",
        "data/factions/factions.toml",
        "data/routes/route_rules.toml",
        "data/factions/relations.toml",
        "data/routes/regions.toml",
        "data/worlds/economy.toml",
        "data/history.toml",
        "data/personae.toml",
        "data/sites.toml",
        "data/hooks.toml",
        "data/missions.toml",
        "data/prose.toml",
    ] {
        assert!(dest.join(rel).exists(), "_full is missing {rel}");
    }
}

#[test]
fn internal_presets_hidden_from_gallery() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    let ids: Vec<String> = sectorforge::presets::list(&dir)
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(!ids.iter().any(|i| i == "_full"), "_full must be hidden");
    assert!(!ids.iter().any(|i| i == "_base"), "_base must be hidden");
}

#[test]
fn random_cli_writes_bundle_and_reports() {
    let presets = presets_dir();
    if !presets.exists() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let proj = tmp.path().join("proj"); // must not exist yet
    let output = Command::new(env!("CARGO_BIN_EXE_sectorforge"))
        .args([
            "random",
            "--size",
            "small",
            "--seed",
            "cli-it",
            "--out",
            proj.to_str().unwrap(),
            "--presets-dir",
            presets.as_str(),
            "--formats",
            "json,markdown",
        ])
        .output()
        .expect("spawn sectorforge random");
    assert!(
        output.status.success(),
        "`sectorforge random` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let out_dir = proj.join("out");
    for f in [
        "sector.json",
        "sector.md",
        "personae.json",
        "sites.json",
        "hooks.json",
        "missions.json",
        "gazetteer.json",
    ] {
        assert!(
            out_dir.join(f).exists(),
            "`sectorforge random` did not write {f}"
        );
    }
    // The synthesised project itself is reusable.
    assert!(proj.join("sectorforge.toml").exists());
}
