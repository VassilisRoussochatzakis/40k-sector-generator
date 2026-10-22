//! WORLD tab — environment and society sections.

use egui::Ui;

use sectorforge::worlds::{Atmosphere, Biosphere, Government, Population, TechLevel, Temperature};
use sectorforge_gui_core::ui_kit;

use crate::builder::BuilderState;

use super::{combo_enum, edit_world_rows};

// ── environment ────────────────────────────────────────────────────────────

pub(super) fn show_environment_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_environment", "Environment", false, |ui| {
        edit_world_rows(ui, state, sys_idx, w_idx, |ui, w| {
            let mut changed = combo_enum(
                ui,
                "Atmosphere",
                "Breathability of the air (schema: atmosphere) — e.g. Breathable, Toxic, Airless.",
                "w_atm",
                &mut w.atmosphere,
                Atmosphere::VARIANTS,
                Atmosphere::display_name,
            );
            changed |= combo_enum(
                ui,
                "Temperature",
                "Surface temperature band (schema: temperature) — Burning to Frozen.",
                "w_temp",
                &mut w.temperature,
                Temperature::VARIANTS,
                Temperature::display_name,
            );
            changed |= combo_enum(
                ui,
                "Biosphere",
                "Native life present on the world (schema: biosphere) — Sterile to Thriving.",
                "w_bio",
                &mut w.biosphere,
                Biosphere::VARIANTS,
                Biosphere::display_name,
            );
            changed
        });
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
        edit_world_rows(ui, state, sys_idx, w_idx, |ui, w| {
            let mut changed = combo_enum(
                ui,
                "Population",
                "How many people live here (schema: population) — Uninhabited to Extremely Dense.",
                "w_pop",
                &mut w.population,
                Population::VARIANTS,
                Population::display_name,
            );
            changed |= combo_enum(
                ui,
                "Tech level",
                "Level of technology available (schema: tech_level) — Primitive to Archaeotech.",
                "w_tech",
                &mut w.tech_level,
                TechLevel::VARIANTS,
                TechLevel::display_name,
            );
            changed |= combo_enum(
                ui,
                "Government",
                "Who rules the world and how (schema: government).",
                "w_gov",
                &mut w.government,
                Government::VARIANTS,
                Government::display_name,
            );
            changed
        });
    });
}
