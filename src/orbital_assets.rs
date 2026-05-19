//! Orbital assets and blockade detection (§2 NEXT.md).
//!
//! Models the design's §6.3 orbital layer as discrete assets attached to a
//! `GeneratedSystem`: stations, shipyards, defense platforms, blockade
//! fleets. Each asset is tagged with its owning faction id and a
//! `strength` 0..=100. The number and kind of assets generated for a
//! system are derived from the per-faction `orbital`, `industrial`, and
//! `military` dimensions of the system-level summary plus tags on the
//! worlds (e.g. `feature:major_spaceyard`).
//!
//! Blockade detection produces a system-level `BlockadeReport` whenever
//! `dominant != orbital_controller` AND the system has at least one
//! blockade fleet from the orbital controller. The result is exposed on
//! `GeneratedSystem.blockade` and surfaced in the renderer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedSystem, PresenceDimensions};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrbitalAssetKind {
    /// Commercial / administrative orbital — banks, freight, embassies.
    Station,
    /// Heavy-industrial dockyard, capable of building or refitting ships.
    Shipyard,
    /// System-defense platform — fortress monitors / void monitors.
    DefensePlatform,
    /// Standing fleet in transit-restriction posture.
    BlockadeFleet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrbitalAsset {
    pub id: String,
    pub kind: OrbitalAssetKind,
    pub faction_id: String,
    /// 0..=100 — derived from the owning faction's orbital / military /
    /// industrial dimension sums across the system.
    pub strength: u8,
    /// Optional named ship counts, for the GUI tooltip. Empty vec when the
    /// asset is not a fleet.
    #[serde(default)]
    pub ship_inventory: Vec<ShipStock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShipStock {
    pub hull_class: String,
    pub count: u16,
}

/// System-level blockade snapshot. `under_blockade` reads true when the
/// orbital controller differs from the surface dominant and at least one
/// blockade fleet is present.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BlockadeReport {
    pub under_blockade: bool,
    pub blockader: Option<String>,
    pub besieged: Option<String>,
    pub intensity: u8,
}

