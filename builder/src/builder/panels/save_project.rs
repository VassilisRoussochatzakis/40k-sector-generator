//! Save-project actions (§P3).
//!
//! Emits two buttons directly into the caller's `ui`:
//!
//!  * "Save" — writes the current project back to `state.project_path`.
//!  * "Save as…" — pops a native folder picker and writes to the chosen path,
//!    updating `state.project_path` on success.
//!
//! §COLUMNS overlap fix: this used to wrap its buttons in its own
//! `ui.horizontal` and append a "clean / ● unsaved" status label. Inside the
//! PROJECT Actions `horizontal_wrapped` strip that nested non-wrapping row
//! became a single unbreakable atom wider than the column, so it bled past the
//! left column's frame and painted over the right column. The buttons now emit
//! straight into the parent so each wraps independently; the dirty status is
//! already shown in the Metadata card, so the redundant label is gone.
//!
//! Errors land in [`crate::builder::ModalKind::Message`] alongside the rest of
//! the builder's transient diagnostics.

use camino::Utf8PathBuf;

use crate::builder::project_io::{save_project, save_project_as};
use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    let save_enabled = state.project_path.is_some();
    if ui
        .add_enabled(save_enabled, egui::Button::new("Save"))
        .clicked()
    {
        if let Err(e) = save_project(state) {
            state.feedback.modal = Some(ModalKind::Message(format!("Save failed: {e}")));
        }
    }
    if ui.button("Save as…").clicked() {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Save project to folder")
            .pick_folder()
        {
            if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                if let Err(e) = save_project_as(state, &path) {
                    state.feedback.modal = Some(ModalKind::Message(format!("Save-as failed: {e}")));
                }
            }
        }
    }
}
