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

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};
use sectorforge_gui_core::widgets;

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

const DEFAULT_PROSE_PATH: &str = "data/prose.toml";

const TONE_VARIANTS: &[ProseTone] = &[ProseTone::Gazetteer, ProseTone::Dispatch];

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    // Audit finding #8: open / settle the catalog-edit coalescing session.
    state.begin_catalog_session();
    ui.heading("Prose");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "The flavour text for your sector — a sector overview plus a paragraph for each system. \
         Regenerate to refresh it; anything you write by hand is kept.",
    );
    ui.separator();

    // §COLUMNS — RC-2: regenerate / tone / system selector / save controls sit
    // in the left column; the editable prose bodies (sector overview + the
    // selected system's paragraphs) fill the right column, width-capped via
    // `reading_column` so the body text keeps a readable line length on a wide
    // window. Collapses to a single stacked column on a narrow window. The
    // system pick is read on the right via `selected_prose_system_id`, which the
    // left picker writes — both run sequentially so the borrow is clean.
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left: actions + tone + system selector + save.
                    let left = &mut cols[0];
                    show_header_actions(left, state);
                    left.separator();
                    show_tone_section(left, state);
                    left.separator();
                    show_system_selector(left, state);
                    left.separator();
                    show_save_row(left, state);
                }
                {
                    // Right — or the same single column when collapsed to one:
                    // the width-capped prose editors.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    if n == 1 {
                        right.separator();
                    }
                    ui_kit::reading_column(right, 720.0, |ui| {
                        show_overview_editor(ui, state);
                        ui.separator();
                        show_system_editor(ui, state);
                    });
                }
            });
        });
}

// ── §PR4 header actions ───────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Regenerate prose")
            .on_hover_text("Rewrite every paragraph from the current sector. Your hand-written overrides are kept.")
            .clicked()
        {
            ensure_prose_catalog(state);
            state.recompute_prose();
        }
        ui.checkbox(&mut state.prose_auto_recompute, "Auto-refresh on edit")
            .on_hover_text("Regenerate automatically after each change instead of waiting for the button.");
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
            "{systems} system(s)  ·  {overrides_count} hand-written{}",
            if overview_override {
                " (+ overview)"
            } else {
                ""
            },
        ))
        .on_hover_text("How many system paragraphs you've overridden by hand, out of the systems in the sector.");
        if state.data_catalogs.prose.is_none() {
            ui.colored_label(
                palette::warning(),
                "using defaults — nothing saved yet",
            )
            .on_hover_text("No prose has been saved for this project; the generated defaults are shown.");
        }
    });
}

// ── §PR3 tone preset ──────────────────────────────────────────────────────

