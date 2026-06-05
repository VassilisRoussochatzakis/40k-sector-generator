//! WORLD tab — environment and society sections.

use egui::Ui;

use sectorforge::worlds::{Atmosphere, Biosphere, Government, Population, TechLevel, Temperature};
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

use super::combo_enum;

// ── environment ────────────────────────────────────────────────────────────

pub(super) fn show_environment_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_environment", "Environment", false, |ui| {
        // §R4: edit a clone and dispatch one EditWorld for the whole group.
        let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
        let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
        let mut changed = false;
        labeled(
            ui,
            "Atmosphere",
            "Breathability of the air (schema: atmosphere) — e.g. Breathable, Toxic, Airless.",
            |ui| {
                if combo_enum::<Atmosphere>(ui, "w_atm", &mut draft.world.atmosphere) {
                    changed = true;
                }
            },
        );
        labeled(
            ui,
            "Temperature",
            "Surface temperature band (schema: temperature) — Burning to Frozen.",
            |ui| {
                if combo_enum::<Temperature>(ui, "w_temp", &mut draft.world.temperature) {
                    changed = true;
                }
            },
        );
        labeled(
            ui,
            "Biosphere",
            "Native life present on the world (schema: biosphere) — Sterile to Thriving.",
            |ui| {
                if combo_enum::<Biosphere>(ui, "w_bio", &mut draft.world.biosphere) {
                    changed = true;
                }
            },
        );
        if changed {
            if let Err(e) = state.run(BuilderCommand::EditWorld {
                world: wid,
                before: None,
                after: Box::new(draft),
            }) {
                state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
    });
}

// ── society ────────────────────────────────────────────────────────────────

pub(super) fn show_society_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_society", "Society", false, |ui| {
        // §R4: edit a clone and dispatch one EditWorld for the whole group.
        let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
        let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
        let mut changed = false;
        labeled(
            ui,
            "Population",
            "How many people live here (schema: population) — Uninhabited to Extremely Dense.",
            |ui| {
                if combo_enum::<Population>(ui, "w_pop", &mut draft.world.population) {
                    changed = true;
                }
            },
        );
        labeled(
            ui,
            "Tech level",
            "Level of technology available (schema: tech_level) — Primitive to Archaeotech.",
            |ui| {
                if combo_enum::<TechLevel>(ui, "w_tech", &mut draft.world.tech_level) {
                    changed = true;
                }
            },
        );
        labeled(
            ui,
            "Government",
            "Who rules the world and how (schema: government).",
            |ui| {
                if combo_enum::<Government>(ui, "w_gov", &mut draft.world.government) {
                    changed = true;
                }
            },
        );
        if changed {
            if let Err(e) = state.run(BuilderCommand::EditWorld {
                world: wid,
                before: None,
                after: Box::new(draft),
            }) {
                state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
    });
}
