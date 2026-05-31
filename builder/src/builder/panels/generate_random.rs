//! Random-sector wizard (RANDOM.md §7.3 / §7.4).
//!
//! Renders the modal for [`crate::builder::ModalKind::GenerateRandom`]. On
//! confirm it picks a parent folder, then dispatches the synthesise → generate
//! → derive → export pipeline to a background worker via
//! [`crate::builder::random_run::RandomGenState`], so big grids (up to
//! `80 × 80`) don't freeze the UI. While the worker runs, this same modal turns
//! into a progress popup driven by the live [`RandomProgress`] snapshot.
//!
//! The wizard owns the whole job lifecycle (there is no central job pump in
//! `app.rs`): each frame [`show`] drains the worker, and on completion reopens
//! the freshly generated project from disk and replaces the [`BuilderState`] —
//! so the sector arrives fully wired (systems, worlds, factions, routes,
//! regions, economy, history) with `project_path` set and every catalog loaded.
//! Like the new-project wizard this is a session boundary, not a command-bus
//! mutation (§R4 carve-out): it legitimately replaces the whole document and
//! clears undo.
//!
//! [`RandomProgress`]: sectorforge::random_sector::RandomProgress

use camino::Utf8PathBuf;
use sectorforge::random_sector::{self, SectorSize, MAX_CUSTOM_DIM};

use crate::builder::project_io::open_project;
use crate::builder::random_run::RandomJobResult;
use crate::builder::{BuilderState, ModalKind};

/// `(id, label)` for the size dropdown. Every preset is square (N × N) — the
/// square-sector invariant. `custom` reveals the (locked-equal) width/height
/// fields.
const SIZES: &[(&str, &str)] = &[
    ("small", "Small — 8 × 8"),
    ("medium", "Medium — 16 × 16"),
    ("large", "Large — 32 × 32"),
    ("vast", "Vast — 48 × 48"),
    ("massive", "Massive — 64 × 64"),
    ("huge", "Huge — 80 × 80"),
    ("custom", "Custom…"),
];

/// `(id, label)` for the baseline dropdown. `_full` is the balanced
/// "everything on" reference; the rest are the themed gallery presets. The
/// chosen baseline supplies the content/overlay data tree, while the layout is
/// still rolled from the seed (RANDOM.md §6: pick a themed baseline, fully roll
/// the rest).
const BASELINES: &[(&str, &str)] = &[
    ("_full", "Everything — balanced, all features"),
    ("m42-classic", "M42 Classic — balanced Imperium"),
    ("embattled-frontier", "Embattled Frontier — Imperium vs Orks"),
    ("dead-sector", "Dead Sector — ruins & Necrons"),
    ("mercantile-crossroads", "Mercantile Crossroads — trade hub"),
];

/// Render the wizard. Returns `true` when the modal should be dismissed
/// (created, cancelled, or surfaced an error message).
pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    // While a background generation is in flight the modal becomes a progress
    // popup (RANDOM.md §7.4). The host pump reopens the project / surfaces any
    // error on completion; here we only animate progress + offer a cancel.
    if state.random_gen.is_running() {
        // Drain a finished job here: there is no central job pump and the modal
        // window has no close affordance, so the wizard owns the whole
        // lifecycle. On success this reopens the generated project — a session
        // boundary that replaces *state and clears undo (§R4 carve-out).
        if let Some(result) = state.random_gen.pump() {
            return apply_result(state, result);
        }
        return show_progress(ui, state);
    }

    let ModalKind::GenerateRandom {
        size,
        custom_w,
        custom_h,
        seed,
        baseline,
    } = state.modal.clone().unwrap_or_else(default_modal)
    else {
        return false;
    };

    let mut size = size;
    let mut custom_w = custom_w;
    let mut custom_h = custom_h;
    let mut seed = seed;
    let mut baseline = baseline;
    let mut close = false;

    ui.heading("Random sector");
    ui.add_space(2.0);
    ui.label(
        "Pick a size and a themed baseline. The layout — systems, worlds, \
         routes, regions — is rolled from the seed, while the baseline seeds the \
         content (factions, history, personae, sites, prose). Every overlay is \
         enabled.",
    );
    ui.label(
        egui::RichText::new("Sectors are square — every size is N × N.")
            .italics()
            .weak(),
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

            ui.label("Baseline");
            egui::ComboBox::from_id_salt("random_baseline")
                .selected_text(baseline_label(&baseline))
                .show_ui(ui, |ui| {
                    for (id, label) in BASELINES {
                        ui.selectable_value(&mut baseline, (*id).to_string(), *label);
                    }
                });
            ui.end_row();

            if size == "custom" {
                // Sectors must be square: editing either dimension mirrors it
                // into the other so the two fields stay locked equal.
                ui.label("Width");
                if ui
                    .add(egui::DragValue::new(&mut custom_w).range(1..=MAX_CUSTOM_DIM))
                    .changed()
                {
                    custom_h = custom_w;
                }
                ui.end_row();
                ui.label("Height");
                if ui
                    .add(egui::DragValue::new(&mut custom_h).range(1..=MAX_CUSTOM_DIM))
                    .changed()
                {
                    custom_w = custom_h;
                }
                ui.end_row();
                ui.label("");
                ui.label(
                    egui::RichText::new("🔒 square — width & height locked equal")
                        .small()
                        .weak(),
                );
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
                    // Off-thread: the popup animates while the worker runs; the
                    // host reopens `dest` once the job lands (RANDOM.md §7.4).
                    state
                        .random_gen
                        .spawn(ui.ctx(), sector_size, baseline.clone(), seed_val, dest);
                }
            }
        }
        if ui.button("Cancel").clicked() {
            close = true;
        }
    });

    if close {
        state.modal = None;
        return true;
    }
    // Persist the in-progress form so edits survive the next frame. When a spawn
    // just fired, keeping the modal open means the next frame renders the
    // progress popup (the `is_running` guard at the top of `show`).
    state.modal = Some(ModalKind::GenerateRandom {
        size,
        custom_w,
        custom_h,
        seed,
        baseline,
    });
    false
}

