//! §P6: Preferences panel — exposes the recent-projects MRU as a click-to-open
//! list. Reads `~/.config/sectorforge/preferences.toml` each frame (cheap; the
//! file is tiny). Selecting an entry kicks off
//! [`crate::builder::project_io::open_project`] and replaces the host state on
//! success.

use camino::Utf8PathBuf;
use egui::{Color32, RichText, ScrollArea};

use crate::builder::preferences::Preferences;
use crate::builder::project_io::open_project;
use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Preferences");
    ui.label(
        Preferences::default_path()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(no $HOME — preferences disabled)".into()),
    );
    ui.separator();
    ui.label(RichText::new("Recent projects").strong());

    let mut prefs = Preferences::load();
    let mut remove_idx: Option<usize> = None;
    let mut open_path: Option<Utf8PathBuf> = None;
    ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if prefs.recent_projects.is_empty() {
                ui.colored_label(Color32::GRAY, "(no recent projects)");
            }
            for (idx, path) in prefs.recent_projects.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button(path.as_str()).clicked() {
                        open_path = Some(path.clone());
                    }
                    if ui.small_button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                });
            }
        });
    if let Some(i) = remove_idx {
        prefs.recent_projects.remove(i);
        let _ = prefs.save();
    }
    if let Some(path) = open_path {
        match open_project(&path) {
            Ok(new_state) => *state = new_state,
            Err(e) => {
                state.feedback.modal = Some(ModalKind::Message(format!("Open failed: {e}")));
            }
        }
    }
}
