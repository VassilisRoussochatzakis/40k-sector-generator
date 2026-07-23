//! "New from preset" gallery (§9 NEW.md). A modal-ish window that lists every
//! preset under `presets/` and lets the user scaffold a new project directory
//! from one. Selecting a preset triggers the same `presets::scaffold` call as
//! the CLI `new` command. The GUI does NOT auto-load the new project — it
//! prints the next-step CLI invocation in a status line.

use camino::{Utf8Path, Utf8PathBuf};
use egui::{RichText, ScrollArea, Ui};

use sectorforge::presets::{self, PresetEntry};

use super::palette;

const PRESETS_DIR: &str = "presets";

#[derive(Default)]
pub(crate) struct PresetGalleryState {
    pub open: bool,
    /// `None` means "list not loaded yet"; `Some(Err)` means load failed.
    cached: Option<Result<Vec<PresetEntry>, String>>,
    /// Path the user is typing for the destination directory.
    pub dest_text: String,
    pub seed_text: String,
    pub status: String,
    pub open_immediately: bool,
    pub pending_open: Option<Utf8PathBuf>,
}

impl PresetGalleryState {
    pub(crate) fn ensure_loaded(&mut self) {
        if self.cached.is_some() {
            return;
        }
        self.cached = Some(
            presets::list(Utf8Path::new(PRESETS_DIR))
                .map_err(|e| format!("failed to list presets: {e}")),
        );
    }

    pub(crate) fn invalidate(&mut self) {
        self.cached = None;
    }
}

/// Render the gallery body. Caller owns the surrounding window/panel.
pub(crate) fn show(ui: &mut Ui, state: &mut PresetGalleryState) {
    state.ensure_loaded();

    ui.checkbox(
        &mut state.open_immediately,
        "Open immediately after creation",
    );
    ui.add_space(4.0);

    ui.label(
        RichText::new(format!("PRESETS DIRECTORY: {PRESETS_DIR}"))
            .color(palette::chrome_text_dim()),
    );
    if ui.button(RichText::new("RELOAD")).clicked() {
        state.invalidate();
        state.ensure_loaded();
    }
    ui.add_space(8.0);

    let entries = match state.cached.clone() {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            ui.label(RichText::new(format!("load failed: {e}")).color(palette::danger()));
            return;
        }
        None => return,
    };

    if entries.is_empty() {
        ui.label(RichText::new("no presets found").color(palette::chrome_text_dim()));
        return;
    }

    ui.label(RichText::new("DESTINATION DIRECTORY").color(palette::chrome_text_dim()));
    ui.text_edit_singleline(&mut state.dest_text);
    ui.add_space(4.0);
    ui.label(RichText::new("SEED OVERRIDE (optional)").color(palette::chrome_text_dim()));
    ui.text_edit_singleline(&mut state.seed_text);
    ui.add_space(8.0);

    ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for entry in &entries {
            ui.group(|ui| {
                ui.label(
                    RichText::new(&entry.title)
                        .color(palette::chrome_text())
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("id: {}", entry.id)).color(palette::chrome_text_dim()),
                );
                ui.label(RichText::new(&entry.description).color(palette::chrome_text_dim()));
                if ui
                    .button(RichText::new("CREATE PROJECT FROM THIS PRESET"))
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

                        match presets::scaffold(
                            Utf8Path::new(PRESETS_DIR),
                            &entry.id,
                            &dest,
                            seed_ref,
                        ) {
                            Ok(_) => {
                                state.status = format!("OK — scaffolded '{}' at {dest}", entry.id);
                                if state.open_immediately {
                                    state.pending_open = Some(dest);
                                }
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
            palette::success()
        } else {
            palette::danger()
        };
        ui.label(RichText::new(&state.status).color(color));
        ui.label(
            RichText::new(
                "Next: launch the GUI with `--project <dest>` or run \
                 `cargo run --bin sectorforge -- generate --project <dest>`.",
            )
            .color(palette::chrome_text_dim()),
        );
    }
}
