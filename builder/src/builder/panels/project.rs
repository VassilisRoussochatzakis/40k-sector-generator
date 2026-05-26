//! PROJECT tab (§N1 / §N2). Composes the Phase A project I/O surfaces
//! (§P1 wizard, §P2 open, §P3 save, §P4 project tree, §P6 preferences) into a
//! single tab. Each sub-panel is its own module under this directory and
//! follows the R10 contract.

use crate::builder::{BuilderState, ModalKind};

use super::{generation, preferences, project_tree, save_project};

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
        if ui.button("Open project…").clicked() {
            state.modal = Some(ModalKind::OpenProject { path: None });
        }
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
            egui::CollapsingHeader::new("Snapshots")
                .default_open(false)
                .show(ui, |ui| show_snapshots(ui, state));
            egui::CollapsingHeader::new("Recent projects")
                .default_open(false)
                .show(ui, |ui| preferences::show(ui, state));
        });
}

fn show_snapshots(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.colored_label(
        egui::Color32::DARK_GRAY,
        "Named save points (U3/U4). Capture before risky edits; revert restores the sector and rewinds the command cursor.",
    );
    let buf_id = egui::Id::new("project_snapshot_name");
    let mut name: String = ui.data_mut(|d| d.get_temp::<String>(buf_id).unwrap_or_default());
    let mut take = false;
    ui.horizontal(|ui| {
        ui.label("name:");
        if ui.text_edit_singleline(&mut name).lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            take = true;
        }
        if ui.button("+ snapshot").clicked() {
            take = true;
        }
    });
    ui.data_mut(|d| d.insert_temp(buf_id, name.clone()));
    if take {
        let label = if name.trim().is_empty() {
            format!("snap-{}", state.snapshots.len() + 1)
        } else {
            name.trim().to_string()
        };
        state.snapshot(label);
        ui.data_mut(|d| d.insert_temp(buf_id, String::new()));
    }
    ui.separator();
    if state.snapshots.is_empty() {
        ui.colored_label(egui::Color32::GRAY, "(no snapshots yet)");
        return;
    }
    let names: Vec<String> = state.snapshots.iter().map(|s| s.name.clone()).collect();
    let mut revert_to: Option<String> = None;
    let mut delete: Option<usize> = None;
    for (i, n) in names.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("• {n}"));
            if ui.small_button("revert").clicked() {
                revert_to = Some(n.clone());
            }
            if ui
                .small_button("×")
                .on_hover_text("Delete snapshot")
                .clicked()
            {
                delete = Some(i);
            }
        });
    }
    if let Some(name) = revert_to {
        if !state.revert_to_snapshot(&name) {
            state.modal = Some(ModalKind::Message(format!("Snapshot '{name}' not found.")));
        }
    }
    if let Some(i) = delete {
        state.snapshots.remove(i);
    }
}
