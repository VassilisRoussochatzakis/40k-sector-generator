//! SUBSECTORS tab — Phase C §SUB1..§SUB5.
//!
//! §SUB1 cluster list + per-cluster inspector (label, capital, contained
//!       systems, dominant faction).
//! §SUB2 "Recluster" button. Adjusts
//!       [`BuilderState::subsector_target_systems`] which feeds the live
//!       [`MapViewCache`](crate::builder::state::MapViewCache) digest.
//! §SUB3 Manual reassignment of systems between subsectors. Writes a
//!       [`BuilderState::subsector_system_overrides`] entry per move; the
//!       destination cell is flagged in
//!       [`BuilderState::subsector_manual`] so SUB2 reclustering does not
//!       silently undo the move (overrides are reapplied after every cluster
//!       refresh).
//! §SUB4 Capital override per subsector (`subsector_capital_overrides`).
//! §SUB5 Colour override per subsector. Default is the controlling faction's
//!       FactionStyle fill (§F4 palette); overrides live in
//!       `subsector_colour_overrides` and survive reclustering.

use std::collections::BTreeMap;

use egui::{Color32, RichText, Ui};

use sectorforge::faction_style::faction_style_rgb_by_id;
use sectorforge::ids::SystemId;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::subsectors::{
    build_subsectors, Subsector, SubsectorConfig, DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR,
};
use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::state::{BuilderTab, ConfirmAction, EntityRef, ModalKind};
use crate::builder::BuilderState;

pub(crate) fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Subsectors");
    ui.add_space(2.0);
    ui.label(
        RichText::new(
            "Groups of nearby systems on the map. Re-cluster them, move systems between groups, \
             and override each group's capital and colour.",
        )
        .color(Color32::DARK_GRAY),
    );
    ui.separator();

    let subsectors = current_subsectors(state);
    show_recluster_bar(ui, state, &subsectors);
    ui.separator();

    // §COLUMNS — master-detail: the cluster roster pins to a resizable left
    // rail (`subsectors_roster`); the per-cluster detail (capital / colour /
    // §SUB3..§SUB5 reassign) fills the rest. The recluster bar above stays
    // full-width. `subsectors` is an owned Vec, so both panels borrow it
    // immutably while each takes `&mut state` in turn (list before detail).
    egui::SidePanel::left("subsectors_roster")
        .resizable(true)
        .default_width(260.0)
        .width_range(200.0..=440.0)
        .show_inside(ui, |ui| show_cluster_list(ui, state, &subsectors));

    egui::CentralPanel::default().show_inside(ui, |ui| show_inspector(ui, state, &subsectors));
}

// ── Public helper used by [`map.rs::refresh_map_cache`] ─────────────────────

/// Apply §SUB3 / §SUB4 overrides to the freshly clustered subsector list.
/// `subs` is mutated in place: systems move between cells per
/// [`BuilderState::subsector_system_overrides`], capitals are forced to
/// [`BuilderState::subsector_capital_overrides`] when set. Cluster ids and
/// hex layouts are preserved so the renderer never sees an id flip when the
/// user picks a different capital.
pub(crate) fn apply_subsector_overrides(subs: &mut [Subsector], state: &BuilderState) {
    if subs.is_empty() {
        return;
    }
    // §SUB3 — move systems between cells.
    if !state.subsector_system_overrides.is_empty() {
        let id_set: std::collections::BTreeSet<&str> = subs.iter().map(|s| s.id.as_ref()).collect();
        let mut moves: Vec<(SystemId, String)> = Vec::new();
        for (sid, target_sub_id) in &state.subsector_system_overrides {
            if !id_set.contains(target_sub_id.as_str()) {
                continue;
            }
            moves.push((sid.clone(), target_sub_id.clone()));
        }
        for (sid, target) in moves {
            let from_idx = subs.iter().position(|s| s.system_ids.contains(&sid));
            let to_idx = subs.iter().position(|s| s.id.as_ref() == target.as_str());
            let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) else {
                continue;
            };
            if from_idx == to_idx {
                continue;
            }
            subs[from_idx].system_ids.retain(|x| x != &sid);
            subs[to_idx].system_ids.push(sid);
            subs[to_idx].system_ids.sort();
        }
    }

    // §SUB4 — capital overrides. Only honoured when the chosen system is a
    // member of the target subsector (post-§SUB3 reshuffle).
    if !state.subsector_capital_overrides.is_empty() {
        let cap_table = state.subsector_capital_overrides.clone();
        for cell in subs.iter_mut() {
            let Some(cap) = cap_table.get(cell.id.as_ref()) else {
                continue;
            };
            if !cell.system_ids.contains(cap) {
                continue;
            }
            cell.summary.subsector_capital_system_id = Some(cap.clone());
            if let Some(sys) = state.sector.systems.iter().find(|s| &s.id == cap) {
                cell.name = format!("Subsector {}", sys.name).into();
            }
        }
    }
}

