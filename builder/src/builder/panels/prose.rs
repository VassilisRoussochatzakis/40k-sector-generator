//! PROSE tab — Phase D §PR1..§PR4.
//!
//! §PR1  Per-system prose editor: rows render the derived paragraphs from the
//!       cached [`ProseReport`] published by [`BuilderState::recompute_prose`].
//!       An "Override" toggle flips
//!       [`ProseConfig::overrides::systems`](sectorforge::prose::ProseOverrides)
//!       for the selected system; while on, the text-edit box edits the
//!       replacement prose, and the derived paragraphs stay cached on
//!       [`SystemProse::derived_paragraphs`] so a one-click "Revert" can
//!       restore them. Overrides survive every "Regenerate prose" pass
//!       because they live inside `data_catalogs.prose`.
//! §PR2  Per-sector overview editor: same Override toggle / text-edit pattern
//!       bound to [`ProseOverrides::overview`]. `overview_is_override` on the
//!       cached report mirrors the toggle state so the panel can flag the
//!       "authored" badge.
//! §PR3  Tone preset selector: a `ComboBox` over
//!       [`ProseTone::Gazetteer`] / [`ProseTone::Dispatch`] bound to
//!       [`ProseConfig::tone`]. Changing the tone rewrites the derived
//!       paragraphs on the next recompute; overrides are untouched because
//!       they store the manual text verbatim.
//! §PR4  "Regenerate prose" button calls [`BuilderState::recompute_prose`]
//!       which runs [`sectorforge::prose::derive_with`]. Manual overrides
//!       survive every recompute because `derive_with` re-applies them after
//!       the deterministic derivation pass.
//!
//! The panel never edits the derived `prose_report` rows directly. All
//! mutations land in [`BuilderState::data_catalogs::prose`] and the recompute
//! pass rewrites the published overlay.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::SystemId;
use sectorforge::prose::{ProseConfig, ProseTone};

use sectorforge_gui_core::ui_kit;

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

const DEFAULT_PROSE_PATH: &str = "data/prose.toml";

const TONE_VARIANTS: &[ProseTone] = &[ProseTone::Gazetteer, ProseTone::Dispatch];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Prose");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§PR1..§PR4 — per-system + sector overview overrides, tone preset, regenerate. \
         Manual overrides survive Regenerate prose.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_tone_section(ui, state);
            ui.separator();
            show_overview_editor(ui, state);
            ui.separator();
            show_system_editor(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── §PR4 header actions ───────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Regenerate prose").clicked() {
            ensure_prose_catalog(state);
            state.recompute_prose();
        }
        ui.checkbox(&mut state.prose_auto_recompute, "auto-recompute on edit");
        let systems = state
            .prose_report
            .as_ref()
            .map(|r| r.system_entries.len())
            .unwrap_or(0);
        let overrides_count = state
            .data_catalogs
            .prose
            .as_ref()
            .map(|c| c.overrides.systems.len())
            .unwrap_or(0);
        let overview_override = state
            .data_catalogs
            .prose
            .as_ref()
            .and_then(|c| c.overrides.overview.as_deref())
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);
        ui.label(format!(
            "systems: {systems}  (system overrides: {overrides_count}{})",
            if overview_override { "+ overview" } else { "" },
        ));
        if state.data_catalogs.prose.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no prose.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §PR3 tone preset ──────────────────────────────────────────────────────

fn show_tone_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("tone preset").strong());
    ensure_prose_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.prose.as_mut() else {
        return;
    };
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("tone");
        ui_kit::combo("pr3_tone", tone_label(cfg.tone)).show_ui(ui, |ui| {
            for t in TONE_VARIANTS {
                if ui
                    .selectable_value(&mut cfg.tone, *t, tone_label(*t))
                    .changed()
                {
                    changed = true;
                }
            }
        });
        ui.separator();
        changed |= ui
            .checkbox(&mut cfg.include_overview, "include sector overview")
            .changed();
        changed |= ui
            .checkbox(&mut cfg.include_per_system, "include per-system entries")
            .changed();
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Florid (Gazetteer) ↔ Administratum Dispatch. Overrides are stored verbatim and ignore the tone setting.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §PR2 sector overview editor ───────────────────────────────────────────

