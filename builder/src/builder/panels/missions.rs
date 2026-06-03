//! MISSIONS tab — Phase D §M1..§M5.
//!
//! §M1  Mission list panel. Rows render kind / patron / target /
//!      primary+secondary location / public objective / hidden complication /
//!      reward / consequence over the cached [`MissionsReport`] published by
//!      [`BuilderState::recompute_missions`]. Selecting a row populates
//!      [`BuilderState::selected_mission_id`] +
//!      [`BuilderState::missions_edit_target`] and focuses the primary
//!      location through [`BuilderState::focus_entity`] so cross-tab links
//!      land first-class.
//! §M2  Manual mission editor: kind picker plus the patron / target /
//!      primary+secondary / objective / hidden / reward / consequence
//!      fields. Manual rows live in
//!      [`MissionsConfig::manual`](sectorforge::missions::MissionsConfig)
//!      and survive every "Auto-derive" pass.
//! §M3  "Auto-derive missions" calls [`BuilderState::recompute_missions`]
//!      which runs [`sectorforge::missions::derive_with`]. Manual entries
//!      survive because `derive_with` appends `cfg.manual` after the
//!      per-anchor cap pass.
//! §M4  Player-edition toggle (mirrors `--player`): flips
//!      [`BuilderState::missions_player_edition`] and re-runs the recompute
//!      so the cached report has `gm_only` rows stripped.
//! §M5  Click-to-highlight: each row plus the "highlight location" button
//!      parses the mission's `primary_location` (`sys` or `sys/world`) into
//!      the matching [`EntityRef`] and calls
//!      [`BuilderState::focus_entity`], landing on the MAP tab via
//!      `EntityRef::Tab(BuilderTab::Map)` so the system / world is selected
//!      under the existing focus overlay.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::missions::{
    MissionKind, MissionScale, MissionSeed, MissionVisibility, MissionsConfig,
};
use sectorforge_gui_core::ui_kit;

use crate::builder::state::{BuilderTab, EntityRef};
use crate::builder::BuilderState;

const DEFAULT_MISSIONS_PATH: &str = "data/missions.toml";

/// Every [`MissionKind`] in panel-display order. Keep in sync with
/// `src/missions.rs::MissionKind`.
const KIND_VARIANTS: &[MissionKind] = &[
    MissionKind::Investigate,
    MissionKind::Escort,
    MissionKind::Sabotage,
    MissionKind::Diplomacy,
    MissionKind::Assassination,
    MissionKind::Recovery,
    MissionKind::Defense,
    MissionKind::Exploration,
];

const SCALE_VARIANTS: &[MissionScale] = &[
    MissionScale::OneShot,
    MissionScale::ShortArc,
    MissionScale::CampaignArc,
];

const VISIBILITY_VARIANTS: &[MissionVisibility] = &[
    MissionVisibility::Public,
    MissionVisibility::Restricted,
    MissionVisibility::Secret,
    MissionVisibility::Misinformation,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Missions");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "mission list, manual entries that survive auto-derive, player-edition toggle, click-to-highlight.",
    );
    ui.separator();

    // §COLUMNS — global controls (regenerate / player-edition / config knobs)
    // stay full-width on top, then master-detail: the ranked mission list pins
    // to a resizable left rail (filter + rows) and the detail card + manual
    // editor + save fill the rest. Replaces the single-column stack whose list
    // and detail scrolled past each other.
    show_header_actions(ui, state);
    ui.separator();
    show_config_section(ui, state);
    ui.separator();

    egui::SidePanel::left("missions_list")
        .resizable(true)
        .default_width(320.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| {
            show_filter_row(ui, state);
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| show_mission_list(ui, state));
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                show_detail_card(ui, state);
                ui.separator();
                show_manual_editor(ui, state);
                ui.separator();
                show_save_row(ui, state);
            });
    });
}

