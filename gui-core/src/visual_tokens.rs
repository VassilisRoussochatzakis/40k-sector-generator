//! Semantic map tokens between sector data and egui painting.
//!
//! Apps pass model data to [`crate::sector_view::SectorView`]. The shared
//! renderer converts model enums into these tokens before drawing, so adding a
//! system kind, route type, or region condition requires one exhaustive match
//! here instead of app-local paint branches.

/// System map glyph. Re-exported from the core lib so the live egui renderer
/// and the PNG/SVG exporters classify systems through one shared
/// [`sectorforge::sector_model::SystemGlyph`]. The `MapSystemGlyph` alias keeps
/// the historical gui-core name at existing call sites.
pub use sectorforge::sector_model::SystemGlyph as MapSystemGlyph;
