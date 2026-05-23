//! Save-project actions (§P3).
//!
//! Renders a tiny widget block with two buttons:
//!
//!  * "Save" — writes the current project back to `state.project_path`.
//!  * "Save as…" — pops a native folder picker and writes to the chosen path,
//!    updating `state.project_path` on success.
//!
//! Errors land in [`crate::builder::ModalKind::Message`] alongside the rest of
//! the builder's transient diagnostics.

use camino::Utf8PathBuf;

use crate::builder::project_io::{save_project, save_project_as};
use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        let save_enabled = state.project_path.is_some();
        if ui
            .add_enabled(save_enabled, egui::Button::new("Save"))
            .clicked()
        {
            if let Err(e) = save_project(state) {
                state.modal = Some(ModalKind::Message(format!("Save failed: {e}")));
            }
        }
        if ui.button("Save as…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Save project to folder")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    if let Err(e) = save_project_as(state, &path) {
                        state.modal = Some(ModalKind::Message(format!("Save-as failed: {e}")));
                    }
                }
            }
        }
        if state.dirty {
            ui.colored_label(egui::Color32::YELLOW, "● unsaved changes");
        } else {
            ui.colored_label(egui::Color32::GRAY, "clean");
        }
    });
}
