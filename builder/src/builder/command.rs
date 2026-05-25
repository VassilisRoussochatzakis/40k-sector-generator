//! Command pattern (U1). Every structural mutation flows through one of
//! these variants so the command log can replay or revert it.
//!
//! The Phase A surface is intentionally narrow — it covers the structural
//! mutations needed by the map and entity inspectors. Overlay mutations
//! (regions, presence, intel, ...) call into `GeneratedSector` directly until
//! the matching panels (Phase C/D) wire up their commands.

use serde::{Deserialize, Serialize};

use sectorforge::archetypes::ArchetypeState;
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::mutation::MutationError;
use sectorforge::sector_model::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, HexCoord,
    RouteStability, RouteType,
};

/// §AR3: per-axis enable mask for `BuilderCommand::AutoAssignArchetypes`. A
/// disabled axis is reset to its default after `sectorforge::archetypes::
/// apply_all` runs, so the user can opt a faction archetype out of the
/// sector-wide derivation without forking `src/archetypes.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchetypeApplyFlags {
    pub imperial: bool,
    pub necron: bool,
    pub tyranid: bool,
    pub ork: bool,
    pub gsc: bool,
    pub tau: bool,
    pub aeldari: bool,
    pub chaos: bool,
}

impl Default for ArchetypeApplyFlags {
    fn default() -> Self {
        Self {
            imperial: true,
            necron: true,
            tyranid: true,
            ork: true,
            gsc: true,
            tau: true,
            aeldari: true,
            chaos: true,
        }
    }
}