// ── §M3 / §M4 header actions ───────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Auto-derive missions").clicked() {
            ensure_missions_catalog(state);
            state.recompute_missions();
        }
        ui.checkbox(&mut state.missions_auto_recompute, "auto-recompute on edit");
        if ui
            .checkbox(
                &mut state.missions_player_edition,
                "player edition (--player)",
            )
            .changed()
        {
            state.recompute_missions();
        }
        let total = state
            .missions_report
            .as_ref()
            .map(|r| r.missions.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .missions
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("missions: {total}  (manual: {manual})"));
        if state.data_catalogs.missions.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no missions.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §M3 config knobs ───────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("missions.toml knobs").strong());
    ensure_missions_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.missions.as_mut() else {
        return;
    };
    let mut changed = false;
    egui::Grid::new("m3_grid").num_columns(2).show(ui, |ui| {
        ui.label("max_per_anchor");
        changed |= ui
            .add(egui::DragValue::new(&mut cfg.max_per_anchor).range(0..=32))
            .changed();
        ui.end_row();
        ui.label("top_n_digest");
        changed |= ui
            .add(egui::DragValue::new(&mut cfg.top_n_digest).range(0..=200))
            .changed();
        ui.end_row();
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Higher max ⇒ more mission seeds per world/system/route. top_n_digest only affects the rendered Markdown digest.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §M1 kind filter ────────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("filter").strong());
        let label = match state.missions_filter_kind {
            None => "all kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        ui_kit::combo("m1_kind", label).show_ui(ui, |ui| {
            ui.selectable_value(&mut state.missions_filter_kind, None, "all kinds");
            for k in KIND_VARIANTS {
                ui.selectable_value(&mut state.missions_filter_kind, Some(*k), kind_label(*k));
            }
        });
    });
}

// ── §M1 ranked list ────────────────────────────────────────────────────────

fn show_mission_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("mission list").strong());
    let Some(report) = state.missions_report.clone() else {
        ui.colored_label(
            Color32::GRAY,
            "No missions yet. Click \"Auto-derive missions\" above.",
        );
        return;
    };
    let filter = state.missions_filter_kind;
    let rows: Vec<&MissionSeed> = report
        .missions
        .iter()
        .filter(|m| filter.is_none_or(|k| m.kind == k))
        .collect();
    if rows.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "No missions matched the current filter / player-edition mask.",
        );
        return;
    }
    let selected = state.selected_mission_id.clone();
    let show_hidden = !state.missions_player_edition;
    // §COLUMNS — compact rail rows: a selectable title line per mission with
    // kind / scale subline; full fields live in the detail card on the right.
    for m in &rows {
        let is_selected = selected.as_deref() == Some(m.id.as_str());
        let title = if m.title.is_empty() {
            kind_label(m.kind).to_string()
        } else {
            m.title.clone()
        };
        let resp = ui.selectable_label(is_selected, RichText::new(title).strong());
        if resp.clicked() {
            select_mission(state, m);
        }
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                Color32::DARK_GRAY,
                format!("{} · {}", kind_label(m.kind), m.scale),
            );
            if show_hidden && m.gm_only {
                ui.colored_label(Color32::from_rgb(220, 170, 80), "GM");
            }
            if ui.small_button("highlight").clicked() {
                select_mission(state, m);
                focus_primary_location(state, &m.primary_location, &m.route_ids);
            }
        });
        ui.separator();
    }
}

// ── §M1 detail card ────────────────────────────────────────────────────────