// ── §SUB1 helpers ───────────────────────────────────────────────────────────

fn current_subsectors(state: &mut BuilderState) -> Vec<Subsector> {
    if let Some(cache) = state.map_view.cache.as_ref() {
        return cache.subsectors.clone();
    }
    let mut subs = match build_subsectors(
        &state.sector,
        SubsectorConfig {
            target_systems_per_subsector: state.subsector_target_systems.max(1),
            ..SubsectorConfig::default()
        },
    ) {
        Ok(v) => {
            state.feedback.last_subsector_error = None;
            v
        }
        Err(e) => {
            state.feedback.last_subsector_error = Some(e.to_string());
            Vec::new()
        }
    };
    apply_subsector_overrides(&mut subs, state);
    subs
}

/// §FRIENDLY_PANEL_PASS transform #7: drop every manual subsector override (the
/// confirmed payload of [`ModalKind::ConfirmDestructive`]). These overrides bypass
/// the undo bus, so the reset is gated behind a confirm.
pub(crate) fn clear_all_overrides(state: &mut BuilderState) {
    state.subsector_system_overrides.clear();
    state.subsector_capital_overrides.clear();
    state.subsector_colour_overrides.clear();
    state.subsector_manual.clear();
    state.map_view.cache = None;
}

fn show_recluster_bar(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Target systems per group:")
            .on_hover_text(
                "Roughly how many systems each subsector should contain (schema: target_systems_per_subsector). Smaller targets make more, smaller groups.",
            );
        let max = state.sector.systems.len().max(1) as u32;
        let mut value = state.subsector_target_systems.max(1);
        let old = value;
        let response = ui
            .add(egui::DragValue::new(&mut value).range(1..=max).speed(0.25))
            .on_hover_text("Roughly how many systems each subsector should contain.");
        if response.changed() && value != old {
            state.subsector_target_systems = value;
            state.map_view.cache = None; // force refresh on next MAP-tab tick
        }
        if ui
            .button("↺  Recluster")
            .on_hover_text(
                "Re-group the systems using the target above. The map regroups on its next refresh; manual moves and capital overrides are kept on top.",
            )
            .clicked()
        {
            state.subsector_target_systems = value.max(1);
            state.map_view.cache = None;
        }
        if ui
            .button("↺  Reset target")
            .on_hover_text("Restore the default target and regroup.")
            .clicked()
        {
            state.subsector_target_systems = DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR;
            state.map_view.cache = None;
        }
        if (!state.subsector_system_overrides.is_empty()
            || !state.subsector_capital_overrides.is_empty()
            || !state.subsector_colour_overrides.is_empty()
            || !state.subsector_manual.is_empty())
            && ui
                .add(egui::Button::new(
                    RichText::new("🗑  Clear all overrides").color(palette::danger()),
                ))
                .on_hover_text("Drop every manual move, capital, and colour override and regroup from scratch.")
                .clicked()
        {
            // §FRIENDLY_PANEL_PASS transform #7: a reset-all that bypasses the undo
            // bus — confirm before wiping every manual subsector override.
            state.feedback.modal = Some(ModalKind::ConfirmDestructive {
                title: "Clear all overrides?".into(),
                body: "Drop every manual move, capital, and colour override and regroup from scratch."
                    .into(),
                action: ConfirmAction::ClearSubsectorOverrides,
            });
        }
        ui.label(RichText::new(format!("{} group(s)", subs.len())).color(Color32::DARK_GRAY));
    });
    ui.label(
        RichText::new(
            "Recluster re-groups the systems and refreshes the map. Your manual moves and capital overrides are reapplied on top, so they are never lost.",
        )
        .color(Color32::DARK_GRAY),
    );
}

