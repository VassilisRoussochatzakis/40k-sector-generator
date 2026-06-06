//! New-project wizard (§P1).
//!
//! Renders a small modal for the wizard parameters captured in
//! [`crate::builder::ModalKind::NewProject`]. On confirm, it scaffolds a fresh
//! project directory on disk and replaces the host [`BuilderState`] with the
//! freshly loaded one. The wizard purposefully does NOT mutate the sector via
//! the command bus — a new project is a session boundary, not a single
//! undo-able command.

use camino::Utf8PathBuf;
use egui::RichText;

use sectorforge_gui_core::ui_kit::labeled;

use crate::builder::project_io::{new_project, NewProjectOptions};
use crate::builder::{BuilderState, ModalKind};

/// Render the wizard. Returns `true` when the modal consumed the close click
/// (e.g. user pressed Cancel) so the caller can drop it from
/// `BuilderState::feedback.modal`.
pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    let ModalKind::NewProject {
        name,
        title,
        seed,
        width,
        height,
    } = state.feedback.modal.clone().unwrap_or_else(default_modal())
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
        .checkbox(&mut tutorial, "Start from the tutorial example")
        .on_hover_text(
            "Fills every field below with the guided walkthrough's starter values \
             so you can follow along step by step.",
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
    labeled(
        ui,
        "Project id",
        "Short folder-safe name for the project. Lowercase, no spaces — used for the project folder and files.",
        |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut name)
                    .hint_text("e.g. my-sector")
                    .desired_width(200.0),
            );
        },
    );
    labeled(
        ui,
        "Title",
        "Friendly display name for the sector, shown in headings and exports.",
        |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut title)
                    .hint_text("e.g. My Sector")
                    .desired_width(200.0),
            );
        },
    );
    labeled(
        ui,
        "Seed",
        "Any text — it makes generation repeatable. The same seed and settings always produce the same sector.",
        |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut seed)
                    .hint_text("any text")
                    .desired_width(200.0),
            );
        },
    );
    // Sectors must be square: editing either dimension mirrors it into the
    // other so width and height stay locked equal.
    labeled(
        ui,
        "Width",
        "Sector size in grid cells. Sectors are always square, so this stays locked equal to Height.",
        |ui| {
            if ui
                .add(egui::DragValue::new(&mut width).range(1..=64))
                .changed()
            {
                height = width;
            }
        },
    );
    labeled(
        ui,
        "Height",
        "Sector size in grid cells. Sectors are always square, so this stays locked equal to Width.",
        |ui| {
            if ui
                .add(egui::DragValue::new(&mut height).range(1..=64))
                .changed()
            {
                width = height;
            }
        },
    );
    ui.label(
        RichText::new("🔒 square — width & height locked equal")
            .small()
            .weak(),
    )
    .on_hover_text("Sectors must be square, so changing one side updates the other automatically.");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .button("📁  Choose folder & create…")
            .on_hover_text("Pick where to save the project, then scaffold and open it")
            .clicked()
        {
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
                            state.feedback.modal =
                                Some(ModalKind::Message(format!("New project failed: {e}")));
                            return;
                        }
                    }
                }
            }
        }
        if ui
            .button("Cancel")
            .on_hover_text("Close without creating a project")
            .clicked()
        {
            close = true;
        }
    });

    if !close {
        state.feedback.modal = Some(ModalKind::NewProject {
            name,
            title,
            seed,
            width,
            height,
        });
    } else if !create {
        state.feedback.modal = None;
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
