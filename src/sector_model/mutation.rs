use std::collections::BTreeMap;
use crate::ids::{system_id, world_id, route_id, SystemId, WorldId};
use super::GeneratedSector;

impl GeneratedSector {
    /// Re-indexes all system and world IDs (§15.2).
    ///
    /// If `stable` is true ("compat" mode), existing valid IDs are preserved.
    /// New IDs are assigned to systems without IDs, and holes are not filled.
    ///
    /// If `stable` is false ("renumber-on-save" mode), all systems are
    /// re-numbered sequentially based on their current order in the `systems` vec.
    pub fn reindex_ids(&mut self, stable: bool) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        if stable {
            self.reindex_stable()
        } else {
            self.reindex_sequential()
        }
    }

    fn reindex_sequential(&mut self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut old_to_new_sys = BTreeMap::new();
        let mut old_to_new_world = BTreeMap::new();

        // 1. Reindex systems
        for (i, sys) in self.systems.iter_mut().enumerate() {
            let new_index = i + 1;
            let new_id = system_id(new_index);
            let old_id = sys.id.clone();
            
            if old_id != new_id {
                old_to_new_sys.insert(old_id.to_string(), new_id.to_string());
                sys.id = new_id;
            }
            sys.index = new_index;

            // 2. Reindex worlds within system
            for (j, world) in sys.worlds.iter_mut().enumerate() {
                let world_index = j + 1;
                let new_w_id = world_id(new_index, world_index);
                let old_w_id = world.id.clone();

                if old_w_id != new_w_id {
                    old_to_new_world.insert(old_w_id.to_string(), new_w_id.to_string());
                    world.id = new_w_id;
                }
                world.index = world_index;
            }
        }

        self.apply_id_migrations(old_to_new_sys.clone(), old_to_new_world.clone());
        (old_to_new_sys, old_to_new_world)
    }

    fn reindex_stable(&mut self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut next_sys_index = self.systems.iter()
            .map(|s| s.index)
            .max()
            .unwrap_or(0) + 1;

        let old_to_new_sys = BTreeMap::new();
        let old_to_new_world = BTreeMap::new();

        for sys in &mut self.systems {
            // If system has no ID or invalid index (0), assign new one
            if sys.id.is_empty() || sys.index == 0 {
                let new_id = system_id(next_sys_index);
                sys.id = new_id;
                sys.index = next_sys_index;
                next_sys_index += 1;
            }

            let mut next_world_index = sys.worlds.iter()
                .map(|w| w.index)
                .max()
                .unwrap_or(0) + 1;

            for world in &mut sys.worlds {
                if world.id.is_empty() || world.index == 0 {
                    let new_id = world_id(sys.index, next_world_index);
                    world.id = new_id;
                    world.index = next_world_index;
                    next_world_index += 1;
                }
            }
        }

        // Note: in stable mode, we don't really have migrations for existing entities
        // unless we explicitly changed something.
        self.apply_id_migrations(old_to_new_sys.clone(), old_to_new_world.clone());
        (old_to_new_sys, old_to_new_world)
    }

    fn apply_id_migrations(&mut self, sys_map: BTreeMap<String, String>, world_map: BTreeMap<String, String>) {
        if sys_map.is_empty() && world_map.is_empty() {
            return;
        }

        // Update routes
        for route in &mut self.routes {
            if let Some(new_from) = sys_map.get(route.from_system_id.as_str()) {
                route.from_system_id = SystemId::new(new_from.clone());
            }
            if let Some(new_to) = sys_map.get(route.to_system_id.as_str()) {
                route.to_system_id = SystemId::new(new_to.clone());
            }
            route.id = route_id(&route.from_system_id, &route.to_system_id);
        }

        // Update factions
        for faction in &mut self.factions {
            for sys_id in &mut faction.system_presence {
                if let Some(new_id) = sys_map.get(sys_id.as_str()) {
                    *sys_id = SystemId::new(new_id.clone());
                }
            }
            for world_id in &mut faction.world_presence {
                if let Some(new_id) = world_map.get(world_id.as_str()) {
                    *world_id = WorldId::new(new_id.clone());
                }
            }
        }

        // Record in id_history
        for (old, new) in sys_map {
            self.id_history.insert(old, new);
        }
        for (old, new) in world_map {
            self.id_history.insert(old, new);
        }
    }
}