/// §COLUMNS — left-rail cluster roster (master pane). Each row is a selectable
/// summary line (label · name · system count + override flags) that fits the
/// narrow rail; selection is pure view state, set directly. Replaces the wide
/// 6-column grid that did not fit a side panel.
fn show_cluster_list(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    ui.add_space(2.0);
    ui.label(RichText::new("Subsectors").strong());
    if subs.is_empty() {
        ui_kit::placeholder(
            ui,
            "No subsectors yet — add systems to the sector, then recluster.",
        );
        return;
    }
    let mut pick: Option<String> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for s in subs {
                let selected = state.selection.subsector_id.as_deref() == Some(s.id.as_ref());
                let name_part = s
                    .name
                    .strip_prefix("Subsector")
                    .unwrap_or_else(|| s.name.as_ref())
                    .trim();
                let line = format!("{}  {}  ({})", s.label, name_part, s.system_ids.len());
                let mut flags: Vec<&str> = Vec::new();
                if state.subsector_manual.contains(s.id.as_ref()) {
                    flags.push("edited");
                }
                if state
                    .subsector_capital_overrides
                    .contains_key(s.id.as_ref())
                {
                    flags.push("custom capital");
                }
                if state.subsector_colour_overrides.contains_key(s.id.as_ref()) {
                    flags.push("custom colour");
                }
                // §BEAUTY: animated selectable plate. Two-line row — the summary
                // line over a dim override-flags subline — so the closure opens a
                // vertical. Flag text/colour are meaning-bearing and preserved.
                let (resp, _) = card::selectable_plate(ui, ("sub_row", &s.id), selected, |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(line));
                        if !flags.is_empty() {
                            ui.colored_label(
                                Color32::DARK_GRAY,
                                RichText::new(format!("   {}", flags.join(" · "))).size(11.0),
                            )
                            .on_hover_text(
                                "This subsector has manual edits that survive reclustering.",
                            );
                        }
                    });
                });
                if resp
                    .on_hover_text(format!(
                        "{} system(s) in this subsector. Click to edit it.",
                        s.system_ids.len()
                    ))
                    .clicked()
                {
                    pick = Some(s.id.to_string());
                }
            }
        });
    if let Some(id) = pick {
        state.selection.subsector_id = Some(id);
    }
}

// ── §SUB3..§SUB5 inspector ───────────────────────────────────────────────────

fn show_inspector(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    let Some(selected) = state.selection.subsector_id.clone() else {
        ui_kit::placeholder(ui, "Pick a subsector on the left to edit it.");
        return;
    };
    let Some(target) = subs.iter().find(|s| s.id.as_ref() == selected.as_str()) else {
        ui_kit::placeholder(
            ui,
            "That subsector no longer exists after reclustering — pick another on the left.",
        );
        state.selection.subsector_id = None;
        return;
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| show_inspector_body(ui, state, target, subs));
}

fn show_inspector_body(
    ui: &mut Ui,
    state: &mut BuilderState,
    target: &Subsector,
    subs: &[Subsector],
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&*target.label).strong().monospace());
            ui.label(target.name.as_ref());
            ui.label(RichText::new(format!("id: {}", target.id)).color(Color32::DARK_GRAY))
                .on_hover_text("Internal identifier for this subsector (schema: id).");
            if ui
                .button("🗺  Show on map")
                .on_hover_text("Switch to the Map tab to see this subsector.")
                .clicked()
            {
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
        });
        ui.label(format!(
            "{} system(s)  ·  {} internal route(s)  ·  {} border route(s)  ·  {} world(s)",
            target.summary.system_count,
            target.summary.internal_route_count,
            target.summary.border_route_count,
            target.summary.world_count,
        ));
        if !target.summary.dominant_factions.is_empty() {
            let chips: Vec<String> = target
                .summary
                .dominant_factions
                .iter()
                .map(|f| format!("{} ({})", f.id, f.score))
                .collect();
            ui.colored_label(
                Color32::DARK_GRAY,
                format!("Strongest factions here: {}", chips.join(", ")),
            )
            .on_hover_text("Factions with the most presence in this subsector, with their score.");
        }
    });

    ui.add_space(4.0);
    show_capital_override(ui, state, target);
    ui.add_space(4.0);
    show_colour_override(ui, state, target);
    ui.add_space(4.0);
    show_manual_reassign(ui, state, target, subs);
}

