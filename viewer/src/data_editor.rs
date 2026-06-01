//! WORLD DATA tab: typed editor over `worlds.toml`.
//!
//! Loads from a project dir (must contain `sectorforge.toml` with an
//! `[inputs] world_data_dir = "..."` entry that points at a folder
//! containing `worlds.toml`).

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use sectorforge::worlds::{
    Atmosphere, Biosphere, GenerationRow, Government, NotableFeature, Population, StarColour,
    TechLevel, Temperature, WorldType,
};
use sectorforge::worlds_toml::{WorldsConfig, DEFAULT_FILENAME as WORLDS_TOML_FILENAME};

#[derive(Debug, Error)]
pub enum DataEditorError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config file error: {0}")]
    Config(String),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

use egui::{Color32, RichText, ScrollArea};

#[derive(Default)]
pub struct DataEditor {
    pub project_dir: Option<PathBuf>,
    pub dirty: bool,
    pub status: String,
    /// §45 WD2/WD4: native typed config loaded from `worlds.toml`.
    pub worlds_toml: Option<WorldsConfig>,
    pub worlds_toml_path: Option<PathBuf>,
}

impl DataEditor {
    pub fn load_from_project(&mut self, project_dir: &Path) -> Result<(), DataEditorError> {
        let cfg_path = project_dir.join("sectorforge.toml");
        let cfg_text = fs::read_to_string(&cfg_path)?;
        let data_rel = extract_world_data_dir(&cfg_text)?;
        let data_dir = project_dir.join(&data_rel);
        let toml_path = data_dir.join(WORLDS_TOML_FILENAME);

        let worlds_toml = if toml_path.exists() {
            let text = fs::read_to_string(&toml_path)?;
            let cfg = WorldsConfig::from_str(&text)
                .map_err(|e| DataEditorError::Config(format!("worlds.toml: {e}")))?;
            Some(cfg)
        } else {
            None
        };

        self.project_dir = Some(project_dir.to_path_buf());
        self.worlds_toml_path = Some(toml_path);
        self.worlds_toml = worlds_toml;
        self.dirty = false;
        self.status = match &self.worlds_toml {
            Some(cfg) => format!(
                "loaded worlds.toml ({} generation rows, {} feature groups)",
                cfg.generation.len(),
                cfg.features.global.len()
                    + cfg.features.by_world_type.len()
                    + cfg.features.by_star_colour.len()
            ),
            None => "no worlds.toml in data dir".to_string(),
        };
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), DataEditorError> {
        let (cfg, path) = match (&self.worlds_toml, &self.worlds_toml_path) {
            (Some(c), Some(p)) => (c, p),
            _ => return Err(DataEditorError::Config("no worlds.toml loaded".to_string())),
        };
        let text = cfg
            .to_toml_string()
            .map_err(|e| DataEditorError::Config(e.to_string()))?;
        fs::write(path, text)?;
        self.dirty = false;
        self.status = format!("saved worlds.toml ({} rows)", cfg.generation.len());
        Ok(())
    }
}

fn extract_world_data_dir(toml_text: &str) -> Result<String, DataEditorError> {
    #[derive(serde::Deserialize)]
    struct Mini {
        inputs: MiniInputs,
    }
    #[derive(serde::Deserialize)]
    struct MiniInputs {
        world_data_dir: String,
    }
    let parsed: Mini = toml::from_str(toml_text)?;
    Ok(parsed.inputs.world_data_dir)
}

// ── UI ──────────────────────────────────────────────────────────────────────

