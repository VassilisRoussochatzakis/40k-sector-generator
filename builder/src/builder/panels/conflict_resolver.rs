//! §P5: dialog raised when the file watcher detects an external change to a
//! buffer the user has been editing. Two outcomes:
//!
//!   * "Reload from disk" — re-reads the affected catalog and clears its
//!     per-file dirty flag.
//!   * "Keep in-memory edits" — leaves the buffer alone so the next save
//!     will overwrite the on-disk file.
//!
//! The modal lives in [`crate::builder::ModalKind::ConflictResolver`].

use std::fs;
use std::path::Path;

use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    let Some(ModalKind::ConflictResolver { rel_path }) = state.modal.clone() else {
        return false;
    };
    ui.heading("External change detected");
    ui.label(format!(
        "The file `{rel_path}` changed on disk while you had unsaved edits."
    ));
    ui.add_space(4.0);

    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("Reload from disk").clicked() {
            if let Some(root) = state.project_path.clone() {
                let abs = root.join(&rel_path);
                if let Ok(text) = fs::read_to_string(Path::new(abs.as_str())) {
                    match crate::builder::project_io::reload_catalog(state, &rel_path, &text) {
                        Ok(()) => state.last_catalog_error = None,
                        Err(e) => state.last_catalog_error = Some(e.to_string()),
                    }
                    state.dirty_files.remove(&rel_path);
                }
            }
            close = true;
        }
        if ui.button("Keep in-memory edits").clicked() {
            close = true;
        }
    });
    if close {
        state.modal = None;
    }
    close
}
