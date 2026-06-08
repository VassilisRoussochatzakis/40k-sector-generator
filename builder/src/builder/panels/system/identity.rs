//! SYSTEM tab — identity (§S2 + §S6), star, and tags/notes sections.

use std::sync::Arc;

use egui::{RichText, Ui};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{HexCoord, SystemKind};
use sectorforge_gui_core::{palette, ui_kit};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

use super::system_kind_label;

pub(super) fn show_identity_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_identity", "Identity", true, |ui| {
        let sys = &state.sector.systems[sys_idx];
        let id = sys.id.clone();
        let coord = sys.coord;
        let kind = sys.kind;
        let name_buf_key = egui::Id::new(("sys_identity_name_buf", id.as_str()));
        let kind_choice_key = egui::Id::new(("sys_identity_kind_choice", id.as_str()));
        let coord_q_key = egui::Id::new(("sys_identity_coord_q", id.as_str()));
        let coord_r_key = egui::Id::new(("sys_identity_coord_r", id.as_str()));
        let source_name = sys.name.to_string();
        // Persist q/r across frames so DragValue edits survive until the
        // user clicks "Apply coord". Without this the locals reseed from
        // `coord` next frame and the in-flight value is lost.
        let mut q = ui
            .data_mut(|d| d.get_temp::<i32>(coord_q_key))
            .unwrap_or(coord.q);
        let mut r = ui
            .data_mut(|d| d.get_temp::<i32>(coord_r_key))
            .unwrap_or(coord.r);
        // Persist kind_choice across frames so the "Apply kind" button
        // remains visible after the user picks a new option in the combo.
        // Without this the local reseeds from `kind` next frame and the
        // pending selection is lost before the user can confirm it.
        let mut kind_choice = ui
            .data_mut(|d| d.get_temp::<SystemKind>(kind_choice_key))
            .unwrap_or(kind);

        let mut name_buf = String::new();
        let mut name_changed = false;
        egui::Grid::new("sys_identity_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("ID")
                    .on_hover_text("Unique system id (schema: id). Read-only — used by routes, presence, and saved files.");
                ui.monospace(id.to_string());
                ui.end_row();
                ui.label("Name")
                    .on_hover_text("Display name shown in lists and on the map (schema: name).");
                let (buf, resp) =
                    crate::builder::panels::persistent_singleline(ui, name_buf_key, &source_name);
                name_buf = buf;
                name_changed = resp.lost_focus();
                ui.end_row();
                ui.label("Coordinate")
                    .on_hover_text("Sector grid cell, column q / row r (schema: coord). Click Apply coordinate to move.");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut q)
                            .range(0..=state.sector.width as i32 - 1)
                            .prefix("q"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut r)
                            .range(0..=state.sector.height as i32 - 1)
                            .prefix("r"),
                    );
                });
                ui.data_mut(|d| {
                    d.insert_temp(coord_q_key, q);
                    d.insert_temp(coord_r_key, r);
                });
                ui.end_row();
                ui.label("Type")
                    .on_hover_text("What kind of location this is (schema: kind). Changes the glyph drawn on the map.");
                ui_kit::combo("sys_kind", system_kind_label(kind_choice)).show_ui(ui, |ui| {
                    for k in [
                        SystemKind::Star,
                        SystemKind::SpecialLocation,
                        SystemKind::BlackHole,
                        SystemKind::WarpAnomaly,
                        SystemKind::SpaceStation,
                    ] {
                        ui.selectable_value(&mut kind_choice, k, system_kind_label(k))
                            .on_hover_text(format!("schema: {}", k.as_slug()));
                    }
                });
                ui.data_mut(|d| d.insert_temp(kind_choice_key, kind_choice));
                ui.end_row();
                ui.label("Pinned")
                    .on_hover_text("When on, the generator won't regenerate or reseed this system.");
                let mut pinned = state.pinned_systems.contains(&id);
                if ui
                    .checkbox(&mut pinned, "Protect from regeneration")
                    .changed()
                {
                    if pinned {
                        state.pinned_systems.insert(id.clone());
                    } else {
                        state.pinned_systems.remove(&id);
                    }
                }
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if (ui
                .button("Apply name")
                .on_hover_text("Rename this system")
                .clicked()
                || name_changed)
                && name_buf != *state.sector.systems[sys_idx].name
            {
                let from = state.sector.systems[sys_idx].name.to_string();
                let cmd = BuilderCommand::RenameSystem {
                    id: id.clone(),
                    from,
                    to: name_buf.clone(),
                };
                if let Err(e) = state.run(cmd) {
                    state.feedback.modal = Some(ModalKind::Message(format!("Rename failed: {e}")));
                } else {
                    crate::builder::panels::persistent_text_clear(ui, name_buf_key);
                }
            }
            if ui
                .button("Apply coordinate")
                .on_hover_text("Move this system to the entered grid cell")
                .clicked()
            {
                let new_coord = HexCoord { q, r };
                if new_coord != coord {
                    apply_coord_move(state, id.clone(), coord, new_coord);
                }
                ui.data_mut(|d| {
                    d.remove::<i32>(coord_q_key);
                    d.remove::<i32>(coord_r_key);
                });
            }
            if kind_choice != kind
                && ui
                    .button("Apply type")
                    .on_hover_text("Change this system's type")
                    .clicked()
            {
                // §R4: route the kind change through EditSystem so undo/redo
                // and the validation pump pick it up (was a direct field
                // write). `worlds` rides through the system clone unchanged.
                let sys_id = state.sector.systems[sys_idx].id.clone();
                if let Err(e) = state.edit_system(sys_id, |sys| sys.kind = kind_choice) {
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("System edit failed: {e}")));
                } else {
                    ui.data_mut(|d| d.remove::<SystemKind>(kind_choice_key));
                }
            }
        });
    });
}

