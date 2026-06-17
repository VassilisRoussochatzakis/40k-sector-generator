//! Fixed option lists for the editor's non-world dropdowns: star-colour codes
//! and the route type/stability tables (`serde` keys + editor labels). The
//! world-classification lists that used to live here were hand-forked CamelCase
//! arrays; they drifted from `worlds.rs` and are gone — the inspector now reads
//! `T::VARIANTS` + `display_name()` directly (see the note below + #33).

use sectorforge::sector_model::RouteType;

pub const STAR_COLOUR_CODES: &[&str] = &["O", "B", "A", "F", "G", "K", "M"];

pub(crate) fn star_colour_name(code: &str) -> &'static str {
    match code {
        "O" => "blue hypergiant",
        "B" => "blue-white",
        "A" => "white",
        "F" => "yellow-white",
        "G" => "yellow",
        "K" => "orange dwarf",
        "M" => "red dwarf",
        _ => "unknown",
    }
}

// #33: the world-classification option lists (`WORLD_TYPES`, `ATMOSPHERES`,
// `TEMPERATURES`, `BIOSPHERES`, `POPULATIONS`, `TECH_LEVELS`, `GOVERNMENTS`,
// `NOTABLE_FEATURES`) used to live here as hand-forked CamelCase string arrays.
// They drifted from the canonical `worlds.rs` tables — the inspector showed
// "AgriWorld" while the `worlds.toml` data editor showed "Agri-World". The
// inspector (`world_panel.rs`) now drives those dropdowns straight off
// `T::VARIANTS` + `display_name()` via the shared `widgets::enum_combo`, so the
// forks are deleted and the two surfaces agree. Star-colour codes and the route
// tuple tables below stay — they are consumed by `system_panel`/`routes_panel`
// and are not variant-name forks.

pub const ROUTE_TYPES: &[(&str, &str)] = &[
    (
        RouteType::StableWarpLane.as_slug(),
        RouteType::StableWarpLane.editor_label(),
    ),
    (
        RouteType::ChartedPassage.as_slug(),
        RouteType::ChartedPassage.editor_label(),
    ),
    (
        RouteType::SecretPassage.as_slug(),
        RouteType::SecretPassage.editor_label(),
    ),
    (RouteType::Webway.as_slug(), RouteType::Webway.editor_label()),
    (
        RouteType::BlackShip.as_slug(),
        RouteType::BlackShip.editor_label(),
    ),
    (
        RouteType::SmugglingLane.as_slug(),
        RouteType::SmugglingLane.editor_label(),
    ),
];

pub const ROUTE_STABILITIES: &[(&str, &str)] = &[
    ("stable", "stable"),
    ("unstable", "unstable"),
    ("hazardous", "hazardous"),
    ("perilous", "perilous"),
];
