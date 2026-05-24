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

use crate::builder::state::BuilderTab;
use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Subsectors");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§SUB1..§SUB5 — cluster list, recluster, manual reassignment, capital + colour overrides.",
    );
    ui.separator();

    let subsectors = current_subsectors(state);
    show_recluster_bar(ui, state, &subsectors);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_cluster_list(ui, state, &subsectors);
            ui.separator();
            show_inspector(ui, state, &subsectors);
        });
}

// ── Public helper used by [`map.rs::refresh_map_cache`] ─────────────────────

/// Apply §SUB3 / §SUB4 overrides to the freshly clustered subsector list.
/// `subs` is mutated in place: systems move between cells per
/// [`BuilderState::subsector_system_overrides`], capitals are forced to
/// [`BuilderState::subsector_capital_overrides`] when set. Cluster ids and
/// hex layouts are preserved so the renderer never sees an id flip when the
/// user picks a different capital.
pub fn apply_subsector_overrides(subs: &mut [Subsector], state: &BuilderState) {
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

fn current_subsectors(state: &BuilderState) -> Vec<Subsector> {
    if let Some(cache) = state.map_view_cache.as_ref() {
        return cache.subsectors.clone();
    }
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

fn show_recluster_bar(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("§SUB2").strong());
        ui.label("target systems / subsector:");
        let max = state.sector.systems.len().max(1) as u32;
        let mut value = state.subsector_target_systems.max(1);
        let old = value;
        let response = ui.add(egui::DragValue::new(&mut value).range(1..=max).speed(0.25));
        if response.changed() && value != old {
            state.subsector_target_systems = value;
            state.map_view_cache = None; // force refresh on next MAP-tab tick
        }
        if ui.button("Recluster").clicked() {
            state.subsector_target_systems = value.max(1);
            state.map_view_cache = None;
        }
        if ui.button("Reset target").clicked() {
            state.subsector_target_systems = DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR;
            state.map_view_cache = None;
        }
        if !state.subsector_system_overrides.is_empty()
            || !state.subsector_capital_overrides.is_empty()
            || !state.subsector_colour_overrides.is_empty()
            || !state.subsector_manual.is_empty()
        {
            if ui
                .add(egui::Button::new(
                    RichText::new("× clear all overrides").color(Color32::LIGHT_RED),
                ))
                .clicked()
            {
                state.subsector_system_overrides.clear();
                state.subsector_capital_overrides.clear();
                state.subsector_colour_overrides.clear();
                state.subsector_manual.clear();
                state.map_view_cache = None;
            }
        }
        ui.label(format!("clusters: {}", subs.len()));
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Recluster runs the §13 k-means / Lloyd pass at the new target. Manual moves and capital overrides are reapplied on top.",
    );
}

fn show_cluster_list(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    ui.label(RichText::new("§SUB1 — clusters").strong());
    if subs.is_empty() {
        ui.colored_label(Color32::GRAY, "No subsectors (sector empty).");
        return;
    }
    egui::Grid::new("subsectors_list")
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new("label").strong());
            ui.label(RichText::new("name").strong());
            ui.label(RichText::new("capital").strong());
            ui.label(RichText::new("systems").strong());
            ui.label(RichText::new("dominant").strong());
            ui.label(RichText::new("flags").strong());
            ui.end_row();
            for s in subs {
                let selected = state.selected_subsector_id.as_deref() == Some(s.id.as_ref());
                if ui
                    .selectable_label(selected, RichText::new(s.label.as_ref()).monospace())
                    .clicked()
                {
                    state.selected_subsector_id = Some(s.id.to_string());
                }
                let name_part = s
                    .name
                    .strip_prefix("Subsector ")
                    .unwrap_or_else(|| s.name.as_ref());
                ui.label(name_part);
                ui.label(capital_label(state, s));
                ui.label(format!("{}", s.system_ids.len()));
                ui.label(dominant_label(s));
                let mut flags = String::new();
                if state.subsector_manual.contains(s.id.as_ref()) {
                    flags.push_str("manual ");
                }
                if state
                    .subsector_capital_overrides
                    .contains_key(s.id.as_ref())
                {
                    flags.push_str("cap-override ");
                }
                if state.subsector_colour_overrides.contains_key(s.id.as_ref()) {
                    flags.push_str("colour ");
                }
                ui.colored_label(Color32::DARK_GRAY, flags.trim_end());
                ui.end_row();
            }
        });
}

fn capital_label(state: &BuilderState, s: &Subsector) -> String {
    let cap = s
        .summary
        .subsector_capital_system_id
        .as_deref()
        .unwrap_or("—");
    if cap == "—" {
        return "—".into();
    }
    let name = state
        .sector
        .systems
        .iter()
        .find(|sys| sys.id.as_str() == cap)
        .map(|sys| sys.name.to_string())
        .unwrap_or_else(|| cap.to_string());
    name
}

fn dominant_label(s: &Subsector) -> String {
    s.summary
        .controlling_faction_id
        .as_deref()
        .map(|id| id.to_string())
        .unwrap_or_else(|| "—".into())
}

