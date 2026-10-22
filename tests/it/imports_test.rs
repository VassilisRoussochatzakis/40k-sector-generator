use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::personae::{Persona, PersonaAnchor, SystemSlot};
use sectorforge::sector_model::GeneratedSector;
use sectorforge::sites::{SiteKind, SiteStatus, WorldSite};

#[test]
fn test_manual_personae_merge() {
    let sector = GeneratedSector::default();
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
    let sector = GeneratedSector::default();
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
