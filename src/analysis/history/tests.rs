//! Determinism + smoke tests for the chronicle pipeline.

use super::*;
use crate::sector_model::{
    ClaimType, FactionClaim, FactionInfluence, GeneratedFaction, GeneratedRoute, GeneratedSector,
    GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord, PowerProfile,
    PresenceDimensions, RouteStability, RouteType, SystemControlSummary, WorldControlSummary,
    WorldDto, WorldFactionPresence,
};
use std::collections::BTreeMap as Map;

fn empty_sector() -> GeneratedSector {
    GeneratedSector {
        id: "test".into(),
        title: "Test".into(),
        seed: "history-seed".into(),
        generator_name: "sectorforge".into(),
        generator_version: "0".into(),
        width: 4,
        height: 4,
        systems: vec![],
        routes: vec![],
        factions: vec![],
        manifest: GenerationManifest {
            project_id: "t".into(),
            generated_at_policy: "n".into(),
            generator_name: "sf".into(),
            generator_version: "0".into(),
            seed: "history-seed".into(),
            seed_hash: "h".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: Map::new(),
            settings_digest: "d".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        },
        influence_field: Default::default(),
        power_projection: Default::default(),
        relations: Default::default(),
        regions: Vec::new().into(),
        economy: Default::default(),
        chronicle: Default::default(),
        ..Default::default()
    }
}

fn world(id: &str, name: &str) -> GeneratedWorld {
    GeneratedWorld {
        id: id.into(),
        index: 1,
        name: name.into(),
        orbit: 1,
        source_row_index: 0,
        world: WorldDto {
            star_colour: crate::worlds::StarColour::Yellow,
            world_type: crate::worlds::WorldType::HiveWorld,
            atmosphere: crate::worlds::Atmosphere::Breathable,
            temperature: crate::worlds::Temperature::Temperate,
            biosphere: crate::worlds::Biosphere::Thriving,
            population: crate::worlds::Population::ExtremelyDense,
            tech_level: crate::worlds::TechLevel::High,
            government: crate::worlds::Government::MilitaryGovernor,
            notable_features: vec![],
        },
        factions: vec![],
        tags: vec![],
        notes: vec![],
        claims: vec![],
        control: WorldControlSummary::default(),
        stability: Default::default(),
        regions: vec![],
        conflict: Default::default(),
        intel: Default::default(),
    }
}

fn system(id: &str) -> GeneratedSystem {
    GeneratedSystem {
        id: id.into(),
        index: 1,
        name: id.into(),
        kind: crate::sector_model::SystemKind::Star,
        coord: HexCoord { q: 0, r: 0 },
        star: Some(GeneratedStar {
            colour_code: "G".into(),
            colour_name: "Yellow".into(),
            spectral_type: None,
            source_row_index: None,
        }),
        worlds: vec![],
        primary_factions: vec![],
        tags: vec![],
        notes: vec![],
        control: SystemControlSummary::default(),
        stability: Default::default(),
        orbital_assets: vec![],
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    }
}

#[test]
fn derive_is_deterministic() {
    let mut sec = empty_sector();
    let mut sys = system("sys-0001");
    let mut w = world("wrld-0001-1", "Alpha Prime");
    w.claims.push(FactionClaim {
        faction_id: "imp".into(),
        claim_type: ClaimType::ImperialMandate,
        strength: 80,
    });
    w.claims.push(FactionClaim {
        faction_id: "chaos".into(),
        claim_type: ClaimType::MilitaryOccupation,
        strength: 70,
    });
    sys.worlds.push(w);
    sec.systems.push(sys);
    sec.factions.push(GeneratedFaction {
        id: "imp".into(),
        name: "Imperium".into(),
        kind: "Imperial".into(),
        disposition: "Order".into(),
        subfactions: Vec::new(),
        system_presence: vec![],
        world_presence: vec![],
        power: PowerProfile::default(),
    });

    let a = derive(&sec);
    let b = derive(&sec);
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb);
    assert!(!a.events.is_empty());
    // Foundation must precede Annexation in the same world chronicle.
    let evs: Vec<&HistoryEvent> = a
        .events
        .iter()
        .filter(|e| matches!(&e.anchor, HistoryAnchor::World { .. }))
        .collect();
    let pos_foundation = evs.iter().position(|e| e.kind == EventKind::Foundation);
    let pos_annexation = evs.iter().position(|e| e.kind == EventKind::Annexation);
    if let (Some(f), Some(a)) = (pos_foundation, pos_annexation) {
        assert!(f < a, "foundation must precede annexation");
    }
}

#[test]
fn empty_sector_yields_empty_report() {
    let sec = empty_sector();
    let r = derive(&sec);
    assert!(r.events.is_empty());
}

#[test]
fn world_with_no_claims_still_gets_foundation() {
    let mut sec = empty_sector();
    let mut sys = system("sys-0001");
    sys.worlds.push(world("wrld-0001-1", "Lonely"));
    sec.systems.push(sys);
    let r = derive(&sec);
    assert!(r.events.iter().any(|e| e.kind == EventKind::Foundation));
}

#[test]
fn route_events_have_refs_and_consequences() {
    let mut sec = empty_sector();
    sec.systems.push(system("sys-0001"));
    sec.systems.push(system("sys-0002"));
    sec.routes.push(GeneratedRoute {
        id: "route-sys-0001-sys-0002".into(),
        from_system_id: "sys-0001".into(),
        to_system_id: "sys-0002".into(),
        distance: 1,
        route_type: RouteType::ChartedPassage,
        stability: RouteStability::Perilous,
        tags: Vec::new(),
        controls: Vec::new(),
    });
    let r = derive(&sec);
    let ev = r
        .events
        .iter()
        .find(|e| matches!(e.anchor, HistoryAnchor::Route { .. }))
        .expect("route event");
    assert!(ev
        .entities
        .iter()
        .any(|x| { x.kind == HistoryEntityKind::Route && x.id == "route-sys-0001-sys-0002" }));
    assert!(ev
        .consequences
        .iter()
        .any(|x| x.kind == HistoryConsequenceKind::RouteHazard));
}

#[test]
fn presence_dims_smoke() {
    // Exercise the unused-elsewhere PresenceDimensions/FactionInfluence
    // imports to keep the test module honest about dependencies.
    let _ = WorldFactionPresence {
        faction_id: "x".into(),
        subfaction_id: None,
        subfaction_name: None,
        force_id: None,
        force_name: None,
        influence: FactionInfluence::Minor,
        relationship_to_government: "neutral".into(),
        dimensions: PresenceDimensions::default(),
        dominance: Default::default(),
        intel_confidence: 100,
    };
}