fn show_overview_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("sector overview").strong());
    ensure_prose_catalog_if_needed(state);
    let report = state.prose_report.clone();
    let Some(cfg) = state.data_catalogs.prose.as_mut() else {
        return;
    };

    let derived_overview = report.as_ref().map(|r| r.overview.clone());
    let mut is_override = cfg
        .overrides
        .overview
        .as_ref()
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        let prev = is_override;
        if ui.checkbox(&mut is_override, "Override").changed() {
            if is_override && !prev {
                // Seed the override with the derived overview so the user
                // edits in place rather than starts from a blank field.
                let seed = derived_overview.clone().unwrap_or_default();
                cfg.overrides.overview = Some(seed);
                changed = true;
            } else if !is_override && prev {
                cfg.overrides.overview = None;
                changed = true;
            }
        }
        if is_override {
            ui.colored_label(Color32::from_rgb(220, 170, 80), "AUTHORED");
        } else {
            ui.colored_label(Color32::DARK_GRAY, "derived");
        }
    });

    if is_override {
        let mut text = cfg.overrides.overview.clone().unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .desired_rows(4)
                .hint_text("Authored overview prose..."),
        );
        if resp.changed() {
            cfg.overrides.overview = Some(text);
            changed = true;
        }
    } else if let Some(text) = derived_overview {
        ui.add(
            egui::TextEdit::multiline(&mut text.as_str())
                .desired_width(f32::INFINITY)
                .desired_rows(4),
        );
    } else {
        ui.colored_label(
            Color32::GRAY,
            "No overview yet. Click \"Regenerate prose\" above.",
        );
    }

    if changed {
        on_catalog_edited(state);
    }
}

// ── §PR1 per-system editor ────────────────────────────────────────────────