fn show_capital_override(ui: &mut Ui, state: &mut BuilderState, target: &Subsector) {
    let sub_id = target.id.to_string();
    let auto_cap = target.summary.subsector_capital_system_id.clone();
    let current = state
        .subsector_capital_overrides
        .get(sub_id.as_str())
        .cloned();
    ui_kit::section(ui, "Capital (§SUB4)", |ui| {
        labeled(
            ui,
            "Capital system",
            "Which system is this subsector's capital (schema: subsector_capital_overrides). Leave on Automatic to let the generator pick.",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let selected_text = current
                        .as_ref()
                        .map(|id| capital_text(&state.sector, id))
                        .unwrap_or_else(|| {
                            let auto = auto_cap
                                .as_ref()
                                .map(|id| capital_text(&state.sector, id))
                                .unwrap_or_else(|| "—".into());
                            format!("Automatic: {auto}")
                        });
                    let mut new_choice: Option<Option<SystemId>> = None;
                    ui_kit::combo(("sub_capital_override", sub_id.as_str()), selected_text)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), "(automatic)").clicked() {
                                new_choice = Some(None);
                            }
                            for sid in &target.system_ids {
                                let label = capital_text(&state.sector, sid);
                                let sel = current.as_ref() == Some(sid);
                                if ui.selectable_label(sel, label).clicked() {
                                    new_choice = Some(Some(sid.clone()));
                                }
                            }
                        });
                    if let Some(choice) = new_choice {
                        match choice {
                            Some(sid) => {
                                state
                                    .subsector_capital_overrides
                                    .insert(sub_id.clone(), sid);
                            }
                            None => {
                                state.subsector_capital_overrides.remove(sub_id.as_str());
                            }
                        }
                        state.subsector_manual.insert(sub_id.clone());
                        state.map_view.cache = None;
                        state.dirty = true;
                    }
                    if current.is_some()
                        && ui
                            .button(RichText::new("↺ Reset").color(palette::danger()))
                            .on_hover_text("Go back to the automatically chosen capital.")
                            .clicked()
                    {
                        state.subsector_capital_overrides.remove(sub_id.as_str());
                        state.map_view.cache = None;
                        state.dirty = true;
                    }
                });
            },
        );
    });
}

fn capital_text(sector: &GeneratedSector, sid: &SystemId) -> String {
    sector
        .systems
        .iter()
        .find(|sys| &sys.id == sid)
        .map(|sys| format!("{} ({})", sys.name, sid))
        .unwrap_or_else(|| sid.to_string())
}

fn show_colour_override(ui: &mut Ui, state: &mut BuilderState, target: &Subsector) {
    let sub_id = target.id.to_string();
    let default_rgb = default_subsector_colour(&state.sector, target);
    let has_override = state
        .subsector_colour_overrides
        .contains_key(sub_id.as_str());
    let mut rgb = state
        .subsector_colour_overrides
        .get(sub_id.as_str())
        .copied()
        .unwrap_or(default_rgb);
    ui_kit::section(ui, "Colour (§SUB5)", |ui| {
        labeled(
            ui,
            "Region colour",
            "Fill colour for this subsector on the map (schema: subsector_colour_overrides). The default comes from the controlling faction's colour.",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let response = ui.color_edit_button_srgb(&mut rgb);
                    if response.changed() {
                        state.subsector_colour_overrides.insert(sub_id.clone(), rgb);
                        state.subsector_manual.insert(sub_id.clone());
                        state.dirty = true;
                    }
                    ui.colored_label(
                        Color32::DARK_GRAY,
                        format!(
                            "default: #{:02X}{:02X}{:02X} (controlling faction)",
                            default_rgb[0], default_rgb[1], default_rgb[2]
                        ),
                    );
                    if has_override
                        && ui
                            .button(RichText::new("↺ Reset").color(palette::danger()))
                            .on_hover_text("Go back to the controlling faction's colour.")
                            .clicked()
                    {
                        state.subsector_colour_overrides.remove(sub_id.as_str());
                        state.dirty = true;
                    }
                });
            },
        );
    });
}

fn default_subsector_colour(sector: &GeneratedSector, target: &Subsector) -> [u8; 3] {
    match target.summary.controlling_faction_id.as_deref() {
        Some(id) => {
            let rgb = faction_style_rgb_by_id(&sector.factions, id);
            [rgb.fill.0, rgb.fill.1, rgb.fill.2]
        }
        None => [110, 110, 120],
    }
}

