//! Blank-DTO constructors — scaffold an empty [`GeneratedSector`] or a single
//! empty system / world / route / faction with sane placeholder fields.
//!
//! Pure domain logic with no RNG and no UI dependency (F-S1): the viewer's
//! in-place editor and any future caller share these instead of hand-rolling a
//! struct literal per call site. Distinct from [`super::mutation`], which mutates
//! an *existing* sector under the invariant-bookkeeping contract; these only
//! *construct* a fresh, internally-consistent blank value.

use crate::ids::{route_id, world_id, FactionId, SystemId};

use super::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSystem,
    GeneratedWorld, HexCoord, RouteStability, RouteType, SystemKind,
};

/// A blank sector: identity + manifest filled, all collections empty.
/// Free-function alias for [`GeneratedSector::empty`].
pub fn empty_sector(id: &str, title: &str, seed: &str, width: u32, height: u32) -> GeneratedSector {
    GeneratedSector::empty(id, title, seed, width, height)
}

/// A blank system at `coord` with the given identity/kind/star and no worlds.
/// [`GeneratedSystem::new_at`] with the kind/star parameterised.
pub fn empty_system(
    id: SystemId,
    index: usize,
    name: String,
    coord: HexCoord,
    kind: SystemKind,
    star: Option<GeneratedStar>,
) -> GeneratedSystem {
    GeneratedSystem {
        kind,
        star,
        ..GeneratedSystem::new_at(id, index, coord, &name)
    }
}

/// A blank world (dead/airless placeholder) in `system_index` at slot `index`.
/// [`GeneratedWorld::new`] with the id derived from the indices.
pub fn empty_world(system_index: usize, index: usize, name: String) -> GeneratedWorld {
    GeneratedWorld::new(world_id(system_index, index), index, &name)
}

/// A blank stable warp lane between two systems (distance 1, no controls).
pub fn empty_route(from: SystemId, to: SystemId) -> GeneratedRoute {
    let id = route_id(&from, &to);
    GeneratedRoute {
        id,
        from_system_id: from,
        to_system_id: to,
        route_type: RouteType::StableWarpLane,
        stability: RouteStability::Stable,
        distance: 1,
        tags: Vec::new(),
        controls: Vec::new(),
    }
}

/// A blank imperial/neutral faction named after its id.
pub fn empty_faction(id: &FactionId) -> GeneratedFaction {
    GeneratedFaction {
        id: id.clone(),
        name: id.as_str().into(),
        kind: "imperial".into(),
        disposition: "neutral".into(),
        subfactions: Vec::new(),
        system_presence: Vec::new(),
        world_presence: Vec::new(),
        power: Default::default(),
    }
}
