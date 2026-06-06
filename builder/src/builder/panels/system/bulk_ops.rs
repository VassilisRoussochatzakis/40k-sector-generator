//! SYSTEM tab — §S4 bulk operations over the multi-selection.

use std::collections::BTreeSet;

use egui::Ui;

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{HexCoord, SystemState};
use sectorforge_gui_core::ui_kit;

use crate::builder::command::BuilderCommand;
use crate::builder::state::{EntityRef, ModalKind};
use crate::builder::BuilderState;

use super::system_state_label;

// ── S4 bulk ops ─────────────────────────────────────────────────────────────

pub(super) fn show_bulk_ops(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "sys_bulk_ops", "Bulk operations", false, |ui| {
        let n = state.selection.systems.len();
        ui.label(format!("{n} system(s) selected"));
        if n == 0 {
            ui_kit::placeholder(
                ui,
                "Nothing selected — Shift-click systems or drag a box on the MAP tab to act on several at once.",
            );
            return;
        }

        ui.horizontal(|ui| {
            if ui
                .button("Clear selection")
                .on_hover_text("Deselect every system")
                .clicked()
            {
                state.selection.systems.clear();
            }
            if ui
                .button("📌 Pin all")
                .on_hover_text("Protect every selected system from regeneration")
                .clicked()
            {
                for id in state.selection.systems.iter().cloned().collect::<Vec<_>>() {
                    state.pinned_systems.insert(id);
                }
            }
            if ui
                .button("Unpin all")
                .on_hover_text("Allow every selected system to be regenerated again")
                .clicked()
            {
                for id in state.selection.systems.iter().cloned().collect::<Vec<_>>() {
                    state.pinned_systems.remove(&id);
                }
            }
        });

        ui.separator();
        ui.label("Rename all selected").on_hover_text(
            "Tokens: {n} = sequence number, {id} = system id, {name} = current name",
        );
        let pattern = ui.data_mut(|d| {
            d.get_temp_mut_or::<String>(egui::Id::new("bulk_rename_pat"), "Sys-{n}".into())
                .clone()
        });
        let mut pattern_buf = pattern;
        if ui
            .add(egui::TextEdit::singleline(&mut pattern_buf).hint_text("e.g. Sys-{n}"))
            .changed()
        {
            ui.data_mut(|d| {
                d.insert_temp(egui::Id::new("bulk_rename_pat"), pattern_buf.clone());
            });
        }
        if ui
            .button("Apply rename pattern")
            .on_hover_text("Rename every selected system using the pattern above")
            .clicked()
        {
            apply_bulk_rename(state, &pattern_buf);
        }

        ui.separator();
        ui.label("Set primary faction for all selected");
        let factions: Vec<_> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (fid, name) in &factions {
                if ui
                    .button(format!("→ {name} ({fid})"))
                    .on_hover_text("Add this faction as a primary on every selected system")
                    .clicked()
                {
                    apply_bulk_primary_faction(state, fid.clone());
                }
                if sectorforge_gui_core::entity_link(ui, fid.to_string(), true).clicked() {
                    state.focus_entity(EntityRef::Faction(fid.clone()));
                }
            }
        });
        if ui
            .button("Clear primary factions")
            .on_hover_text("Remove all primary factions from the selected systems")
            .clicked()
        {
            apply_bulk_clear_factions(state);
        }

        ui.separator();
        ui.label("Set control status for all selected");
        ui.horizontal_wrapped(|ui| {
            for s in [
                None,
                Some(SystemState::Pacified),
                Some(SystemState::Fragmented),
                Some(SystemState::Blockaded),
                Some(SystemState::Warzone),
                Some(SystemState::Infiltrated),
                Some(SystemState::Quarantined),
                Some(SystemState::Uncharted),
            ] {
                let (label, hover) = match s {
                    None => ("(none)", "schema: control.state = unset".to_string()),
                    Some(v) => (system_state_label(v), format!("schema: {}", v.as_slug())),
                };
                if ui.button(label).on_hover_text(hover).clicked() {
                    apply_bulk_control_state(state, s);
                }
            }
        });

        ui.separator();
        ui.label("Reseed worlds for all selected").on_hover_text(
            "Drops each selected system's worlds and re-rolls them. Pinned systems are skipped.",
        );
        if ui
            .button("🔄 Reseed worlds")
            .on_hover_text("Re-roll worlds for every selected system")
            .clicked()
        {
            apply_bulk_reseed(state);
        }
    });
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` so the MAP tab right-click
/// multi-selection menu can dispatch the same bulk-rename helper. Pattern
/// tokens (`{n}`/`{id}`/`{name}`) match the §S4 bulk-ops dialog.
pub(crate) fn apply_bulk_rename(state: &mut BuilderState, pattern: &str) {
    let selection: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
    for (n, id) in selection.into_iter().enumerate() {
        let from = match state.sector.systems.iter().find(|s| s.id == id) {
            Some(s) => s.name.to_string(),
            None => continue,
        };
        let to = pattern
            .replace("{n}", &(n + 1).to_string())
            .replace("{id}", id.as_ref())
            .replace("{name}", &from);
        if to == from {
            continue;
        }
        let cmd = BuilderCommand::RenameSystem {
            id: id.clone(),
            from,
            to,
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Bulk rename failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` so the MAP tab right-click
/// multi-selection menu can dispatch the same primary-faction assignment.
pub(crate) fn apply_bulk_primary_faction(
    state: &mut BuilderState,
    fid: sectorforge::ids::FactionId,
) {
    // §R4: each affected system rides its own EditSystem (was an in-place
    // `primary_factions.push`) so the bulk assignment is undoable. One undo
    // entry per system mutated; systems already carrying `fid` are skipped so
    // they don't emit no-op commands.
    let ids: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
    for id in ids {
        let draft = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .filter(|s| !s.primary_factions.contains(&fid))
            .cloned();
        let Some(mut draft) = draft else {
            continue;
        };
        draft.primary_factions.push(fid.clone());
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu.
pub(crate) fn apply_bulk_clear_factions(state: &mut BuilderState) {
    // §R4: clear each affected system's primary factions through EditSystem
    // (was an in-place `primary_factions.clear()` over `sector_mut()`).
    // Systems already empty are skipped so they don't emit no-op commands.
    let ids: BTreeSet<SystemId> = state.selection.systems.clone();
    let targets: Vec<SystemId> = state
        .sector
        .systems
        .iter()
        .filter(|s| ids.contains(&s.id) && !s.primary_factions.is_empty())
        .map(|s| s.id.clone())
        .collect();
    for id in targets {
        let Some(mut draft) = state.sector.systems.iter().find(|s| s.id == id).cloned() else {
            continue;
        };
        draft.primary_factions.clear();
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu. `value = None` clears the control flag.
pub(crate) fn apply_bulk_control_state(state: &mut BuilderState, value: Option<SystemState>) {
    // §R4: flip each selected system's control state through EditSystem (was an
    // in-place `set_system_control_state` over `sector` that bypassed the bus,
    // matching the sibling `apply_bulk_clear_factions`). Systems already at
    // `value` are skipped so they don't emit no-op commands.
    let ids: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
    for id in ids {
        let Some(mut draft) = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id && s.control.state != value)
            .cloned()
        else {
            continue;
        };
        draft.control.state = value;
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.feedback.modal = Some(ModalKind::Message(format!("Control flip failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu. Pinned systems are skipped (§S3).
pub(crate) fn apply_bulk_reseed(state: &mut BuilderState) {
    let targets: Vec<(SystemId, HexCoord, usize)> = state
        .selection
        .systems
        .iter()
        .filter_map(|id| {
            let sys = state.sector.systems.iter().find(|s| s.id == *id)?;
            if state.pinned_systems.contains(id) {
                return None;
            }
            Some((id.clone(), sys.coord, sys.index))
        })
        .collect();
    for (_id, coord, index) in targets {
        if let Err(e) = state.generate_system_here(coord, index, None) {
            state.feedback.modal = Some(ModalKind::Message(format!("Reseed failed: {e}")));
            return;
        }
    }
}