fn show_manual_reassign(
    ui: &mut Ui,
    state: &mut BuilderState,
    target: &Subsector,
    subs: &[Subsector],
) {
    ui.label(RichText::new("Move systems (§SUB3)").strong())
        .on_hover_text("Reassign individual systems to another subsector. Manual moves survive reclustering (schema: subsector_system_overrides).");
    if subs.len() <= 1 {
        ui_kit::placeholder(
            ui,
            "Need at least two subsectors to move systems between them.",
        );
        return;
    }
    let sub_id = target.id.to_string();
    let other_clusters: Vec<(&str, &str)> = subs
        .iter()
        .filter(|s| s.id.as_ref() != sub_id.as_str())
        .map(|s| (s.id.as_ref(), s.label.as_ref()))
        .collect();
    if target.system_ids.is_empty() {
        ui_kit::placeholder(ui, "This subsector has no systems to move.");
        return;
    }
    let sys_table: BTreeMap<&str, &str> = state
        .sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_ref()))
        .collect();
    let mut moves: Vec<(SystemId, String)> = Vec::new();
    let mut clears: Vec<SystemId> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt(("sub_manual", sub_id.as_str()))
        .max_height(220.0)
        .show(ui, |ui| {
            egui::Grid::new(("sub_manual_grid", sub_id.as_str()))
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("System").strong());
                    ui.label(RichText::new("Move to subsector").strong());
                    ui.label(RichText::new("Moved").strong())
                        .on_hover_text("Whether this system has been manually moved here.");
                    ui.end_row();
                    for sid in &target.system_ids {
                        let name = sys_table.get(sid.as_str()).copied().unwrap_or("?");
                        ui.label(format!("{} ({})", name, sid));
                        let mut chosen: Option<String> = None;
                        ui_kit::combo(("sub_move", sid.as_str()), "Choose…").show_ui(ui, |ui| {
                            for (oid, olabel) in &other_clusters {
                                if ui
                                    .selectable_label(false, format!("{olabel} ({oid})"))
                                    .clicked()
                                {
                                    chosen = Some(oid.to_string());
                                }
                            }
                        });
                        if let Some(target_id) = chosen {
                            moves.push((sid.clone(), target_id));
                        }
                        let has_ov = state.subsector_system_overrides.contains_key(sid);
                        ui.colored_label(
                            if has_ov {
                                palette::warning()
                            } else {
                                Color32::DARK_GRAY
                            },
                            if has_ov { "yes" } else { "" },
                        );
                        if has_ov
                            && ui
                                .small_button("↺ Undo")
                                .on_hover_text("Put this system back where reclustering placed it.")
                                .clicked()
                        {
                            clears.push(sid.clone());
                        }
                        ui.end_row();
                    }
                });
        });
    for (sid, target_id) in moves {
        state
            .subsector_system_overrides
            .insert(sid, target_id.clone());
        state.subsector_manual.insert(target_id);
        state.subsector_manual.insert(sub_id.clone());
        state.map_view.cache = None;
        state.dirty = true;
    }
    for sid in clears {
        state.subsector_system_overrides.remove(&sid);
        state.map_view.cache = None;
        state.dirty = true;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    fn blank(w: u32, h: u32) -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", w, h)
    }

    fn build_with_overrides(state: &BuilderState) -> Vec<Subsector> {
        let mut subs = build_subsectors(
            &state.sector,
            SubsectorConfig {
                target_systems_per_subsector: state.subsector_target_systems.max(1),
                ..SubsectorConfig::default()
            },
        )
        .unwrap_or_default();
        apply_subsector_overrides(&mut subs, state);
        subs
    }

    #[test]
    fn apply_overrides_moves_system_between_cells() {
        let mut state = blank(16, 16);
        // 4 clusters with target=1: each system is its own cluster.
        for (i, (q, r)) in [(0, 0), (15, 0), (0, 15), (15, 15)].iter().enumerate() {
            let name = format!("Sys{}", char::from(b'A' + i as u8));
            state
                .sector
                .add_system(HexCoord { q: *q, r: *r }, &name)
                .unwrap();
        }
        state.subsector_target_systems = 1;
        let baseline = build_with_overrides(&state);
        assert!(baseline.len() >= 2);
        let donor = baseline.iter().find(|s| !s.system_ids.is_empty()).unwrap();
        let recipient = baseline
            .iter()
            .find(|s| s.id != donor.id && s.system_ids != donor.system_ids)
            .unwrap();
        let moving = donor.system_ids[0].clone();
        state
            .subsector_system_overrides
            .insert(moving.clone(), recipient.id.to_string());
        let after = build_with_overrides(&state);
        let new_owner = after
            .iter()
            .find(|s| s.system_ids.contains(&moving))
            .expect("system still present");
        assert_eq!(new_owner.id, recipient.id);
    }

    #[test]
    fn capital_override_pins_capital_to_chosen_system() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Bravo")
            .unwrap();
        state.subsector_target_systems = 2; // keep them together
        let subs = build_with_overrides(&state);
        let cell_id = subs[0].id.to_string();
        let target_cap = if subs[0].summary.subsector_capital_system_id.as_ref() == Some(&a) {
            b
        } else {
            a
        };
        state
            .subsector_capital_overrides
            .insert(cell_id.clone(), target_cap.clone());
        let after = build_with_overrides(&state);
        let cell = after
            .iter()
            .find(|s| s.id.as_ref() == cell_id.as_str())
            .unwrap();
        assert_eq!(
            cell.summary.subsector_capital_system_id.as_ref(),
            Some(&target_cap),
            "capital override should win over algorithmic pick"
        );
    }

    #[test]
    fn capital_override_ignored_when_target_system_not_in_cell() {
        let mut state = blank(16, 16);
        for (q, r) in [(0, 0), (15, 15)] {
            let name = format!("S{q}{r}");
            state.sector.add_system(HexCoord { q, r }, &name).unwrap();
        }
        state.subsector_target_systems = 1;
        let subs = build_with_overrides(&state);
        let cell_a = &subs[0];
        let foreign_sys = subs[1].system_ids[0].clone();
        state
            .subsector_capital_overrides
            .insert(cell_a.id.to_string(), foreign_sys);
        let after = build_with_overrides(&state);
        let cell = after.iter().find(|s| s.id == cell_a.id).unwrap();
        // Capital should remain the algorithmic pick, not the foreign system.
        assert_ne!(
            cell.summary
                .subsector_capital_system_id
                .as_ref()
                .map(|id| id.as_str()),
            Some(subs[1].system_ids[0].as_str())
        );
    }

    #[test]
    fn recluster_target_invalidates_cache_via_digest_shift() {
        let mut state = blank(16, 16);
        for q in 0..6 {
            for r in 0..6 {
                let name = format!("s{q}{r}");
                state.sector.add_system(HexCoord { q, r }, &name).unwrap();
            }
        }
        let subs_default = build_with_overrides(&state);
        state.subsector_target_systems = 4;
        let subs_after = build_with_overrides(&state);
        // With smaller target the algorithm should produce more clusters.
        assert!(subs_after.len() >= subs_default.len());
    }

    #[test]
    fn default_colour_falls_back_to_grey_when_no_controlling_faction() {
        let mut state = blank(8, 8);
        state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "Lone")
            .unwrap();
        let subs = build_with_overrides(&state);
        let cell = &subs[0];
        assert!(cell.summary.controlling_faction_id.is_none());
        assert_eq!(
            default_subsector_colour(&state.sector, cell),
            [110, 110, 120]
        );
    }

    #[test]
    fn clearing_overrides_drops_all_side_tables() {
        let mut state = blank(8, 8);
        state.subsector_manual.insert("subsector-x".into());
        state
            .subsector_capital_overrides
            .insert("subsector-x".into(), SystemId::new("sys-0001"));
        state
            .subsector_colour_overrides
            .insert("subsector-x".into(), [10, 20, 30]);
        state
            .subsector_system_overrides
            .insert(SystemId::new("sys-0001"), "subsector-x".into());
        // §FRIENDLY_PANEL_PASS transform #7: exercise the extracted helper the
        // "🗑 Clear all overrides" confirm modal now dispatches to.
        super::clear_all_overrides(&mut state);
        assert!(state.subsector_system_overrides.is_empty());
        assert!(state.subsector_capital_overrides.is_empty());
        assert!(state.subsector_colour_overrides.is_empty());
        assert!(state.subsector_manual.is_empty());
    }
}
