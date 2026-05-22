//! "New from preset" gallery (§9 NEW.md). A modal-ish window that lists every
//! preset under `presets/` and lets the user scaffold a new project directory
//! from one. Selecting a preset triggers the same `presets::scaffold` call as
//! the CLI `new` command. The GUI does NOT auto-load the new project — it
//! prints the next-step CLI invocation in a status line.

use camino::Utf8PathBuf;
use egui::{Color32, RichText, ScrollArea, Ui};
use thiserror::Error;

use sectorforge::presets::{self, PresetEntry};

use super::palette::{TEXT, TEXT_DIM};

#[derive(Debug, Error, Clone)]
pub enum PresetGalleryError {
    #[error("failed to list presets: {0}")]
    Load(String),
}

#[derive(Default)]
pub struct PresetGalleryState {
    pub open: bool,
    pub presets_dir: Option<Utf8PathBuf>,
    /// `None` means "list not loaded yet"; `Some(Err)` means load failed.
    cached: Option<Result<Vec<PresetEntry>, PresetGalleryError>>,
    /// Path the user is typing for the destination directory.
    pub dest_text: String,
    pub seed_text: String,
    pub status: String,
}

impl PresetGalleryState {
    pub fn ensure_loaded(&mut self) {
        if self.cached.is_some() {
            return;
        }
        let dir = self.resolved_dir();
        self.cached =
            Some(presets::list(&dir).map_err(|e| PresetGalleryError::Load(e.to_string())));
    }

    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    fn resolved_dir(&self) -> Utf8PathBuf {
        self.presets_dir
            .clone()
            .unwrap_or_else(|| Utf8PathBuf::from("presets"))
    }
}

/// Render the gallery body. Caller owns the surrounding window/panel.
pub fn show(ui: &mut Ui, state: &mut PresetGalleryState) {
    state.ensure_loaded();
    let dir = state.resolved_dir();
    ui.label(
        RichText::new(format!("PRESETS DIRECTORY: {dir}"))
            .color(TEXT_DIM)
            .monospace(),
    );
    if ui.button(RichText::new("RELOAD").monospace()).clicked() {
        state.invalidate();
        state.ensure_loaded();
    }
    ui.add_space(8.0);

    let entries = match state.cached.clone() {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            ui.label(
                RichText::new(format!("load failed: {e}"))
                    .color(Color32::from_rgb(235, 90, 90))
                    .monospace(),
            );
            return;
        }
        None => return,
    };

    if entries.is_empty() {
        ui.label(
            RichText::new("no presets found")
                .color(TEXT_DIM)
                .monospace(),
        );
        return;
    }

    ui.label(
        RichText::new("DESTINATION DIRECTORY")
            .color(TEXT_DIM)
            .monospace(),
    );
    ui.text_edit_singleline(&mut state.dest_text);
    ui.add_space(4.0);
    ui.label(
        RichText::new("SEED OVERRIDE (optional)")
            .color(TEXT_DIM)
            .monospace(),
    );
    ui.text_edit_singleline(&mut state.seed_text);
    ui.add_space(8.0);

    ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for entry in &entries {
            ui.group(|ui| {
                ui.label(RichText::new(&entry.title).color(TEXT).monospace().strong());
                ui.label(
                    RichText::new(format!("id: {}", entry.id))
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.label(
                    RichText::new(&entry.description)
                        .color(TEXT_DIM)
                        .monospace(),
                );
                if ui
                    .button(RichText::new("CREATE FROM THIS PRESET").monospace())
                    .clicked()
                {
                    let dest_str = state.dest_text.trim();
                    if dest_str.is_empty() {
                        state.status = "Set a destination directory first.".into();
                    } else {
                        let dest = Utf8PathBuf::from(dest_str);
                        let seed = if state.seed_text.trim().is_empty() {
                            None
                        } else {
                            Some(state.seed_text.trim().to_string())
                        };
                        let seed_ref = seed.as_deref();
                        match presets::scaffold(&dir, &entry.id, &dest, seed_ref) {
                            Ok(_) => {
                                state.status = format!("OK — scaffolded '{}' at {dest}", entry.id);
                            }
                            Err(e) => {
                                state.status = format!("FAILED: {e}");
                            }
                        }
                    }
                }
            });
            ui.add_space(6.0);
        }
    });

    if !state.status.is_empty() {
        ui.add_space(8.0);
        let color = if state.status.starts_with("OK") {
            Color32::from_rgb(120, 220, 130)
        } else {
            Color32::from_rgb(235, 90, 90)
        };
        ui.label(RichText::new(&state.status).color(color).monospace());
        ui.label(
            RichText::new(
                "Next: launch the GUI with `--project <dest>` or run \
                 `cargo run --bin sectorforge -- generate --project <dest>`.",
            )
            .color(TEXT_DIM)
            .monospace(),
        );
    }
}
