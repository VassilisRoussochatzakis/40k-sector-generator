use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::personae::{Persona, PersonaAnchor, SystemSlot};
use sectorforge::sector_model::{GeneratedSector, GenerationManifest};
use sectorforge::sites::{SiteKind, SiteStatus, WorldSite};

fn empty_sector() -> GeneratedSector {
    GeneratedSector {
        id: "test".into(),
        title: "Test".into(),
        seed: "seed".into(),
        generator_name: "sf".into(),
        generator_version: "0".into(),
        width: 1,
        height: 1,
        manifest: GenerationManifest {
            project_id: "p".into(),
            generated_at_policy: "n".into(),
            generator_name: "sf".into(),
            generator_version: "0".into(),
            seed: "s".into(),
            seed_hash: "h".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: Default::default(),
            settings_digest: "d".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        },
        ..Default::default()
    }
}

#[test]
fn test_manual_personae_merge() {
    let sector = empty_sector();
    let mut cfg = sectorforge::personae::PersonaeConfig::default();

    let manual = Persona {
        id: "manual-p".into(),
        faction_id: FactionId::new("imp"),
        faction_kind: "imperial".into(),
        anchor: PersonaAnchor::System {
            system_id: SystemId::new("sys-0001"),
            slot: SystemSlot::Sovereign,
        },
        name: "Manual Bob".into(),
        title: "Lord of Manuals".into(),
        traits: vec!["Boring".into()],
        agenda: "Write docs".into(),
    };

    cfg.manual.push(manual);

    let report = sectorforge::derive_personae_with(&sector, &cfg);
    assert!(report.personae.iter().any(|p| p.id == "manual-p"));
}

#[test]
fn test_manual_sites_merge() {
    let sector = empty_sector();
    let mut cfg = sectorforge::sites::SitesConfig::default();

    let manual = WorldSite {
        id: "manual-s".into(),
        world_id: WorldId::new("sys-0001-w1"),
        system_id: SystemId::new("sys-0001"),
        region_kind: None,
        kind: SiteKind::GovernorsPalace,
        name: "Manual Palace".into(),
        controlling_faction: Some(FactionId::new("imp")),
        known_to: vec![],
        public_status: SiteStatus::Active,
        actual_status: SiteStatus::Active,
        tags: vec![],
        hook: "It's manual".into(),
    };

    cfg.manual.push(manual);

    let report = sectorforge::derive_sites_with(&sector, &cfg);
    assert!(report.sites.iter().any(|s| s.id == "manual-s"));
}
