//! MAP tab transient dialogs.
//!
//! Modal-style egui windows that survive a frame on `state.pending_*` fields:
//! Place, Rename, Bulk-Rename, Region-Rename, Collision. Kept inside the panel
//! so the host shell does not need to learn new [`ModalKind`] variants for
//! §S1 / §S6 / §CTX1 Phase 3 / §CTX1 Phase 5.

use crate::builder::command::BuilderCommand;
use crate::builder::state::{PendingBulkRename, PendingPlace, PendingRegionRename, PendingRename};
use crate::builder::{BuilderState, ModalKind};

pub(super) fn show_place_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.drag.pending_place.clone() else {
        return;
    };
    let mut name = pending.name.clone();
    let mut close = false;
    let mut commit = false;
    egui::Window::new("Place system")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("hex ({}, {})", pending.coord.q, pending.coord.r));
            ui.text_edit_singleline(&mut name);
            ui.horizontal(|ui| {
                if ui.button("Place").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let cmd = BuilderCommand::AddSystem {
            coord: pending.coord,
            name: name.clone(),
            result_id: None,
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Add failed: {e}")));
        }
    }
    if close {
        state.drag.pending_place = None;
    } else {
        state.drag.pending_place = Some(PendingPlace {
            coord: pending.coord,
            name,
        });
    }
}

pub(super) fn show_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.drag.pending_rename.clone() else {
        return;
    };
    let mut text = pending.text.clone();
    let mut close = false;
    let mut commit = false;
    egui::Window::new("Rename system")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(pending.id.to_string());
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let from = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == pending.id)
            .map(|s| s.name.to_string())
            .unwrap_or_default();
        let cmd = BuilderCommand::RenameSystem {
            id: pending.id.clone(),
            from,
            to: text.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Rename failed: {e}")));
        }
    }
    if close {
        state.drag.pending_rename = None;
    } else {
        state.drag.pending_rename = Some(PendingRename {
            id: pending.id,
            text,
        });
    }
}

/// §CTX1 Phase 3 — BULK RENAME pattern dialog opened from the MAP tab's
/// right-click multi-selection menu. Pattern tokens (`{n}`, `{id}`,
/// `{name}`) match the §S4 bulk-ops dialog and dispatch through
/// [`crate::builder::panels::system::apply_bulk_rename`] on commit.
pub(super) fn show_bulk_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.drag.pending_bulk_rename.clone() else {
        return;
    };
    let n = state.selection.systems.len();
    let mut pattern = pending.pattern.clone();
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Bulk rename selection")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("{n} system(s) selected"));
            ui.label("Pattern — `{n}` = sequence, `{id}` = system id, `{name}` = current name");
            ui.text_edit_singleline(&mut pattern);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        crate::builder::panels::system::apply_bulk_rename(state, &pattern);
    }
    if close {
        state.drag.pending_bulk_rename = None;
    } else {
        state.drag.pending_bulk_rename = Some(PendingBulkRename { pattern });
    }
}

/// §CTX1 Phase 5 — modal rename dialog for the §6.5 "RENAME REGION…" entry.
/// Commits through [`BuilderCommand::RenameRegion`] so the change is undoable.
pub(super) fn show_region_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.drag.pending_region_rename.clone() else {
        return;
    };
    let before = state
        .sector
        .regions
        .iter()
        .find(|r| r.id == pending.region)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let mut text = pending.text.clone();
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Rename region")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("region: {} — current: {}", pending.region, before));
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                let enabled = !text.trim().is_empty() && text != before;
                if ui
                    .add_enabled(enabled, egui::Button::new("Rename"))
                    .clicked()
                {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let cmd = BuilderCommand::RenameRegion {
            region: pending.region.clone(),
            before: before.clone(),
            after: text.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Rename region failed: {e}")));
        }
    }
    if close {
        state.drag.pending_region_rename = None;
    } else {
        state.drag.pending_region_rename = Some(PendingRegionRename {
            region: pending.region,
            text,
        });
    }
}

pub(super) fn show_collision_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.drag.pending_collision.clone() else {
        return;
    };
    let mut close = false;
    let mut action: Option<CollisionAction> = None;
    egui::Window::new("Hex occupied")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!(
                "Hex ({},{}) is held by {}.",
                pending.target.q, pending.target.r, pending.occupant
            ));
            ui.horizontal(|ui| {
                if ui.button("Swap").clicked() {
                    action = Some(CollisionAction::Swap);
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if let Some(CollisionAction::Swap) = action {
        let cmd = BuilderCommand::SwapSystems {
            a: pending.dragging.clone(),
            b: pending.occupant.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Swap failed: {e}")));
        }
    }
    if close {
        state.drag.pending_collision = None;
    }
}

enum CollisionAction {
    Swap,
}