pub(super) fn apply_coord_move(
    state: &mut BuilderState,
    id: SystemId,
    from: HexCoord,
    to: HexCoord,
) {
    if to.q < 0
        || to.r < 0
        || (to.q as u32) >= state.sector.width
        || (to.r as u32) >= state.sector.height
    {
        state.feedback.modal = Some(ModalKind::Message(format!(
            "Coord ({},{}) out of bounds {}x{}.",
            to.q, to.r, state.sector.width, state.sector.height
        )));
        return;
    }
    let occupant = state
        .sector
        .systems
        .iter()
        .find(|s| s.coord == to && s.id != id)
        .map(|s| s.id.clone());
    if let Some(occupant) = occupant {
        state.drag.pending_collision = Some(crate::builder::state::PendingCollision {
            dragging: id,
            target: to,
            occupant,
        });
        return;
    }
    let cmd = BuilderCommand::MoveSystem { id, from, to };
    if let Err(e) = state.run(cmd) {
        state.feedback.modal = Some(ModalKind::Message(format!("Move failed: {e}")));
    }
}

// ── star ────────────────────────────────────────────────────────────────────

pub(super) fn show_star_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
) -> egui::CollapsingResponse<()> {
    // §UO P3a: framed to match `ui_kit::collapsing_section`, but kept as a raw
    // `Frame::group` + `CollapsingHeader` because the caller consumes
    // `header_response.scroll_to_me` (§S star-grid anchor) — which the helper's
    // `Option<R>` return does not expose. Margins mirror the helper exactly.
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(8.0))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            egui::CollapsingHeader::new("Star")
                .default_open(false)
                .show(ui, |ui| {
                    let sys_id_key = state.sector.systems[sys_idx].id.as_str().to_string();
                    let mut has_star = state.sector.systems[sys_idx].star.is_some();
                    let mut toggle_star = false;
                    if ui
                        .checkbox(&mut has_star, "Has a central star")
                        .on_hover_text("Whether this system has a star at its centre (schema: star).")
                        .changed()
                    {
                        toggle_star = true;
                    }
                    let mut star_buf = state.sector.systems[sys_idx].star.clone();
                    let mut field_changed = false;
                    let (code_key, name_key, spectral_key) = (
                        egui::Id::new(("sys_star_code_buf", sys_id_key.as_str())),
                        egui::Id::new(("sys_star_name_buf", sys_id_key.as_str())),
                        egui::Id::new(("sys_star_spectral_buf", sys_id_key.as_str())),
                    );
                    let mut new_code = String::new();
                    let mut new_name = String::new();
                    let mut new_spectral = String::new();
                    if let Some(star) = star_buf.as_mut() {
                        let code_src = star.colour_code.to_string();
                        let name_src = star.colour_name.to_string();
                        let spectral_src = star
                            .spectral_type
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        egui::Grid::new("sys_star_grid")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        palette::star_color(&code_src),
                                        RichText::new("⬛").small(),
                                    );
                                    ui.label("Colour class").on_hover_text(
                                        "Spectral colour class, e.g. G or M (schema: colour_code). Sets the star's tint on the map.",
                                    );
                                });
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui, code_key, &code_src,
                                );
                                new_code = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                                ui.label("Colour name").on_hover_text(
                                    "Human name for the colour, e.g. Yellow (schema: colour_name).",
                                );
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui, name_key, &name_src,
                                );
                                new_name = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                                ui.label("Spectral type").on_hover_text(
                                    "Optional detailed spectral type, e.g. G2V (schema: spectral_type). Leave blank if unknown.",
                                );
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui,
                                    spectral_key,
                                    &spectral_src,
                                );
                                new_spectral = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                            });
                        star.colour_code = Arc::from(new_code.as_str());
                        star.colour_name = Arc::from(new_name.as_str());
                        star.spectral_type = if new_spectral.trim().is_empty() {
                            None
                        } else {
                            Some(Arc::from(new_spectral.as_str()))
                        };
                    }

                    // §R4: both the present-toggle and the colour/spectral field edits
                    // now funnel through SetStar (was a direct `sector_mut().star`
                    // write). `before: None` lets `apply` snapshot the prior star so
                    // revert is exact. Mirrors the SetStar call shape used by the
                    // in-system right-click menu in `panels/system_map.rs`.
                    let current_star = state.sector.systems[sys_idx].star.clone();
                    if toggle_star {
                        let after = if has_star && current_star.is_none() {
                            Some(sectorforge::sector_model::GeneratedStar {
                                colour_code: Arc::from("G"),
                                colour_name: Arc::from("Yellow"),
                                spectral_type: None,
                                source_row_index: None,
                            })
                        } else if !has_star {
                            None
                        } else {
                            current_star.clone()
                        };
                        // Only the present/absent toggle is meaningful here; the no-op
                        // re-check (star already present, still present) leaves `after`
                        // == `current_star` and must not push a command. `GeneratedStar`
                        // has no `PartialEq`, so compare presence rather than value.
                        if after.is_some() != current_star.is_some() {
                            let cmd = BuilderCommand::SetStar {
                                system: state.sector.systems[sys_idx].id.clone(),
                                before: None,
                                after,
                            };
                            if let Err(e) = state.run(cmd) {
                                state.feedback.modal =
                                    Some(ModalKind::Message(format!("Star update failed: {e}")));
                            }
                        }
                    } else if field_changed {
                        let cmd = BuilderCommand::SetStar {
                            system: state.sector.systems[sys_idx].id.clone(),
                            before: None,
                            after: star_buf,
                        };
                        if let Err(e) = state.run(cmd) {
                            state.feedback.modal =
                                Some(ModalKind::Message(format!("Star update failed: {e}")));
                        } else {
                            crate::builder::panels::persistent_text_clear(ui, code_key);
                            crate::builder::panels::persistent_text_clear(ui, name_key);
                            crate::builder::panels::persistent_text_clear(ui, spectral_key);
                        }
                    }
                })
        })
        .inner
}

