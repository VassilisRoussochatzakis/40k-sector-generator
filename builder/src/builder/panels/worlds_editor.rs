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
use sectorforge_gui_core::{palette, ui_kit, widgets};

use crate::builder::project_io;
use crate::builder::state::ConfirmAction;
use crate::builder::{BuilderState, ModalKind};

/// Per-column help shown on the generation-grid headers. Tuple is
/// `(visible header, hover text)`; the hover names the underlying `worlds.toml`
/// field plus a one-line note so the schema mapping stays discoverable behind a
/// friendlier label. Order matches the grid's columns left-to-right.
const COLUMN_HELP: &[(&str, &str)] = &[
    ("#", "Row number (display only)."),
    (
        "Star colour",
        "Star colour for worlds rolled from this row (schema: star_colour). Leave as — to allow any.",
    ),
    (
        "World type",
        "Kind of world (schema: world_type), e.g. hive, agri, death world. Leave as — to allow any.",
    ),
    (
        "Atmosphere",
        "Breathability of the air (schema: atmosphere). Leave as — to allow any.",
    ),
    (
        "Temperature",
        "Surface climate band (schema: temperature). Leave as — to allow any.",
    ),
    (
        "Biosphere",
        "Native life present on the world (schema: biosphere). Leave as — to allow any.",
    ),
    (
        "Population",
        "Rough headcount band (schema: population). Leave as — to allow any.",
    ),
    (
        "Tech level",
        "Level of technology (schema: tech). Leave as — to allow any.",
    ),
    (
        "Government",
        "How the world is ruled (schema: government). Leave as — to allow any.",
    ),
    (
        "Notable feature",
        "Special trait or landmark (schema: notable_feature). Leave as — for none.",
    ),
    (
        "Weight",
        "How often this row is picked (schema: weight). Higher = more common; 0 or blank means unset.",
    ),
    ("", "Insert a new blank row above this one."),
    ("", "Delete this row."),
];

/// Project-relative path of the worlds catalog given the active config's
/// `[inputs] world_data_dir`.
fn worlds_rel(state: &BuilderState) -> String {
    let dir = state.config.inputs.world_data_dir.trim_end_matches('/');
    format!("{dir}/{WORLDS_TOML_FILENAME}")
}

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("World data");
    ui.label(
        RichText::new(
            "The pool of world recipes the generator draws from — one row per \
             possible combination, weighted by how often it appears.",
        )
        .color(Color32::DARK_GRAY),
    );
    ui.add_space(4.0);

    if state.project_path.is_none() {
        ui_kit::placeholder(ui, "Open a project to edit its world data.");
        return;
    }
    if state.data_catalogs.worlds.is_none() {
        ui_kit::placeholder(
            ui,
            "This project has no world catalog yet. Add a worlds.toml under its \
             data folder, then reopen the project.",
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
            RichText::new(format!("{row_count} world recipe(s)"))
                .strong()
                .color(palette::chrome_text_dim()),
        );
        if dirty {
            ui.colored_label(palette::warning(), "●  unsaved changes");
        }
    });

    // Validation surface: round-trip the config (serialise → re-parse) and
    // report the first error, if any.
    let validation = validate(state.data_catalogs.worlds.as_ref().unwrap());
    match &validation {
        Ok(()) => {
            ui.colored_label(palette::success(), "✓ valid world data");
        }
        Err(msg) => {
            ui.colored_label(
                palette::danger(),
                RichText::new(format!("✗ {msg}")).monospace(),
            );
        }
    }

    let mut do_save = false;
    ui.horizontal(|ui| {
        let can_save = dirty && validation.is_ok();
        if ui
            .add_enabled(can_save, egui::Button::new("💾  Save worlds.toml"))
            .on_hover_text("Write all changes back to the world catalog on disk")
            .clicked()
        {
            do_save = true;
        }
    });
    ui.separator();

    let mut any_change = false;
    let mut delete_request: Option<usize> = None;
    edit_rows(
        ui,
        state.data_catalogs.worlds.as_mut().unwrap(),
        &mut any_change,
        &mut delete_request,
    );

    if any_change {
        state.dirty = true;
        state.dirty_files.insert(rel.clone());
    }
    // §FRIENDLY_PANEL_PASS transform #7: a worlds.toml row delete bypasses the undo
    // bus — confirm before dropping a recipe (the in-grid 🗑 only requests it).
    if let Some(idx) = delete_request {
        state.feedback.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete world recipe?".into(),
            body: format!("Delete world recipe row #{}.", idx + 1),
            action: ConfirmAction::DeleteWorldGenRow(idx),
        });
    }
    if do_save {
        if let Err(e) = save(state, &rel) {
            state.feedback.modal = Some(ModalKind::Message(format!("Save failed: {e}")));
        }
    }
}

