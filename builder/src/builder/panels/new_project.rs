//! New-project wizard (§P1).
//!
//! Renders a small modal for the wizard parameters captured in
//! [`crate::builder::ModalKind::NewProject`]. On confirm, it scaffolds a fresh
//! project directory on disk and replaces the host [`BuilderState`] with the
//! freshly loaded one. The wizard purposefully does NOT mutate the sector via
//! the command bus — a new project is a session boundary, not a single
//! undo-able command.

use camino::Utf8PathBuf;

use crate::builder::project_io::{new_project, NewProjectOptions};
use crate::builder::{BuilderState, ModalKind};

/// Render the wizard. Returns `true` when the modal consumed the close click
/// (e.g. user pressed Cancel) so the caller can drop it from
/// `BuilderState::modal`.
pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    let ModalKind::NewProject {
        name,
        title,
        seed,
        width,
        height,
    } = state.modal.clone().unwrap_or_else(default_modal())
    else {
        return false;
    };

    let mut name = name;
    let mut title = title;
    let mut seed = seed;
    let mut width = width;
    let mut height = height;
    let mut close = false;
    let mut create = false;

    ui.heading("New project");
    ui.add_space(4.0);
    let tutorial_match = name == "tutorial-sector"
        && title == "Tutorial Sector"
        && seed == "walkthrough-1"
        && width == 8
        && height == 8;
    let mut tutorial = tutorial_match;
    if ui
        .checkbox(
            &mut tutorial,
            "Tutorial (fill BUILDER.md walkthrough values)",
        )
        .changed()
        && tutorial
    {
        name = "tutorial-sector".to_string();
        title = "Tutorial Sector".to_string();
        seed = "walkthrough-1".to_string();
        width = 8;
        height = 8;
    }
    ui.add_space(4.0);
    egui::Grid::new("new_project_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Project id");
            ui.text_edit_singleline(&mut name);
            ui.end_row();
            ui.label("Title");
            ui.text_edit_singleline(&mut title);
            ui.end_row();
            ui.label("Seed");
            ui.text_edit_singleline(&mut seed);
            ui.end_row();
            // Sectors must be square: editing either dimension mirrors it into
            // the other so width and height stay locked equal.
            ui.label("Width");
            if ui
                .add(egui::DragValue::new(&mut width).range(1..=64))
                .changed()
            {
                height = width;
            }
            ui.end_row();
            ui.label("Height");
            if ui
                .add(egui::DragValue::new(&mut height).range(1..=64))
                .changed()
            {
                width = height;
            }
            ui.end_row();
            ui.label("");
            ui.label(
                egui::RichText::new("🔒 square — width & height locked equal")
                    .small()
                    .weak(),
            );
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Choose folder & create…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("New project folder")
                .pick_folder()
            {
                if let Ok(dest) = Utf8PathBuf::from_path_buf(folder) {
                    let dest = dest.join(&name);
                    let opts = NewProjectOptions {
                        dest: dest.clone(),
                        id: name.clone(),
                        title: title.clone(),
                        seed: seed.clone(),
                        width,
                        height,
                        preset: None,
                    };
                    match new_project(opts) {
                        Ok(new_state) => {
                            *state = new_state;
                            create = true;
                            close = true;
                        }
                        Err(e) => {
                            state.modal =
                                Some(ModalKind::Message(format!("New project failed: {e}")));
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

    if !close {
        state.modal = Some(ModalKind::NewProject {
            name,
            title,
            seed,
            width,
            height,
        });
    } else if !create {
        state.modal = None;
    }
    close
}

fn default_modal() -> impl FnOnce() -> ModalKind {
    || ModalKind::NewProject {
        name: "new-sector".to_string(),
        title: "New Sector".to_string(),
        seed: "seed-1".to_string(),
        width: 8,
        height: 8,
    }
}