// ── tags + notes ────────────────────────────────────────────────────────────

pub(super) fn show_tags_notes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_tags_notes", "Tags + Notes", false, |ui| {
        let sys_id_key = state.sector.systems[sys_idx].id.as_str().to_string();
        let tags_key = egui::Id::new(("sys_tags_buf", sys_id_key.as_str()));
        let notes_key = egui::Id::new(("sys_notes_buf", sys_id_key.as_str()));
        let tags_src = state.sector.systems[sys_idx]
            .tags
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let notes_src = state.sector.systems[sys_idx]
            .notes
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.label("Tags").on_hover_text(
            "Free-form labels, comma-separated (schema: tags). Used for filtering and flavour.",
        );
        let (tags_buf, tags_resp) =
            crate::builder::panels::persistent_singleline(ui, tags_key, &tags_src);
        let tags_changed = tags_resp.lost_focus();
        ui.label("Notes")
            .on_hover_text("GM notes, one per line (schema: notes).");
        let (notes_buf, notes_resp) =
            crate::builder::panels::persistent_multiline(ui, notes_key, &notes_src);
        let notes_changed = notes_resp.lost_focus();
        if tags_changed {
            // §R4: tags edit rides an EditSystem clone (was a direct
            // `systems[i].tags` write) so it lands on the undo log.
            let sys_id = state.sector.systems[sys_idx].id.clone();
            if let Err(e) = state.edit_system(sys_id, |sys| {
                sys.tags = tags_buf
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            } else {
                crate::builder::panels::persistent_text_clear(ui, tags_key);
            }
        }
        if notes_changed {
            // §R4: notes edit rides an EditSystem clone (was a direct
            // `systems[i].notes` write) so it lands on the undo log.
            let sys_id = state.sector.systems[sys_idx].id.clone();
            if let Err(e) = state.edit_system(sys_id, |sys| {
                sys.notes = notes_buf
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            } else {
                crate::builder::panels::persistent_text_clear(ui, notes_key);
            }
        }
    });
}
