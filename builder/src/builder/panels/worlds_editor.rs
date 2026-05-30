//! §PF3: typed `worlds.toml` editor for the builder.
//!
//! Parity with the viewer's `data_editor` (§WD4) but bound to the builder's own
//! [`BuilderState::data_catalogs`] so world-data editing never routes through
//! `sectorforge-viewer`. Each generation row is edited through enum
//! `ComboBox`es over `Enum::VARIANTS` and a `DragValue<f64>` weight; rows can be
//! inserted (above / append) and deleted (§PF3 extensions). A validation
//! surface re-serialises + re-parses the config on demand so a malformed pool
//! is caught before it reaches disk, and **Save worlds.toml** writes the file
//! atomically (§PF6) on its own.

use egui::{Color32, RichText, ScrollArea};

use sectorforge::worlds::{
    Atmosphere, Biosphere, GenerationRow, Government, NotableFeature, Population, StarColour,
    TechLevel, Temperature, WorldType,
};
use sectorforge::worlds_toml::{WorldsConfig, DEFAULT_FILENAME as WORLDS_TOML_FILENAME};

use crate::builder::project_io;
use crate::builder::{BuilderState, ModalKind};

/// Project-relative path of the worlds catalog given the active config's
/// `[inputs] world_data_dir`.
fn worlds_rel(state: &BuilderState) -> String {
    let dir = state.config.inputs.world_data_dir.trim_end_matches('/');
    format!("{dir}/{WORLDS_TOML_FILENAME}")
}

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    if state.project_path.is_none() {
        ui.colored_label(Color32::GRAY, "(open a project to edit world data)");
        return;
    }
    if state.data_catalogs.worlds.is_none() {
        ui.colored_label(
            Color32::GRAY,
            "This project has no worlds catalog. Add a worlds.toml under its \
             data dir, then reopen the project.",
        );
        return;
    }

    let rel = worlds_rel(state);
    let dirty = state.dirty_files.contains(&rel);
    let row_count = state
        .data_catalogs
        .worlds
        .as_ref()
        .map_or(0, |c| c.generation.len());

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{row_count} generation rows"))
                .monospace()
                .color(Color32::GRAY),
        );
        if dirty {
            ui.colored_label(Color32::from_rgb(240, 200, 90), "● unsaved");
        }
    });

    // §PF3 validation surface: round-trip the config (serialise → re-parse) and
    // report the first error, if any.
    let validation = validate(state.data_catalogs.worlds.as_ref().unwrap());
    match &validation {
        Ok(()) => {
            ui.colored_label(Color32::from_rgb(120, 200, 120), "✓ valid worlds.toml");
        }
        Err(msg) => {
            ui.colored_label(
                Color32::from_rgb(230, 120, 120),
                RichText::new(format!("✗ {msg}")).monospace(),
            );
        }
    }

    let mut do_save = false;
    ui.horizontal(|ui| {
        let can_save = dirty && validation.is_ok();
        if ui
            .add_enabled(can_save, egui::Button::new("Save worlds.toml"))
            .on_hover_text("Write the worlds catalog atomically (§PF6)")
            .clicked()
        {
            do_save = true;
        }
    });
    ui.separator();

    let mut any_change = false;
    edit_rows(ui, state.data_catalogs.worlds.as_mut().unwrap(), &mut any_change);

    if any_change {
        state.dirty = true;
        state.dirty_files.insert(rel.clone());
    }
    if do_save {
        if let Err(e) = save(state, &rel) {
            state.modal = Some(ModalKind::Message(format!("Save failed: {e}")));
        }
    }
}

