//! "New from preset" gallery (§9 NEW.md). A modal-ish window that lists every
//! preset under `presets/` and lets the user scaffold a new project directory
//! from one. Selecting a preset triggers the same `presets::scaffold` call as
//! the CLI `new` command. The GUI does NOT auto-load the new project — it
//! prints the next-step CLI invocation in a status line.

use camino::Utf8PathBuf;
use egui::{Color32, RichText, ScrollArea, Ui};
use thiserror::Error;

use sectorforge::presets::{self, PresetEntry};

use super::palette;

#[derive(Debug, Error, Clone)]
pub enum PresetGalleryError {
    #[error("failed to list presets: {0}")]
    Load(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreationTarget {
    #[default]
    Project,
    Segmentum,
    Sector,
    System,
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
    pub target: CreationTarget,
    pub width: u32,
    pub height: u32,
    pub irregular_dimensions: bool,
    pub add_to_existing: bool,
    pub open_immediately: bool,
    pub pending_open: Option<Utf8PathBuf>,
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

    ui.horizontal(|ui| {
        ui.label(RichText::new("CREATE:").color(palette::chrome_text_dim()));
        ui.selectable_value(&mut state.target, CreationTarget::Project, "PROJECT");
        ui.selectable_value(&mut state.target, CreationTarget::Segmentum, "SEGMENTUM");
        ui.selectable_value(&mut state.target, CreationTarget::Sector, "SECTOR");
        ui.selectable_value(&mut state.target, CreationTarget::System, "SYSTEM");
    });

    ui.checkbox(&mut state.add_to_existing, "Add to existing project");
    ui.checkbox(
        &mut state.open_immediately,
        "Open immediately after creation",
    );
    ui.add_space(4.0);

    if state.target == CreationTarget::Sector {
        ui.group(|ui| {
            ui.label(RichText::new("SECTOR DIMENSIONS").color(palette::chrome_text_dim()));
            ui.checkbox(&mut state.irregular_dimensions, "Irregular dimensions");

            if state.irregular_dimensions {
                ui.colored_label(
                    Color32::from_rgb(235, 180, 50),
                    "⚠ abnormal dimensions can cause problems in segmenta or joining sectors",
                );
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new("WIDTH").color(palette::chrome_text_dim()));
                let w_res = ui.add(egui::DragValue::new(&mut state.width).range(1..=64));
                ui.label(RichText::new("HEIGHT").color(palette::chrome_text_dim()));
                let h_res = ui.add(egui::DragValue::new(&mut state.height).range(1..=64));

                if !state.irregular_dimensions {
                    if state.width == 0 {
                        state.width = 8;
                        state.height = 8;
                    }
                    if w_res.changed() {
                        state.height = state.width;
                    } else if h_res.changed() {
                        state.width = state.height;
                    }
                }
            });
        });
        ui.add_space(8.0);
    }

    let dir = state.resolved_dir();
    ui.label(RichText::new(format!("PRESETS DIRECTORY: {dir}")).color(palette::chrome_text_dim()));
    if ui.button(RichText::new("RELOAD")).clicked() {
        state.invalidate();
        state.ensure_loaded();
    }
    ui.add_space(8.0);

    let entries = match state.cached.clone() {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            ui.label(
                RichText::new(format!("load failed: {e}")).color(Color32::from_rgb(235, 90, 90)),
            );
            return;
        }
        None => return,
    };

    if entries.is_empty() {
        ui.label(RichText::new("no presets found").color(palette::chrome_text_dim()));
        return;
    }

    ui.label(
        RichText::new(if state.add_to_existing {
            "PROJECT DIRECTORY"
        } else {
            "DESTINATION DIRECTORY"
        })
        .color(palette::chrome_text_dim()),
    );
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
                    .button(RichText::new(format!(
                        "CREATE {} FROM THIS PRESET",
                        match state.target {
                            CreationTarget::Project => "PROJECT",
                            CreationTarget::Segmentum => "SEGMENTUM",
                            CreationTarget::Sector => "SECTOR",
                            CreationTarget::System => "SYSTEM",
                        }
                    )))
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

                        // NOTE: For now, segmentum/sector/system creation via preset
                        // might need specific backend logic. Project scaffolding works.
                        // If add_to_existing is true, we might need to modify the
                        // existing project instead of creating a new one.

                        match presets::scaffold(&dir, &entry.id, &dest, seed_ref) {
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
            Color32::from_rgb(120, 220, 130)
        } else {
            Color32::from_rgb(235, 90, 90)
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
