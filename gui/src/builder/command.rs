//! Command pattern (U1). Every structural mutation flows through one of
//! these variants so the command log can replay or revert it.
//!
//! The Phase A surface is intentionally narrow — it covers the structural
//! mutations needed by the map and entity inspectors. Overlay mutations
//! (regions, presence, intel, ...) call into `GeneratedSector` directly until
//! the matching panels (Phase C/D) wire up their commands.

use serde::{Deserialize, Serialize};

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::mutation::MutationError;
use sectorforge::sector_model::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, HexCoord,
    RouteStability, RouteType,
};

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
    AddFaction {
        id: FactionId,
        name: String,
        kind: String,
    },
    RemoveFaction {
        id: FactionId,
        before: Option<Box<GeneratedFaction>>,
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
            Self::AddFaction { id, .. } => sector.remove_faction(id),
            Self::RemoveFaction { before, .. } => {
                if let Some(f) = before {
                    sector.factions.push((**f).clone());
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
}
