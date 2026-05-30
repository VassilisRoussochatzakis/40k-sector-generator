//! Deterministic sector generation pipeline.
//!
//! `generation` is the top-level orchestrator. Around it sit the world pool
//! (`world_pool`), name dictionaries (`names`), faction assignment
//! (`factions` / `faction_style`), route graph + concealed routes (`routes` /
//! `hidden_routes`), warp regions (`regions`), planetary surface regions
//! (`surface_region`), orbital assets (`orbital_assets`), authored sites
//! (`sites`), archetype activations (`archetypes`), and the lazy ECS
//! projection (`world_ecs`).

pub mod archetypes;
pub mod faction_style;
pub mod factions;
pub mod generation;
pub mod hidden_routes;
pub mod names;
pub mod orbital_assets;
pub mod random_sector;
pub mod regions;
pub mod routes;
pub mod sites;
pub mod surface_region;
pub mod world_ecs;
pub mod world_pool;
