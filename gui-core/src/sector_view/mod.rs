//! Sector hex-grid widget: pointy-top hexes, routes, system disks. Clickable.
//!
//! Split into submodules (AREA_F F3, verbatim carve of the former 1711-LOC
//! `sector_view.rs`):
//! - [`cache`] — `SectorMapCache` per-frame lookup tables.
//! - [`view`] — the `SectorView` widget + `SectorClick` + `show()`.
//! - [`render`] — `SectorGeom`, hex math, paint helpers, geometry tests.
//!
//! The public surface (`SectorView`, `SectorClick`, `SectorMapCache`,
//! `SectorGeom`, `paint_system_rings`) is re-exported here so existing
//! `sector_view::*` paths in the builder, viewer, and tests stay unchanged.

mod cache;
mod render;
mod view;

pub use cache::SectorMapCache;
pub use render::{paint_system_rings, SectorGeom};
pub use view::{SectorClick, SectorView};