impl BlockadeReport {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Derive a system's `orbital_assets` and blockade summary from finalised
/// per-world presences. Pure; runs after `control::derive_system_control`.
#[must_use]
pub fn derive_orbital_assets(sys: &GeneratedSystem) -> (Vec<OrbitalAsset>, BlockadeReport) {
    if sys.worlds.is_empty() {
        return (Vec::new(), BlockadeReport::default());
    }

    let mut sums: BTreeMap<&str, PresenceDimensions> = BTreeMap::new();
    for w in &sys.worlds {
        for p in &w.factions {
            let entry = sums.entry(p.faction_id.as_str()).or_default();
            entry.admin += p.dimensions.admin;
            entry.military += p.dimensions.military;
            entry.orbital += p.dimensions.orbital;
            entry.economic += p.dimensions.economic;
            entry.industrial += p.dimensions.industrial;
            entry.covert += p.dimensions.covert;
            entry.visibility += p.dimensions.visibility;
        }
    }

    let has_spaceyard = sys.worlds.iter().any(|w| {
        w.world
            .notable_features
            .iter()
            .any(|f| f == "MajorSpaceyard")
    });
    let war_zone = sys.worlds.iter().any(|w| {
        w.tags.iter().any(|t| t.ends_with(":war_zone"))
            || w.world
                .notable_features
                .iter()
                .any(|f| f == "WarZone" || f == "DaemonicCorruption")
    });
    let quarantined = sys
        .worlds
        .iter()
        .any(|w| w.tags.iter().any(|t| t.ends_with(":quarantined")));

    let mut assets: Vec<OrbitalAsset> = Vec::new();
    for (id, d) in &sums {
        // Station: any meaningful admin + orbital presence.
        if d.orbital >= 25.0 && d.admin >= 15.0 {
            assets.push(OrbitalAsset {
                id: format!("{}-station-{}", sys.id, id),
                kind: OrbitalAssetKind::Station,
                faction_id: (*id).to_string(),
                strength: scale_strength(d.orbital * 0.5 + d.admin * 0.5),
                ship_inventory: Vec::new(),
            });
        }
        // Shipyard: explicit MajorSpaceyard feature OR high industrial+orbital.
        if (has_spaceyard && d.industrial >= 30.0) || d.industrial >= 60.0 && d.orbital >= 30.0 {
            assets.push(OrbitalAsset {
                id: format!("{}-shipyard-{}", sys.id, id),
                kind: OrbitalAssetKind::Shipyard,
                faction_id: (*id).to_string(),
                strength: scale_strength(d.industrial * 0.6 + d.orbital * 0.4),
                ship_inventory: Vec::new(),
            });
        }
        // Defense platform: high military+orbital, or any military if the
        // system is a war_zone.
        if d.military >= 40.0 && d.orbital >= 25.0 || war_zone && d.military >= 30.0 {
            assets.push(OrbitalAsset {
                id: format!("{}-defense-{}", sys.id, id),
                kind: OrbitalAssetKind::DefensePlatform,
                faction_id: (*id).to_string(),
                strength: scale_strength(d.military * 0.6 + d.orbital * 0.4),
                ship_inventory: vec![ShipStock {
                    hull_class: "monitor".into(),
                    count: 2,
                }],
            });
        }
        // Blockade fleet: orbital dominance under quarantine OR
        // dominant ≠ orbital_controller in the system summary.
        let orbital_dominant = sys
            .control
            .orbital_controller
            .as_deref()
            .map(|x| x == *id)
            .unwrap_or(false);
        let mismatch = sys.control.dominant.as_deref() != sys.control.orbital_controller.as_deref()
            && sys.control.dominant.is_some()
            && orbital_dominant;
        if (quarantined && d.orbital >= 25.0 && orbital_dominant)
            || (mismatch && d.military >= 30.0)
        {
            assets.push(OrbitalAsset {
                id: format!("{}-blockade-{}", sys.id, id),
                kind: OrbitalAssetKind::BlockadeFleet,
                faction_id: (*id).to_string(),
                strength: scale_strength(d.military * 0.5 + d.orbital * 0.5),
                ship_inventory: vec![
                    ShipStock {
                        hull_class: "cruiser".into(),
                        count: 1,
                    },
                    ShipStock {
                        hull_class: "escort".into(),
                        count: 3,
                    },
                ],
            });
        }
    }
    assets.sort_by(|a, b| a.id.cmp(&b.id));

    let blockade = blockade_report(sys, &assets);
    (assets, blockade)
}

fn scale_strength(raw: f32) -> u8 {
    raw.clamp(0.0, 100.0).round() as u8
}

fn blockade_report(sys: &GeneratedSystem, assets: &[OrbitalAsset]) -> BlockadeReport {
    let fleet = assets
        .iter()
        .find(|a| a.kind == OrbitalAssetKind::BlockadeFleet);
    let Some(fleet) = fleet else {
        return BlockadeReport::default();
    };
    let dominant = sys.control.dominant.clone();
    let orbital = sys.control.orbital_controller.clone();
    let mismatch = dominant.is_some() && orbital.is_some() && dominant != orbital;
    let quarantined = sys
        .worlds
        .iter()
        .any(|w| w.tags.iter().any(|t| t.ends_with(":quarantined")));
    if !(mismatch || quarantined) {
        return BlockadeReport::default();
    }
    BlockadeReport {
        under_blockade: true,
        blockader: Some(fleet.faction_id.clone()),
        besieged: dominant,
        intensity: fleet.strength,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedStar, GeneratedSystem, GeneratedWorld, HexCoord,
        SystemControlSummary, WorldControlSummary, WorldDto, WorldFactionPresence,
    };

    fn mk_sys(
        dominant: Option<&str>,
        orbital: Option<&str>,
        presences: Vec<(&str, PresenceDimensions)>,
        tags: Vec<&str>,
    ) -> GeneratedSystem {
        GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Test".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![GeneratedWorld {
                id: "sys-0001-w1".into(),
                index: 1,
                name: "W".into(),
                orbit: 1,
                source_row_index: 0,
                world: WorldDto {
                    star_colour: "amber".into(),
                    star_colour_code: "A".into(),
                    world_type: "ForgeWorld".into(),
                    atmosphere: "Breathable".into(),
                    temperature: "Temperate".into(),
                    biosphere: "Thriving".into(),
                    population: "DenselyPopulated".into(),
                    tech_level: "High".into(),
                    government: "MagistrateCouncil".into(),
                    notable_features: vec!["MajorSpaceyard".into()],
                },
                factions: presences
                    .into_iter()
                    .map(|(fid, dims)| WorldFactionPresence {
                        faction_id: fid.into(),
                        influence: FactionInfluence::Significant,
                        relationship_to_government: "lawful".into(),
                        dimensions: dims,
                        dominance: DominanceState::default(),
                        intel_confidence: 100,
                    })
                    .collect(),
                tags: tags.into_iter().map(|t| t.into()).collect(),
                notes: vec![],
                claims: vec![],
                control: WorldControlSummary::default(),
                stability: Default::default(),
                regions: Vec::new(),
                conflict: Default::default(),
            }],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary {
                dominant: dominant.map(|s| s.into()),
                orbital_controller: orbital.map(|s| s.into()),
                ..Default::default()
            },
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    fn dims(admin: f32, military: f32, orbital: f32, industrial: f32) -> PresenceDimensions {
        PresenceDimensions {
            admin,
            military,
            orbital,
            industrial,
            visibility: 80.0,
            ..Default::default()
        }
    }

    #[test]
    fn shipyard_appears_with_major_spaceyard_feature() {
        let sys = mk_sys(
            Some("forge"),
            Some("forge"),
            vec![("forge", dims(20.0, 30.0, 40.0, 60.0))],
            vec![],
        );
        let (assets, _) = derive_orbital_assets(&sys);
        assert!(assets
            .iter()
            .any(|a| a.kind == OrbitalAssetKind::Shipyard && a.faction_id == "forge"));
    }

    #[test]
    fn blockade_fleet_appears_on_dominant_orbital_mismatch() {
        let sys = mk_sys(
            Some("ground"),
            Some("blockader"),
            vec![
                ("ground", dims(60.0, 30.0, 5.0, 10.0)),
                ("blockader", dims(0.0, 70.0, 80.0, 10.0)),
            ],
            vec![],
        );
        let (assets, report) = derive_orbital_assets(&sys);
        assert!(report.under_blockade);
        assert_eq!(report.blockader.as_deref(), Some("blockader"));
        assert_eq!(report.besieged.as_deref(), Some("ground"));
        assert!(assets
            .iter()
            .any(|a| a.kind == OrbitalAssetKind::BlockadeFleet));
    }
}
