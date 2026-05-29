//! Semantic map tokens between sector data and egui painting.
//!
//! Apps pass model data to [`crate::sector_view::SectorView`]. The shared
//! renderer converts model enums into these tokens before drawing, so adding a
//! system kind, route type, or region condition requires one exhaustive match
//! here instead of app-local paint branches.

use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{
    GeneratedSystem, RoutePattern, RouteType, RouteViewMode, SystemKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapSystemGlyph {
    Star,
    UncataloguedStar,
    SpecialLocation,
    BlackHole,
    WarpAnomaly,
    SpaceStation,
}

impl MapSystemGlyph {
    #[must_use]
    pub fn from_system(system: &GeneratedSystem) -> Self {
        match system.kind {
            SystemKind::Star => {
                if system.star.is_some() {
                    Self::Star
                } else {
                    Self::UncataloguedStar
                }
            }
            SystemKind::SpecialLocation => {
                if system.star.is_some() {
                    Self::Star
                } else {
                    Self::SpecialLocation
                }
            }
            SystemKind::BlackHole => {
                if system.star.is_some() {
                    Self::Star
                } else {
                    Self::BlackHole
                }
            }
            SystemKind::WarpAnomaly => {
                if system.star.is_some() {
                    Self::Star
                } else {
                    Self::WarpAnomaly
                }
            }
            SystemKind::SpaceStation => {
                if system.star.is_some() {
                    Self::Star
                } else {
                    Self::SpaceStation
                }
            }
            _ => Self::UncataloguedStar,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapRouteVisual {
    StableWarpLane,
    ChartedPassage,
    SecretPassage,
    Webway,
    BlackShip,
    SmugglingLane,
    WarpRoute,
    WebwayThread,
}

impl MapRouteVisual {
    #[must_use]
    pub fn from_route_type(route_type: RouteType, mode: RouteViewMode) -> Self {
        match mode {
            RouteViewMode::Detailed => match route_type {
                RouteType::StableWarpLane => Self::StableWarpLane,
                RouteType::ChartedPassage => Self::ChartedPassage,
                RouteType::SecretPassage => Self::SecretPassage,
                RouteType::Webway => Self::Webway,
                RouteType::BlackShip => Self::BlackShip,
                RouteType::SmugglingLane => Self::SmugglingLane,
                _ => Self::ChartedPassage,
            },
            RouteViewMode::TopLevel => match route_type {
                RouteType::Webway => Self::WebwayThread,
                RouteType::StableWarpLane
                | RouteType::ChartedPassage
                | RouteType::SecretPassage
                | RouteType::BlackShip
                | RouteType::SmugglingLane => Self::WarpRoute,
                _ => Self::WarpRoute,
            },
            _ => Self::WarpRoute,
        }
    }

    #[must_use]
    pub const fn pattern(self) -> RoutePattern {
        match self {
            Self::StableWarpLane | Self::WarpRoute => RoutePattern::Solid,
            Self::ChartedPassage => RoutePattern::Dashed,
            Self::SecretPassage => RoutePattern::Dotted,
            Self::Webway | Self::WebwayThread => RoutePattern::Burst,
            Self::BlackShip => RoutePattern::Quartet,
            Self::SmugglingLane => RoutePattern::Gravel,
        }
    }
}

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
    fn route_visual_tokens_match_model_defaults() {
        for route_type in RouteType::ALL {
            for mode in [RouteViewMode::Detailed, RouteViewMode::TopLevel] {
                let token = MapRouteVisual::from_route_type(route_type, mode);
                assert_eq!(token.pattern(), route_type.pattern(mode));
            }
        }
    }

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
