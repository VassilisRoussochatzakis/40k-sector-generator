//! PROJECT tab (§N1 / §N2). Composes the Phase A project I/O surfaces
//! (§P1 wizard, §P2 open, §P3 save, §P4 project tree, §P6 preferences) into a
//! single tab. Each sub-panel is its own module under this directory and
//! follows the R10 contract.

use sectorforge_gui_core::ui_kit;

use crate::builder::{BuilderState, ModalKind};

use super::{files, generation, preferences, project_tree, save_project, worlds_editor};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Project");
    ui.add_space(4.0);

    // §COLUMNS — RC-2: project actions + read-only metadata on the left, the
    // project Tree + recent + the §PF2 Files / §PF3 World-data editors on the
    // right. Hand-assigned (not round-robin) so the action buttons + metadata
    // stay grouped in column 0 and the heavier editors flow into column 1; when
    // the window is narrow the two columns collapse to one and everything stacks.
    ui_kit::columns_responsive(ui, 2, 360.0, |cols| {
        let n = cols.len();
        {
            // Left column: actions + metadata.
            let left = &mut cols[0];
            show_actions(left, state);
            left.add_space(6.0);
            show_metadata(left, state);
            left.add_space(6.0);
            ui_kit::collapsing_section(left, "proj_snapshots", "Snapshots", false, |ui| {
                show_snapshots(ui, state)
            });
            left.add_space(4.0);
            ui_kit::collapsing_section(left, "proj_recent", "Recent projects", false, |ui| {
                preferences::show(ui, state)
            });
        }
        {
            // Right column — or the same single column when collapsed to one:
            // the project tree + file / world-data editors + generation.
            let right = &mut cols[if n > 1 { 1 } else { 0 }];
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(right, |ui| {
                    ui_kit::collapsing_section(ui, "proj_tree", "Tree", true, |ui| {
                        project_tree::show(ui, state)
                    });
                    ui_kit::collapsing_section(ui, "proj_files", "Files (§PF2)", false, |ui| {
                        files::show(ui, state)
                    });
                    ui_kit::collapsing_section(
                        ui,
                        "proj_world_data",
                        "World data (§PF3)",
                        false,
                        |ui| worlds_editor::show(ui, state),
                    );
                    ui_kit::collapsing_section(ui, "proj_generation", "Generation", false, |ui| {
                        generation::show(ui, state, None)
                    });
                });
        }
    });
}

/// §COLUMNS — project I/O action buttons (New / Open / Random / Save / Save all).
/// Every button keeps its existing modal / command / IO mechanism intact.
fn show_actions(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Actions", |ui| {
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
            if ui
                .button("Random sector…")
                .on_hover_text(
                    "RANDOM.md — synthesise a fully-complete, fully-randomised sector \
                     from just a size (every overlay enabled)",
                )
                .clicked()
            {
                state.modal = Some(ModalKind::GenerateRandom {
                    size: "medium".to_string(),
                    custom_w: 10,
                    custom_h: 12,
                    seed: String::new(),
                    baseline: "_full".to_string(),
                });
            }
            save_project::show(ui, state);
            // §PF5: single "Save all" — flush every dirty TOML editor buffer,
            // then run the full project save.
            let any_dirty = state.dirty || !state.dirty_files.is_empty();
            if ui
                .add_enabled(
                    state.project_path.is_some() && any_dirty,
                    egui::Button::new("Save all"),
                )
                .on_hover_text("§PF5 — flush every dirty file + save the whole project")
                .clicked()
            {
                if let Err(e) = files::save_all(state) {
                    state.modal = Some(ModalKind::Message(format!("Save all failed: {e}")));
                }
            }
        });
    });
}

/// §COLUMNS — read-only project metadata card. Mirrors what the tree / wizard
/// already own; shown here so the left rail is not just a button strip. No model
/// writes happen here (editing stays in the wizard + Generation section).
fn show_metadata(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Metadata", |ui| {
        let proj = &state.config.project;
        ui_kit::kv(ui, "id", &proj.id);
        ui_kit::kv(ui, "title", &proj.title);
        if let Some(version) = proj.version.as_deref() {
            ui_kit::kv(ui, "version", version);
        }
        if let Some(desc) = proj.description.as_deref() {
            if !desc.is_empty() {
                ui_kit::kv(ui, "description", desc);
            }
        }
        let gen = &state.config.generation;
        ui_kit::kv(ui, "seed", &gen.seed);
        ui_kit::kv(
            ui,
            "size",
            &format!("{} × {}", gen.sector_width, gen.sector_height),
        );
        let path = state
            .project_path
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(unsaved)".into());
        ui_kit::kv(ui, "path", &path);
        if state.dirty || !state.dirty_files.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(240, 200, 90), "● unsaved changes");
        }
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
