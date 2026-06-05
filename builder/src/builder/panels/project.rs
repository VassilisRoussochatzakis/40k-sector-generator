//! PROJECT tab (§N1 / §N2). Composes the Phase A project I/O surfaces
//! (§P1 wizard, §P2 open, §P3 save, §P4 project tree, §P6 preferences) into a
//! single tab. Each sub-panel is its own module under this directory and
//! follows the R10 contract.

use egui::RichText;

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};
use sectorforge_gui_core::widgets;

use crate::builder::state::ConfirmAction;
use crate::builder::{BuilderState, ModalKind};

use super::{files, generation, preferences, project_tree, save_project, worlds_editor};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Project");
    ui.label(
        RichText::new("Create, open, and save your sector — plus its files and save points.")
            .color(palette::chrome_text_dim()),
    );
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
                    ui_kit::collapsing_section(ui, "proj_tree", "Project tree", true, |ui| {
                        project_tree::show(ui, state)
                    });
                    ui_kit::collapsing_section(ui, "proj_files", "Files", false, |ui| {
                        files::show(ui, state)
                    });
                    ui_kit::collapsing_section(ui, "proj_world_data", "World data", false, |ui| {
                        worlds_editor::show(ui, state)
                    });
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
            // §SPRUCE D3: "New project…" is the primary action on this tab — give it
            // the brass primary button so it reads as *the* thing to click, with the
            // other I/O actions staying as quieter stock buttons.
            if widgets::primary_button(ui, "➕  New project…")
                .on_hover_text("Start a fresh, empty sector from a name, seed, and size")
                .clicked()
            {
                state.modal = Some(ModalKind::NewProject {
                    name: "new-sector".to_string(),
                    title: "New Sector".to_string(),
                    seed: "seed-1".to_string(),
                    width: 8,
                    height: 10,
                });
            }
            if ui
                .button("📂  Open project…")
                .on_hover_text("Open an existing project folder")
                .clicked()
            {
                state.modal = Some(ModalKind::OpenProject { path: None });
            }
            if ui
                .button("🎲  Random sector…")
                .on_hover_text(
                    "Synthesise a fully-complete, fully-randomised sector from just a size \
                     (every overlay enabled)",
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
            // Single "Save all" — flush every dirty editor buffer, then run the
            // full project save.
            let any_dirty = state.dirty || !state.dirty_files.is_empty();
            if ui
                .add_enabled(
                    state.project_path.is_some() && any_dirty,
                    egui::Button::new("💾  Save all"),
                )
                .on_hover_text("Flush every unsaved file, then save the whole project")
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
    ui_kit::section(ui, "Details", |ui| {
        let proj = &state.config.project;
        labeled(
            ui,
            "ID",
            "Unique project identifier (schema: project.id). Used for file and folder names.",
            |ui| {
                // §SPRUCE D4: IDs / seeds / sizes / paths are data — set them in
                // monospace so they align and stop competing with the prose labels.
                ui.label(RichText::new(proj.id.as_str()).monospace());
            },
        );
        labeled(
            ui,
            "Title",
            "Human-readable name shown on the map and exports (schema: project.title).",
            |ui| {
                ui.label(&proj.title);
            },
        );
        if let Some(version) = proj.version.as_deref() {
            labeled(
                ui,
                "Version",
                "Optional version tag for this project (schema: project.version).",
                |ui| {
                    ui.label(RichText::new(version).monospace());
                },
            );
        }
        if let Some(desc) = proj.description.as_deref() {
            if !desc.is_empty() {
                labeled(
                    ui,
                    "Description",
                    "Optional free-text notes about this project (schema: project.description).",
                    |ui| {
                        ui.label(desc);
                    },
                );
            }
        }
        let gen = &state.config.generation;
        labeled(
            ui,
            "Seed",
            "Random seed driving generation (schema: generation.seed). Same seed → same sector.",
            |ui| {
                ui.label(RichText::new(gen.seed.as_str()).monospace());
            },
        );
        labeled(
            ui,
            "Size",
            "Sector grid size in hexes (schema: generation.sector_width × sector_height). Always square.",
            |ui| {
                ui.label(
                    RichText::new(format!("{} × {}", gen.sector_width, gen.sector_height))
                        .monospace(),
                );
            },
        );
        let path = state
            .project_path
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(unsaved)".into());
        labeled(
            ui,
            "Folder",
            "Where this project is saved on disk. '(unsaved)' until you save it the first time.",
            |ui| {
                ui.label(RichText::new(path).monospace());
            },
        );
        if state.dirty || !state.dirty_files.is_empty() {
            ui.label(RichText::new("● unsaved changes").color(palette::warning()));
        }
    });
}

fn show_snapshots(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.label(
        RichText::new(
            "Named save points. Capture one before risky edits; reverting restores the sector to that point.",
        )
        .color(palette::chrome_text_dim()),
    );
    let buf_id = egui::Id::new("project_snapshot_name");
    let mut name: String = ui.data_mut(|d| d.get_temp::<String>(buf_id).unwrap_or_default());
    let mut take = false;
    ui.horizontal(|ui| {
        ui.label("Name:");
        if ui
            .add(egui::TextEdit::singleline(&mut name).hint_text("snapshot name…"))
            .lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            take = true;
        }
        if ui
            .button("📸  Take snapshot")
            .on_hover_text("Save the current sector as a named restore point")
            .clicked()
        {
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
        ui_kit::placeholder(
            ui,
            "No snapshots yet — click Take snapshot to save a restore point.",
        );
        return;
    }
    let names: Vec<String> = state.snapshots.iter().map(|s| s.name.clone()).collect();
    let mut revert_to: Option<String> = None;
    let mut delete: Option<String> = None;
    for n in &names {
        ui.horizontal(|ui| {
            ui.label(format!("• {n}"));
            if ui
                .small_button("↩ revert")
                .on_hover_text("Restore the sector to this save point")
                .clicked()
            {
                revert_to = Some(n.clone());
            }
            if ui
                .small_button("×")
                .on_hover_text("Delete this snapshot")
                .clicked()
            {
                delete = Some(n.clone());
            }
        });
    }
    if let Some(name) = revert_to {
        if !state.revert_to_snapshot(&name) {
            state.modal = Some(ModalKind::Message(format!("Snapshot '{name}' not found.")));
        }
    }
    // §FRIENDLY_PANEL_PASS transform #7: a snapshot is a save point that bypasses
    // the undo bus, so route deletion through a confirm rather than dropping it on
    // a single misclick.
    if let Some(name) = delete {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete snapshot?".into(),
            body: format!("Delete the snapshot “{name}”."),
            action: ConfirmAction::DeleteSnapshot(name),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: remove the snapshot named `name` (the
/// confirmed payload of [`ModalKind::ConfirmDestructive`]). No-op if it is gone.
pub(crate) fn delete_snapshot(state: &mut BuilderState, name: &str) {
    if let Some(idx) = state.snapshots.iter().position(|s| s.name == name) {
        state.snapshots.remove(idx);
    }
}