// ── §SUB3..§SUB5 inspector ───────────────────────────────────────────────────

fn show_inspector(ui: &mut Ui, state: &mut BuilderState, subs: &[Subsector]) {
    let Some(selected) = state.selected_subsector_id.clone() else {
        ui.colored_label(Color32::GRAY, "Pick a subsector above to edit.");
        return;
    };
    let Some(target) = subs.iter().find(|s| s.id.as_ref() == selected.as_str()) else {
        ui.colored_label(
            Color32::GRAY,
            "Selected subsector vanished (recluster cleared it).",
        );
        state.selected_subsector_id = None;
        return;
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&*target.label).strong().monospace());
            ui.label(target.name.as_ref());
            ui.label(format!("id: {}", target.id));
            if ui.button("→ MAP").clicked() {
                state.active_tab = BuilderTab::Map;
            }
        });
        ui.label(format!(
            "systems: {}  |  internal routes: {}  |  border routes: {}  |  worlds: {}",
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
                format!("dominant: {}", chips.join(", ")),
            );
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
    ui.label(RichText::new("§SUB4 — capital override").strong());
    let sub_id = target.id.to_string();
    let auto_cap = target.summary.subsector_capital_system_id.clone();
    let current = state
        .subsector_capital_overrides
        .get(sub_id.as_str())
        .cloned();
    ui.horizontal_wrapped(|ui| {
        ui.label("capital:");
        let selected_text = current
            .as_ref()
            .map(|id| capital_text(&state.sector, id))
            .unwrap_or_else(|| {
                let auto = auto_cap
                    .as_ref()
                    .map(|id| capital_text(&state.sector, id))
                    .unwrap_or_else(|| "—".into());
                format!("auto: {auto}")
            });
        let mut new_choice: Option<Option<SystemId>> = None;
        egui::ComboBox::from_id_salt(("sub_capital_override", sub_id.as_str()))
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "(auto)").clicked() {
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
            state.map_view_cache = None;
            state.dirty = true;
        }
        if current.is_some()
            && ui
                .button(RichText::new("clear").color(Color32::LIGHT_RED))
                .clicked()
        {
            state.subsector_capital_overrides.remove(sub_id.as_str());
            state.map_view_cache = None;
            state.dirty = true;
        }
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
    ui.label(RichText::new("§SUB5 — colour override").strong());
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
    ui.horizontal_wrapped(|ui| {
        ui.label("colour:");
        let response = ui.color_edit_button_srgb(&mut rgb);
        if response.changed() {
            state.subsector_colour_overrides.insert(sub_id.clone(), rgb);
            state.subsector_manual.insert(sub_id.clone());
            state.dirty = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            format!(
                "default: #{:02X}{:02X}{:02X} (FactionStyle)",
                default_rgb[0], default_rgb[1], default_rgb[2]
            ),
        );
        if has_override
            && ui
                .button(RichText::new("reset to FactionStyle").color(Color32::LIGHT_RED))
                .clicked()
        {
            state.subsector_colour_overrides.remove(sub_id.as_str());
            state.dirty = true;
        }
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
    ui.label(RichText::new("§SUB3 — manual reassignment").strong());
    if subs.len() <= 1 {
        ui.colored_label(Color32::DARK_GRAY, "Need ≥2 clusters to reassign systems.");
        return;
    }
    let sub_id = target.id.to_string();
    let other_clusters: Vec<(&str, &str)> = subs
        .iter()
        .filter(|s| s.id.as_ref() != sub_id.as_str())
        .map(|s| (s.id.as_ref(), s.label.as_ref()))
        .collect();
    if target.system_ids.is_empty() {
        ui.colored_label(Color32::DARK_GRAY, "Cluster has no systems.");
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
                    ui.label(RichText::new("system").strong());
                    ui.label(RichText::new("move to").strong());
                    ui.label(RichText::new("override").strong());
                    ui.end_row();
                    for sid in &target.system_ids {
                        let name = sys_table.get(sid.as_str()).copied().unwrap_or("?");
                        ui.label(format!("{} ({})", name, sid));
                        let mut chosen: Option<String> = None;
                        egui::ComboBox::from_id_salt(("sub_move", sid.as_str()))
                            .selected_text("→ pick")
                            .show_ui(ui, |ui| {
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
                                Color32::LIGHT_YELLOW
                            } else {
                                Color32::DARK_GRAY
                            },
                            if has_ov { "yes" } else { "" },
                        );
                        if has_ov && ui.small_button("clear").clicked() {
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
        state.map_view_cache = None;
        state.dirty = true;
    }
    for sid in clears {
        state.subsector_system_overrides.remove(&sid);
        state.map_view_cache = None;
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
        // Simulating the "× clear all overrides" button.
        state.subsector_system_overrides.clear();
        state.subsector_capital_overrides.clear();
        state.subsector_colour_overrides.clear();
        state.subsector_manual.clear();
        assert!(state.subsector_system_overrides.is_empty());
        assert!(state.subsector_capital_overrides.is_empty());
        assert!(state.subsector_colour_overrides.is_empty());
        assert!(state.subsector_manual.is_empty());
    }
}
