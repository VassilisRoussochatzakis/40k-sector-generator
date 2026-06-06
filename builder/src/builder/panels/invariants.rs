//! Invariants panel (§V2). Renders the post-generation
//! [`sectorforge::InvariantReport`] as a tree grouped by entity stratum
//! (systems / worlds / routes / factions / regions / manifest / other).
//! Each leaf is a button that focuses the offending entity by writing into
//! the [`crate::builder::BuilderState`] selection mailbox so the inspector
//! tabs can jump to it.
//!
//! §COLUMNS — master-detail: the stratum-grouped violation list lives in a
//! persistent left rail (`SidePanel::left("invariants_list")`, keeping the
//! per-entity jump); the selected violation's detail, the entity deep-link,
//! "Re-run checks", and the read-only §V5 catalogue live in the filling right
//! `CentralPanel`. The header summary stays full-width on top. Which violation
//! is "selected" is pure view state — keyed in `ui.data_mut` temp (no model
//! state, no command bus); the right pane re-finds the matching violation from
//! the live report each frame.
//!
//! The §V5 invariant catalogue (read-only, list of every code that may fire)
//! lives in its own sub-section in the right pane so users can audit what is
//! checked even when the current sector is clean.

use std::collections::BTreeMap;

use egui::{Color32, RichText};

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::invariants::{InvariantReport, InvariantViolation};
use sectorforge_gui_core::{card, palette, ui_kit::{self, labeled}};

use crate::builder::BuilderState;

/// Humanize a stratum key for display. The raw key stays the section id-salt so
/// collapse state persists; only the visible title changes.
fn stratum_title(stratum: &str) -> &'static str {
    match stratum {
        "systems" => "Systems",
        "worlds" => "Worlds",
        "routes" => "Routes",
        "factions" => "Factions",
        "regions" => "Regions",
        "economy" => "Economy",
        "manifest" => "Manifest / metadata",
        _ => "Other",
    }
}

/// Stratum groupings used for the panel tree. Order matters — it controls the
/// display order in the panel.
const STRATA: &[&str] = &[
    "systems", "worlds", "routes", "factions", "regions", "economy", "manifest", "other",
];

/// §COLUMNS — view-state key identifying the focused violation. Stored in
/// `ui.data_mut` temp under this id so the right pane can re-find the matching
/// violation from the live report; never persisted, never a model field.
const SELECTED_KEY_ID: &str = "invariants_selected_violation";

pub(crate) fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Invariants");
    ui.label(
        RichText::new("Automatic structural checks on the generated sector — anything broken shows up here, with a one-click jump to the culprit.")
            .color(Color32::DARK_GRAY),
    );
    ui.separator();

    let Some(report) = state.invariant_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No checks have run yet. Generate the sector, then re-run the checks.",
        );
        return;
    };

    // Header summary stays full-width on top.
    render_summary(ui, &report);
    ui.separator();

    // §COLUMNS — master-detail. Two separate statements so the first &mut state
    // borrow (the list) ends before the second (the detail).
    egui::SidePanel::left("invariants_list")
        .resizable(true)
        .default_width(320.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| show_violation_list(ui, state, &report));

    egui::CentralPanel::default().show_inside(ui, |ui| show_violation_detail(ui, state, &report));
}

// ── selection key (view state) ──────────────────────────────────────────────

/// Build a stable selection key from a violation's content. `InvariantViolation`
/// has no unique id and does not derive `Hash`/`Eq`, so we key on code + path +
/// message together.
fn violation_key(vio: &InvariantViolation) -> String {
    format!(
        "{}|{}|{}",
        vio.code,
        vio.path.as_deref().unwrap_or(""),
        vio.message,
    )
}

fn selected_key(ui: &egui::Ui) -> Option<String> {
    ui.data_mut(|d| d.get_temp::<String>(egui::Id::new(SELECTED_KEY_ID)))
}

fn set_selected_key(ui: &egui::Ui, key: String) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(SELECTED_KEY_ID), key));
}

// ── header summary (full width) ─────────────────────────────────────────────

fn render_summary(ui: &mut egui::Ui, report: &InvariantReport) {
    ui.horizontal(|ui| {
        if report.ok {
            ui.colored_label(palette::success(), "✓ Sector is sound");
        } else {
            ui.colored_label(
                palette::danger(),
                format!("✗ {} problem(s) found", report.violations.len()),
            );
        }
    });
}

// ── violation list (left rail) ──────────────────────────────────────────────

fn show_violation_list(ui: &mut egui::Ui, state: &mut BuilderState, report: &InvariantReport) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if report.violations.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No invariant violations — the sector is structurally sound.",
                );
                return;
            }
            let grouped = group_by_stratum(&report.violations);
            for stratum in STRATA {
                let Some(group) = grouped.get(*stratum) else {
                    continue;
                };
                ui_kit::collapsing_section(
                    ui,
                    ("inv_stratum", stratum),
                    &format!("{} ({})", stratum_title(stratum), group.len()),
                    true,
                    |ui| {
                        for (idx, vio) in group.iter().enumerate() {
                            violation_row(ui, state, vio, idx);
                        }
                    },
                );
            }
        });
}

