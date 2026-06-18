//! RANDOM.md — size-only → fully-complete random-sector integration tests.
//!
//! Covers the "no gotcha" completeness guard (every overlay + all five
//! post-generation derivations populated), determinism (same seed ⇒
//! byte-identical), the CLI `random` subcommand, and the `_full` preset.

use std::process::Command;

use camino::Utf8PathBuf;
use sectorforge::random_sector::{
    build_random_config, generate_random_sector, generate_random_sector_from, mint_seed,
    RandomReport, SectorSize,
};

use crate::shared::presets_dir;

/// The four themed gallery presets, all usable as `random --baseline` sources.
const THEMED_BASELINES: [&str; 4] = [
    "m42-classic",
    "embattled-frontier",
    "dead-sector",
    "mercantile-crossroads",
];

/// The 14 `[inputs]` files a fully-overlaid (`_full`) project ships — asserted
/// to exist after `_full` scaffold and after a themed-baseline scaffold.
const FULL_INPUT_FILES: [&str; 14] = [
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
];

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
fn mint_seed_is_16_hex_and_varies() {
    // `mint_seed` is the ONE intentionally non-deterministic step (RANDOM.md
    // §5.1): a 16-char lowercase-hex string folded from wall-clock entropy.
    // BTreeSet (not FxSet) to honour the no-FxSet-iteration invariant.
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let s = mint_seed();
        assert_eq!(s.len(), 16, "seed not 16 chars: {s}");
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "non-hex char in {s}"
        );
        assert!(
            s.chars().all(|c| !c.is_ascii_uppercase()),
            "uppercase hex in {s}"
        );
        set.insert(s);
    }
    // `> 1` (not `== 8`) tolerates a vanishingly-unlikely nanos collision.
    assert!(
        set.len() > 1,
        "8 mints all identical — entropy source broken"
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
    for rel in FULL_INPUT_FILES {
        assert!(dest.join(rel).exists(), "_full is missing {rel}");
    }
}

/// P10 serde-compat guard: every on-disk `route_rules.toml` under `presets/`
/// and `examples/` must still deserialize now that `RouteCondition`'s fields are
/// typed enums. A misspelled condition value would make `RouteRulesFile`
/// deserialization fail — this test would catch a preset that silently broke.
#[test]
fn all_preset_route_rules_deserialize_under_typed_conditions() {
    use sectorforge::routes::RouteRulesFile;

    let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0_usize;
    for base in ["presets", "examples"] {
        let dir = root.join(base);
        if !dir.exists() {
            continue;
        }
        for entry in walkdir(dir.as_std_path()) {
            if entry.file_name().and_then(|n| n.to_str()) != Some("route_rules.toml") {
                continue;
            }
            let text = std::fs::read_to_string(&entry)
                .unwrap_or_else(|e| panic!("reading {}: {e}", entry.display()));
            let parsed: Result<RouteRulesFile, _> = toml::from_str(&text);
            assert!(
                parsed.is_ok(),
                "{} failed to deserialize under typed RouteCondition: {:?}",
                entry.display(),
                parsed.err()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 6,
        "expected to check several route_rules.toml files, only saw {checked}"
    );
}

/// Spot-check that a known PascalCase / snake_case mix in a real preset maps to
/// the exact typed variants (not just "parses").
#[test]
fn full_preset_route_rules_map_to_typed_variants() {
    use sectorforge::routes::RouteRulesFile;
    use sectorforge::worlds::{NotableFeature, WorldType};

    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("presets/_full/data/routes/route_rules.toml");
    if !path.exists() {
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let file: RouteRulesFile = toml::from_str(&text).unwrap();
    let conds: Vec<_> = file.routes.modifiers.iter().map(|m| &m.when).collect();
    assert!(
        conds
            .iter()
            .any(|c| c.notable_feature == Some(NotableFeature::TradeHub)),
        "_full route_rules should carry a TradeHub feature condition"
    );
    assert!(
        conds
            .iter()
            .any(|c| c.world_type == Some(WorldType::ForgeWorld)),
        "_full route_rules should carry a ForgeWorld world_type condition"
    );
}

/// Minimal recursive directory walk (no extra dev-dependency).
fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
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

/// Every themed baseline scaffolds, loads, and validates cleanly, and carries
/// the full overlay set at the nested `_full`-layout paths the random scaffolder
/// references — so each one works as a `random --baseline` source. The flat
/// `_base` copies of relations/regions/economy are *not* enough; the preset must
/// overlay the nested versions (this test guards that requirement).
#[test]
fn themed_baselines_scaffold_load_validate_and_enable_overlays() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    for preset in THEMED_BASELINES {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = fresh_dest(&tmp, preset);
        sectorforge::presets::scaffold(&dir, preset, &dest, Some("baseline-test")).unwrap();

        let input = sectorforge::load_project(&dest).unwrap();
        let report = sectorforge::validate_project(&input).unwrap();
        assert!(
            report.errors.is_empty(),
            "{preset} has validation errors: {:?}",
            report.errors
        );

        for rel in FULL_INPUT_FILES {
            assert!(
                dest.join(rel).exists(),
                "{preset} is missing {rel} after scaffold (needed as a random baseline)"
            );
        }

        assert!(
            input.catalogs.regions.enabled,
            "{preset} regions overlay must be enabled"
        );
        assert!(
            input.catalogs.economy.enabled,
            "{preset} economy overlay must be enabled"
        );
        assert!(
            input.catalogs.history.enabled,
            "{preset} history overlay must be enabled"
        );
    }
}

/// Driving `random` from a themed baseline produces a sector as complete as the
/// `_full` default — same completeness guarantees, different content tree.
#[test]
fn random_from_each_themed_baseline_is_complete() {
    let dir = presets_dir();
    if !dir.exists() {
        return;
    }
    for preset in THEMED_BASELINES {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = fresh_dest(&tmp, preset);
        let report = generate_random_sector_from(
            SectorSize::Small,
            Some(format!("baseline-it-{preset}")),
            preset,
            &dir,
            &dest,
        )
        .unwrap_or_else(|e| panic!("random from baseline {preset} failed: {e}"));
        assert_complete(&report, SectorSize::Small);
    }
}
