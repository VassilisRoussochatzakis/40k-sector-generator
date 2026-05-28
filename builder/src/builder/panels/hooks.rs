//! HOOKS tab (§N1 / §N2) — Phase D §HK1..§HK6.
//!
//! §HK1  Ranked hook list filterable by [`HookKind`]. The list is the
//!        cached [`HooksReport`] published by
//!        [`BuilderState::recompute_hooks`]; `derive_with` already orders
//!        by descending dramatic weight, so the panel only re-applies the
//!        kind filter and §HK5 player-edition mask on top.
//! §HK2  Per-hook details: anchor link, situation/stakes/complications,
//!        weight, GM-only flag. Selecting a row populates
//!        [`BuilderState::selected_hook_id`] and
//!        [`BuilderState::hooks_edit_target`] so cross-tab links land here
//!        first-class.
//! §HK3  "+ manual hook" appends a blank [`Hook`] onto
//!        `HooksConfig::manual` with editors for kind / anchor / prose.
//! §HK4  "Regenerate hooks" calls [`BuilderState::recompute_hooks`].
//!        Manual entries survive because [`sectorforge::hooks::derive_with`]
//!        drops any derived hook sharing a manual id and then appends the
//!        whole `cfg.manual` block.
//! §HK5  Player-edition toggle (mirrors `--player`): flips
//!        [`BuilderState::hooks_player_edition`] and re-runs the recompute
//!        so the cached report has `gm_only = true` rows stripped.
//! §HK6  Click-to-highlight anchor on map: anchor link in every row +
//!        "highlight on map" button on the detail card use
//!        [`BuilderState::focus_entity`] to jump to the System / World /
//!        Route the hook references (`EntityRef::Tab(BuilderTab::Map)` as
//!        the route fallback when the lookup misses).
//!
//! The panel never edits derived `hooks_report` rows directly. All
//! mutations land in [`BuilderState::data_catalogs::hooks`] and the
//! recompute pass rewrites the published overlay.

use egui::{Color32, RichText, Ui};

use sectorforge::hooks::{Hook, HookAnchor, HookKind, HooksConfig};
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};

use crate::builder::state::{BuilderTab, EntityRef};
use crate::builder::BuilderState;

const DEFAULT_HOOKS_PATH: &str = "data/hooks.toml";

/// Every [`HookKind`] in panel-display order. Keep in sync with
/// `src/hooks.rs::HookKind`.
const KIND_VARIANTS: &[HookKind] = &[
    HookKind::CounterInfiltration,
    HookKind::Reconquest,
    HookKind::LostPassage,
    HookKind::ConvoyEscort,
    HookKind::BlockadeRun,
    HookKind::HoldTheLine,
    HookKind::SealedTombs,
    HookKind::CrushUprising,
    HookKind::SealedSystem,
    HookKind::CultPurge,
    HookKind::DiplomaticCrisis,
    HookKind::SuccessionDispute,
    HookKind::StarvingWorld,
    HookKind::LifelineLane,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Hooks");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§HK1..§HK6 — ranked hook list, manual hooks, player-edition toggle, click-to-highlight anchor.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_filter_row(ui, state);
            ui.separator();
            show_hook_list(ui, state);
            ui.separator();
            show_detail_card(ui, state);
            ui.separator();
            show_manual_editor(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── §HK4 / §HK5 header actions ─────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Regenerate hooks").clicked() {
            ensure_hooks_catalog(state);
            state.recompute_hooks();
        }
        ui.checkbox(&mut state.hooks_auto_recompute, "auto-recompute on edit");
        if ui
            .checkbox(&mut state.hooks_player_edition, "player edition (--player)")
            .changed()
        {
            state.recompute_hooks();
        }
        let total = state
            .hooks_report
            .as_ref()
            .map(|r| r.hooks.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .hooks
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("hooks: {total}  (manual: {manual})"));
        if state.data_catalogs.hooks.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no hooks.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §HK1 kind filter ───────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("§HK1 — filter").strong());
        let label = match state.hooks_filter_kind {
            None => "all kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        egui::ComboBox::from_id_salt("hk1_kind")
            .selected_text(label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.hooks_filter_kind, None, "all kinds");
                for k in KIND_VARIANTS {
                    ui.selectable_value(&mut state.hooks_filter_kind, Some(*k), kind_label(*k));
                }
            });
    });
}

// ── §HK1 / §HK2 ranked list ────────────────────────────────────────────────

