//! Invariants panel (§V2). Renders the post-generation
//! [`sectorforge::InvariantReport`] as a tree grouped by entity stratum
//! (systems / worlds / routes / factions / regions / manifest / other).
//! Each leaf is a button that focuses the offending entity by writing into
//! the [`crate::builder::BuilderState`] selection mailbox so the inspector
//! tabs can jump to it.
//!
//! The §V5 invariant catalogue (read-only, list of every code that may fire)
//! lives in its own sub-section so users can audit what is checked even when
//! the current sector is clean.

use std::collections::BTreeMap;

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::invariants::{InvariantReport, InvariantViolation};

use crate::builder::BuilderState;

/// Stratum groupings used for the panel tree. Order matters — it controls the
/// display order in the panel.
const STRATA: &[&str] = &[
    "systems", "worlds", "routes", "factions", "regions", "economy", "manifest", "other",
];

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.heading("Invariants");
        if ui.button("Re-check now").clicked() {
            state.invariant_report = Some(sectorforge::invariants::check_sector(&state.sector));
        }
    });
    ui.separator();

    let Some(report) = state.invariant_report.clone() else {
        ui.colored_label(egui::Color32::GRAY, "no invariant report yet");
        return;
    };

    render_summary(ui, &report);
    ui.separator();

    if report.violations.is_empty() {
        ui.colored_label(egui::Color32::GREEN, "✓ no invariant violations");
    } else {
        let grouped = group_by_stratum(&report.violations);
        for stratum in STRATA {
            let Some(group) = grouped.get(*stratum) else {
                continue;
            };
            egui::CollapsingHeader::new(format!("{stratum} ({})", group.len()))
                .default_open(true)
                .id_salt(format!("invariants-{stratum}"))
                .show(ui, |ui| {
                    for vio in group {
                        violation_row(ui, state, vio);
                    }
                });
        }
    }

    ui.separator();
    render_catalogue(ui);
}

fn render_summary(ui: &mut egui::Ui, report: &InvariantReport) {
    ui.horizontal(|ui| {
        if report.ok {
            ui.colored_label(egui::Color32::GREEN, "✓ ok");
        } else {
            ui.colored_label(egui::Color32::RED, "✗ violations");
        }
        ui.label(format!("{} violation(s)", report.violations.len()));
    });
}

fn violation_row(ui: &mut egui::Ui, state: &mut BuilderState, vio: &InvariantViolation) {
    ui.horizontal(|ui| {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), &vio.code);
        let label = match &vio.path {
            Some(p) => format!("{p}: {}", vio.message),
            None => vio.message.clone(),
        };
        if ui.link(label).clicked() {
            jump_to(state, vio);
        }
    });
}

fn jump_to(state: &mut BuilderState, vio: &InvariantViolation) {
    let Some(path) = vio.path.as_deref() else {
        return;
    };
    if let Some((system, world)) = parse_system_world(path) {
        state.selected_system_id = Some(SystemId::new(system.as_str()));
        state.selected_world_id = Some(WorldId::new(world.as_str()));
        return;
    }
    if let Some(system) = parse_path(path, "systems.") {
        state.selected_system_id = Some(SystemId::new(system.as_str()));
        return;
    }
    if let Some(route) = parse_path(path, "routes.") {
        state.selected_route_id = Some(RouteId::new(route.as_str()));
        return;
    }
    if let Some(faction) = parse_path(path, "factions.") {
        state.selected_faction_id = Some(FactionId::new(faction.as_str()));
        return;
    }
    if let Some(region) = parse_path(path, "regions.") {
        state.selected_region_id = Some(region);
    }
}

/// Extract `systems.<sys>.worlds.<world>` if `path` matches.
fn parse_system_world(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("systems.")?;
    let mut parts = rest.splitn(2, '.');
    let system = parts.next()?.to_string();
    let after = parts.next()?;
    let world = after.strip_prefix("worlds.")?;
    let world_id = world.split('.').next()?.to_string();
    Some((system, world_id))
}

/// Extract the first id token after `prefix` from a `<prefix><id>(.suffix)?`
/// path, returning `None` when the prefix doesn't match.
fn parse_path(path: &str, prefix: &str) -> Option<String> {
    path.strip_prefix(prefix)
        .map(|rest| rest.split('.').next().unwrap_or(rest).to_string())
}