pub fn show(ui: &mut egui::Ui, editor: &mut DataEditor) {
    ui.horizontal(|ui| {
        let row_count = editor
            .worlds_toml
            .as_ref()
            .map_or(0, |c| c.generation.len());
        ui.label(
            RichText::new(format!("{row_count} rows")).color(super::palette::chrome_text_dim()),
        );
        if editor.dirty {
            ui.label(RichText::new("• unsaved").color(Color32::from_rgb(240, 200, 90)));
        }
    });

    ui.add_space(4.0);

    if editor.project_dir.is_none() {
        ui.label(
            RichText::new("load a project from the toolbar to edit world data")
                .color(super::palette::chrome_text_dim()),
        );
        return;
    }

    if editor.worlds_toml.is_none() {
        ui.label(
            RichText::new("project has no worlds.toml in its data dir")
                .color(super::palette::chrome_text_dim()),
        );
        return;
    }

    show_native(ui, editor);
}

// ── §45 WD4: native typed editor ────────────────────────────────────────────

fn show_native(ui: &mut egui::Ui, editor: &mut DataEditor) {
    let Some(cfg) = editor.worlds_toml.as_mut() else {
        ui.label("no worlds.toml loaded");
        return;
    };

    let mut any_change = false;
    let mut delete_row: Option<usize> = None;

    ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("worlds_toml_grid")
                .num_columns(12)
                .striped(true)
                .min_col_width(90.0)
                .show(ui, |ui| {
                    let headers = [
                        "#",
                        "star",
                        "world type",
                        "atmosphere",
                        "temperature",
                        "biosphere",
                        "population",
                        "tech",
                        "government",
                        "feature",
                        "weight",
                        "",
                    ];
                    for h in headers {
                        ui.label(
                            RichText::new(h.to_ascii_uppercase())
                                .color(super::palette::chrome_text())
                                .strong(),
                        );
                    }
                    ui.end_row();

                    for (idx, row) in cfg.generation.iter_mut().enumerate() {
                        ui.label(
                            RichText::new((idx + 1).to_string())
                                .color(super::palette::chrome_text_dim()),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("sc_{idx}"),
                            &mut row.star_colour,
                            StarColour::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("wt_{idx}"),
                            &mut row.world_type,
                            WorldType::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("at_{idx}"),
                            &mut row.atmosphere,
                            Atmosphere::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("te_{idx}"),
                            &mut row.temperature,
                            Temperature::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("bi_{idx}"),
                            &mut row.biosphere,
                            Biosphere::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("po_{idx}"),
                            &mut row.population,
                            Population::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("tl_{idx}"),
                            &mut row.tech,
                            TechLevel::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("gv_{idx}"),
                            &mut row.government,
                            Government::VARIANTS,
                            |v| v.display_name().to_string(),
                        );
                        any_change |= enum_combo(
                            ui,
                            format!("nf_{idx}"),
                            &mut row.notable_feature,
                            NotableFeature::VARIANTS,
                            |v| v.display_name().to_string(),
                        );

                        let mut weight = row.weight.unwrap_or(0.0);
                        let resp = ui.add(
                            egui::DragValue::new(&mut weight)
                                .speed(0.1)
                                .range(0.0..=1e6),
                        );
                        if resp.changed() {
                            row.weight = if weight > 0.0 { Some(weight) } else { None };
                            any_change = true;
                        }

                        if ui.button("✕").on_hover_text("delete row").clicked() {
                            delete_row = Some(idx);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(8.0);
            if ui.button("+ ADD ROW").clicked() {
                cfg.generation.push(GenerationRow::default());
                any_change = true;
            }
        });

    if let Some(idx) = delete_row {
        cfg.generation.remove(idx);
        any_change = true;
    }

    if any_change {
        editor.dirty = true;
    }
}

/// Generic enum dropdown: shows a `ComboBox` over `variants`, binding
/// the current `Option<T>` value (None = unset).
fn enum_combo<T, F>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut Option<T>,
    variants: &'static [T],
    label_of: F,
) -> bool
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    let current_label = value
        .as_ref()
        .map(&label_of)
        .unwrap_or_else(|| "—".to_string());
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "—").clicked() && value.is_some() {
                *value = None;
                changed = true;
            }
            for v in variants {
                let label = label_of(v);
                let selected = value.as_ref() == Some(v);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    *value = Some(v.clone());
                    changed = true;
                }
            }
        });
    changed
}
