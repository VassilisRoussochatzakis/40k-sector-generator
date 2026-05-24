//! Canonical mutation API for `GeneratedSector`.
//!
//! Every mutating operation on a sector goes through one of these methods.
//! Panels and the builder command bus call into here so structural
//! invariants (ID indexing, route bookkeeping, tombstones) stay consistent.
//!
//! See `BUILDER_REQS.txt` §D3 and §49 for the contract.

use std::collections::BTreeMap;
use thiserror::Error;

use crate::ids::{route_id, system_id, world_id, FactionId, RouteId, SystemId, WorldId};
use crate::regions::{RegionConditionKind, WarpRegion};
use crate::sector_model::{
    ClaimType, DominanceState, FactionClaim, GeneratedFaction, GeneratedRoute, GeneratedSystem,
    GeneratedWorld, HexCoord, RouteStability, RouteType, SystemState, WorldFactionPresence,
};

use super::GeneratedSector;

#[derive(Debug, Error)]
pub enum MutationError {
    #[error("system not found: {0}")]
    SystemNotFound(String),
    #[error("world not found: {0}")]
    WorldNotFound(String),
    #[error("route not found: {0}")]
    RouteNotFound(String),
    #[error("faction not found: {0}")]
    FactionNotFound(String),
    #[error("region not found: {0}")]
    RegionNotFound(String),
    #[error("coord out of bounds: ({q},{r}) for sector {w}x{h}", q = .0.q, r = .0.r, w = .1, h = .2)]
    CoordOutOfBounds(HexCoord, u32, u32),
    #[error("coord occupied: ({q},{r})", q = .0.q, r = .0.r)]
    CoordOccupied(HexCoord),
    #[error("duplicate route between {0} and {1}")]
    DuplicateRoute(String, String),
    #[error("route endpoints must differ ({0})")]
    SelfRoute(String),
    #[error("history event index out of range: {0}")]
    EventNotFound(usize),
}

impl GeneratedSector {
    // ── System mutations ────────────────────────────────────────────────────

    /// Insert a new system at `coord` with `name`. Returns the assigned id.
    pub fn add_system(&mut self, coord: HexCoord, name: &str) -> Result<SystemId, MutationError> {
        if coord.q < 0
            || coord.r < 0
            || (coord.q as u32) >= self.width
            || (coord.r as u32) >= self.height
        {
            return Err(MutationError::CoordOutOfBounds(
                coord,
                self.width,
                self.height,
            ));
        }
        if self.systems.iter().any(|s| s.coord == coord) {
            return Err(MutationError::CoordOccupied(coord));
        }
        let index = self.systems.iter().map(|s| s.index).max().unwrap_or(0) + 1;
        let id = system_id(index);
        self.systems
            .push(GeneratedSystem::new_at(id.clone(), index, coord, name));
        self.manifest.system_count = self.systems.len();
        Ok(id)
    }

    /// Remove a system + all its worlds + all routes touching it + faction
    /// presence references.
    pub fn remove_system(&mut self, id: &SystemId) -> Result<(), MutationError> {
        let pos = self
            .systems
            .iter()
            .position(|s| s.id == *id)
            .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
        let removed = self.systems.remove(pos);
        let world_ids: Vec<WorldId> = removed.worlds.iter().map(|w| w.id.clone()).collect();

        self.routes
            .retain(|r| r.from_system_id != *id && r.to_system_id != *id);
        for f in &mut self.factions {
            f.system_presence.retain(|s| s != id);
            f.world_presence.retain(|w| !world_ids.contains(w));
        }
        self.manifest.system_count = self.systems.len();
        self.manifest.world_count = self.systems.iter().map(|s| s.worlds.len()).sum();
        self.manifest.route_count = self.routes.len();
        Ok(())
    }

    /// Move a system to a new hex coord. Fails on out-of-bounds or collision.
    pub fn move_system(&mut self, id: &SystemId, coord: HexCoord) -> Result<(), MutationError> {
        if coord.q < 0
            || coord.r < 0
            || (coord.q as u32) >= self.width
            || (coord.r as u32) >= self.height
        {
            return Err(MutationError::CoordOutOfBounds(
                coord,
                self.width,
                self.height,
            ));
        }
        if self.systems.iter().any(|s| s.id != *id && s.coord == coord) {
            return Err(MutationError::CoordOccupied(coord));
        }
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *id)
            .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
        sys.coord = coord;