fn group_by_stratum(
    violations: &[InvariantViolation],
) -> BTreeMap<String, Vec<InvariantViolation>> {
    let mut out: BTreeMap<String, Vec<InvariantViolation>> = BTreeMap::new();
    for v in violations {
        let key = stratum_of(v).to_string();
        out.entry(key).or_default().push(v.clone());
    }
    out
}

fn stratum_of(v: &InvariantViolation) -> &'static str {
    let head = v
        .path
        .as_deref()
        .and_then(|p| p.split(['.', '[']).next())
        .unwrap_or("");
    match head {
        "systems" => {
            if v.path.as_deref().is_some_and(|p| p.contains(".worlds")) {
                "worlds"
            } else {
                "systems"
            }
        }
        "routes" => "routes",
        "factions" => "factions",
        "regions" => "regions",
        "economy" => "economy",
        "manifest" => "manifest",
        _ => "other",
    }
}

/// §V5: read-only catalogue of every invariant the checker emits. Not the
/// dynamic list of *firing* invariants — the static list of codes so users
/// can audit what is enforced even on a clean sector. Sourced from
/// [`sectorforge::invariants`] (codes mirrored manually; promoted to a
/// shared const list in Phase E §V5).
const INVARIANT_CATALOGUE: &[&str] = &[
    "DUPLICATE_SYSTEM_ID",
    "DUPLICATE_WORLD_ID_IN_SYSTEM",
    "DUPLICATE_WORLD_ID_GLOBAL",
    "DUPLICATE_COORDINATE",
    "COORD_OUT_OF_BOUNDS",
    "SYSTEM_INDEX_ZERO",
    "WORLD_INDEX_OR_ORBIT_ZERO",
    "WORLD_ID_PREFIX",
    "WORLD_TAG_NAMESPACE_MISSING",
    "WORLD_TAG_DUPLICATE",
    "ROUTE_SELF_REFERENCE",
    "ROUTE_UNKNOWN_FROM",
    "ROUTE_UNKNOWN_TO",
    "ROUTE_DUPLICATE_UNDIRECTED",
    "ROUTE_DISTANCE_MISMATCH",
    "FACTION_SYSTEM_PRESENCE_UNKNOWN",
    "FACTION_WORLD_PRESENCE_UNKNOWN",
    "WORLD_FACTION_MISSING_SUMMARY",
    "WORLD_CLAIM_UNKNOWN_FACTION",
    "WORLD_CLAIM_STRENGTH_OUT_OF_RANGE",
    "PRESENCE_DIMENSION_OUT_OF_RANGE",
    "PRIMARY_FACTION_MISSING_SUMMARY",
    "REGION_HEX_OUT_OF_BOUNDS",
    "REGION_HEX_OVERLAP",
    "REGION_ISOLATES_SECTOR",
    "ECONOMY_ENABLED_NO_WORLDS",
    "MANIFEST_SYSTEM_COUNT_MISMATCH",
    "MANIFEST_WORLD_COUNT_MISMATCH",
    "MANIFEST_ROUTE_COUNT_MISMATCH",
];

fn render_catalogue(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(format!(
        "Invariant catalogue ({} codes)",
        INVARIANT_CATALOGUE.len()
    ))
    .show(ui, |ui| {
        for code in INVARIANT_CATALOGUE {
            ui.label(*code);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vio(path: &str) -> InvariantViolation {
        InvariantViolation {
            code: "X".into(),
            message: "m".into(),
            path: Some(path.into()),
        }
    }

    #[test]
    fn stratum_split_system_vs_world() {
        assert_eq!(stratum_of(&vio("systems.sys-0001")), "systems");
        assert_eq!(
            stratum_of(&vio("systems.sys-0001.worlds.sys-0001-w01")),
            "worlds"
        );
    }

    #[test]
    fn parse_system_world_extracts_ids() {
        let (sys, world) = parse_system_world("systems.sys-0001.worlds.sys-0001-w02.tags").unwrap();
        assert_eq!(sys, "sys-0001");
        assert_eq!(world, "sys-0001-w02");
    }

    #[test]
    fn parse_path_picks_first_id() {
        assert_eq!(
            parse_path("routes.route-sys-0001-sys-0002.distance", "routes."),
            Some("route-sys-0001-sys-0002".to_string())
        );
        assert_eq!(
            parse_path("factions.imperial", "factions."),
            Some("imperial".into())
        );
    }
}
