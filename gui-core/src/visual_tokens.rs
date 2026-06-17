//! Semantic map tokens between sector data and egui painting.
//!
//! Apps pass model data to [`crate::sector_view::SectorView`]. The shared
//! renderer converts model enums into these tokens before drawing, so adding a
//! system kind, route type, or region condition requires one exhaustive match
//! here instead of app-local paint branches.

use sectorforge::regions::RegionConditionKind;

/// System map glyph. Re-exported from the core lib so the live egui renderer
/// and the PNG/SVG exporters classify systems through one shared
/// [`sectorforge::sector_model::SystemGlyph`]. The `MapSystemGlyph` alias keeps
/// the historical gui-core name at existing call sites.
pub use sectorforge::sector_model::SystemGlyph as MapSystemGlyph;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapRegionOverlay {
    WarpStorm,
    Turbulence,
    CalmCorridor,
    Blackout,
    Anomaly,
    NecropolisDrift,
    BeaconChain,
    EmpyricBleed,
}

impl MapRegionOverlay {
    #[must_use]
    pub const fn from_condition(kind: RegionConditionKind) -> Self {
        match kind {
            RegionConditionKind::WarpStorm => Self::WarpStorm,
            RegionConditionKind::Turbulence => Self::Turbulence,
            RegionConditionKind::CalmCorridor => Self::CalmCorridor,
            RegionConditionKind::Blackout => Self::Blackout,
            RegionConditionKind::Anomaly => Self::Anomaly,
            RegionConditionKind::NecropolisDrift => Self::NecropolisDrift,
            RegionConditionKind::BeaconChain => Self::BeaconChain,
            RegionConditionKind::EmpyricBleed => Self::EmpyricBleed,
            _ => Self::Anomaly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_overlay_tokens_cover_all_conditions() {
        let pairs: &[(RegionConditionKind, MapRegionOverlay)] = &[
            (RegionConditionKind::WarpStorm, MapRegionOverlay::WarpStorm),
            (
                RegionConditionKind::Turbulence,
                MapRegionOverlay::Turbulence,
            ),
            (
                RegionConditionKind::CalmCorridor,
                MapRegionOverlay::CalmCorridor,
            ),
            (RegionConditionKind::Blackout, MapRegionOverlay::Blackout),
            (RegionConditionKind::Anomaly, MapRegionOverlay::Anomaly),
            (
                RegionConditionKind::NecropolisDrift,
                MapRegionOverlay::NecropolisDrift,
            ),
            (
                RegionConditionKind::BeaconChain,
                MapRegionOverlay::BeaconChain,
            ),
            (
                RegionConditionKind::EmpyricBleed,
                MapRegionOverlay::EmpyricBleed,
            ),
        ];
        for (kind, expected) in pairs {
            assert_eq!(
                MapRegionOverlay::from_condition(*kind),
                *expected,
                "overlay mismatch for {:?}",
                kind
            );
        }
    }
}