/// Consume a finished generation job. Always returns `true` (dismiss): success
/// replaces the whole session with the freshly reopened project (whose `modal`
/// is already `None`, so the window closes); a failure swaps in a message modal.
fn apply_result(state: &mut BuilderState, result: RandomJobResult) -> bool {
    match result {
        RandomJobResult::Done { dest } => match open_project(&dest) {
            Ok(new_state) => *state = new_state,
            Err(e) => {
                state.modal = Some(ModalKind::Message(format!(
                    "Generated the sector but reopening it failed: {e}"
                )));
            }
        },
        RandomJobResult::Failed(message) => {
            state.modal = Some(ModalKind::Message(message));
        }
    }
    true
}

/// Progress popup shown while the background generation worker runs. Reads the
/// live [`sectorforge::random_sector::RandomProgress`] snapshot; returns `true`
/// (dismiss the modal) only when the user cancels — completion is handled by the
/// host pump in `app.rs`.
fn show_progress(ui: &mut egui::Ui, state: &mut BuilderState) -> bool {
    ui.heading("Generating random sector");
    ui.add_space(4.0);
    ui.label(
        "Synthesising systems, worlds, factions, routes, regions, economy, and \
         history. Large grids take a moment.",
    );
    ui.add_space(8.0);

    let fraction = state.random_gen.fraction();
    ui.add(
        egui::ProgressBar::new(fraction)
            .show_percentage()
            .animate(true),
    );
    ui.add_space(4.0);
    match state.random_gen.progress_snapshot() {
        Some(progress) => ui.label(progress.label()),
        None => ui.label("Starting…"),
    };
    ui.add_space(8.0);

    let mut dismiss = false;
    if ui.button("Cancel").clicked() {
        // Detach the worker (it finishes in the background; its result is
        // dropped on the next pump because the revision no longer matches) and
        // close the wizard — `show_modal` only dismisses via `state.modal`.
        state.random_gen.cancel();
        state.modal = None;
        dismiss = true;
    }

    // Keep the frame loop spinning so the bar animates as snapshots arrive.
    ui.ctx().request_repaint();
    dismiss
}

fn resolve_size(size: &str, custom_w: u32, custom_h: u32) -> SectorSize {
    match size {
        "small" => SectorSize::Small,
        "large" => SectorSize::Large,
        "vast" => SectorSize::Vast,
        "massive" => SectorSize::Massive,
        "huge" => SectorSize::Huge,
        "custom" => SectorSize::Custom {
            // Square: the two fields are locked equal in the UI; fold to one
            // side length defensively in case they ever diverge.
            dim: custom_w.max(custom_h).clamp(1, MAX_CUSTOM_DIM),
        },
        _ => SectorSize::Medium,
    }
}

fn size_label(id: &str) -> &'static str {
    SIZES
        .iter()
        .find(|(s, _)| *s == id)
        .map_or("Medium — 16 × 16", |(_, label)| *label)
}

fn baseline_label(id: &str) -> &'static str {
    BASELINES
        .iter()
        .find(|(s, _)| *s == id)
        .map_or("Everything — balanced, all features", |(_, label)| *label)
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
        custom_w: 16,
        custom_h: 16,
        seed: String::new(),
        baseline: "_full".to_string(),
    }
}
