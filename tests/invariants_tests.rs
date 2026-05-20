//! Post-generation invariant checks (spec §11.11) and standalone APIs.

use camino::Utf8PathBuf;

use sectorforge::sector_model::{HexCoord, RouteStability, RouteType};

#[test]
fn generated_sector_passes_invariants() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    let report = sectorforge::validate_sector(&sector);
    assert!(
        report.ok,
        "expected invariants to pass; violations: {:?}",
        report.violations
    );
}

#[test]
fn sector_round_trips_through_json() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    let text = serde_json::to_string(&sector).unwrap();
    let parsed: sectorforge::GeneratedSector = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.id, sector.id);
    assert_eq!(parsed.systems.len(), sector.systems.len());
    assert_eq!(parsed.routes.len(), sector.routes.len());
    assert_eq!(parsed.manifest.world_count, sector.manifest.world_count);
}

#[test]
fn invariants_detect_route_distance_tamper() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let mut sector = sectorforge::generate_sector(input).unwrap();
    if let Some(r) = sector.routes.first_mut() {
        r.distance = r.distance.saturating_add(99);
    } else {
        return; // no routes — skip
    }
    let report = sectorforge::validate_sector(&sector);
    assert!(!report.ok);
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == "ROUTE_DISTANCE_MISMATCH"));
}

#[test]
fn invariants_detect_manifest_count_tamper() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let mut sector = sectorforge::generate_sector(input).unwrap();
    sector.manifest.system_count += 1;
    let report = sectorforge::validate_sector(&sector);
    assert!(!report.ok);
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == "MANIFEST_SYSTEM_COUNT_MISMATCH"));
}

#[test]
fn invariants_detect_unknown_faction_in_world() {
    use sectorforge::sector_model::{FactionInfluence, WorldFactionPresence};
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let mut sector = sectorforge::generate_sector(input).unwrap();
    if let Some(sys) = sector.systems.first_mut() {
        if let Some(world) = sys.worlds.first_mut() {
            world.factions.push(WorldFactionPresence {
                faction_id: "does_not_exist".into(),
                subfaction_id: None,
                subfaction_name: None,
                influence: FactionInfluence::Minor,
                relationship_to_government: "test".to_string(),
                dimensions: Default::default(),
                dominance: Default::default(),
                intel_confidence: 100,
            });
        }
    }
    let report = sectorforge::validate_sector(&sector);
    assert!(!report.ok);
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == "WORLD_FACTION_MISSING_SUMMARY"));
}

#[test]
fn generate_system_standalone_succeeds() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sys = sectorforge::generate_system_standalone(input, 1, HexCoord { q: 0, r: 0 }).unwrap();
    assert_eq!(sys.id, "sys-0001");
    assert_eq!(sys.index, 1);
    assert!(!sys.worlds.is_empty());
    // Spec §10.10: every world must have all tag namespaces present.
    for w in &sys.worlds {
        for prefix in [
            "atmosphere:",
            "biosphere:",
            "gov:",
            "population:",
            "star:",
            "tech:",
            "temperature:",
            "world_type:",
        ] {
            assert!(
                w.tags.iter().any(|t| t.starts_with(prefix)),
                "world {} missing tag namespace {}: {:?}",
                w.id,
                prefix,
                w.tags
            );
        }
    }
}

#[test]
fn generate_system_standalone_is_deterministic() {
    let project = fixture_project();
    let a = sectorforge::generate_system_standalone(
        sectorforge::load_project(&project).unwrap(),
        2,
        HexCoord { q: 1, r: 1 },
    )
    .unwrap();
    let b = sectorforge::generate_system_standalone(
        sectorforge::load_project(&project).unwrap(),
        2,
        HexCoord { q: 1, r: 1 },
    )
    .unwrap();
    let a_text = serde_json::to_string(&a).unwrap();
    let b_text = serde_json::to_string(&b).unwrap();
    assert_eq!(a_text, b_text);
}

