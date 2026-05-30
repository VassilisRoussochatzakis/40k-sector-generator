//! Random-sector wizard (RANDOM.md §7.3).
//!
//! Renders the modal for [`crate::builder::ModalKind::GenerateRandom`]. On
//! confirm it synthesises a fully-randomised project under a chosen folder via
//! [`sectorforge::random_sector::generate_random_sector`], persists the
//! generated sector to `out/sector.json`, then replaces the host
//! [`BuilderState`] with the freshly opened project — so the sector arrives
//! fully wired (systems, worlds, factions, routes, regions, economy, history)
//! with `project_path` set and every catalog loaded. Like the new-project
//! wizard this is a session boundary, not a command-bus mutation (§R4
//! carve-out): it legitimately replaces the whole document and clears undo.

use camino::Utf8PathBuf;
use sectorforge::config::OutputFormat;
use sectorforge::random_sector::{self, generate_random_sector, SectorSize};

use crate::builder::project_io::open_project;
use crate::builder::{BuilderState, ModalKind};

/// `(id, label)` for the size dropdown. `custom` reveals width/height fields.
const SIZES: &[(&str, &str)] = &[
    ("small", "Small — 6 × 8"),
    ("medium", "Medium — 8 × 10"),
    ("large", "Large — 12 × 14"),
    ("huge", "Huge — 16 × 20"),
    ("custom", "Custom…"),
];

/// Render the wizard. Returns `true` when the modal should be dismissed
/// (created, cancelled, or surfaced an error message).
pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    let ModalKind::GenerateRandom {
        size,
        custom_w,
        custom_h,
        seed,
    } = state.modal.clone().unwrap_or_else(default_modal)
    else {
        return false;
    };

    let mut size = size;
    let mut custom_w = custom_w;
    let mut custom_h = custom_h;
    let mut seed = seed;
    let mut close = false;
    let mut created = false;
    let mut error: Option<String> = None;

    ui.heading("Random sector");
    ui.add_space(2.0);
    ui.label(
        "Pick a size; everything else — systems, worlds, factions, routes, \
         regions, economy, history — is rolled from the seed, and every overlay \
         is enabled.",
    );
    ui.add_space(6.0);

    egui::Grid::new("generate_random_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Size");
            egui::ComboBox::from_id_salt("random_size")
                .selected_text(size_label(&size))
                .show_ui(ui, |ui| {
                    for (id, label) in SIZES {
                        ui.selectable_value(&mut size, (*id).to_string(), *label);
                    }
                });
            ui.end_row();

            if size == "custom" {
                ui.label("Width");
                ui.add(egui::DragValue::new(&mut custom_w).range(1..=64));
                ui.end_row();
                ui.label("Height");
                ui.add(egui::DragValue::new(&mut custom_h).range(1..=64));
                ui.end_row();
            }

            ui.label("Seed");
            ui.text_edit_singleline(&mut seed);
            ui.end_row();
        });
    ui.small("Leave the seed blank to mint a fresh random one (echoed in the project title).");
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        if ui.button("Choose folder & create…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Random sector parent folder")
                .pick_folder()
            {
                if let Ok(parent) = Utf8PathBuf::from_path_buf(folder) {
                    let seed_val = if seed.trim().is_empty() {
                        random_sector::mint_seed()
                    } else {
                        seed.trim().to_string()
                    };
                    let dest = parent.join(format!("random-{}", dir_slug(&seed_val)));
                    let sector_size = resolve_size(&size, custom_w, custom_h);
                    let presets_dir = sectorforge::presets::default_presets_dir();
                    match generate_random_sector(
                        sector_size,
                        Some(seed_val.clone()),
                        &presets_dir,
                        &dest,
                    ) {
                        Ok(report) => {
                            // open_project loads `out/sector.json` when present,
                            // otherwise a *blank* sector — so persist the
                            // generated one first, then reopen for a fully-wired
                            // state (catalogs + watcher + MRU set up identically
                            // to any other open).
                            let mut out_cfg = report.input.config.outputs.clone();
                            out_cfg.formats = vec![OutputFormat::Json];
                            let out_dir = dest.join(&out_cfg.directory);
                            if let Err(e) =
                                sectorforge::export_sector(&report.sector, &out_cfg, &out_dir)
                            {
                                error = Some(format!("Writing sector.json failed: {e}"));
                            } else {
                                match open_project(&dest) {
                                    Ok(new_state) => {
                                        *state = new_state;
                                        created = true;
                                    }
                                    Err(e) => {
                                        error =
                                            Some(format!("Opening generated project failed: {e}"));
                                    }
                                }
                            }
                        }
                        Err(e) => error = Some(format!("Random generation failed: {e}")),
                    }
                }
            }
        }
        if ui.button("Cancel").clicked() {
            close = true;
        }
    });

    if created {
        // `*state` is now the freshly opened project; its `modal` is already
        // `None`. Leave it untouched.
        return true;
    }
    if let Some(msg) = error {
        state.modal = Some(ModalKind::Message(msg));
        return true;
    }
    if close {
        state.modal = None;
        return true;
    }
    // Persist the in-progress form so edits survive the next frame.
    state.modal = Some(ModalKind::GenerateRandom {
        size,
        custom_w,
        custom_h,
        seed,
    });
    false
}

fn resolve_size(size: &str, custom_w: u32, custom_h: u32) -> SectorSize {
    match size {
        "small" => SectorSize::Small,
        "large" => SectorSize::Large,
        "huge" => SectorSize::Huge,
        "custom" => SectorSize::Custom {
            width: custom_w.max(1),
            height: custom_h.max(1),
        },
        _ => SectorSize::Medium,
    }
}

fn size_label(id: &str) -> &'static str {
    SIZES
        .iter()
        .find(|(s, _)| *s == id)
        .map_or("Medium — 8 × 10", |(_, label)| *label)
}

fn dir_slug(seed: &str) -> String {
    let s: String = seed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(16)
        .collect();
    if s.is_empty() {
        "sector".to_string()
    } else {
        s
    }
}

fn default_modal() -> ModalKind {
    ModalKind::GenerateRandom {
        size: "medium".to_string(),
        custom_w: 10,
        custom_h: 12,
        seed: String::new(),
    }
}
