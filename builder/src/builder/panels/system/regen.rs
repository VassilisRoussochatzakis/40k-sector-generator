//! SYSTEM tab — §S5 single-system regenerate.

use egui::{RichText, Ui};

use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::{palette, ui_kit};

use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

// ── S5 regen ────────────────────────────────────────────────────────────────

pub(super) fn show_regen_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_regen", "Generate one system here", false, |ui| {
        let sys = &state.sector.systems[sys_idx];
        let original_coord = sys.coord;
        let sys_id = sys.id.clone();
        let id = sys.id.clone();
        let regen_q_key = egui::Id::new(("sys_regen_coord_q", id.as_str()));
        let regen_r_key = egui::Id::new(("sys_regen_coord_r", id.as_str()));
        let regen_index_key = egui::Id::new(("sys_regen_index", id.as_str()));
        // Persist q/r/index across frames so DragValue edits survive until
        // the user clicks a Regenerate button.
        let mut q = ui
            .data_mut(|d| d.get_temp::<i32>(regen_q_key))
            .unwrap_or(sys.coord.q);
        let mut r = ui
            .data_mut(|d| d.get_temp::<i32>(regen_r_key))
            .unwrap_or(sys.coord.r);
        let mut index = ui
            .data_mut(|d| d.get_temp::<usize>(regen_index_key))
            .unwrap_or(sys.index);
        let seed_src = state.config.generation.seed.clone();
        let seed_key = egui::Id::new(("sys_regen_seed_buf", id.as_str()));
        let mut seed = seed_src.clone();

        egui::Grid::new("sys_regen_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Coordinate")
                    .on_hover_text("Target grid cell to regenerate at, column q / row r (schema: coord).");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut q)
                            .range(0..=state.sector.width as i32 - 1)
                            .prefix("q"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut r)
                            .range(0..=state.sector.height as i32 - 1)
                            .prefix("r"),
                    );
                });
                ui.end_row();
                ui.label("Sequence number")
                    .on_hover_text("System ordering index used while generating (schema: index).");
                ui.add(egui::DragValue::new(&mut index).range(1..=usize::MAX));
                ui.data_mut(|d| {
                    d.insert_temp(regen_q_key, q);
                    d.insert_temp(regen_r_key, r);
                    d.insert_temp(regen_index_key, index);
                });
                ui.end_row();
                ui.label("Seed")
                    .on_hover_text("Random seed for this system. Change it to get a different result; leave it to reproduce the same one (schema: generation.seed).");
                let (buf, _) =
                    crate::builder::panels::persistent_singleline(ui, seed_key, &seed_src);
                seed = buf;
                ui.end_row();
            });

        ui.horizontal(|ui| {
                if ui
                    .button("🔄 Regenerate here")
                    .on_hover_text("Replace this system with a freshly generated one at its current cell")
                    .clicked()
                {
                    run_regen(state, original_coord, index, &seed);
                    ui.data_mut(|d| {
                        d.remove::<i32>(regen_q_key);
                        d.remove::<i32>(regen_r_key);
                        d.remove::<usize>(regen_index_key);
                    });
                }
                if (q, r) != (original_coord.q, original_coord.r)
                    && ui
                        .button("🔄 Regenerate at new cell")
                        .on_hover_text("Generate a fresh system at the entered coordinate instead")
                        .clicked()
                {
                    let new_coord = HexCoord { q, r };
                    let occupant = state
                        .sector
                        .systems
                        .iter()
                        .find(|s| s.coord == new_coord && s.id != sys_id)
                        .map(|s| s.id.clone());
                    if let Some(occupant) = occupant {
                        state.feedback.modal = Some(ModalKind::Message(format!(
                            "Hex ({},{}) is held by {occupant}. Move or delete it before regenerating here.",
                            new_coord.q, new_coord.r
                        )));
                    } else {
                        run_regen(state, new_coord, index, &seed);
                        ui.data_mut(|d| {
                            d.remove::<i32>(regen_q_key);
                            d.remove::<i32>(regen_r_key);
                            d.remove::<usize>(regen_index_key);
                        });
                    }
                }
            });
        ui.label(
            RichText::new(format!(
                "Editing {id}. Regenerating overwrites the current contents. Pinned systems are skipped."
            ))
            .small()
            .color(palette::chrome_text_dim()),
        );
    });
}

fn run_regen(state: &mut BuilderState, coord: HexCoord, index: usize, seed: &str) {
    let seed_override = if seed == state.config.generation.seed {
        None
    } else {
        Some(seed)
    };
    match state.generate_system_here(coord, index, seed_override) {
        Ok(id) => {
            state.focus_system(id);
        }
        Err(e) => {
            state.feedback.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
        }
    }
}