fn violation_row(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    vio: &InvariantViolation,
    idx: usize,
) {
    let key = violation_key(vio);
    let is_selected = selected_key(ui).as_deref() == Some(key.as_str());
    // §BEAUTY: animated selectable plate. The severity dot keeps its
    // meaning-carrying colour, the message text leads, and the raw check code
    // stays behind the hover — all inside the plate content. The salt combines
    // the violation key with its stratum index so it is unique even if two rows
    // somehow shared a key.
    let (resp, _) = card::selectable_plate(ui, ("inv_row", &key, idx), is_selected, |ui| {
        ui.label(RichText::new("●").color(palette::danger()));
        // Lead with the human-readable message; selecting a row pins the
        // violation into the right detail pane and also focuses the offending
        // entity via the selection mailbox (existing jump).
        ui.label(RichText::new(&vio.message))
            .on_hover_text(format!("check: {}", vio.code));
    });
    if resp.clicked() {
        set_selected_key(ui, key.clone());
        jump_to(state, vio);
    }
}

// ── violation detail + catalogue (right pane) ───────────────────────────────

fn show_violation_detail(ui: &mut egui::Ui, state: &mut BuilderState, report: &InvariantReport) {
    ui.horizontal(|ui| {
        if ui
            .button("🔄  Re-run checks")
            .on_hover_text("Re-run every structural check against the current sector")
            .clicked()
        {
            state.invariant_report = Some(sectorforge::invariants::check_sector(&state.sector));
        }
    });
    ui.separator();

    let selected = selected_key(ui);
    let vio = selected
        .as_deref()
        .and_then(|key| report.violations.iter().find(|v| violation_key(v) == key));

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if let Some(vio) = vio {
                render_detail_card(ui, state, vio);
            } else if report.violations.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No invariant violations — the sector is structurally sound.",
                );
            } else {
                ui_kit::placeholder(
                    ui,
                    "Pick a problem from the list on the left to see details.",
                );
            }

            ui.separator();
            render_catalogue(ui);
        });
}

fn render_detail_card(ui: &mut egui::Ui, state: &mut BuilderState, vio: &InvariantViolation) {
    ui_kit::section(ui, "Problem details", |ui| {
        ui_kit::reading_column(ui, 720.0, |ui| {
            // Lead with the plain-language description of what's wrong.
            ui.horizontal(|ui| {
                ui.colored_label(palette::danger(), "●");
                ui.label(RichText::new(&vio.message).strong());
            });
            ui.add_space(8.0);

            // Raw check id, demoted to a dim secondary token behind a hover.
            labeled(
                ui,
                "Check",
                "The invariant code that fired (schema: code). See the catalogue below for what each code means.",
                |ui| {
                    ui.monospace(&vio.code);
                },
            );

            if let Some(path) = &vio.path {
                labeled(
                    ui,
                    "Located at",
                    "Where in the sector the problem sits (schema: path) — the entity the jump below selects.",
                    |ui| {
                        ui.monospace(path);
                    },
                );

                // Focus deep-link: jump the relevant inspector tab to the entity.
                if focusable(path) {
                    ui.add_space(8.0);
                    if ui
                        .button("▸  Jump to entity")
                        .on_hover_text("Select this entity in the relevant inspector tab")
                        .clicked()
                    {
                        jump_to(state, vio);
                    }
                }
            }
        });
    });
}

/// Whether a violation `path` resolves to an entity the §V2 jump can focus.
fn focusable(path: &str) -> bool {
    parse_system_world(path).is_some()
        || parse_path(path, "systems.").is_some()
        || parse_path(path, "routes.").is_some()
        || parse_path(path, "factions.").is_some()
        || parse_path(path, "regions.").is_some()
}

fn jump_to(state: &mut BuilderState, vio: &InvariantViolation) {
    let Some(path) = vio.path.as_deref() else {
        return;
    };
    if let Some((system, world)) = parse_system_world(path) {
        state.selection.system_id = Some(SystemId::new(system.as_str()));
        state.selection.world_id = Some(WorldId::new(world.as_str()));
        return;
    }
    if let Some(system) = parse_path(path, "systems.") {
        state.selection.system_id = Some(SystemId::new(system.as_str()));
        return;
    }
    if let Some(route) = parse_path(path, "routes.") {
        state.selection.route_id = Some(RouteId::new(route.as_str()));
        return;
    }
    if let Some(faction) = parse_path(path, "factions.") {
        state.selection.faction_id = Some(FactionId::new(faction.as_str()));
        return;
    }
    if let Some(region) = parse_path(path, "regions.") {
        state.selection.region_id = Some(region);
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
        &format!(
            "Invariant catalogue — what gets checked ({} codes)",
            INVARIANT_CODES.len()
        ),
        false,
        |ui| {
            ui.label(
                RichText::new("Every structural check the sector is held to:")
                    .small()
                    .color(palette::chrome_text_dim()),
            );
            for (code, desc) in INVARIANT_CODES {
                ui.horizontal(|ui| {
                    ui.monospace(*code);
                    ui.colored_label(palette::chrome_text_dim(), *desc);
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

    #[test]
    fn violation_key_distinguishes_path() {
        let a = vio("systems.sys-0001");
        let b = vio("systems.sys-0002");
        assert_ne!(violation_key(&a), violation_key(&b));
        // Stable across re-derive so selection survives a frame.
        assert_eq!(violation_key(&a), violation_key(&vio("systems.sys-0001")));
    }

    #[test]
    fn focusable_matches_known_strata() {
        assert!(focusable("systems.sys-0001"));
        assert!(focusable("systems.sys-0001.worlds.sys-0001-w01"));
        assert!(focusable("routes.route-a-b"));
        assert!(focusable("factions.imperial"));
        assert!(focusable("regions.reg-1"));
        assert!(!focusable("manifest.checksum"));
    }
}
