//! Sector-level settings panel: id, title, seed, width/height, manifest hints.

use egui::Ui;

use super::state::EditorState;
use super::ui_helpers::{dim, label, section, text_field};

pub fn show_settings(ui: &mut Ui, state: &mut EditorState) {
    let Some(sector) = state.sector.as_mut() else {
        dim(ui, "no sector loaded");
        return;
    };
    let mut dirty = false;

    section(ui, "SECTOR");
    ui.horizontal(|ui| {
        label(ui, "ID");
        if text_field(ui, &mut sector.id, "sector-id").changed() {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        label(ui, "TITLE");
        if text_field(ui, &mut sector.title, "Title").changed() {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        label(ui, "SEED");
        if text_field(ui, &mut sector.seed, "seed").changed() {
            sector.manifest.seed = sector.seed.clone();
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        label(ui, "WIDTH");
        if ui
            .add(egui::DragValue::new(&mut sector.width).range(1..=64))
            .changed()
        {
            dirty = true;
        }
        label(ui, "HEIGHT");
        if ui
            .add(egui::DragValue::new(&mut sector.height).range(1..=64))
            .changed()
        {
            dirty = true;
        }
    });

    ui.add_space(8.0);
    section(ui, "VIEW");
    ui.horizontal(|ui| {
        label(ui, "ROUTE VIEW");
        if ui
            .selectable_label(
                state.route_view_mode == crate::sector_model::RouteViewMode::TopLevel,
                "TOP-LEVEL",
            )
            .clicked()
        {
            state.route_view_mode = crate::sector_model::RouteViewMode::TopLevel;
        }
        if ui
            .selectable_label(
                state.route_view_mode == crate::sector_model::RouteViewMode::Detailed,
                "DETAILED",
            )
            .clicked()
        {
            state.route_view_mode = crate::sector_model::RouteViewMode::Detailed;
        }
    });

    ui.add_space(8.0);
    section(ui, "STATS (auto-updated on save)");
    dim(ui, &format!("systems: {}", sector.systems.len()));
    dim(
        ui,
        &format!(
            "worlds: {}",
            sector.systems.iter().map(|s| s.worlds.len()).sum::<usize>()
        ),
    );
    dim(ui, &format!("routes: {}", sector.routes.len()));
    dim(ui, &format!("factions: {}", sector.factions.len()));
    if let Some(path) = state.loaded_from.as_deref() {
        ui.add_space(6.0);
        dim(ui, &format!("loaded from: {path}"));
    }

    if dirty {
        state.mark_dirty();
    }
}