/// Render the generation-row grid with typed dropdowns, weights, and per-row
/// insert / delete controls. Sets `any_change` when the user edits anything.
fn edit_rows(ui: &mut egui::Ui, cfg: &mut WorldsConfig, any_change: &mut bool) {
    let mut delete_row: Option<usize> = None;
    let mut insert_above: Option<usize> = None;

    ScrollArea::both()
        .auto_shrink([false, false])
        .max_height(420.0)
        .show(ui, |ui| {
            egui::Grid::new("worlds_toml_grid")
                .num_columns(13)
                .striped(true)
                .min_col_width(86.0)
                .show(ui, |ui| {
                    for h in [
                        "#",
                        "STAR",
                        "WORLD TYPE",
                        "ATMOSPHERE",
                        "TEMPERATURE",
                        "BIOSPHERE",
                        "POPULATION",
                        "TECH",
                        "GOVERNMENT",
                        "FEATURE",
                        "WEIGHT",
                        "",
                        "",
                    ] {
                        ui.label(RichText::new(h).strong().monospace());
                    }
                    ui.end_row();

                    for (idx, row) in cfg.generation.iter_mut().enumerate() {
                        ui.label(RichText::new((idx + 1).to_string()).monospace().color(Color32::GRAY));
                        *any_change |=
                            enum_combo(ui, ("sc", idx), &mut row.star_colour, StarColour::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("wt", idx), &mut row.world_type, WorldType::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("at", idx), &mut row.atmosphere, Atmosphere::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("te", idx), &mut row.temperature, Temperature::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("bi", idx), &mut row.biosphere, Biosphere::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("po", idx), &mut row.population, Population::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("tl", idx), &mut row.tech, TechLevel::VARIANTS, |v| v.display_name());
                        *any_change |=
                            enum_combo(ui, ("gv", idx), &mut row.government, Government::VARIANTS, |v| v.display_name());
                        *any_change |= enum_combo(ui, ("nf", idx), &mut row.notable_feature, NotableFeature::VARIANTS, |v| {
                            v.display_name()
                        });

                        let mut weight = row.weight.unwrap_or(0.0);
                        if ui
                            .add(egui::DragValue::new(&mut weight).speed(0.1).range(0.0..=1e6))
                            .changed()
                        {
                            row.weight = (weight > 0.0).then_some(weight);
                            *any_change = true;
                        }

                        if ui.small_button("＋").on_hover_text("insert row above").clicked() {
                            insert_above = Some(idx);
                        }
                        if ui.small_button("✕").on_hover_text("delete row").clicked() {
                            delete_row = Some(idx);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            if ui.button("+ Add row").clicked() {
                cfg.generation.push(GenerationRow::default());
                *any_change = true;
            }
        });

    if let Some(idx) = insert_above {
        cfg.generation.insert(idx, GenerationRow::default());
        *any_change = true;
    }
    if let Some(idx) = delete_row {
        cfg.generation.remove(idx);
        *any_change = true;
    }
}

/// Generic enum dropdown over `Enum::VARIANTS`, binding an `Option<T>` (None =
/// the unset `—` sentinel). `label_of` maps a variant to its display label
/// (forwarding to each enum's inherent `display_name`). Returns whether the
/// selection changed.
fn enum_combo<T, F>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut Option<T>,
    variants: &'static [T],
    label_of: F,
) -> bool
where
    T: Clone + PartialEq,
    F: Fn(&T) -> &'static str,
{
    let current = value.as_ref().map(&label_of).unwrap_or("—");
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "—").clicked() && value.is_some() {
                *value = None;
                changed = true;
            }
            for v in variants {
                let selected = value.as_ref() == Some(v);
                if ui.selectable_label(selected, label_of(v)).clicked() && !selected {
                    *value = Some(v.clone());
                    changed = true;
                }
            }
        });
    changed
}

/// §PF3 validation: serialise then re-parse the config, returning the first
/// error message. A clean round-trip guarantees `save` will succeed.
fn validate(cfg: &WorldsConfig) -> Result<(), String> {
    let text = cfg.to_toml_string().map_err(|e| e.to_string())?;
    WorldsConfig::from_str(&text).map_err(|e| e.to_string())?;
    Ok(())
}

/// §PF5 / §PF6: write just the worlds catalog to disk atomically and clear its
/// dirty flag. Keeps any open §PF2 raw-editor tab for the same file in sync.
fn save(state: &mut BuilderState, rel: &str) -> Result<(), String> {
    let root = state
        .project_path
        .clone()
        .ok_or_else(|| "no project open".to_string())?;
    let text = state
        .data_catalogs
        .worlds
        .as_ref()
        .ok_or_else(|| "no worlds catalog".to_string())?
        .to_toml_string()
        .map_err(|e| e.to_string())?;
    let abs = root.join(rel);
    project_io::atomic_write(&abs, text.as_bytes()).map_err(|e| e.to_string())?;
    state.dirty_files.remove(rel);
    // Mirror the bytes into a raw-editor tab if one is open for this file.
    if let Some(buf) = state.toml_editor.open.get_mut(rel) {
        buf.buffer = text.clone();
        buf.mark_saved();
        buf.revalidate();
    }
    project_io::refresh_watcher_baseline(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_round_trips_default_config() {
        assert!(validate(&WorldsConfig::default()).is_ok());
    }

    #[test]
    fn worlds_rel_uses_config_dir() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.config.inputs.world_data_dir = "data/worlds".to_string();
        assert_eq!(worlds_rel(&state), "data/worlds/worlds.toml");
        state.config.inputs.world_data_dir = "custom/".to_string();
        assert_eq!(worlds_rel(&state), "custom/worlds.toml");
    }
}