fn show_hook_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§HK1 / §HK2 — ranked hooks").strong());
    let Some(report) = state.hooks_report.clone() else {
        ui.colored_label(
            Color32::GRAY,
            "No hooks yet. Click \"Regenerate hooks\" above.",
        );
        return;
    };
    let filter = state.hooks_filter_kind;
    let rows: Vec<&Hook> = report
        .hooks
        .iter()
        .filter(|h| filter.map_or(true, |k| h.kind == k))
        .collect();
    if rows.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "No hooks matched the current filter / player-edition mask.",
        );
        return;
    }
    let selected = state.selected_hook_id.clone();
    egui::ScrollArea::horizontal()
        .id_salt("hk_grid_scroll")
        .show(ui, |ui| {
            egui::Grid::new("hk_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Kind").strong());
                    ui.label(RichText::new("Weight").strong());
                    ui.label(RichText::new("Anchor").strong());
                    ui.label(RichText::new("Title").strong());
                    ui.label(RichText::new("GM").strong());
                    ui.label(RichText::new("").strong());
                    ui.end_row();

                    for h in &rows {
                        let is_selected = selected.as_deref() == Some(h.id.as_str());
                        if ui
                            .selectable_label(is_selected, kind_label(h.kind))
                            .clicked()
                        {
                            state.selected_hook_id = Some(h.id.to_string());
                            state.hooks_edit_target = Some(h.id.to_string());
                        }
                        ui.label(format!("{}", h.weight));
                        show_anchor_link(ui, state, &h.anchor);
                        ui.label(RichText::new(h.title.clone()).strong());
                        if h.gm_only {
                            ui.colored_label(Color32::from_rgb(200, 90, 90), "GM");
                        } else {
                            ui.label("");
                        }
                        if ui.button("highlight").clicked() {
                            state.selected_hook_id = Some(h.id.to_string());
                            state.hooks_edit_target = Some(h.id.to_string());
                            focus_anchor(state, &h.anchor);
                        }
                        ui.end_row();
                    }
                });
        });
}

// ── §HK2 / §HK6 detail card ────────────────────────────────────────────────

fn show_detail_card(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§HK2 — detail").strong());
    let target = state
        .hooks_edit_target
        .clone()
        .or_else(|| state.selected_hook_id.clone());
    let Some(target_id) = target else {
        ui.colored_label(Color32::GRAY, "Select a hook above to see its details.");
        return;
    };
    let Some(hook) = state
        .hooks_report
        .as_ref()
        .and_then(|r| r.hooks.iter().find(|h| h.id == target_id))
        .cloned()
    else {
        ui.colored_label(
            Color32::GRAY,
            format!("Hook id `{target_id}` is gone — regenerate to refresh."),
        );
        return;
    };
    egui::Grid::new("hk_detail_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.label(RichText::new(hook.id.clone()).monospace());
            ui.end_row();
            ui.label("kind");
            ui.label(kind_label(hook.kind));
            ui.end_row();
            ui.label("anchor");
            show_anchor_link(ui, state, &hook.anchor);
            ui.end_row();
            ui.label("weight");
            ui.label(format!("{}", hook.weight));
            ui.end_row();
            ui.label("gm-only");
            ui.label(if hook.gm_only { "yes" } else { "no" });
            ui.end_row();
            ui.label("title");
            ui.label(RichText::new(hook.title.clone()).strong());
            ui.end_row();
            ui.label("situation");
            ui.label(hook.situation.clone());
            ui.end_row();
            ui.label("stakes");
            ui.label(hook.stakes.clone());
            ui.end_row();
            ui.label("factions");
            if hook.factions.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(
                    hook.factions
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            ui.end_row();
            ui.label("complications");
            if hook.complications.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(hook.complications.join("\n"));
            }
            ui.end_row();
        });
    ui.horizontal_wrapped(|ui| {
        if ui.button("highlight on map").clicked() {
            focus_anchor(state, &hook.anchor);
        }
    });
}

