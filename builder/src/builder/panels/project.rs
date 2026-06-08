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
                state.feedback.modal = Some(ModalKind::NewProject {
                    name: "new-sector".to_string(),
                    title: "New Sector".to_string(),
                    seed: "seed-1".to_string(),
                    width: 8,
                    height: 8,
                });
            }
            if ui
                .button("📂  Open project…")
                .on_hover_text("Open an existing project folder")
                .clicked()
            {
                state.feedback.modal = Some(ModalKind::OpenProject { path: None });
            }
            if ui
                .button("🎲  Random sector…")
                .on_hover_text(
                    "Synthesise a fully-complete, fully-randomised sector from just a size \
                     (every overlay enabled)",
                )
                .clicked()
            {
                state.feedback.modal = Some(ModalKind::GenerateRandom {
                    size: "medium".to_string(),
                    custom_w: 10,
                    custom_h: 12,
                    seed: String::new(),
                    baseline: "_full".to_string(),
                });
            }
            // ITERATIVE_GENERATION.md Phase P — open the stage-by-stage wizard.
            // Mirrors the random launcher but, instead of a one-shot modal,
            // installs a transient `IterativeGenSession` (never document state)
            // and switches to the ITERATIVE tab. The session adopts the current
            // project's config as its baseline and rolls fresh generation knobs
            // from a freshly-minted seed at the default Medium (square) size.
            if ui
                .button("🧭  Iterative sector…")
                .on_hover_text(
                    "Build a random sector one step at a time — tune and re-roll each stage \
                     (size, placement, regions, contents, factions, routes), then commit",
                )
                .clicked()
            {
                let seed = sectorforge::random_sector::mint_seed();
                let mut session = crate::builder::state::IterativeGenSession::new(
                    state.config.clone(),
                    seed,
                    sectorforge::random_sector::SectorSize::Medium,
                );
                // A wizard launched with no project open has no world pool in the
                // live `data_catalogs` (every field is `None`), so preview, re-roll,
                // and commit would all dead-end on the missing worlds catalog. Seed
                // the session from the bundled `_base` preset — the same catalog set
                // the one-shot random generator scaffolds — so the wizard works
                // standalone. When a project IS open its worlds catalog is `Some`, so
                // leave the live catalogs in place (the override stays `None`).
                if state.data_catalogs.worlds.is_none() {
                    if let Some((catalogs, inputs)) =
                        crate::builder::project_io::load_base_catalogs()
                    {
                        session.config.inputs = inputs;
                        session.catalogs_override = Some(catalogs);
                    }
                }
                state.iterative_gen = Some(session);
                state.set_active_tab(crate::builder::state::BuilderTab::IterativeGen);
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
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("Save all failed: {e}")));
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
    // §U4: a revert rolls the sector back and discards every command logged
    // after the snapshot — it can't itself be undone, so it is gated behind the
    // shared confirm dialog rather than firing on a single click.
    if let Some(name) = revert_to {
        state.feedback.modal = Some(ModalKind::ConfirmDestructive {
            title: "Revert to snapshot?".into(),
            body: format!("Revert to “{name}”. Commands after it will be discarded."),
            action: ConfirmAction::RevertToSnapshot(name),
        });
    }
    // §FRIENDLY_PANEL_PASS transform #7: a snapshot is a save point that bypasses
    // the undo bus, so route deletion through a confirm rather than dropping it on
    // a single misclick.
    if let Some(name) = delete {
        state.feedback.modal = Some(ModalKind::ConfirmDestructive {
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

/// §U4: roll the sector back to the named snapshot (the confirmed payload of
/// [`ModalKind::ConfirmDestructive`]). Surfaces a message if the snapshot has
/// since been deleted out from under the modal.
pub(crate) fn revert_snapshot(state: &mut BuilderState, name: &str) {
    if !state.revert_to_snapshot(name) {
        state.feedback.modal = Some(ModalKind::Message(format!("Snapshot '{name}' not found.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::command::BuilderCommand;
    use sectorforge::sector_model::HexCoord;

    fn add_system(state: &mut BuilderState, q: i32, name: &str) {
        state
            .run(BuilderCommand::AddSystem {
                coord: HexCoord { q, r: 0 },
                name: name.into(),
                result_id: None,
            })
            .unwrap();
    }

    /// §U4: the confirmed revert action rolls the sector back to the snapshot
    /// and rewinds the command cursor, discarding the command logged after it.
    #[test]
    fn revert_snapshot_rolls_sector_back() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 16, 16);
        add_system(&mut state, 0, "alpha");
        state.snapshot("base");
        let cursor_at_snapshot = state.command_cursor;
        add_system(&mut state, 1, "beta");
        assert_eq!(state.sector.systems.len(), 2);

        revert_snapshot(&mut state, "base");

        assert_eq!(
            state.sector.systems.len(),
            1,
            "revert should drop the post-snapshot system"
        );
        assert_eq!(state.command_cursor, cursor_at_snapshot);
        assert!(
            !matches!(state.feedback.modal, Some(ModalKind::Message(_))),
            "a successful revert leaves no error message"
        );
    }

    /// §U4: reverting to a missing snapshot is a no-op that surfaces a message
    /// rather than mutating the sector.
    #[test]
    fn revert_missing_snapshot_surfaces_message() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 16, 16);
        add_system(&mut state, 0, "alpha");
        revert_snapshot(&mut state, "nope");
        assert_eq!(state.sector.systems.len(), 1, "missing snapshot is a no-op");
        assert!(matches!(state.feedback.modal, Some(ModalKind::Message(_))));
    }
}