        // Refresh route distances anchored on this system.
        let updated: Vec<(RouteId, u32)> = self
            .routes
            .iter()
            .filter(|r| r.from_system_id == *id || r.to_system_id == *id)
            .filter_map(|r| {
                let from = self.systems.iter().find(|s| s.id == r.from_system_id)?;
                let to = self.systems.iter().find(|s| s.id == r.to_system_id)?;
                Some((r.id.clone(), super::hex_distance(from.coord, to.coord)))
            })
            .collect();
        for (rid, dist) in updated {
            if let Some(r) = self.routes.iter_mut().find(|r| r.id == rid) {
                r.distance = dist;
            }
        }
        Ok(())
    }

    /// Rename a system. ID remains stable.
    pub fn rename_system(&mut self, id: &SystemId, name: &str) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *id)
            .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
        sys.name = name.into();
        Ok(())
    }

    /// Swap the coords of two systems. Used by §S6 collision-swap dialog so a
    /// drag onto an occupied hex can exchange the two without violating the
    /// `CoordOccupied` rule. Both systems must exist; otherwise nothing is
    /// mutated and `SystemNotFound` is returned. Distances of routes anchored
    /// on either endpoint are refreshed.
    pub fn swap_systems(&mut self, a: &SystemId, b: &SystemId) -> Result<(), MutationError> {
        if a == b {
            return Ok(());
        }
        let coord_a = self
            .systems
            .iter()
            .find(|s| s.id == *a)
            .ok_or_else(|| MutationError::SystemNotFound(a.to_string()))?
            .coord;
        let coord_b = self
            .systems
            .iter()
            .find(|s| s.id == *b)
            .ok_or_else(|| MutationError::SystemNotFound(b.to_string()))?
            .coord;
        for sys in &mut self.systems {
            if sys.id == *a {
                sys.coord = coord_b;
            } else if sys.id == *b {
                sys.coord = coord_a;
            }
        }
        let touched: Vec<(RouteId, u32)> = self
            .routes
            .iter()
            .filter(|r| {
                r.from_system_id == *a
                    || r.to_system_id == *a
                    || r.from_system_id == *b
                    || r.to_system_id == *b
            })
            .filter_map(|r| {
                let from = self.systems.iter().find(|s| s.id == r.from_system_id)?;
                let to = self.systems.iter().find(|s| s.id == r.to_system_id)?;
                Some((r.id.clone(), super::hex_distance(from.coord, to.coord)))
            })
            .collect();
        for (rid, dist) in touched {
            if let Some(r) = self.routes.iter_mut().find(|r| r.id == rid) {
                r.distance = dist;
            }
        }
        Ok(())
    }

    // ── World mutations ─────────────────────────────────────────────────────

    pub fn add_world_to_system(
        &mut self,
        system_id_ref: &SystemId,
        name: &str,
    ) -> Result<WorldId, MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system_id_ref)
            .ok_or_else(|| MutationError::SystemNotFound(system_id_ref.to_string()))?;
        let next = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1;
        let wid = world_id(sys.index, next);
        sys.worlds
            .push(GeneratedWorld::new(wid.clone(), next, name));
        self.manifest.world_count = self.systems.iter().map(|s| s.worlds.len()).sum();
        Ok(wid)
    }

    pub fn remove_world(&mut self, id: &WorldId) -> Result<(), MutationError> {
        let mut removed = false;
        for sys in &mut self.systems {
            if let Some(pos) = sys.worlds.iter().position(|w| w.id == *id) {
                sys.worlds.remove(pos);
                removed = true;
                break;
            }
        }
        if !removed {
            return Err(MutationError::WorldNotFound(id.to_string()));
        }
        for f in &mut self.factions {
            f.world_presence.retain(|w| w != id);
        }
        self.manifest.world_count = self.systems.iter().map(|s| s.worlds.len()).sum();
        Ok(())
    }

    // ── Route mutations ─────────────────────────────────────────────────────

    pub fn add_route(
        &mut self,
        from: &SystemId,
        to: &SystemId,
        route_type: RouteType,
        stability: RouteStability,
    ) -> Result<RouteId, MutationError> {
        if from == to {
            return Err(MutationError::SelfRoute(from.to_string()));
        }
        let from_sys = self
            .systems
            .iter()
            .find(|s| s.id == *from)
            .ok_or_else(|| MutationError::SystemNotFound(from.to_string()))?;
        let to_sys = self
            .systems
            .iter()
            .find(|s| s.id == *to)
            .ok_or_else(|| MutationError::SystemNotFound(to.to_string()))?;
        let id = route_id(from, to);
        if self.routes.iter().any(|r| r.id == id) {
            return Err(MutationError::DuplicateRoute(
                from.to_string(),
                to.to_string(),
            ));
        }
        let distance = super::hex_distance(from_sys.coord, to_sys.coord);
        self.routes.push(GeneratedRoute {
            id: id.clone(),
            from_system_id: from.clone(),
            to_system_id: to.clone(),
            distance,
            route_type,
            stability,
            tags: Vec::new(),
            controls: Vec::new(),
        });
        self.manifest.route_count = self.routes.len();
        Ok(id)
    }

    pub fn remove_route(&mut self, id: &RouteId) -> Result<(), MutationError> {
        let pos = self
            .routes
            .iter()
            .position(|r| r.id == *id)
            .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
        self.routes.remove(pos);
        self.manifest.route_count = self.routes.len();
        Ok(())
    }

    /// Swap a route's endpoints (rare — used when a route is reattached
    /// after a system move + symmetric rebuild).
    pub fn swap_route_endpoints(
        &mut self,
        id: &RouteId,
        from: &SystemId,
        to: &SystemId,
    ) -> Result<RouteId, MutationError> {
        if from == to {
            return Err(MutationError::SelfRoute(from.to_string()));
        }
        let pos = self
            .routes
            .iter()
            .position(|r| r.id == *id)
            .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
        let new_id = route_id(from, to);
        let from_sys = self
            .systems
            .iter()
            .find(|s| s.id == *from)
            .ok_or_else(|| MutationError::SystemNotFound(from.to_string()))?;
        let to_sys = self
            .systems
            .iter()
            .find(|s| s.id == *to)
            .ok_or_else(|| MutationError::SystemNotFound(to.to_string()))?;
        let distance = super::hex_distance(from_sys.coord, to_sys.coord);
        let route = &mut self.routes[pos];
        route.id = new_id.clone();
        route.from_system_id = from.clone();
        route.to_system_id = to.clone();
        route.distance = distance;
        Ok(new_id)
    }

    // ── Faction mutations ───────────────────────────────────────────────────

    pub fn add_faction(&mut self, id: FactionId, name: &str, kind: &str) -> FactionId {
        if !self.factions.iter().any(|f| f.id == id) {
            self.factions.push(GeneratedFaction {
                id: id.clone(),
                name: name.into(),
                kind: kind.into(),
                disposition: "neutral".into(),
                subfactions: Vec::new(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: Default::default(),
            });
        }
        id
    }

    pub fn remove_faction(&mut self, id: &FactionId) -> Result<(), MutationError> {
        let pos = self
            .factions
            .iter()
            .position(|f| f.id == *id)
            .ok_or_else(|| MutationError::FactionNotFound(id.to_string()))?;
        self.factions.remove(pos);

        // Strip the faction from every world presence + system primary list +
        // claim entry.
        for sys in &mut self.systems {
            sys.primary_factions.retain(|f| f != id);
            for w in &mut sys.worlds {
                w.factions.retain(|p| &p.faction_id != id);
                w.claims.retain(|c| &c.faction_id != id);
            }
        }
        Ok(())
    }

    // ── Presence + control ──────────────────────────────────────────────────

    pub fn add_world_presence(
        &mut self,
        world: &WorldId,
        presence: WorldFactionPresence,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.factions.retain(|p| p.faction_id != presence.faction_id);
        w.factions.push(presence);
        Ok(())
    }

    pub fn remove_world_presence(
        &mut self,
        world: &WorldId,
        faction: &FactionId,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.factions.retain(|p| &p.faction_id != faction);
        Ok(())
    }

    pub fn add_claim(
        &mut self,
        world: &WorldId,
        faction: FactionId,
        claim_type: ClaimType,
        strength: u8,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.claims.push(FactionClaim {
            faction_id: faction,
            claim_type,
            strength,
        });
        Ok(())
    }

    pub fn remove_claim(
        &mut self,
        world: &WorldId,
        faction: &FactionId,
        claim_type: ClaimType,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.claims
            .retain(|c| !(&c.faction_id == faction && c.claim_type == claim_type));
        Ok(())
    }

    pub fn set_system_control_state(
        &mut self,
        id: &SystemId,
        state: Option<SystemState>,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *id)
            .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
        sys.control.state = state;
        Ok(())
    }

    pub fn set_world_dominance(
        &mut self,
        world: &WorldId,
        faction: &FactionId,
        dominance: DominanceState,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        if let Some(p) = w.factions.iter_mut().find(|p| &p.faction_id == faction) {
            p.dominance = dominance;
            Ok(())
        } else {
            Err(MutationError::FactionNotFound(faction.to_string()))
        }
    }

    // ── Region mutations ────────────────────────────────────────────────────

    pub fn add_region(
        &mut self,
        id: &str,
        name: &str,
        kind: RegionConditionKind,
        centre: HexCoord,
    ) -> String {
        let mut regions = (*self.regions).clone();
        regions.push(WarpRegion {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            hexes: vec![centre],
            centre,
        });
        self.regions = std::sync::Arc::new(regions);
        id.to_string()
    }

    pub fn remove_region(&mut self, id: &str) -> Result<(), MutationError> {
        let mut regions = (*self.regions).clone();
        let len_before = regions.len();
        regions.retain(|r| r.id != id);
        if regions.len() == len_before {
            return Err(MutationError::RegionNotFound(id.to_string()));
        }
        self.regions = std::sync::Arc::new(regions);
        Ok(())
    }

    pub fn add_region_hex(&mut self, id: &str, hex: HexCoord) -> Result<(), MutationError> {
        let mut regions = (*self.regions).clone();
        let region = regions
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| MutationError::RegionNotFound(id.to_string()))?;
        if !region.hexes.contains(&hex) {
            region.hexes.push(hex);
        }
        self.regions = std::sync::Arc::new(regions);
        Ok(())
    }

    pub fn remove_region_hex(&mut self, id: &str, hex: HexCoord) -> Result<(), MutationError> {
        let mut regions = (*self.regions).clone();
        let region = regions
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| MutationError::RegionNotFound(id.to_string()))?;
        region.hexes.retain(|h| *h != hex);
        self.regions = std::sync::Arc::new(regions);
        Ok(())
    }

    // ── Orbital assets / surface regions ────────────────────────────────────

    pub fn add_orbital_asset(
        &mut self,
        system: &SystemId,
        asset: crate::orbital_assets::OrbitalAsset,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        sys.orbital_assets.push(asset);
        Ok(())
    }

    pub fn remove_orbital_asset(
        &mut self,
        system: &SystemId,
        asset_id: &str,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        sys.orbital_assets.retain(|a| a.id != asset_id);
        Ok(())
    }

    pub fn add_surface_region(
        &mut self,
        world: &WorldId,
        region: crate::surface_region::SurfaceRegion,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.regions.push(region);
        Ok(())
    }

    pub fn remove_surface_region(
        &mut self,
        world: &WorldId,
        name: &str,
    ) -> Result<(), MutationError> {
        let w = self.find_world_mut(world)?;
        w.regions.retain(|r| r.name != name);
        Ok(())
    }

    // ── Archetype / intel / history / overrides ─────────────────────────────

    pub fn set_archetype(
        &mut self,
        system: &SystemId,
        state: crate::archetypes::ArchetypeState,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        sys.archetype = state;
        Ok(())
    }

    pub fn clear_archetype(&mut self, system: &SystemId) -> Result<(), MutationError> {
        self.set_archetype(system, crate::archetypes::ArchetypeState::default())
    }

    pub fn add_intel_record(
        &mut self,
        system: &SystemId,
        observer: FactionId,
        view: crate::intel::ObserverView,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        sys.intel.by_observer.insert(observer.to_string(), view);
        Ok(())
    }

    pub fn remove_intel_record(
        &mut self,
        system: &SystemId,
        observer: &FactionId,
    ) -> Result<(), MutationError> {
        let sys = self
            .systems
            .iter_mut()
            .find(|s| s.id == *system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        sys.intel.by_observer.remove(observer.as_str());
        Ok(())
    }

    pub fn add_history_event(&mut self, ev: crate::history::HistoryEvent) {
        let mut events = self.chronicle.events.clone();
        events.push(ev);
        self.chronicle.events = events;
    }

    pub fn remove_history_event(&mut self, idx: usize) -> Result<(), MutationError> {
        if idx >= self.chronicle.events.len() {
            return Err(MutationError::EventNotFound(idx));
        }
        let mut events = self.chronicle.events.clone();
        events.remove(idx);
        self.chronicle.events = events;
        Ok(())
    }

    pub fn edit_event(
        &mut self,
        idx: usize,
        ev: crate::history::HistoryEvent,
    ) -> Result<(), MutationError> {
        if idx >= self.chronicle.events.len() {
            return Err(MutationError::EventNotFound(idx));
        }
        let mut events = self.chronicle.events.clone();
        events[idx] = ev;
        self.chronicle.events = events;
        Ok(())
    }

    // ── Lookup helpers ──────────────────────────────────────────────────────

    fn find_world_mut(&mut self, id: &WorldId) -> Result<&mut GeneratedWorld, MutationError> {
        for sys in &mut self.systems {
            if let Some(w) = sys.worlds.iter_mut().find(|w| w.id == *id) {
                return Ok(w);
            }
        }
        Err(MutationError::WorldNotFound(id.to_string()))
    }

    // ── Re-indexing (kept from §49) ─────────────────────────────────────────

    /// Re-indexes all system and world IDs (§15.2).
    ///
    /// If `stable` is true ("compat" mode), existing valid IDs are preserved.
    /// New IDs are assigned to systems without IDs, and holes are not filled.
    ///
    /// If `stable` is false ("renumber-on-save" mode), all systems are
    /// re-numbered sequentially based on their current order in the
    /// `systems` vec.
    pub fn reindex_ids(
        &mut self,
        stable: bool,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        if stable {
            self.reindex_stable()
        } else {
            self.reindex_sequential()
        }
    }

    fn reindex_sequential(&mut self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut old_to_new_sys = BTreeMap::new();
        let mut old_to_new_world = BTreeMap::new();

        for (i, sys) in self.systems.iter_mut().enumerate() {
            let new_index = i + 1;
            let new_id = system_id(new_index);
            let old_id = sys.id.clone();

            if old_id != new_id {
                old_to_new_sys.insert(old_id.to_string(), new_id.to_string());
                sys.id = new_id;
            }
            sys.index = new_index;

            for (j, world) in sys.worlds.iter_mut().enumerate() {
                let widx = j + 1;
                let new_w_id = world_id(new_index, widx);
                let old_w_id = world.id.clone();

                if old_w_id != new_w_id {
                    old_to_new_world.insert(old_w_id.to_string(), new_w_id.to_string());
                    world.id = new_w_id;
                }
                world.index = widx;
            }
        }

        self.apply_id_migrations(old_to_new_sys.clone(), old_to_new_world.clone());
        (old_to_new_sys, old_to_new_world)
    }

    fn reindex_stable(&mut self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut next_sys_index = self.systems.iter().map(|s| s.index).max().unwrap_or(0) + 1;

        let old_to_new_sys = BTreeMap::new();
        let old_to_new_world = BTreeMap::new();

        for sys in &mut self.systems {
            if sys.id.is_empty() || sys.index == 0 {
                let new_id = system_id(next_sys_index);
                sys.id = new_id;
                sys.index = next_sys_index;
                next_sys_index += 1;
            }

            let mut next_world_index = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1;

            for world in &mut sys.worlds {
                if world.id.is_empty() || world.index == 0 {
                    let new_id = world_id(sys.index, next_world_index);
                    world.id = new_id;
                    world.index = next_world_index;
                    next_world_index += 1;
                }
            }
        }

        self.apply_id_migrations(old_to_new_sys.clone(), old_to_new_world.clone());
        (old_to_new_sys, old_to_new_world)
    }

    fn apply_id_migrations(
        &mut self,
        sys_map: BTreeMap<String, String>,
        world_map: BTreeMap<String, String>,
    ) {
        if sys_map.is_empty() && world_map.is_empty() {
            return;
        }

        for route in &mut self.routes {
            if let Some(new_from) = sys_map.get(route.from_system_id.as_str()) {
                route.from_system_id = SystemId::new(new_from.clone());
            }
            if let Some(new_to) = sys_map.get(route.to_system_id.as_str()) {
                route.to_system_id = SystemId::new(new_to.clone());
            }
            route.id = route_id(&route.from_system_id, &route.to_system_id);
        }

        for faction in &mut self.factions {
            for sys_id in &mut faction.system_presence {
                if let Some(new_id) = sys_map.get(sys_id.as_str()) {
                    *sys_id = SystemId::new(new_id.clone());
                }
            }
            for wid in &mut faction.world_presence {
                if let Some(new_id) = world_map.get(wid.as_str()) {
                    *wid = WorldId::new(new_id.clone());
                }
            }
        }

        for (old, new) in sys_map {
            self.id_history.insert(old, new);
        }
        for (old, new) in world_map {
            self.id_history.insert(old, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::HexCoord;

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "Title", "seed", 8, 8)
    }

    #[test]
    fn add_system_assigns_sequential_ids() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        assert_eq!(a.as_str(), "sys-0001");
        assert_eq!(b.as_str(), "sys-0002");
        assert_eq!(s.manifest.system_count, 2);
    }

    #[test]
    fn add_system_rejects_collision() {
        let mut s = empty();
        s.add_system(HexCoord { q: 1, r: 1 }, "A").unwrap();
        let err = s.add_system(HexCoord { q: 1, r: 1 }, "B");
        assert!(matches!(err, Err(MutationError::CoordOccupied(_))));
    }

    #[test]
    fn remove_system_cleans_up_routes() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 2, r: 0 }, "B").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        assert_eq!(s.routes.len(), 1);
        s.remove_system(&a).unwrap();
        assert!(s.routes.is_empty());
        assert_eq!(s.systems.len(), 1);
    }

    #[test]
    fn move_system_updates_route_distance() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        assert_eq!(s.routes[0].distance, 1);
        s.move_system(&b, HexCoord { q: 5, r: 0 }).unwrap();
        assert_eq!(s.routes[0].distance, 5);
    }

    #[test]
    fn swap_systems_exchanges_coords_and_refreshes_distance() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 4, r: 0 }, "B").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        s.swap_systems(&a, &b).unwrap();
        let sa = s.systems.iter().find(|x| x.id == a).unwrap();
        let sb = s.systems.iter().find(|x| x.id == b).unwrap();
        assert_eq!(sa.coord, HexCoord { q: 4, r: 0 });
        assert_eq!(sb.coord, HexCoord { q: 0, r: 0 });
        assert_eq!(s.routes[0].distance, 4);
    }

    #[test]
    fn swap_systems_unknown_id_errors() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let phantom = SystemId::new("sys-9999");
        assert!(matches!(
            s.swap_systems(&a, &phantom),
            Err(MutationError::SystemNotFound(_))
        ));
    }

    #[test]
    fn reindex_stable_preserves_existing() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        s.reindex_ids(true);
        assert!(s.systems.iter().any(|sys| sys.id == a));
    }

    #[test]
    fn reindex_sequential_writes_tombstones() {
        let mut s = empty();
        s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        s.systems[0].id = SystemId::new("sys-0099");
        s.reindex_ids(false);
        assert!(s.id_history.contains_key("sys-0099"));
        assert_eq!(
            s.id_history.get("sys-0099").map(String::as_str),
            Some("sys-0001")
        );
    }
}
