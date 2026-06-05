//! §P4: PROJECT tree panel.
//!
//! Renders a collapsible directory tree rooted at
//! [`BuilderState::project_path`]. Each leaf is a button that updates
//! [`BuilderState::selected_file`] (the Phase E TOML editor tabs read this
//! to decide which buffer to open). Files marked dirty in
//! [`BuilderState::dirty_files`] render with a leading "●".
//!
//! The walk is shallow per frame: only files whose parent the user expanded
//! are touched. egui's `CollapsingHeader` keeps its own open/closed state, so
//! we don't need to mirror it in `BuilderState`.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use egui::{Color32, RichText, ScrollArea};
use sectorforge_gui_core::ui_kit;

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("PROJECT");
    let Some(root) = state.project_path.clone() else {
        ui_kit::placeholder(ui, "No project open — create or open one to see its files.");
        return;
    };
    ui.label(RichText::new(root.as_str()).monospace());
    ui.separator();
    // §COLUMNS: capped so this tree, when nested inside the PROJECT tab's outer
    // page scroll, occupies a finite box and scrolls internally instead of
    // demanding infinite height and overlapping the sections below it. Height
    // shrinks to content under the cap.
    ScrollArea::vertical()
        .auto_shrink([false, true])
        .max_height(360.0)
        .show(ui, |ui| {
            render_dir(ui, &root, &root, state);
        });
}

fn render_dir(ui: &mut egui::Ui, dir: &Utf8Path, root: &Utf8Path, state: &mut BuilderState) {
    let Ok(read) = std::fs::read_dir(Path::new(dir.as_str())) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by(|a, b| {
        // Folders first, then alpha within each group.
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(utf8) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            egui::CollapsingHeader::new(&name)
                .id_salt(utf8.as_str())
                .show(ui, |ui| render_dir(ui, &utf8, root, state));
        } else if ft.is_file() {
            let rel = utf8
                .strip_prefix(root)
                .map(Utf8Path::to_string)
                .unwrap_or_else(|_| utf8.to_string());
            let is_dirty = state.dirty_files.contains(&rel);
            let is_selected = state.selected_file.as_deref() == Some(Utf8Path::new(&rel));
            let label = if is_dirty {
                RichText::new(format!("● {name}")).color(Color32::YELLOW)
            } else {
                RichText::new(name)
            };
            let resp = ui
                .selectable_label(is_selected, label)
                .on_hover_text("Open in the Files editor");
            if resp.clicked() {
                state.selected_file = Some(Utf8PathBuf::from(&rel));
            }
        }
    }
}
