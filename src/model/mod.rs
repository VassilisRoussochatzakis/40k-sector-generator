//! Core data model + foundational primitives.
//!
//! Grouped here: the sector output DTOs (`sector_model`), typed entity IDs
//! (`ids`), top-level `SectorError`, deterministic RNG, and the variant-name
//! ↔ enum bridge used by config loaders.

pub mod errors;
pub mod ids;
pub mod rng;
pub mod sector_model;
pub mod taxonomy;
