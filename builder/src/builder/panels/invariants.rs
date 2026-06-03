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
use sectorforge_gui_core::ui_kit;

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

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if report.violations.is_empty() {
                ui.colored_label(egui::Color32::GREEN, "✓ no invariant violations");
            } else {
                let grouped = group_by_stratum(&report.violations);
                for stratum in STRATA {
                    let Some(group) = grouped.get(*stratum) else {
                        continue;
                    };
                    ui_kit::collapsing_section(
                        ui,
                        ("inv_stratum", stratum),
                        &format!("{stratum} ({})", group.len()),
                        true,
                        |ui| {
                            for vio in group {
                                violation_row(ui, state, vio);
                            }
                        },
                    );
                }
            }

            ui.separator();
            render_catalogue(ui);
        });
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

/// §V5: render the read-only catalogue of every invariant the checker may
/// emit. This is the static list of *checked* codes (not the dynamic list of
/// *firing* ones) so users can audit what is enforced even on a clean sector.
///
/// The list is sourced verbatim from the authoritative
/// [`sectorforge::invariants::INVARIANT_CODES`] table — the single source of
/// truth shared with the checker — so it can never drift from the codes the
/// engine actually raises.
fn render_catalogue(ui: &mut egui::Ui) {
    use sectorforge::invariants::INVARIANT_CODES;
    ui_kit::collapsing_section(
        ui,
        "inv_catalogue",
        &format!("Invariant catalogue ({} codes)", INVARIANT_CODES.len()),
        false,
        |ui| {
            ui.label(
                egui::RichText::new("Every invariant checked on the live sector:")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            for (code, desc) in INVARIANT_CODES {
                ui.horizontal(|ui| {
                    ui.monospace(*code);
                    ui.colored_label(egui::Color32::GRAY, *desc);
                });
            }
        },
    );
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
