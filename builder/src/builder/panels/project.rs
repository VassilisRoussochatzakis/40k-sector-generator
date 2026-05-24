//! PROJECT tab (§N1 / §N2). Composes the Phase A project I/O surfaces
//! (§P1 wizard, §P2 open, §P3 save, §P4 project tree, §P6 preferences) into a
//! single tab. Each sub-panel is its own module under this directory and
//! follows the R10 contract.

use crate::builder::{BuilderState, ModalKind};

use super::{generation, open_project, preferences, project_tree, save_project};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Project");
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        if ui.button("New project…").clicked() {
            state.modal = Some(ModalKind::NewProject {
                name: "new-sector".to_string(),
                title: "New Sector".to_string(),
                seed: "seed-1".to_string(),
                width: 8,
                height: 10,
            });
        }
        let _ = open_project::show(ui, state);
        save_project::show(ui, state);
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            egui::CollapsingHeader::new("Tree")
                .default_open(true)
                .show(ui, |ui| project_tree::show(ui, state));
            egui::CollapsingHeader::new("Generation (§6)")
                .default_open(false)
                .show(ui, |ui| generation::show(ui, state, None));
            egui::CollapsingHeader::new("Recent projects")
                .default_open(false)
                .show(ui, |ui| preferences::show(ui, state));
        });
}
