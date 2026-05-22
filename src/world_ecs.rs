//! Minimal ECS-style adapter over `GeneratedSector` (§12 NEXT.md, design
//! §13).
//!
//! The full design recommends a `bevy_ecs` / `hecs` migration with
//! discrete `SimulationSystems`, `RenderWorld`, `UIState` modules. A
//! cold-start ECS rewrite of the existing 27 kLOC code is out of scope
//! for a single pass — but we *can* expose the data in an ECS-friendly
//! shape so consumers can write systems against it without re-walking
//! the nested tree every frame.
//!
//! `EntityWorld` is a flat columnar view: parallel `Vec`s of entity ids
//! and component slices. It is built from a finalised `GeneratedSector`
//! and snapshots the runtime state. A future `bevy_ecs` integration can
//! adopt this column layout directly.
//!
//! Entity kinds:
//! * `EntityKind::System` — one per `GeneratedSystem`.
//! * `EntityKind::World` — one per `GeneratedWorld`.
//! * `EntityKind::Faction` — one per `GeneratedFaction`.
//! * `EntityKind::Route` — one per `GeneratedRoute`.
//!
//! Components are referenced by index into the parallel vectors and
//! identified by typed `EntityRef`s.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedSector, HexCoord};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    System,
    World,
    Faction,
    Route,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityWorld {
    pub kinds: Vec<EntityKind>,
    pub names: Vec<String>,
    /// id_lookup[ "sys-0001" ] = EntityId(0)
    pub id_lookup: BTreeMap<String, EntityId>,
    /// System component data, indexed by entity-id-of-system.
    pub system_components: BTreeMap<EntityId, SystemComponents>,
    pub world_components: BTreeMap<EntityId, WorldComponents>,
    pub faction_components: BTreeMap<EntityId, FactionComponents>,
    pub route_components: BTreeMap<EntityId, RouteComponents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemComponents {
    pub id: crate::ids::SystemId,
    pub coord: HexCoord,
    pub world_entities: Vec<EntityId>,
    pub state: Option<crate::sector_model::SystemState>,
    pub dominant: Option<crate::ids::FactionId>,
    pub conflict_intensity: u8,
    pub blockade_under: bool,
    pub stability_order: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldComponents {
    pub id: crate::ids::WorldId,
    pub system_entity: EntityId,
    pub dominant: Option<crate::ids::FactionId>,
    pub contested: bool,
    pub presences: Vec<(crate::ids::FactionId, u8)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionComponents {
    pub id: crate::ids::FactionId,
    pub kind: String,
    pub total_projection: f32,
    pub system_count: u32,
    pub world_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteComponents {
    pub id: crate::ids::RouteId,
    pub from_entity: EntityId,
    pub to_entity: EntityId,
    pub route_type: crate::sector_model::RouteType,
    pub stability: crate::sector_model::RouteStability,
}

/// Build the flat entity-world view from a finalised sector. Pure.
#[must_use]
pub fn build(sector: &GeneratedSector) -> EntityWorld {
    let mut w = EntityWorld::default();
    let mut next_id: u32 = 0;
    let mut alloc = |w: &mut EntityWorld, name: &str, kind: EntityKind| -> EntityId {
        let id = EntityId(next_id);
        next_id += 1;
        w.kinds.push(kind);
        w.names.push(name.to_string());
        w.id_lookup.insert(name.to_string(), id);
        id
    };

    let mut system_eids: BTreeMap<crate::ids::SystemId, EntityId> = BTreeMap::new();
    for sys in &sector.systems {
        let eid = alloc(&mut w, &sys.id, EntityKind::System);
        system_eids.insert(sys.id.clone(), eid);
    }
    for sys in &sector.systems {
        let sys_eid = system_eids[&sys.id];
        let worlds = sector.get_worlds_for_system(sys);
        let mut world_eids: Vec<EntityId> = Vec::with_capacity(worlds.len());
        for world in worlds {
            let eid = alloc(&mut w, &world.id, EntityKind::World);
            world_eids.push(eid);
            w.world_components.insert(
                eid,
                WorldComponents {
                    id: world.id.clone(),
                    system_entity: sys_eid,
                    dominant: world.control.dominant.clone(),
                    contested: world.control.contested,
                    presences: world
                        .factions
                        .iter()
                        .map(|p| {
                            (
                                p.faction_id.clone(),
                                p.dimensions.local_control_score().clamp(0.0, 100.0).round() as u8,
                            )
                        })
                        .collect(),
                },
            );
        }
        w.system_components.insert(
            sys_eid,
            SystemComponents {
                id: sys.id.clone(),
                coord: sys.coord,
                world_entities: world_eids,
                state: sys.control.state,
                dominant: sys.control.dominant.clone(),
                conflict_intensity: sys.conflict.intensity,
                blockade_under: sys.blockade.under_blockade,
                stability_order: sys.stability.public_order.round() as u8,
            },
        );
    }
    for f in &sector.factions {
        let eid = alloc(&mut w, &f.id, EntityKind::Faction);
        w.faction_components.insert(
            eid,
            FactionComponents {
                id: f.id.clone(),
                kind: f.kind.to_string(),
                total_projection: f.power.total_projection(),
                system_count: f.system_presence.len() as u32,
                world_count: f.world_presence.len() as u32,
            },
        );
    }
    for r in &sector.routes {
        let eid = alloc(&mut w, &r.id, EntityKind::Route);
        let from = system_eids.get(&r.from_system_id).copied();
        let to = system_eids.get(&r.to_system_id).copied();
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        w.route_components.insert(
            eid,
            RouteComponents {
                id: r.id.clone(),
                from_entity: from,
                to_entity: to,
                route_type: r.route_type,
                stability: r.stability,
            },
        );
    }
    w
}

/// Convenience: every System entity, paired with its components.
pub fn systems(w: &EntityWorld) -> impl Iterator<Item = (EntityId, &SystemComponents)> {
    w.system_components.iter().map(|(e, c)| (*e, c))
}

/// Convenience: every Faction entity, paired with its components.
pub fn factions(w: &EntityWorld) -> impl Iterator<Item = (EntityId, &FactionComponents)> {
    w.faction_components.iter().map(|(e, c)| (*e, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord, SystemControlSummary,
    };

    #[test]
    fn build_yields_one_system_entity_per_system() {
        let sys = GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "t".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            },
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
        };
        let sector = GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "t".into(),
            generator_name: "t".into(),
            generator_version: "t".into(),
            width: 4,
            height: 4,
            systems: vec![sys],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "t".into(),
                generator_name: "t".into(),
                generator_version: "t".into(),
                seed: "t".into(),
                seed_hash: "t".into(),
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "t".into(),
                system_count: 1,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
        };
        let w = build(&sector);
        assert_eq!(w.kinds.len(), 1);
        assert_eq!(w.kinds[0], EntityKind::System);
    }
}
