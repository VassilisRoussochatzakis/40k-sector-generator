//! Global keyboard shortcuts for the builder (§U2).
//!
//! Call [`handle`] once per frame from the top-level builder loop. It consumes
//! the matching key combos so other widgets do not see them:
//!
//! * `Ctrl-Z` / `Cmd-Z` → [`BuilderState::undo`]
//! * `Ctrl-Y` / `Cmd-Y` → [`BuilderState::redo`]
//! * `Ctrl-Shift-Z` / `Cmd-Shift-Z` → [`BuilderState::redo`] (mac-style alias)
//!
//! Errors from `undo` / `redo` are surfaced via [`crate::builder::ModalKind::Message`]
//! so the user sees the failure instead of it being silently dropped.

use crate::builder::{BuilderState, ModalKind};

pub fn handle(ctx: &egui::Context, state: &mut BuilderState) {
    let (undo, redo) = ctx.input_mut(|i| {
        let undo = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Z,
        ));
        let redo_y = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Y,
        ));
        let redo_shift = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        ));
        (undo, redo_y || redo_shift)
    });
    if undo {
        if let Err(e) = state.undo() {
            state.modal = Some(ModalKind::Message(format!("undo failed: {e}")));
        }
    }
    if redo {
        if let Err(e) = state.redo() {
            state.modal = Some(ModalKind::Message(format!("redo failed: {e}")));
        }
    }
}