fn show_system_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("per-system prose").strong());
    ensure_prose_catalog_if_needed(state);
    let Some(report) = state.prose_report.clone() else {
        ui.colored_label(
            Color32::GRAY,
            "No prose yet. Click \"Regenerate prose\" above.",
        );
        return;
    };
    if report.system_entries.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "Sector has no systems — generate the sector before authoring prose.",
        );
        return;
    }

    // §PR1 system picker. Seed from the SYSTEM tab's selection on first focus
    // so cross-tab navigation lands on a sensible row, then track the panel's
    // own pick.
    if state.selected_prose_system_id.is_none() {
        state.selected_prose_system_id = state.selected_system_id.clone();
    }
    let mut selected = state.selected_prose_system_id.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label("system");
        let label_for = |sid: &SystemId| -> String {
            report
                .system_entries
                .iter()
                .find(|e| &e.system_id == sid)
                .map(|e| format!("{} — {}", e.system_id, e.name))
                .unwrap_or_else(|| sid.to_string())
        };
        let current_label = selected
            .as_ref()
            .map(label_for)
            .unwrap_or_else(|| "select a system".to_string());
        ui_kit::combo("pr1_system", current_label).show_ui(ui, |ui| {
            for e in &report.system_entries {
                if ui
                    .selectable_label(
                        selected.as_ref() == Some(&e.system_id),
                        format!("{} — {}", e.system_id, e.name),
                    )
                    .clicked()
                {
                    selected = Some(e.system_id.clone());
                }
            }
        });
        if let Some(sid) = selected.as_ref() {
            if ui.link("→ system tab").clicked() {
                state.focus_entity(EntityRef::System(sid.clone()));
            }
        }
    });
    if selected != state.selected_prose_system_id {
        state.selected_prose_system_id = selected.clone();
    }

    let Some(sid) = selected else {
        ui.colored_label(Color32::GRAY, "Pick a system above to edit its prose.");
        return;
    };
    let Some(entry) = report
        .system_entries
        .iter()
        .find(|e| e.system_id == sid)
        .cloned()
    else {
        ui.colored_label(
            Color32::GRAY,
            format!("System `{sid}` is gone — regenerate to refresh."),
        );
        return;
    };

    let Some(cfg) = state.data_catalogs.prose.as_mut() else {
        return;
    };
    let mut is_override = cfg
        .overrides
        .systems
        .get(&sid)
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        let prev = is_override;
        if ui.checkbox(&mut is_override, "Override").changed() {
            if is_override && !prev {
                let seed = entry.paragraphs.join("\n\n");
                cfg.overrides.systems.insert(sid.clone(), seed);
                changed = true;
            } else if !is_override && prev {
                cfg.overrides.systems.remove(&sid);
                changed = true;
            }
        }
        if is_override {
            ui.colored_label(Color32::from_rgb(220, 170, 80), "AUTHORED");
            if ui.button("Revert to derived").clicked() {
                cfg.overrides.systems.remove(&sid);
                is_override = false;
                changed = true;
            }
        } else {
            ui.colored_label(Color32::DARK_GRAY, "derived");
        }
    });

    if is_override {
        let mut text = cfg.overrides.systems.get(&sid).cloned().unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .desired_rows(8)
                .hint_text("Authored system prose..."),
        );
        if resp.changed() {
            cfg.overrides.systems.insert(sid.clone(), text);
            changed = true;
        }
        if !entry.derived_paragraphs.is_empty() {
            ui.collapsing("Derived paragraphs (read-only)", |ui| {
                for p in &entry.derived_paragraphs {
                    ui.add(
                        egui::TextEdit::multiline(&mut p.as_str())
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                }
            });
        }
    } else {
        for p in &entry.paragraphs {
            ui.add(
                egui::TextEdit::multiline(&mut p.as_str())
                    .desired_width(f32::INFINITY)
                    .desired_rows(2),
            );
        }
        if entry.paragraphs.is_empty() {
            ui.colored_label(
                Color32::GRAY,
                "No derived paragraphs — system has no political / archetype colour to render.",
            );
        }
    }

    if changed {
        on_catalog_edited(state);
    }
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.prose.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save prose.toml"))
            .clicked()
        {
            if state.config.inputs.prose.is_none() {
                state.config.inputs.prose = Some(DEFAULT_PROSE_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save prose.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .prose
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_PROSE_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ────────────────────────────────────────────────────────

fn ensure_prose_catalog(state: &mut BuilderState) {
    if state.data_catalogs.prose.is_none() {
        state.data_catalogs.prose = Some(ProseConfig::default());
    }
    if state.config.inputs.prose.is_none() {
        state.config.inputs.prose = Some(DEFAULT_PROSE_PATH.into());
    }
}

fn ensure_prose_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.prose.is_none() {
        state.data_catalogs.prose = Some(ProseConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.prose.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_PROSE_PATH.into());
    }
    state.mark_validation_dirty();
    if state.prose_auto_recompute {
        state.recompute_prose();
    }
}

fn tone_label(t: ProseTone) -> &'static str {
    match t {
        ProseTone::Gazetteer => "Florid (Gazetteer)",
        ProseTone::Dispatch => "Administratum Dispatch",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.prose.is_none());
        ensure_prose_catalog(&mut state);
        assert!(state.data_catalogs.prose.is_some());
        assert_eq!(
            state.config.inputs.prose.as_deref(),
            Some(DEFAULT_PROSE_PATH)
        );
    }

    #[test]
    fn recompute_prose_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.prose = Some(ProseConfig::default());
        state.recompute_prose();
        assert!(state.prose_report.is_some());
    }

    #[test]
    fn overview_override_survives_recompute() {
        use sectorforge::prose::ProseOverrides;
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.prose = Some(ProseConfig {
            overrides: ProseOverrides {
                overview: Some("Authored overview.".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        state.recompute_prose();
        let report = state.prose_report.as_ref().unwrap();
        assert!(report.overview_is_override);
        assert_eq!(report.overview, "Authored overview.");
    }

    #[test]
    fn tone_change_threads_into_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.prose = Some(ProseConfig {
            tone: ProseTone::Dispatch,
            ..Default::default()
        });
        state.recompute_prose();
        let report = state.prose_report.as_ref().unwrap();
        assert!(report.tone.contains("dispatch"));
    }
}