// ── §HK3 manual entry editor ───────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§HK3 — manual hooks").strong());
    ensure_hooks_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.hooks.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ manual hook").clicked() {
            cfg.manual.push(blank_manual_hook(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Manual entries are appended after derivation and survive Regenerate.",
        );
    });
    if cfg.manual.is_empty() {
        ui.colored_label(Color32::GRAY, "No manual hooks yet.");
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, h) in cfg.manual.iter_mut().enumerate() {
            let header = egui::CollapsingHeader::new(
                RichText::new(format!(
                    "[{idx}] {}",
                    if h.title.is_empty() {
                        "(untitled)"
                    } else {
                        h.title.as_str()
                    }
                ))
                .strong(),
            )
            .id_salt(format!("hk_manual_{idx}"))
            .default_open(idx == last_idx);
            header.show(ui, |ui| {
                changed |= manual_hook_editor(ui, idx, h);
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

fn manual_hook_editor(ui: &mut Ui, idx: usize, h: &mut Hook) -> bool {
    let mut changed = false;
    egui::Grid::new(format!("hk_manual_grid_{idx}"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            let mut id_buf = h.id.to_string();
            if ui.text_edit_singleline(&mut id_buf).changed() {
                h.id = id_buf.into();
                changed = true;
            }
            ui.end_row();
            ui.label("kind");
            egui::ComboBox::from_id_salt(format!("hk_manual_kind_{idx}"))
                .selected_text(kind_label(h.kind))
                .show_ui(ui, |ui| {
                    for k in KIND_VARIANTS {
                        if ui
                            .selectable_value(&mut h.kind, *k, kind_label(*k))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("anchor scope");
            let mut scope = anchor_scope(&h.anchor);
            egui::ComboBox::from_id_salt(format!("hk_manual_scope_{idx}"))
                .selected_text(match scope {
                    AnchorScope::System => "system",
                    AnchorScope::World => "world",
                    AnchorScope::Route => "route",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut scope, AnchorScope::System, "system")
                        .changed()
                    {
                        h.anchor = HookAnchor::System {
                            system_id: SystemId::new(""),
                        };
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut scope, AnchorScope::World, "world")
                        .changed()
                    {
                        h.anchor = HookAnchor::World {
                            system_id: SystemId::new(""),
                            world_id: WorldId::new(""),
                        };
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut scope, AnchorScope::Route, "route")
                        .changed()
                    {
                        h.anchor = HookAnchor::Route {
                            route_id: RouteId::new(""),
                        };
                        changed = true;
                    }
                });
            ui.end_row();
            match &mut h.anchor {
                HookAnchor::System { system_id } => {
                    ui.label("system id");
                    let mut s = system_id.to_string();
                    if ui.text_edit_singleline(&mut s).changed() {
                        *system_id = SystemId::new(s.as_str());
                        changed = true;
                    }
                    ui.end_row();
                }
                HookAnchor::World {
                    system_id,
                    world_id,
                } => {
                    ui.label("system id");
                    let mut s = system_id.to_string();
                    if ui.text_edit_singleline(&mut s).changed() {
                        *system_id = SystemId::new(s.as_str());
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("world id");
                    let mut w = world_id.to_string();
                    if ui.text_edit_singleline(&mut w).changed() {
                        *world_id = WorldId::new(w.as_str());
                        changed = true;
                    }
                    ui.end_row();
                }
                HookAnchor::Route { route_id } => {
                    ui.label("route id");
                    let mut r = route_id.to_string();
                    if ui.text_edit_singleline(&mut r).changed() {
                        *route_id = RouteId::new(r.as_str());
                        changed = true;
                    }
                    ui.end_row();
                }
                _ => {}
            }
            ui.label("title");
            changed |= ui.text_edit_singleline(&mut h.title).changed();
            ui.end_row();
            ui.label("situation");
            changed |= ui.text_edit_multiline(&mut h.situation).changed();
            ui.end_row();
            ui.label("stakes");
            changed |= ui.text_edit_multiline(&mut h.stakes).changed();
            ui.end_row();
            ui.label("weight");
            changed |= ui
                .add(egui::DragValue::new(&mut h.weight).range(0..=200))
                .changed();
            ui.end_row();
            ui.label("gm-only");
            changed |= ui
                .checkbox(&mut h.gm_only, "hidden in player edition")
                .changed();
            ui.end_row();
            ui.label("factions (comma)");
            let mut csv = h
                .factions
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if ui.text_edit_singleline(&mut csv).changed() {
                h.factions = csv
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(FactionId::new)
                    .collect();
                changed = true;
            }
            ui.end_row();
            ui.label("complications (one per line)");
            let mut comp = h.complications.join("\n");
            if ui.text_edit_multiline(&mut comp).changed() {
                h.complications = comp
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                changed = true;
            }
            ui.end_row();
        });
    changed
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorScope {
    System,
    World,
    Route,
}

fn anchor_scope(a: &HookAnchor) -> AnchorScope {
    match a {
        HookAnchor::System { .. } => AnchorScope::System,
        HookAnchor::World { .. } => AnchorScope::World,
        HookAnchor::Route { .. } => AnchorScope::Route,
        _ => AnchorScope::System,
    }
}

fn blank_manual_hook(seq: usize) -> Hook {
    Hook {
        id: format!("hook-manual-{seq:04}").into(),
        kind: HookKind::DiplomaticCrisis,
        anchor: HookAnchor::System {
            system_id: SystemId::new(""),
        },
        title: String::new(),
        situation: String::new(),
        stakes: String::new(),
        factions: Vec::new(),
        complications: Vec::new(),
        weight: 50,
        gm_only: false,
    }
}

// ── §HK6 anchor link / highlight ───────────────────────────────────────────

fn show_anchor_link(ui: &mut Ui, state: &mut BuilderState, a: &HookAnchor) {
    let label = anchor_label(a);
    if ui.link(label).clicked() {
        focus_anchor(state, a);
    }
}

fn focus_anchor(state: &mut BuilderState, a: &HookAnchor) {
    let target = match a {
        HookAnchor::System { system_id } => {
            if system_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::System(system_id.clone())
            }
        }
        HookAnchor::World {
            system_id,
            world_id,
        } => {
            if system_id.as_ref().is_empty() || world_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::World {
                    system: system_id.clone(),
                    world: world_id.clone(),
                }
            }
        }
        HookAnchor::Route { route_id } => {
            if route_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::Route(route_id.clone())
            }
        }
        _ => EntityRef::Tab(BuilderTab::Map),
    };
    state.focus_entity(target);
}

fn anchor_label(a: &HookAnchor) -> String {
    match a {
        HookAnchor::System { system_id } => format!("system {system_id}"),
        HookAnchor::World {
            system_id,
            world_id,
        } => format!("world {system_id}/{world_id}"),
        HookAnchor::Route { route_id } => format!("route {route_id}"),
        _ => "unknown".into(),
    }
}

fn kind_label(k: HookKind) -> &'static str {
    match k {
        HookKind::CounterInfiltration => "Counter-infiltration",
        HookKind::Reconquest => "Reconquest",
        HookKind::LostPassage => "Lost passage",
        HookKind::ConvoyEscort => "Convoy escort",
        HookKind::BlockadeRun => "Blockade run",
        HookKind::HoldTheLine => "Hold the line",
        HookKind::SealedTombs => "Sealed tombs",
        HookKind::CrushUprising => "Crush uprising",
        HookKind::SealedSystem => "Sealed system",
        HookKind::CultPurge => "Cult purge",
        HookKind::DiplomaticCrisis => "Diplomatic crisis",
        HookKind::SuccessionDispute => "Succession dispute",
        HookKind::StarvingWorld => "Starving world",
        HookKind::LifelineLane => "Lifeline lane",
        _ => "Unknown",
    }
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.hooks.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save hooks.toml"))
            .clicked()
        {
            if state.config.inputs.hooks.is_none() {
                state.config.inputs.hooks = Some(DEFAULT_HOOKS_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save hooks.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .hooks
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_HOOKS_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn ensure_hooks_catalog(state: &mut BuilderState) {
    if state.data_catalogs.hooks.is_none() {
        state.data_catalogs.hooks = Some(HooksConfig::default());
    }
    if state.config.inputs.hooks.is_none() {
        state.config.inputs.hooks = Some(DEFAULT_HOOKS_PATH.into());
    }
}

fn ensure_hooks_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.hooks.is_none() {
        state.data_catalogs.hooks = Some(HooksConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.hooks.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_HOOKS_PATH.into());
    }
    state.mark_validation_dirty();
    if state.hooks_auto_recompute {
        state.recompute_hooks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.hooks.is_none());
        ensure_hooks_catalog(&mut state);
        assert!(state.data_catalogs.hooks.is_some());
        assert_eq!(
            state.config.inputs.hooks.as_deref(),
            Some(DEFAULT_HOOKS_PATH)
        );
    }

    #[test]
    fn recompute_hooks_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.hooks = Some(HooksConfig::default());
        state.recompute_hooks();
        assert!(state.hooks_report.is_some());
    }

    #[test]
    fn manual_hook_survives_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = HooksConfig::default();
        let mut h = blank_manual_hook(0);
        h.title = "Test".into();
        cfg.manual.push(h);
        state.data_catalogs.hooks = Some(cfg);
        state.recompute_hooks();
        let report = state.hooks_report.as_ref().unwrap();
        assert!(report.hooks.iter().any(|h| h.id == "hook-manual-0000"));
    }

    #[test]
    fn player_edition_strips_gm_only_hooks() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = HooksConfig::default();
        let mut h = blank_manual_hook(0);
        h.title = "Secret".into();
        h.gm_only = true;
        cfg.manual.push(h);
        state.data_catalogs.hooks = Some(cfg);
        state.hooks_player_edition = true;
        state.recompute_hooks();
        let report = state.hooks_report.as_ref().unwrap();
        assert!(!report.hooks.iter().any(|h| h.id == "hook-manual-0000"));
    }
}