impl ArchetypeApplyFlags {
    /// Reset every axis on `state` whose flag is `false` to the default value
    /// from `ArchetypeState::default()`. Used by
    /// `BuilderCommand::AutoAssignArchetypes::apply` after
    /// `sectorforge::archetypes::apply_all` writes its derived values.
    pub fn mask(self, state: &mut ArchetypeState) {
        let d = ArchetypeState::default();
        if !self.imperial {
            state.imperial_co_sovereigns = d.imperial_co_sovereigns.clone();
        }
        if !self.necron {
            state.necron_phase = d.necron_phase;
        }
        if !self.tyranid {
            state.tyranid_stage = d.tyranid_stage;
        }
        if !self.ork {
            state.ork_waaagh = d.ork_waaagh;
        }
        if !self.gsc {
            state.gsc_stage = d.gsc_stage;
        }
        if !self.tau {
            state.tau_sphere = d.tau_sphere;
        }
        if !self.aeldari {
            state.aeldari_activity = d.aeldari_activity;
        }
        if !self.chaos {
            state.chaos_corruption = d.chaos_corruption;
            state.daemon_manifestation = d.daemon_manifestation;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuilderCommand {
    AddSystem {
        coord: HexCoord,
        name: String,
        /// Filled by `apply` so `revert` can put the system back deterministically.
        result_id: Option<SystemId>,
    },
    RemoveSystem {
        id: SystemId,
        /// Filled by `apply` so `revert` can restore the removed entity.
        before: Option<Box<GeneratedSystem>>,
        /// Routes removed alongside the system. Used by `revert`.
        removed_routes: Vec<GeneratedRoute>,
    },
    MoveSystem {
        id: SystemId,
        from: HexCoord,
        to: HexCoord,
    },
    RenameSystem {
        id: SystemId,
        from: String,
        to: String,
    },
    /// §S6: collision-swap. Used when a drag targets an occupied hex and the
    /// user picks "Swap" in the resolver dialog. Reverts by swapping again.
    SwapSystems { a: SystemId, b: SystemId },
    /// §S5: drop in a freshly generated system at `coord`. The caller
    /// (typically [`crate::builder::state::BuilderState::generate_system_here`])
    /// builds `new_system` via `sectorforge::generate_system_standalone` and
    /// hands the payload to the command bus. `before` records any system that
    /// was replaced at the same coord so revert can restore it.
    ReplaceSystem {
        coord: HexCoord,
        new_system: Box<GeneratedSystem>,
        before: Option<Box<GeneratedSystem>>,
    },
    AddWorld {
        system: SystemId,
        name: String,
        result_id: Option<WorldId>,
    },
    RemoveWorld {
        world: WorldId,
        before: Option<Box<GeneratedWorld>>,
        parent_system: Option<SystemId>,
        parent_position: Option<usize>,
    },
    AddRoute {
        from: SystemId,
        to: SystemId,
        route_type: RouteType,
        stability: RouteStability,
        result_id: Option<RouteId>,
    },
    RemoveRoute {
        id: RouteId,
        before: Option<Box<GeneratedRoute>>,
    },
    ReplaceRoutes {
        before: Vec<GeneratedRoute>,
        after: Vec<GeneratedRoute>,
    },
    AddFaction {
        id: FactionId,
        name: String,
        kind: String,
    },
    RemoveFaction {
        id: FactionId,
        before: Option<Box<GeneratedFaction>>,
    },
    /// §AR1: pin one system's `ArchetypeState` to `after`. `before` is filled
    /// by `apply` so `revert` restores the prior value exactly.
    SetArchetype {
        system: SystemId,
        before: Option<Box<ArchetypeState>>,
        after: ArchetypeState,
    },
    /// §AR2: sector-wide auto-assign that runs `sectorforge::archetypes::
    /// apply_all` then masks per-axis using `flags`. `before` snapshots every
    /// system's prior state so `revert` is a deterministic restore.
    AutoAssignArchetypes {
        flags: ArchetypeApplyFlags,
        before: Vec<(SystemId, ArchetypeState)>,
    },
}

impl BuilderCommand {
    pub fn apply(&mut self, sector: &mut GeneratedSector) -> Result<(), MutationError> {
        match self {
            Self::AddSystem {
                coord,
                name,
                result_id,
            } => {
                let id = sector.add_system(*coord, name)?;
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveSystem {
                id,
                before,
                removed_routes,
            } => {
                let sys = sector
                    .systems
                    .iter()
                    .find(|s| s.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
                *removed_routes = sector
                    .routes
                    .iter()
                    .filter(|r| r.from_system_id == *id || r.to_system_id == *id)
                    .cloned()
                    .collect();
                sector.remove_system(id)?;
                *before = Some(Box::new(sys));
                Ok(())
            }
            Self::MoveSystem { id, from: _, to } => sector.move_system(id, *to),
            Self::RenameSystem { id, from: _, to } => sector.rename_system(id, to),
            Self::SwapSystems { a, b } => sector.swap_systems(a, b),
            Self::ReplaceSystem {
                coord,
                new_system,
                before,
            } => {
                if let Some(pos) = sector.systems.iter().position(|s| s.coord == *coord) {
                    *before = Some(Box::new(sector.systems.remove(pos)));
                }
                sector.systems.push((**new_system).clone());
                sector.manifest.system_count = sector.systems.len();
                sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
                Ok(())
            }
            Self::AddWorld {
                system,
                name,
                result_id,
            } => {
                let id = sector.add_world_to_system(system, name)?;
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveWorld {
                world,
                before,
                parent_system,
                parent_position,
            } => {
                for (si, sys) in sector.systems.iter().enumerate() {
                    if let Some(pos) = sys.worlds.iter().position(|w| w.id == *world) {
                        *before = Some(Box::new(sys.worlds[pos].clone()));
                        *parent_system = Some(sys.id.clone());
                        *parent_position = Some(pos);
                        let _ = si; // silence unused warning under some configs
                        break;
                    }
                }
                sector.remove_world(world)
            }
            Self::AddRoute {
                from,
                to,
                route_type,
                stability,
                result_id,
            } => {
                let id = sector.add_route(from, to, *route_type, *stability)?;
                let systems_by_id: std::collections::BTreeMap<&str, &GeneratedSystem> =
                    sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();
                if let Some(route) = sector.routes.iter_mut().find(|r| r.id == id) {
                    route.controls = sectorforge::route_control::derive_route_controls(
                        route,
                        &systems_by_id,
                        &sector.factions,
                    );
                }
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveRoute { id, before } => {
                let r = sector
                    .routes
                    .iter()
                    .find(|r| r.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
                sector.remove_route(id)?;
                *before = Some(Box::new(r));
                Ok(())
            }
            Self::ReplaceRoutes { before, after } => {
                *before = sector.routes.clone();
                sector.routes = after.clone();
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::AddFaction { id, name, kind } => {
                sector.add_faction(id.clone(), name, kind);
                Ok(())
            }
            Self::RemoveFaction { id, before } => {
                let f = sector
                    .factions
                    .iter()
                    .find(|f| f.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::FactionNotFound(id.to_string()))?;
                sector.remove_faction(id)?;
                *before = Some(Box::new(f));
                Ok(())
            }
            Self::SetArchetype {
                system,
                before,
                after,
            } => {
                let prev = sector
                    .systems
                    .iter()
                    .find(|s| s.id == *system)
                    .map(|s| s.archetype.clone())
                    .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
                sector.set_archetype(system, after.clone())?;
                *before = Some(Box::new(prev));
                Ok(())
            }
            Self::AutoAssignArchetypes { flags, before } => {
                *before = sector
                    .systems
                    .iter()
                    .map(|s| (s.id.clone(), s.archetype.clone()))
                    .collect();
                sectorforge::archetypes::apply_all(sector);
                for sys in &mut sector.systems {
                    flags.mask(&mut sys.archetype);
                }
                Ok(())
            }
        }
    }

    pub fn revert(&self, sector: &mut GeneratedSector) -> Result<(), MutationError> {
        match self {
            Self::AddSystem { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_system(id)?;
                }
                Ok(())
            }
            Self::RemoveSystem {
                before,
                removed_routes,
                ..
            } => {
                if let Some(sys) = before {
                    sector.systems.push((**sys).clone());
                    sector.manifest.system_count = sector.systems.len();
                    sector.manifest.world_count =
                        sector.systems.iter().map(|s| s.worlds.len()).sum();
                }
                for r in removed_routes {
                    sector.routes.push(r.clone());
                }
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::MoveSystem { id, from, .. } => sector.move_system(id, *from),
            Self::RenameSystem { id, from, .. } => sector.rename_system(id, from),
            Self::SwapSystems { a, b } => sector.swap_systems(a, b),
            Self::ReplaceSystem {
                new_system, before, ..
            } => {
                if let Some(pos) = sector.systems.iter().position(|s| s.id == new_system.id) {
                    sector.systems.remove(pos);
                }
                if let Some(prev) = before {
                    sector.systems.push((**prev).clone());
                }
                sector.manifest.system_count = sector.systems.len();
                sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
                Ok(())
            }
            Self::AddWorld { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_world(id)?;
                }
                Ok(())
            }
            Self::RemoveWorld {
                before,
                parent_system,
                parent_position,
                ..
            } => {
                if let (Some(world), Some(sys_id), Some(pos)) =
                    (before, parent_system, parent_position)
                {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *sys_id) {
                        let insert_at = (*pos).min(sys.worlds.len());
                        sys.worlds.insert(insert_at, (**world).clone());
                        sector.manifest.world_count =
                            sector.systems.iter().map(|s| s.worlds.len()).sum();
                    }
                }
                Ok(())
            }
            Self::AddRoute { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_route(id)?;
                }
                Ok(())
            }
            Self::RemoveRoute { before, .. } => {
                if let Some(r) = before {
                    sector.routes.push((**r).clone());
                    sector.manifest.route_count = sector.routes.len();
                }
                Ok(())
            }
            Self::ReplaceRoutes { before, .. } => {
                sector.routes = before.clone();
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::AddFaction { id, .. } => sector.remove_faction(id),
            Self::RemoveFaction { before, .. } => {
                if let Some(f) = before {
                    sector.factions.push((**f).clone());
                }
                Ok(())
            }
            Self::SetArchetype { system, before, .. } => {
                if let Some(prev) = before {
                    sector.set_archetype(system, (**prev).clone())?;
                }
                Ok(())
            }
            Self::AutoAssignArchetypes { before, .. } => {
                for (id, state) in before {
                    if sector.systems.iter().any(|s| s.id == *id) {
                        sector.set_archetype(id, state.clone())?;
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "T", "seed", 8, 8)
    }

    #[test]
    fn add_system_round_trip() {
        let mut s = empty();
        let mut cmd = BuilderCommand::AddSystem {
            coord: HexCoord { q: 1, r: 1 },
            name: "Alpha".into(),
            result_id: None,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.systems.len(), 1);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.systems.len(), 0);
    }

    #[test]
    fn remove_system_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 2, r: 2 }, "Beta").unwrap();
        let mut cmd = BuilderCommand::RemoveSystem {
            id: id.clone(),
            before: None,
            removed_routes: Vec::new(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.systems.len(), 0);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.systems.len(), 1);
        assert_eq!(s.systems[0].id, id);
    }

    #[test]
    fn swap_systems_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 4, r: 0 }, "B").unwrap();
        let mut cmd = BuilderCommand::SwapSystems {
            a: a.clone(),
            b: b.clone(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.systems.iter().find(|x| x.id == a).unwrap().coord,
            HexCoord { q: 4, r: 0 }
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.systems.iter().find(|x| x.id == a).unwrap().coord,
            HexCoord { q: 0, r: 0 }
        );
    }

    #[test]
    fn replace_system_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 1, r: 1 }, "A").unwrap();
        let new_sys = GeneratedSystem::new_at(
            sectorforge::ids::system_id(99),
            99,
            HexCoord { q: 1, r: 1 },
            "Replacement",
        );
        let mut cmd = BuilderCommand::ReplaceSystem {
            coord: HexCoord { q: 1, r: 1 },
            new_system: Box::new(new_sys),
            before: None,
        };
        cmd.apply(&mut s).unwrap();
        assert!(s.systems.iter().any(|x| x.id != a));
        assert!(s.systems.iter().any(|x| &*x.name == "Replacement"));
        cmd.revert(&mut s).unwrap();
        assert!(s.systems.iter().any(|x| x.id == a));
        assert!(!s.systems.iter().any(|x| &*x.name == "Replacement"));
    }

    #[test]
    fn replace_routes_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let c = s.add_system(HexCoord { q: 2, r: 0 }, "C").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let before = s.routes.clone();
        let after = vec![GeneratedRoute {
            id: sectorforge::ids::route_id(&b, &c),
            from_system_id: b,
            to_system_id: c,
            distance: 1,
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Hazardous,
            tags: vec!["bridge".into()],
            controls: Vec::new(),
        }];
        let mut cmd = BuilderCommand::ReplaceRoutes {
            before: Vec::new(),
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.routes, after);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.routes, before);
    }

    #[test]
    fn set_archetype_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 1, r: 1 }, "A").unwrap();
        let mut after = ArchetypeState::default();
        after.ork_waaagh = 73;
        after.necron_phase = sectorforge::archetypes::NecronPhase::Awakening;
        let mut cmd = BuilderCommand::SetArchetype {
            system: id.clone(),
            before: None,
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 73);
        assert_eq!(
            sys.archetype.necron_phase,
            sectorforge::archetypes::NecronPhase::Awakening
        );
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert!(ArchetypeState::is_default(&sys.archetype));
    }

    #[test]
    fn auto_assign_archetypes_round_trip_respects_flag_mask() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        // Pre-seed a non-default archetype so revert has something to restore.
        let mut seed = ArchetypeState::default();
        seed.ork_waaagh = 11;
        s.set_archetype(&id, seed.clone()).unwrap();
        let flags = ArchetypeApplyFlags {
            ork: false,
            ..ArchetypeApplyFlags::default()
        };
        let mut cmd = BuilderCommand::AutoAssignArchetypes {
            flags,
            before: Vec::new(),
        };
        cmd.apply(&mut s).unwrap();
        // Ork axis is masked → defaults restored; pre-seeded value gone.
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 0);
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 11);
    }

    /// R8 determinism: a fixed command sequence applied to a blank sector
    /// must produce the same canonical-JSON BLAKE3 digest across runs.
    #[test]
    fn command_log_determinism_blake3() {
        use sectorforge::rng::digest_bytes;
        fn replay() -> String {
            let mut s = GeneratedSector::empty("t", "T", "seed", 8, 8);
            let cmds = vec![
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 1, r: 1 },
                    name: "Alpha".into(),
                    result_id: None,
                },
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 2, r: 3 },
                    name: "Beta".into(),
                    result_id: None,
                },
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 4, r: 5 },
                    name: "Gamma".into(),
                    result_id: None,
                },
            ];
            for mut c in cmds {
                c.apply(&mut s).unwrap();
            }
            digest_bytes(&serde_json::to_vec(&s).unwrap())
        }
        let a = replay();
        let b = replay();
        assert_eq!(a, b, "BuilderCommand log must be byte-stable");
    }
}
