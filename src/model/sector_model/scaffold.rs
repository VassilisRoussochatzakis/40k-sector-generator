//! Blank-DTO constructors — scaffold an empty [`GeneratedSector`] or a single
//! empty system / world / route / faction with sane placeholder fields.
//!
//! Pure domain logic with no RNG and no UI dependency (F-S1): the viewer's
//! in-place editor and any future caller share these instead of hand-rolling a
//! struct literal per call site. Distinct from [`super::mutation`], which mutates
//! an *existing* sector under the invariant-bookkeeping contract; these only
//! *construct* a fresh, internally-consistent blank value.

use crate::ids::{route_id, world_id, FactionId, SystemId};
use crate::worlds::{
    Atmosphere, Biosphere, Government, Population, StarColour, Temperature, TechLevel, WorldType,
};

use super::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSystem,
    GeneratedWorld, GenerationManifest, HexCoord, RouteStability, RouteType, SystemKind, WorldDto,
};

/// A blank sector: identity + manifest filled, all collections empty.
pub fn empty_sector(id: &str, title: &str, seed: &str, width: u32, height: u32) -> GeneratedSector {
    GeneratedSector {
        id: id.into(),
        title: title.into(),
        seed: seed.into(),
        generator_name: "sectorforge".into(),
        generator_version: "0.1.0".into(),
        width,
        height,
        manifest: GenerationManifest {
            project_id: id.into(),
            generated_at_policy: "unknown".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: seed.into(),
            seed_hash: "".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: Default::default(),
            settings_digest: "".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        },
        systems: Vec::new(),
        routes: Vec::new(),
        factions: Vec::new(),
        relations: Default::default(),
        regions: Vec::new().into(),
        economy: Default::default(),
        chronicle: Default::default(),
        influence_field: Default::default(),
        power_projection: Default::default(),
        id_history: Default::default(),
    }
}

/// A blank system at `coord` with the given identity/kind/star and no worlds.
pub fn empty_system(
    id: SystemId,
    index: usize,
    name: String,
    coord: HexCoord,
    kind: SystemKind,
    star: Option<GeneratedStar>,
) -> GeneratedSystem {
    GeneratedSystem {
        id,
        index,
        name: name.into(),
        coord,
        kind,
        star,
        worlds: Vec::new(),
        primary_factions: Vec::new(),
        tags: Vec::new(),
        notes: Vec::new(),
        control: Default::default(),
        stability: Default::default(),
        orbital_assets: Vec::new(),
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    }
}

/// A blank world (dead/airless placeholder) in `system_index` at slot `index`.
pub fn empty_world(system_index: usize, index: usize, name: String) -> GeneratedWorld {
    let id = world_id(system_index, index);
    GeneratedWorld {
        id,
        index,
        name: name.into(),
        orbit: 1,
        source_row_index: 0,
        world: WorldDto {
            star_colour: StarColour::White,
            world_type: WorldType::DeadWorld,
            atmosphere: Atmosphere::Airless,
            temperature: Temperature::Temperate,
            biosphere: Biosphere::Nonexistent,
            population: Population::Uninhabited,
            tech_level: TechLevel::Low,
            government: Government::None,
            notable_features: Vec::new(),
        },
        factions: Vec::new(),
        tags: Vec::new(),
        notes: Vec::new(),
        claims: Vec::new(),
        control: Default::default(),
        stability: Default::default(),
        regions: Vec::new(),
        conflict: Default::default(),
        intel: Default::default(),
    }
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
