//! Open-project picker (§P2).
//!
//! Triggers a native folder picker, then calls
//! [`crate::builder::project_io::open_project`]. Parse errors are routed into
//! [`ModalKind::Message`] so they appear in the standard modal-message slot.

use camino::Utf8PathBuf;

use crate::builder::project_io::open_project;
use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    let mut close = false;
    ui.heading("Open project");
    ui.label("Pick a directory containing sectorforge.toml.");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Choose folder…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Open project directory")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    match open_project(&path) {
                        Ok(new_state) => {
                            *state = new_state;
                            close = true;
                        }
                        Err(e) => {
                            state.modal = Some(ModalKind::Message(format!("Open failed: {e}")));
                            return;
                        }
                    }
                }
            }
        }
        if ui.button("Cancel").clicked() {
            close = true;
        }
    });
    if close && !matches!(state.modal, Some(ModalKind::Message(_))) {
        state.modal = None;
    }
    close
}