/// Render the generation-row grid with typed dropdowns, weights, and per-row
/// insert / delete controls. Sets `any_change` when the user edits anything, and
/// records the row a 🗑 click wants removed in `delete_request` so the caller
/// (which owns `state`) can confirm it (§FRIENDLY_PANEL_PASS transform #7).
fn edit_rows(
    ui: &mut egui::Ui,
    cfg: &mut WorldsConfig,
    any_change: &mut bool,
    delete_request: &mut Option<usize>,
) {
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
                    for (header, help) in COLUMN_HELP {
                        ui.label(RichText::new(*header).strong())
                            .on_hover_text(*help);
                    }
                    ui.end_row();

                    if cfg.generation.is_empty() {
                        ui_kit::placeholder(
                            ui,
                            "No world recipes yet — use “➕ Add row” below to start.",
                        );
                        ui.end_row();
                    }

                    for (idx, row) in cfg.generation.iter_mut().enumerate() {
                        ui.label(
                            RichText::new((idx + 1).to_string())
                                .monospace()
                                .color(palette::chrome_text_dim()),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("sc", idx),
                            &mut row.star_colour,
                            StarColour::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("wt", idx),
                            &mut row.world_type,
                            WorldType::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("at", idx),
                            &mut row.atmosphere,
                            Atmosphere::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("te", idx),
                            &mut row.temperature,
                            Temperature::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("bi", idx),
                            &mut row.biosphere,
                            Biosphere::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("po", idx),
                            &mut row.population,
                            Population::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |=
                            enum_combo(ui, ("tl", idx), &mut row.tech, TechLevel::VARIANTS, |v| {
                                v.display_name()
                            });
                        *any_change |= enum_combo(
                            ui,
                            ("gv", idx),
                            &mut row.government,
                            Government::VARIANTS,
                            |v| v.display_name(),
                        );
                        *any_change |= enum_combo(
                            ui,
                            ("nf", idx),
                            &mut row.notable_feature,
                            NotableFeature::VARIANTS,
                            |v| v.display_name(),
                        );

                        let mut weight = row.weight.unwrap_or(0.0);
                        if ui
                            .add(
                                egui::DragValue::new(&mut weight)
                                    .speed(0.1)
                                    .range(0.0..=1e6),
                            )
                            .changed()
                        {
                            row.weight = (weight > 0.0).then_some(weight);
                            *any_change = true;
                        }

                        if ui
                            .small_button("＋")
                            .on_hover_text("Insert a new blank row above this one")
                            .clicked()
                        {
                            insert_above = Some(idx);
                        }
                        if ui
                            .small_button("✕")
                            .on_hover_text("Delete this row")
                            .clicked()
                        {
                            delete_row = Some(idx);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            if ui
                .button("➕  Add row")
                .on_hover_text("Append a new blank world recipe to the end of the list")
                .clicked()
            {
                cfg.generation.push(GenerationRow::default());
                *any_change = true;
            }
        });

    if let Some(idx) = insert_above {
        cfg.generation.insert(idx, GenerationRow::default());
        *any_change = true;
    }
    if let Some(idx) = delete_row {
        *delete_request = Some(idx);
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: delete the worlds.toml generation row at
/// `idx` (confirmed payload of [`ModalKind::ConfirmDestructive`]) and reproduce the
/// panel's dirty bookkeeping. The worlds catalogue bypasses the undo bus.
pub(crate) fn delete_gen_row(state: &mut BuilderState, idx: usize) {
    // Audit finding #8: dispatched from the confirm modal (out of any catalog
    // session), so commit immediately via `edit_catalogs` for one undoable
    // entry. No-op (and no undo entry) when the row index is out of range.
    if state
        .data_catalogs
        .worlds
        .as_ref()
        .is_none_or(|cfg| idx >= cfg.generation.len())
    {
        return;
    }
    let committed = state
        .edit_catalogs(|snap| {
            if let Some(cfg) = snap.catalogs.worlds.as_mut() {
                if idx < cfg.generation.len() {
                    cfg.generation.remove(idx);
                }
            }
        })
        .is_ok();
    if committed {
        let rel = worlds_rel(state);
        state.dirty_files.insert(rel);
    }
}

/// Generic enum dropdown over `Enum::VARIANTS`, binding an `Option<T>` (None =
/// the unset `—` sentinel). `label_of` maps a variant to its friendly display
/// label (forwarding to each enum's inherent `display_name`); the raw schema key
/// stays reachable on each item's hover. Returns whether the selection changed.
///
/// Forwards to the shared [`widgets::enum_combo`] (F2) with this panel's hover
/// policy: the `—` sentinel explains the unset option and each variant exposes its
/// raw `{v:?}` schema key (the source of the `Debug` bound, which the shared widget
/// itself does not require).
fn enum_combo<T, F>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut Option<T>,
    variants: &'static [T],
    label_of: F,
) -> bool
where
    T: Clone + PartialEq + std::fmt::Debug,
    F: Fn(&T) -> &'static str,
{
    widgets::enum_combo(
        ui,
        id,
        value,
        variants,
        label_of,
        |v| Some(format!("key: {v:?}")),
        Some("Any — leave this field unset"),
    )
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
