//! WORLD tab — identity (§W1 / §W3), classification, and tags/notes sections.

use std::sync::Arc;

use egui::Ui;

use sectorforge::worlds::{StarColour, WorldType};
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

use super::{combo_enum, edit_world_rows};

// ── identity (W1 / W3) ──────────────────────────────────────────────────────

pub(super) fn show_identity_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_identity", "Identity", true, |ui| {
        let w = &state.sector.systems[sys_idx].worlds[w_idx];
        let wid = w.id.clone();
        let name_buf_key = egui::Id::new(("world_identity_name_buf", wid.as_str()));
        let name_src = w.name.to_string();
        let mut name_buf = name_src.clone();
        let index_str = w.index.to_string();
        let source_row_str = w.source_row_index.to_string();
        let mut orbit = i32::from(w.orbit);
        let mut name_changed = false;
        labeled(
            ui,
            "ID",
            "Unique identifier (schema: id). Stable handle used by routes, presence, and saved files.",
            |ui| {
                ui.monospace(wid.to_string());
            },
        );
        labeled(
            ui,
            "Order in system",
            "Position of this world within its system, counting outward (schema: index).",
            |ui| {
                ui.monospace(index_str);
            },
        );
        labeled(
            ui,
            "Source row",
            "Row this world came from in the worlds data table (schema: source_row_index).",
            |ui| {
                ui.monospace(source_row_str);
            },
        );
        labeled(
            ui,
            "Name",
            "Display name shown in lists and on the map (schema: name).",
            |ui| {
                let (buf, resp) =
                    crate::builder::panels::persistent_singleline(ui, name_buf_key, &name_src);
                name_buf = buf;
                name_changed = resp.lost_focus();
            },
        );
        labeled(
            ui,
            "Orbit",
            "Orbital slot of this world around its star (schema: orbit). 1 = innermost.",
            |ui| {
                ui.add(egui::DragValue::new(&mut orbit).range(1..=99));
            },
        );
        labeled(
            ui,
            "Pinned",
            "Protect this world from re-roll and preview regeneration (schema: pinned_worlds).",
            |ui| {
                let mut pinned = state.pinned_worlds.contains(&wid);
                if ui.checkbox(&mut pinned, "keep on re-roll").changed() {
                    if pinned {
                        state.pinned_worlds.insert(wid.clone());
                    } else {
                        state.pinned_worlds.remove(&wid);
                    }
                }
            },
        );
        // §R4: orbit/name now route through the narrow commands so the edit
        // is undoable. Snapshot the live values, then dispatch with no
        // borrow of `state.sector` held across `state.run`.
        let cur_orbit = state.sector.systems[sys_idx].worlds[w_idx].orbit;
        let cur_name = state.sector.systems[sys_idx].worlds[w_idx].name.to_string();
        let new_orbit = orbit.clamp(1, 99) as u8;
        if new_orbit != cur_orbit {
            if let Err(e) = state.run(BuilderCommand::SetWorldOrbit {
                world: wid.clone(),
                before: 0,
                after: new_orbit,
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
        if name_changed && name_buf.trim() != cur_name {
            let after = name_buf.trim().to_string();
            if let Err(e) = state.run(BuilderCommand::RenameWorld {
                world: wid.clone(),
                before: String::new(),
                after,
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
            crate::builder::panels::persistent_text_clear(ui, name_buf_key);
        }
    });
}

// ── classification ─────────────────────────────────────────────────────────

pub(super) fn show_classification_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_classification", "Classification", true, |ui| {
        edit_world_rows(ui, state, sys_idx, w_idx, |ui, w| {
            let prev = w.star_colour;
            labeled(
                ui,
                "Star colour",
                "Spectral class of the system's star (schema: star_colour_code). Sets the light and habitable band.",
                |ui| {
                    ui_kit::combo("w_star", format!("{} ({})", prev.code(), prev.short_name()))
                        .show_ui(ui, |ui| {
                            for v in StarColour::VARIANTS {
                                ui.selectable_value(
                                    &mut w.star_colour,
                                    *v,
                                    format!("{} — {}", v.code(), v.short_name()),
                                );
                            }
                        });
                },
            );
            let mut changed = w.star_colour != prev;
            changed |= combo_enum(
                ui,
                "World type",
                "Overall world archetype (schema: world_type) — e.g. Hive, Agri, Forge, Death.",
                "w_type",
                &mut w.world_type,
                WorldType::VARIANTS,
                WorldType::display_name,
            );
            changed
        });
    });
}

// ── tags + notes ────────────────────────────────────────────────────────────

pub(super) fn show_tags_notes_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_tags_notes", "Tags + Notes", false, |ui| {
        let wid_key = state.sector.systems[sys_idx].worlds[w_idx]
            .id
            .as_str()
            .to_string();
        let tags_key = egui::Id::new(("world_tags_buf", wid_key.as_str()));
        let notes_key = egui::Id::new(("world_notes_buf", wid_key.as_str()));
        let tags_src = state.sector.systems[sys_idx].worlds[w_idx]
            .tags
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let notes_src = state.sector.systems[sys_idx].worlds[w_idx]
            .notes
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.label("Tags")
            .on_hover_text("Comma-separated keywords for filtering and search (schema: tags).");
        let (tags_buf, tags_resp) =
            crate::builder::panels::persistent_singleline(ui, tags_key, &tags_src);
        let tags_changed = tags_resp.lost_focus();
        ui.label("Notes")
            .on_hover_text("Free-form notes, one per line (schema: notes).");
        let (notes_buf, notes_resp) =
            crate::builder::panels::persistent_multiline(ui, notes_key, &notes_src);
        let notes_changed = notes_resp.lost_focus();
        if tags_changed {
            // §R4: commit tags via EditWorld on a world clone.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(wid, |w| {
                w.tags = tags_buf
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
            crate::builder::panels::persistent_text_clear(ui, tags_key);
        }
        if notes_changed {
            // §R4: commit notes via EditWorld on a world clone.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(wid, |w| {
                w.notes = notes_buf
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
            crate::builder::panels::persistent_text_clear(ui, notes_key);
        }
    });
}
