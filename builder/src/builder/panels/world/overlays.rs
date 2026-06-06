//! WORLD tab — control summary, map-overlay summary, §H8 chronicle snippets,
//! and the §W4 re-roll section.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::FactionId;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::state::{BuilderTab, EntityRef, ModalKind};
use crate::builder::BuilderState;

// ── control summary ────────────────────────────────────────────────────────

pub(super) fn show_control_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(
        ui,
        "world_control",
        "Who holds power (read-only)",
        false,
        |ui| {
            let c = state.sector.systems[sys_idx].worlds[w_idx].control.clone();
            ui_kit::placeholder(
                ui,
                "Derived from faction presence and claims. Edit those above to change it.",
            );
            let who = |f: &Option<FactionId>| {
                f.as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "(none)".to_string())
            };
            labeled(
                ui,
                "Dominant",
                "Faction with the strongest overall hold (schema: control.dominant).",
                |ui| {
                    ui.label(who(&c.dominant));
                },
            );
            labeled(
                ui,
                "Sovereign",
                "Recognised legal ruler (schema: control.sovereign).",
                |ui| {
                    ui.label(who(&c.sovereign));
                },
            );
            labeled(
                ui,
                "Occupier",
                "Faction holding the world by force (schema: control.occupier).",
                |ui| {
                    ui.label(who(&c.occupier));
                },
            );
            labeled(
                ui,
                "Economic power",
                "Faction that dominates trade and industry (schema: control.economic_hegemon).",
                |ui| {
                    ui.label(who(&c.economic_hegemon));
                },
            );
            labeled(
                ui,
                "Popular authority",
                "Faction with the people's loyalty (schema: control.popular_authority).",
                |ui| {
                    ui.label(who(&c.popular_authority));
                },
            );
            labeled(
                ui,
                "Hidden master",
                "Faction secretly pulling the strings (schema: control.hidden_master).",
                |ui| {
                    ui.label(who(&c.hidden_master));
                },
            );
            labeled(
                ui,
                "Contested",
                "Whether control is currently in dispute (schema: control.contested).",
                |ui| {
                    ui.label(if c.contested { "yes" } else { "no" });
                },
            );
            labeled(
                ui,
                "Control score",
                "Overall strength of the dominant faction's hold (schema: control.control_score).",
                |ui| {
                    ui.label(format!("{:.1}", c.control_score));
                },
            );
        },
    );
}

// ── overlays read-only ─────────────────────────────────────────────────────

pub(super) fn show_overlays_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(
        ui,
        "world_overlays",
        "Map overlays (summary)",
        false,
        |ui| {
            let w = &state.sector.systems[sys_idx].worlds[w_idx];
            let conflict_default = sectorforge::conflict::ConflictState::is_default(&w.conflict);
            let stability_default =
                sectorforge::stability::StabilityState::is_default(&w.stability);
            labeled(
            ui,
            "Surface regions",
            "Number of mapped surface regions on this world (schema: regions). Edit them in the section below.",
            |ui| {
                ui.label(w.regions.len().to_string());
            },
        );
            labeled(
                ui,
                "Conflict",
                "Whether this world carries custom conflict data (schema: conflict).",
                |ui| {
                    ui.label(if conflict_default {
                        "none set (default)"
                    } else {
                        "customised"
                    });
                },
            );
            labeled(
                ui,
                "Stability",
                "Whether this world carries custom stability data (schema: stability).",
                |ui| {
                    ui.label(if stability_default {
                        "none set (default)"
                    } else {
                        "customised"
                    });
                },
            );
        },
    );
}

// ── §H8 chronicle snippets ─────────────────────────────────────────────────

pub(super) fn show_chronicle_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let sys_id = state.sector.systems[sys_idx].id.clone();
    let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
    // Snapshot rows up-front so the closure body can mutate `state` freely.
    let rows: Vec<(String, String, String, String, bool)> = {
        let events =
            crate::builder::panels::history::world_chronicle_events(state, &sys_id, &world_id);
        events
            .iter()
            .map(|e| {
                (
                    e.id.clone(),
                    e.date.clone(),
                    crate::builder::panels::history::kind_label(e.kind).to_string(),
                    e.narrative.clone(),
                    e.manual,
                )
            })
            .collect()
    };
    let count = rows.len();
    ui_kit::collapsing_section(
        ui,
        "world_chronicle",
        &format!("Chronicle snippets ({count})"),
        false,
        |ui| {
            if rows.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No timeline events set on this world yet. Open the History tab and regenerate to create some.",
                );
                if sectorforge_gui_core::entity_link(ui, "History tab", true).clicked() {
                    state.focus_entity(EntityRef::Tab(BuilderTab::History));
                }
                return;
            }
            let mut jump_to: Option<String> = None;
            for (id, date, kind, narrative, manual) in &rows {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(date).monospace().strong());
                    ui.label(kind.as_str());
                    if *manual {
                        ui.colored_label(palette::success(), "manual");
                    }
                    if ui
                        .small_button("Open in History →")
                        .on_hover_text("Jump to this event on the History tab")
                        .clicked()
                    {
                        jump_to = Some(id.clone());
                    }
                });
                ui.colored_label(Color32::DARK_GRAY, narrative);
                ui.separator();
            }
            if let Some(id) = jump_to {
                state.focus_entity(EntityRef::HistoryEvent(id));
            }
        },
    );
}

// ── W4 regen ───────────────────────────────────────────────────────────────

pub(super) fn show_regen_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_reroll", "Re-roll this world", false, |ui| {
        let pinned = state
            .pinned_worlds
            .contains(&state.sector.systems[sys_idx].worlds[w_idx].id);
        let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
        if pinned {
            ui.colored_label(
                palette::warning(),
                "Pinned — unpin in Identity above to re-roll.",
            );
            return;
        }
        ui.label("Randomly redraws star colour, type, atmosphere, temperature, biosphere, population, tech, government and features from your current data tables.");
        ui.label(
            RichText::new(format!(
                "re-rolls this session: {}",
                state.generation.world_reroll_counter
            ))
            .color(Color32::DARK_GRAY),
        );
        if ui
            .button("🔄 Re-roll this world")
            .on_hover_text("Generate a fresh random world here, keeping its id and orbit")
            .clicked()
        {
            if let Err(e) = state.regenerate_world(&wid) {
                state.feedback.modal =
                    Some(ModalKind::Message(format!("World re-roll failed: {e}")));
            }
        }
    });
}
