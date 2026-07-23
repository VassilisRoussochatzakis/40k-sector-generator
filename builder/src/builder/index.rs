//! Deterministic lookup index over a `GeneratedSector`. Rebuilt after every
//! structural mutation by the command bus. `BTreeMap` (not `HashMap`) so
//! iteration order is stable and downstream JSON stays byte-stable.

use std::collections::BTreeMap;

use sectorforge::ids::{RouteId, SystemId, WorldId};
use sectorforge::sector_model::GeneratedSector;

#[derive(Debug, Clone, Default)]
pub struct BuilderIndex {
    pub systems: BTreeMap<SystemId, usize>,
    pub worlds: BTreeMap<WorldId, (usize, usize)>,
    pub routes: BTreeMap<RouteId, usize>,
}

impl BuilderIndex {
    pub fn rebuild(sector: &GeneratedSector) -> Self {
        let mut idx = Self::default();
        for (i, sys) in sector.systems.iter().enumerate() {
            idx.systems.insert(sys.id.clone(), i);
            for (j, w) in sys.worlds.iter().enumerate() {
                idx.worlds.insert(w.id.clone(), (i, j));
            }
        }
        for (i, r) in sector.routes.iter().enumerate() {
            idx.routes.insert(r.id.clone(), i);
        }
        idx
    }
}