#[test]
fn generate_system_standalone_different_index_differs() {
    let project = fixture_project();
    let a = sectorforge::generate_system_standalone(
        sectorforge::load_project(&project).unwrap(),
        1,
        HexCoord { q: 0, r: 0 },
    )
    .unwrap();
    let b = sectorforge::generate_system_standalone(
        sectorforge::load_project(&project).unwrap(),
        7,
        HexCoord { q: 0, r: 0 },
    )
    .unwrap();
    // IDs derived from index.
    assert_eq!(a.id, "sys-0001");
    assert_eq!(b.id, "sys-0007");
    // World contents almost always diverge because per-system RNG seeds on sys_id.
    assert_ne!(
        serde_json::to_string(&a.worlds).unwrap(),
        serde_json::to_string(&b.worlds).unwrap(),
    );
}

#[test]
fn render_markdown_round_trips_from_loaded_sector() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let json_path = tmp_path.join("sector.json");
    sectorforge::write_sector_json(&json_path, &sector).unwrap();

    let reloaded = sectorforge::load_sector_json(&json_path).unwrap();
    let md_a = sectorforge::render_sector_markdown(&sector);
    let md_b = sectorforge::render_sector_markdown(&reloaded);
    assert_eq!(md_a, md_b, "markdown rendering must round-trip via JSON");
    assert!(md_a.contains("## Sector map"));
    assert!(md_a.contains("## System index"));
    assert!(md_a.contains("## Routes"));
}

#[test]
fn world_factions_are_sorted_by_influence() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    for sys in &sector.systems {
        for w in &sys.worlds {
            for pair in w.factions.windows(2) {
                let rank_a = influence_rank(pair[0].influence);
                let rank_b = influence_rank(pair[1].influence);
                assert!(
                    rank_a >= rank_b,
                    "world {} faction influence not sorted: {:?}",
                    w.id,
                    w.factions
                );
            }
        }
    }
}

#[test]
fn primary_factions_capped_at_three() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    for sys in &sector.systems {
        assert!(
            sys.primary_factions.len() <= 3,
            "system {} has too many primary factions: {:?}",
            sys.id,
            sys.primary_factions
        );
    }
}

#[test]
fn generated_factions_are_kind_groups_with_subfactions() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let expected_kinds: std::collections::BTreeSet<String> =
        input.factions.iter().map(|f| f.kind.clone()).collect();
    let sector = sectorforge::generate_sector(input).unwrap();
    let generated_ids: std::collections::BTreeSet<String> =
        sector.factions.iter().map(|f| f.id.to_string()).collect();

    assert_eq!(generated_ids, expected_kinds);
    assert!(sector.factions.iter().any(|f| f.subfactions.len() > 1));

    for sys in &sector.systems {
        for w in &sys.worlds {
            for p in &w.factions {
                assert!(generated_ids.contains(p.faction_id.as_str()));
                assert!(
                    p.subfaction_id.is_some(),
                    "world {} presence {} missing subfaction",
                    w.id,
                    p.faction_id
                );
                assert!(
                    p.subfaction_name.is_some(),
                    "world {} presence {} missing subfaction name",
                    w.id,
                    p.faction_id
                );
            }
        }
    }
}

#[test]
fn routes_have_known_types() {
    let project = fixture_project();
    let input = sectorforge::load_project(project).unwrap();
    let sector = sectorforge::generate_sector(input).unwrap();
    for r in &sector.routes {
        // Smoke check on enum variants — also confirms Deserialize chain works.
        let json = serde_json::to_string(&r.route_type).unwrap();
        let back: RouteType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r.route_type);
        let json = serde_json::to_string(&r.stability).unwrap();
        let back: RouteStability = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r.stability);
    }
}

fn influence_rank(i: sectorforge::sector_model::FactionInfluence) -> u8 {
    use sectorforge::sector_model::FactionInfluence;
    match i {
        FactionInfluence::Dominant => 3,
        FactionInfluence::Significant => 2,
        FactionInfluence::Minor => 1,
        FactionInfluence::Hidden => 0,
    }
}

fn fixture_project() -> Utf8PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(manifest_dir).join("examples/m42_project")
}
