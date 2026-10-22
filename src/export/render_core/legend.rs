//! Legend content shared by PNG and SVG; layout stays in each backend.

use super::routes::ControlKind;
use super::RenderOptions;
use crate::heatmap::HeatmapMode;
use crate::importance::DisplayBucket;
use crate::map_theme::LegendStyle;
use crate::sector_model::{GeneratedSector, RouteKind, RouteStability, RouteType, RouteViewMode};

pub(crate) const STABILITY_ROWS: [(RouteStability, &str); 4] = [
    (RouteStability::Stable, "STABLE"),
    (RouteStability::Unstable, "UNSTABLE"),
    (RouteStability::Hazardous, "HAZARDOUS"),
    (RouteStability::Perilous, "PERILOUS"),
];

pub(crate) const COMPACT_STABILITY_ROWS: [(RouteStability, &str); 4] = [
    (RouteStability::Stable, "STABLE"),
    (RouteStability::Unstable, "UNSTABLE"),
    (RouteStability::Hazardous, "HAZARD"),
    (RouteStability::Perilous, "PERIL"),
];

pub(crate) const CONTROL_ROWS: [(&str, ControlKind); 4] = [
    ("PATROL", ControlKind::Patrol),
    ("TOLL", ControlKind::Toll),
    ("INTERDICTION", ControlKind::Interdiction),
    ("PIRACY", ControlKind::Piracy),
];

pub(crate) fn legend_lines(sector: &GeneratedSector, opts: &RenderOptions) -> usize {
    let heatmap_lines = if matches!(opts.heatmap, HeatmapMode::Off) {
        0
    } else {
        2
    };
    if matches!(opts.theme.legend, LegendStyle::Compact) {
        return 4 + 1 + 5 + 1 + factions_visible(sector) + heatmap_lines;
    }
    let route_type_rows = match opts.route_view_mode {
        RouteViewMode::Detailed => RouteType::ALL.len(),
        RouteViewMode::TopLevel => RouteKind::ALL.len(),
    };
    let route_control_lines = if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        5
    } else {
        0
    };
    4 + 1
        + route_type_rows
        + 1
        + 1
        + 4
        + 1
        + route_control_lines
        + 1
        + factions_visible(sector)
        + heatmap_lines
}

pub(crate) fn faction_row(bucket: &DisplayBucket) -> (String, usize, usize, (u8, u8, u8)) {
    match bucket {
        DisplayBucket::Faction {
            name,
            kind,
            id,
            system_count,
            world_count,
            ..
        } => {
            let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
            (name.to_uppercase(), *system_count, *world_count, style.fill)
        }
        DisplayBucket::Aggregated {
            label,
            system_count,
            world_count,
            ..
        } => (
            label.to_uppercase(),
            *system_count,
            *world_count,
            (140, 140, 150),
        ),
    }
}

/// Number of legend lines the FACTIONS block occupies: a header line plus one
/// row per importance-display bucket. Private helper of [`legend_lines`].
fn factions_visible(sector: &GeneratedSector) -> usize {
    if sector.factions.is_empty() {
        return 0;
    }
    let buckets = crate::importance::compute_display_buckets(
        sector,
        crate::importance::DEFAULT_MINOR_FRACTION,
        crate::importance::DEFAULT_DISPLAY_CAP,
    );
    1 + buckets.len()
}
