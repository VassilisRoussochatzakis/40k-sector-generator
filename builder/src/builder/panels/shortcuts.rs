//! Global keyboard shortcuts for the builder (§U2 + §LINK3).
//!
//! Call [`handle`] once per frame from the top-level builder loop. It consumes
//! the matching key combos so other widgets do not see them:
//!
//! * `Ctrl-Z` / `Cmd-Z` → [`BuilderState::undo`]
//! * `Ctrl-Y` / `Cmd-Y` → [`BuilderState::redo`]
//! * `Ctrl-Shift-Z` / `Cmd-Shift-Z` → [`BuilderState::redo`] (mac-style alias)
//! * `Alt-←` → [`BuilderState::nav_back`] (§LINK3)
//! * `Alt-→` → [`BuilderState::nav_forward`] (§LINK3)
//! * `Ctrl-K` / `Cmd-K` → toggle the §41 (N5) command palette
//!   ([`crate::builder::ModalKind::CommandPalette`])
//!
//! Errors from `undo` / `redo` are surfaced via [`crate::builder::ModalKind::Message`]
//! so the user sees the failure instead of it being silently dropped.

use crate::builder::{BuilderState, ModalKind};

pub fn handle(ctx: &egui::Context, state: &mut BuilderState) {
    let (undo, redo, nav_back, nav_forward, palette) = ctx.input_mut(|i| {
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
        let nav_back = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::ALT,
            egui::Key::ArrowLeft,
        ));
        let nav_forward = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::ALT,
            egui::Key::ArrowRight,
        ));
        let palette = i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::K,
        ));
        (undo, redo_y || redo_shift, nav_back, nav_forward, palette)
    });
    // §41 (N5): Ctrl-K toggles the command palette. Opening only when no other
    // modal is in the way; re-pressing while it is open closes it. Both writes
    // are transient UI state (the palette never lands in `sector.json`), so they
    // bypass the command bus per the §R4 carve-out.
    if palette {
        match &state.feedback.modal {
            Some(ModalKind::CommandPalette { .. }) => state.feedback.modal = None,
            None => {
                state.feedback.modal = Some(ModalKind::CommandPalette {
                    query: String::new(),
                    selected: 0,
                });
            }
            // Some other modal owns the screen — leave it be.
            Some(_) => {}
        }
    }
    if undo {
        if let Err(e) = state.undo() {
            state.feedback.modal = Some(ModalKind::Message(format!("undo failed: {e}")));
        }
    }
    if redo {
        if let Err(e) = state.redo() {
            state.feedback.modal = Some(ModalKind::Message(format!("redo failed: {e}")));
        }
    }
    if nav_back {
        state.nav_back();
    }
    if nav_forward {
        state.nav_forward();
    }
}