fn show_detail_card(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("detail").strong());
    let target = state
        .missions_edit_target
        .clone()
        .or_else(|| state.selected_mission_id.clone());
    let Some(target_id) = target else {
        ui_kit::placeholder(ui, "Select a mission above to see its details.");
        return;
    };
    let Some(mission) = state
        .missions_report
        .as_ref()
        .and_then(|r| r.missions.iter().find(|m| m.id == target_id))
        .cloned()
    else {
        ui.colored_label(
            Color32::GRAY,
            format!("Mission id `{target_id}` is gone — regenerate to refresh."),
        );
        return;
    };
    let show_hidden = !state.missions_player_edition;
    egui::Grid::new("m_detail_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.label(RichText::new(mission.id.clone()).monospace());
            ui.end_row();
            ui.label("kind");
            ui.label(kind_label(mission.kind));
            ui.end_row();
            ui.label("title");
            ui.label(RichText::new(mission.title.clone()).strong());
            ui.end_row();
            ui.label("patron");
            if let Some(f) = &mission.patron {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
            ui.end_row();
            ui.label("target");
            if let Some(f) = &mission.target {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
            ui.end_row();
            ui.label("primary location");
            if ui
                .link(RichText::new(mission.primary_location.clone()).monospace())
                .clicked()
            {
                focus_primary_location(state, &mission.primary_location, &mission.route_ids);
            }
            ui.end_row();
            ui.label("secondary location");
            if let Some(sec) = &mission.secondary_location {
                let sec_clone = sec.clone();
                let route_ids = mission.route_ids.clone();
                if ui.link(RichText::new(sec.clone()).monospace()).clicked() {
                    focus_primary_location(state, &sec_clone, &route_ids);
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
            ui.end_row();
            ui.label("routes");
            if mission.route_ids.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.horizontal_wrapped(|ui| {
                    for rid in &mission.route_ids {
                        if ui.link(rid.to_string()).clicked() {
                            state.focus_entity(EntityRef::Route(rid.clone()));
                        }
                    }
                });
            }
            ui.end_row();
            ui.label("objective");
            ui.label(mission.public_objective.clone());
            ui.end_row();
            if show_hidden {
                ui.label("hidden complication");
                if let Some(c) = &mission.hidden_complication {
                    let txt = RichText::new(c.clone()).color(Color32::from_rgb(220, 170, 80));
                    ui.label(txt);
                } else {
                    ui.colored_label(Color32::DARK_GRAY, "—");
                }
                ui.end_row();
            }
            ui.label("reward");
            ui.label(mission.reward.clone());
            ui.end_row();
            ui.label("consequence");
            ui.label(mission.if_ignored.clone());
            ui.end_row();
            ui.label("scale / visibility");
            ui.label(format!("{} / {}", mission.scale, mission.visibility));
            ui.end_row();
            ui.label("weight");
            ui.label(mission.weight.to_string());
            ui.end_row();
        });
    ui.horizontal_wrapped(|ui| {
        if ui.button("highlight primary on map").clicked() {
            focus_primary_location(state, &mission.primary_location, &mission.route_ids);
        }
        if let Some(sec) = mission.secondary_location.clone() {
            let route_ids = mission.route_ids.clone();
            if ui.button("highlight secondary on map").clicked() {
                focus_primary_location(state, &sec, &route_ids);
            }
        }
    });
}

// ── §M2 manual editor ──────────────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("manual missions").strong());
    ensure_missions_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.missions.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ manual mission").clicked() {
            cfg.manual.push(blank_manual_mission(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Manual entries are appended after derivation and survive Auto-derive.",
        );
    });
    if cfg.manual.is_empty() {
        ui.colored_label(Color32::GRAY, "No manual missions yet.");
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, m) in cfg.manual.iter_mut().enumerate() {
            let title = format!(
                "[{idx}] {} — {}",
                if m.title.is_empty() {
                    "(untitled)"
                } else {
                    m.title.as_str()
                },
                kind_label(m.kind),
            );
            ui_kit::collapsing_section(ui, ("mis_manual", idx), &title, idx == last_idx, |ui| {
                changed |= manual_mission_editor(ui, idx, m);
                if ui
                    .button(RichText::new("✕ remove").color(Color32::from_rgb(200, 90, 90)))
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
    }
    if let Some(idx) = remove_idx {
        cfg.manual.remove(idx);
        changed = true;
    }
    if changed {
        on_catalog_edited(state);
    }
}

fn manual_mission_editor(ui: &mut Ui, idx: usize, m: &mut MissionSeed) -> bool {
    let mut changed = false;
    egui::Grid::new(format!("m_manual_grid_{idx}"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            let mut id_buf = m.id.to_string();
            if ui.text_edit_singleline(&mut id_buf).changed() {
                m.id = id_buf.into();
                changed = true;
            }
            ui.end_row();
            ui.label("kind");
            ui_kit::combo(format!("m_manual_kind_{idx}"), kind_label(m.kind)).show_ui(ui, |ui| {
                for k in KIND_VARIANTS {
                    if ui
                        .selectable_value(&mut m.kind, *k, kind_label(*k))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("title");
            changed |= ui.text_edit_singleline(&mut m.title).changed();
            ui.end_row();
            ui.label("patron faction id");
            let mut patron = m.patron.as_ref().map(|f| f.to_string()).unwrap_or_default();
            if ui.text_edit_singleline(&mut patron).changed() {
                let trimmed = patron.trim();
                m.patron = if trimmed.is_empty() {
                    None
                } else {
                    Some(FactionId::new(trimmed))
                };
                changed = true;
            }
            ui.end_row();
            ui.label("target faction id");
            let mut target = m.target.as_ref().map(|f| f.to_string()).unwrap_or_default();
            if ui.text_edit_singleline(&mut target).changed() {
                let trimmed = target.trim();
                m.target = if trimmed.is_empty() {
                    None
                } else {
                    Some(FactionId::new(trimmed))
                };
                changed = true;
            }
            ui.end_row();
            ui.label("primary location (sys or sys/world)");
            changed |= ui.text_edit_singleline(&mut m.primary_location).changed();
            ui.end_row();
            ui.label("secondary location");
            let mut sec = m.secondary_location.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut sec).changed() {
                let trimmed = sec.trim();
                m.secondary_location = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                changed = true;
            }
            ui.end_row();
            ui.label("route ids (comma)");
            let mut csv = m
                .route_ids
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if ui.text_edit_singleline(&mut csv).changed() {
                m.route_ids = csv
                    .split(',')
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .map(RouteId::new)
                    .collect();
                changed = true;
            }
            ui.end_row();
            ui.label("objective");
            changed |= ui.text_edit_multiline(&mut m.public_objective).changed();
            ui.end_row();
            ui.label("hidden complication");
            let mut hidden = m.hidden_complication.clone().unwrap_or_default();
            if ui.text_edit_multiline(&mut hidden).changed() {
                let trimmed = hidden.trim();
                m.hidden_complication = if trimmed.is_empty() {
                    None
                } else {
                    Some(hidden.clone())
                };
                changed = true;
            }
            ui.end_row();
            ui.label("reward");
            changed |= ui.text_edit_singleline(&mut m.reward).changed();
            ui.end_row();
            ui.label("if ignored");
            changed |= ui.text_edit_multiline(&mut m.if_ignored).changed();
            ui.end_row();
            ui.label("scale");
            ui_kit::combo(format!("m_manual_scale_{idx}"), format!("{}", m.scale)).show_ui(
                ui,
                |ui| {
                    for v in SCALE_VARIANTS {
                        if ui
                            .selectable_value(&mut m.scale, *v, format!("{v}"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                },
            );
            ui.end_row();
            ui.label("visibility");
            ui_kit::combo(format!("m_manual_vis_{idx}"), format!("{}", m.visibility)).show_ui(
                ui,
                |ui| {
                    for v in VISIBILITY_VARIANTS {
                        if ui
                            .selectable_value(&mut m.visibility, *v, format!("{v}"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                },
            );
            ui.end_row();
            ui.label("weight");
            changed |= ui
                .add(egui::DragValue::new(&mut m.weight).range(0..=1000))
                .changed();
            ui.end_row();
            ui.label("gm only");
            changed |= ui
                .checkbox(&mut m.gm_only, "hidden under player edition")
                .changed();
            ui.end_row();
        });
    changed
}

fn blank_manual_mission(seq: usize) -> MissionSeed {
    MissionSeed {
        id: format!("mission-manual-{seq:04}").into(),
        kind: MissionKind::Investigate,
        title: String::new(),
        patron: None,
        target: None,
        primary_location: String::new(),
        secondary_location: None,
        route_ids: Vec::new(),
        public_objective: String::new(),
        hidden_complication: None,
        reward: String::new(),
        if_ignored: String::new(),
        scale: MissionScale::OneShot,
        visibility: MissionVisibility::Public,
        weight: 50,
        gm_only: false,
    }
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.missions.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save missions.toml"))
            .clicked()
        {
            if state.config.inputs.missions.is_none() {
                state.config.inputs.missions = Some(DEFAULT_MISSIONS_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save missions.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .missions
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_MISSIONS_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn ensure_missions_catalog(state: &mut BuilderState) {
    if state.data_catalogs.missions.is_none() {
        state.data_catalogs.missions = Some(MissionsConfig::default());
    }
    if state.config.inputs.missions.is_none() {
        state.config.inputs.missions = Some(DEFAULT_MISSIONS_PATH.into());
    }
}

fn ensure_missions_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.missions.is_none() {
        state.data_catalogs.missions = Some(MissionsConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.missions.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_MISSIONS_PATH.into());
    }
    state.mark_validation_dirty();
    if state.missions_auto_recompute {
        state.recompute_missions();
    }
}

fn select_mission(state: &mut BuilderState, m: &MissionSeed) {
    state.selected_mission_id = Some(m.id.to_string());
    state.missions_edit_target = Some(m.id.to_string());
}

/// §M5 — resolve a mission location string into an [`EntityRef`] and call
/// [`BuilderState::focus_entity`]. Accepts either `"sys"` (system anchor) or
/// `"sys/world"` (per-world anchor). When the string is empty the focus
/// falls through to the MAP tab plus the first route id, when one exists.
fn focus_primary_location(state: &mut BuilderState, loc: &str, routes: &[RouteId]) {
    let trimmed = loc.trim();
    if trimmed.is_empty() {
        if let Some(rid) = routes.first() {
            state.focus_entity(EntityRef::Route(rid.clone()));
        } else {
            state.focus_entity(EntityRef::Tab(BuilderTab::Map));
        }
        return;
    }
    if let Some((sys, world)) = trimmed.split_once('/') {
        state.focus_entity(EntityRef::World {
            system: SystemId::new(sys),
            world: WorldId::new(world),
        });
    } else {
        state.focus_entity(EntityRef::System(SystemId::new(trimmed)));
    }
}

fn kind_label(k: MissionKind) -> &'static str {
    match k {
        MissionKind::Investigate => "investigate",
        MissionKind::Escort => "escort",
        MissionKind::Sabotage => "sabotage",
        MissionKind::Diplomacy => "diplomacy",
        MissionKind::Assassination => "assassination",
        MissionKind::Recovery => "recovery",
        MissionKind::Defense => "defense",
        MissionKind::Exploration => "exploration",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.missions.is_none());
        ensure_missions_catalog(&mut state);
        assert!(state.data_catalogs.missions.is_some());
        assert_eq!(
            state.config.inputs.missions.as_deref(),
            Some(DEFAULT_MISSIONS_PATH)
        );
    }

    #[test]
    fn recompute_missions_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.missions = Some(MissionsConfig::default());
        state.recompute_missions();
        assert!(state.missions_report.is_some());
    }

    #[test]
    fn manual_mission_survives_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = MissionsConfig::default();
        let mut m = blank_manual_mission(0);
        m.title = "Test Op".into();
        m.primary_location = "sys-0001".into();
        cfg.manual.push(m);
        state.data_catalogs.missions = Some(cfg);
        state.recompute_missions();
        let report = state.missions_report.as_ref().unwrap();
        assert!(report
            .missions
            .iter()
            .any(|m| m.id == "mission-manual-0000"));
    }

    #[test]
    fn player_edition_flag_threads_into_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.missions = Some(MissionsConfig::default());
        state.missions_player_edition = true;
        state.recompute_missions();
        let report = state.missions_report.as_ref().unwrap();
        assert!(report.missions.iter().all(|m| !m.gm_only));
    }

    #[test]
    fn focus_primary_location_parses_sys_world() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        focus_primary_location(&mut state, "sys-0001/sys-0001-w1", &[]);
        assert_eq!(
            state.selected_system_id.as_ref().map(|s| s.as_str()),
            Some("sys-0001")
        );
        assert_eq!(
            state.selected_world_id.as_ref().map(|w| w.as_str()),
            Some("sys-0001-w1")
        );
    }

    #[test]
    fn focus_primary_location_parses_sys_only() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        focus_primary_location(&mut state, "sys-0002", &[]);
        assert_eq!(
            state.selected_system_id.as_ref().map(|s| s.as_str()),
            Some("sys-0002")
        );
    }
}
