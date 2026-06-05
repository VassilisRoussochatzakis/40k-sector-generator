//! Sector-level settings panel: id, title, seed, width/height, manifest hints.

use egui::Ui;

use super::state::EditorState;
use super::ui_helpers::{dim, label, section, text_field};

pub(crate) fn show_settings(ui: &mut Ui, state: &mut EditorState) {
    let Some(sector) = state.sector.as_mut() else {
        dim(ui, "no sector loaded");
        return;
    };
    let mut dirty = false;

    section(ui, "SECTOR");
    ui.horizontal(|ui| {
        label(ui, "ID");
        let mut id = sector.id.to_string();
        if text_field(ui, &mut id, "sector-id").changed() {
            sector.id = id.into();
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        label(ui, "TITLE");
        let mut title = sector.title.to_string();
        if text_field(ui, &mut title, "Title").changed() {
            sector.title = title.into();
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        label(ui, "SEED");
        let mut seed = sector.seed.to_string();
        if text_field(ui, &mut seed, "seed").changed() {
            sector.seed = seed.clone().into();
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

    if sector.width != sector.height {
        ui.colored_label(
            crate::palette::warning(),
            "⚠ IRREGULAR DIMENSIONS: Joining sectors in a Segmentum usually requires square (H=W) sectors.",
        );
    }

    ui.add_space(8.0);
    section(ui, "VIEW");
    ui.horizontal(|ui| {
        label(ui, "ROUTE VIEW");
        if ui
            .selectable_label(
                state.route_view_mode == sectorforge::sector_model::RouteViewMode::TopLevel,
                "TOP-LEVEL",
            )
            .clicked()
        {
            state.route_view_mode = sectorforge::sector_model::RouteViewMode::TopLevel;
        }
        if ui
            .selectable_label(
                state.route_view_mode == sectorforge::sector_model::RouteViewMode::Detailed,
                "DETAILED",
            )
            .clicked()
        {
            state.route_view_mode = sectorforge::sector_model::RouteViewMode::Detailed;
        }
    });

    ui.add_space(8.0);
    section(ui, "EDITOR PREFERENCES");
    ui.checkbox(&mut state.auto_save, "AUTO-SAVE TO sector.json");
    dim(ui, "Background saving on every change.");
    // VED-2: expose stable_ids_on_rename so it is no longer stuck at its default.
    ui.checkbox(&mut state.stable_ids_on_rename, "Stable IDs on rename")
        .on_hover_text(
            "§49: keep existing system/world IDs stable when renaming (off = compact renumber).",
        );

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