fn show_tone_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Writing style").strong());
    ensure_prose_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.prose.as_mut() else {
        return;
    };
    let mut changed = false;
    labeled(
        ui,
        "Tone",
        "How the generated text reads (schema: tone). Florid for evocative gazetteer prose, or Dispatch for a terse Administratum report.",
        |ui| {
            ui_kit::combo("pr3_tone", tone_label(cfg.tone)).show_ui(ui, |ui| {
                for t in TONE_VARIANTS {
                    if ui
                        .selectable_value(&mut cfg.tone, *t, tone_label(*t))
                        .on_hover_text(format!("key: {}", t.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "Sector overview",
        "Include the sector-wide overview paragraph in the output (schema: include_overview).",
        |ui| {
            changed |= ui.checkbox(&mut cfg.include_overview, "Include").changed();
        },
    );
    labeled(
        ui,
        "Per-system entries",
        "Include a paragraph for each system in the output (schema: include_per_system).",
        |ui| {
            changed |= ui
                .checkbox(&mut cfg.include_per_system, "Include")
                .changed();
        },
    );
    ui.colored_label(
        Color32::DARK_GRAY,
        "Tone only affects generated text — anything you write by hand is stored as-is and ignores this setting.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §PR2 sector overview editor ───────────────────────────────────────────

fn show_overview_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Sector overview").strong());
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
        if ui
            .checkbox(&mut is_override, "Write my own")
            .on_hover_text("Replace the generated overview with text you type here.")
            .changed()
        {
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
            ui.colored_label(palette::warning(), "Hand-written");
        } else {
            ui.colored_label(Color32::DARK_GRAY, "Generated");
        }
    });

    if is_override {
        let mut text = cfg.overrides.overview.clone().unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .desired_rows(4)
                .hint_text("Type the sector overview here..."),
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
        ui_kit::placeholder(
            ui,
            "No overview yet — click \"Regenerate prose\" above to create one.",
        );
    }

    if changed {
        on_catalog_edited(state);
    }
}

// ── §PR1 per-system selector (left column) ────────────────────────────────

/// §COLUMNS — the system picker lives in the left config rail; the prose body
/// for the picked system is rendered by [`show_system_editor`] in the right
/// reading column. Both run sequentially so each takes `state` fresh.
fn show_system_selector(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Per-system prose").strong());
    ensure_prose_catalog_if_needed(state);
    let Some(report) = state.prose_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No prose yet — click \"Regenerate prose\" above to create it.",
        );
        return;
    };
    if report.system_entries.is_empty() {
        ui_kit::placeholder(
            ui,
            "This sector has no systems yet — add some before writing prose.",
        );
        return;
    }

    // §PR1 system picker. Seed from the SYSTEM tab's selection on first focus
    // so cross-tab navigation lands on a sensible row, then track the panel's
    // own pick.
    if state.selection.prose_system_id.is_none() {
        state.selection.prose_system_id = state.selection.system_id.clone();
    }
    let mut selected = state.selection.prose_system_id.clone();
    let label_for = |sid: &SystemId| -> String {
        report
            .system_entries
            .iter()
            .find(|e| &e.system_id == sid)
            .map(|e| format!("{} — {}", e.name, e.system_id))
            .unwrap_or_else(|| sid.to_string())
    };
    labeled(
        ui,
        "System",
        "Which system's paragraph you're editing on the right. Pick one to load its prose.",
        |ui| {
            let current_label = selected
                .as_ref()
                .map(label_for)
                .unwrap_or_else(|| "Pick a system…".to_string());
            ui_kit::combo("pr1_system", current_label).show_ui(ui, |ui| {
                for e in &report.system_entries {
                    if ui
                        .selectable_label(
                            selected.as_ref() == Some(&e.system_id),
                            format!("{} — {}", e.name, e.system_id),
                        )
                        .clicked()
                    {
                        selected = Some(e.system_id.clone());
                    }
                }
            });
            if let Some(sid) = selected.as_ref() {
                if ui
                    .link("Open in System tab  →")
                    .on_hover_text("Jump to the System tab to edit this system's data.")
                    .clicked()
                {
                    state.focus_entity(EntityRef::System(sid.clone()));
                }
            }
        },
    );
    if selected != state.selection.prose_system_id {
        state.selection.prose_system_id = selected.clone();
    }
}

// ── §PR1 per-system editor (right reading column) ─────────────────────────

fn show_system_editor(ui: &mut Ui, state: &mut BuilderState) {
    ensure_prose_catalog_if_needed(state);
    let Some(report) = state.prose_report.clone() else {
        return;
    };
    if report.system_entries.is_empty() {
        return;
    }

    let Some(sid) = state.selection.prose_system_id.clone() else {
        ui_kit::placeholder(ui, "Pick a system on the left to edit its prose here.");
        return;
    };
    let Some(entry) = report
        .system_entries
        .iter()
        .find(|e| e.system_id == sid)
        .cloned()
    else {
        ui_kit::placeholder(
            ui,
            &format!("System {sid} is no longer in the sector — click \"Regenerate prose\" to refresh the list."),
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
        if ui
            .checkbox(&mut is_override, "Write my own")
            .on_hover_text("Replace this system's generated paragraphs with text you type here.")
            .changed()
        {
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
            ui.colored_label(palette::warning(), "Hand-written");
            if ui
                .button("↺  Use generated text")
                .on_hover_text("Discard your edits and go back to the generated paragraphs.")
                .clicked()
            {
                cfg.overrides.systems.remove(&sid);
                is_override = false;
                changed = true;
            }
        } else {
            ui.colored_label(Color32::DARK_GRAY, "Generated");
        }
    });

    if is_override {
        let mut text = cfg.overrides.systems.get(&sid).cloned().unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .desired_rows(8)
                .hint_text("Type this system's prose here..."),
        );
        if resp.changed() {
            cfg.overrides.systems.insert(sid.clone(), text);
            changed = true;
        }
        if !entry.derived_paragraphs.is_empty() {
            ui.collapsing("Generated text (for reference)", |ui| {
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
            ui_kit::placeholder(
                ui,
                "Nothing to show for this system yet — it has no political or archetype colour to describe.",
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
            .add_enabled(has_catalog, egui::Button::new("💾  Save prose"))
            .on_hover_text("Write the prose for this project to disk.")
            .clicked()
        {
            if state.config.inputs.prose.is_none() {
                state.config.inputs.prose = Some(DEFAULT_PROSE_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.feedback.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Couldn't save prose: {e}"
                )));
            }
        }
        let saved_path = state.config.inputs.prose.clone();
        let (status, where_hover) = match &saved_path {
            Some(p) => ("Saves to the project prose file.", p.clone()),
            None => (
                "Not saved yet — will create a new prose file.",
                DEFAULT_PROSE_PATH.to_string(),
            ),
        };
        ui.colored_label(Color32::DARK_GRAY, status)
            .on_hover_text(format!("File: {where_hover}"));
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
    // P6: the lazy seed above is a document-state write — arm the coalescing
    // session so the seeded catalog is tracked for dirty/undo.
    state.note_catalog_edit();
}

fn ensure_prose_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.prose.is_none() {
        state.data_catalogs.prose = Some(ProseConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    // Audit finding #8: arm the coalescing session (see personae.rs).
    state.note_catalog_edit();
    state.mark_catalog_dirty(state.config.inputs.prose.clone(), DEFAULT_PROSE_PATH);
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
