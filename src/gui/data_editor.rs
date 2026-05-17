//! WORLD DATA tab: tabular editor for `key.csv` + `generator.csv`.
//!
//! Loads from a project dir (which must contain `sectorforge.toml` with an
//! `[inputs] world_data_dir = "..."` entry). Edits raw cell strings; saves
//! back by writing both CSVs in place.

use std::fs;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, ScrollArea, TextEdit};

use crate::worlds::parse_csv;

const KEY_HEADER: &[&str] = &[
    "star_colour",
    "world_type",
    "atmosphere",
    "temperature",
    "biosphere",
    "population",
    "tech_level",
    "government",
    "notable_feature",
];

const GEN_HEADER: &[&str] = &[
    "star_colour",
    "world_type",
    "atmosphere",
    "temperature",
    "biosphere",
    "population",
    "tech_level",
    "government",
    "notable_feature",
    "counter",
    "weight",
];

#[derive(Default)]
pub struct DataEditor {
    pub project_dir: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub gen_path: Option<PathBuf>,
    pub key_rows: Vec<Vec<String>>,
    pub gen_rows: Vec<Vec<String>>,
    pub dirty: bool,
    pub status: String,
    pub view: DataView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataView {
    #[default]
    Key,
    Generator,
}

impl DataEditor {
    pub fn load_from_project(&mut self, project_dir: &Path) -> Result<(), String> {
        let cfg_path = project_dir.join("sectorforge.toml");
        let cfg_text = fs::read_to_string(&cfg_path)
            .map_err(|e| format!("read {}: {e}", cfg_path.display()))?;
        let data_rel = extract_world_data_dir(&cfg_text)?;
        let data_dir = project_dir.join(&data_rel);
        let key_path = data_dir.join("key.csv");
        let gen_path = data_dir.join("generator.csv");

        let key_text = fs::read_to_string(&key_path)
            .map_err(|e| format!("read {}: {e}", key_path.display()))?;
        let gen_text = fs::read_to_string(&gen_path)
            .map_err(|e| format!("read {}: {e}", gen_path.display()))?;
        let (_kh, key_rows) = parse_csv(&key_text)?;
        let (_gh, gen_rows) = parse_csv(&gen_text)?;

        self.project_dir = Some(project_dir.to_path_buf());
        self.key_path = Some(key_path);
        self.gen_path = Some(gen_path);
        self.key_rows = key_rows;
        self.gen_rows = gen_rows;
        self.dirty = false;
        self.status = format!(
            "loaded {} key rows, {} generator rows",
            self.key_rows.len(),
            self.gen_rows.len()
        );
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), String> {
        let key_path = self
            .key_path
            .as_ref()
            .ok_or_else(|| "no key.csv path".to_string())?;
        let gen_path = self
            .gen_path
            .as_ref()
            .ok_or_else(|| "no generator.csv path".to_string())?;
        let key_text = serialize_csv(KEY_HEADER, &self.key_rows);
        let gen_text = serialize_csv(GEN_HEADER, &self.gen_rows);
        fs::write(key_path, key_text)
            .map_err(|e| format!("write {}: {e}", key_path.display()))?;
        fs::write(gen_path, gen_text)
            .map_err(|e| format!("write {}: {e}", gen_path.display()))?;
        self.dirty = false;
        self.status = format!(
            "saved {} key rows, {} generator rows",
            self.key_rows.len(),
            self.gen_rows.len()
        );
        Ok(())
    }
}

fn extract_world_data_dir(toml_text: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Mini {
        inputs: MiniInputs,
    }
    #[derive(serde::Deserialize)]
    struct MiniInputs {
        world_data_dir: String,
    }
    let parsed: Mini = toml::from_str(toml_text)
        .map_err(|e| format!("parse sectorforge.toml: {e}"))?;
    Ok(parsed.inputs.world_data_dir)
}

fn serialize_csv(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(&header.join(","));
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = (0..header.len())
            .map(|i| escape_cell(row.get(i).map(|s| s.as_str()).unwrap_or("")))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn escape_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

// ── UI ──────────────────────────────────────────────────────────────────────

pub fn show(ui: &mut egui::Ui, editor: &mut DataEditor) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut editor.view, DataView::Key, "KEY");
        ui.selectable_value(&mut editor.view, DataView::Generator, "GENERATOR");
        ui.separator();
        let row_count = match editor.view {
            DataView::Key => editor.key_rows.len(),
            DataView::Generator => editor.gen_rows.len(),
        };
        ui.label(
            RichText::new(format!("{row_count} rows"))
                .color(super::palette::TEXT_DIM)
                .monospace(),
        );
        if editor.dirty {
            ui.label(
                RichText::new("• unsaved")
                    .color(Color32::from_rgb(240, 200, 90))
                    .monospace(),
            );
        }
    });

    ui.add_space(4.0);

    if editor.key_path.is_none() {
        ui.label(
            RichText::new("load a project from the toolbar to edit world data")
                .color(super::palette::TEXT_DIM)
                .monospace(),
        );
        return;
    }

    let (header, rows) = match editor.view {
        DataView::Key => (KEY_HEADER, &mut editor.key_rows),
        DataView::Generator => (GEN_HEADER, &mut editor.gen_rows),
    };

    ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut delete_row: Option<usize> = None;
            let mut any_change = false;
            egui::Grid::new(format!("data_grid_{:?}", editor.view))
                .num_columns(header.len() + 2)
                .striped(true)
                .min_col_width(110.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("#")
                            .color(super::palette::TEXT_DIM)
                            .monospace(),
                    );
                    for h in header {
                        ui.label(
                            RichText::new(h.to_ascii_uppercase())
                                .color(super::palette::TEXT)
                                .strong()
                                .monospace(),
                        );
                    }
                    ui.label("");
                    ui.end_row();

                    for (row_idx, row) in rows.iter_mut().enumerate() {
                        while row.len() < header.len() {
                            row.push(String::new());
                        }
                        ui.label(
                            RichText::new(format!("{}", row_idx + 1))
                                .color(super::palette::TEXT_DIM)
                                .monospace(),
                        );
                        for (col_idx, col_name) in header.iter().enumerate() {
                            let changed = edit_cell(ui, col_name, &mut row[col_idx]);
                            if changed {
                                any_change = true;
                            }
                        }
                        if ui.button("✕").on_hover_text("delete row").clicked() {
                            delete_row = Some(row_idx);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(8.0);
            if ui.button("+ ADD ROW").clicked() {
                rows.push(vec![String::new(); header.len()]);
                any_change = true;
            }

            if let Some(idx) = delete_row {
                rows.remove(idx);
                any_change = true;
            }
            if any_change {
                editor.dirty = true;
            }
        });
}

fn edit_cell(ui: &mut egui::Ui, column: &str, value: &mut String) -> bool {
    if let Some(options) = enum_options(column) {
        let mut current = value.clone();
        let mut changed = false;
        egui::ComboBox::from_id_salt((column, value.as_ptr() as usize))
            .selected_text(if current.is_empty() { "—" } else { &current })
            .width(160.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_empty(), "—").clicked() {
                    current.clear();
                    changed = true;
                }
                for opt in options {
                    if ui
                        .selectable_label(current == *opt, *opt)
                        .clicked()
                    {
                        current = (*opt).to_string();
                        changed = true;
                    }
                }
            });
        if changed {
            *value = current;
        }
        changed
    } else {
        let resp = ui.add(TextEdit::singleline(value).desired_width(110.0));
        resp.changed()
    }
}

fn enum_options(column: &str) -> Option<&'static [&'static str]> {
    Some(match column {
        "star_colour" => &["O", "B", "A", "F", "G", "K", "M"],
        "world_type" => WORLD_TYPE_OPTS,
        "atmosphere" => ATMOSPHERE_OPTS,
        "temperature" => TEMPERATURE_OPTS,
        "biosphere" => BIOSPHERE_OPTS,
        "population" => POPULATION_OPTS,
        "tech_level" => TECH_LEVEL_OPTS,
        "government" => GOVERNMENT_OPTS,
        "notable_feature" => NOTABLE_FEATURE_OPTS,
        _ => return None,
    })
}

// Canonical strings — must match `FromStr` impls in `worlds.rs`.
const WORLD_TYPE_OPTS: &[&str] = &[
    "Agri-World",
    "Asteroid",
    "Bastion World",
    "Death World",
    "Dead World",
    "Extractive Colony",
    "Feral World",
    "Feudal World",
    "Forge World",
    "Frontier World",
    "Hive World",
    "Industrial World",
    "Orbital",
    "Penal World",
    "Planetary Dump",
    "Planetary Monument",
    "Pleasure World",
    "Research Station",
    "Shrine World",
    "Tomb World",
    "Warp-Lost World",
    "Worldship",
    "Xenos World",
];

const ATMOSPHERE_OPTS: &[&str] = &[
    "Airless",
    "Breathable",
    "Corrosive",
    "Exotic",
    "Thin",
    "Tainted",
    "Toxic",
];

const TEMPERATURE_OPTS: &[&str] = &["Freezing", "Cold", "Temperate", "Hot", "Boiling"];

const BIOSPHERE_OPTS: &[&str] = &[
    "Nonexistent",
    "Minimal",
    "Thriving",
    "Poisoned",
    "Xeno Hybrid",
    "Xeno Dominance",
];

const POPULATION_OPTS: &[&str] = &[
    "Uninhabited",
    "Minimal",
    "Lightly Populated",
    "Sole Settlement",
    "Densely Populated",
    "Extremely Dense",
];

const TECH_LEVEL_OPTS: &[&str] = &[
    "Primitive",
    "Low",
    "Standard",
    "High",
    "Xeno Hybrid",
    "Archaeotech",
];

const GOVERNMENT_OPTS: &[&str] = &[
    "Balkanized Local Factions",
    "Chaos Cult",
    "Clans / Tribes",
    "Communards",
    "Corrupt Aristocrats",
    "Demagogue",
    "Ecclesiarchical Appointee",
    "Elitist Tyrant",
    "Explorator Authority",
    "Guilds / Combines",
    "Hereteks",
    "Heretical Imperial Cult",
    "Infractionist Gang",
    "Local Religious Authorities",
    "Loyalist Mass Movement",
    "Magistrate Council",
    "Mechanicus Forge-Lord",
    "Megacorporations",
    "Military Governor",
    "None",
    "Populist Tyrant",
    "Puppet Government",
    "Revolutionary Junta",
    "Rogue Trader Dynasty",
    "Shadowy Psyker Cabal",
    "Traditional Oligarchy",
    "Traditionalist Aristocracy",
    "Warlords",
    "Warrior Aristocracy",
    "Xenos Overlords",
];

const NOTABLE_FEATURE_OPTS: &[&str] = &[
    "Abhumans",
    "Administrative Hub",
    "Altered Humans",
    "Ancient Archive",
    "Ancient Tombs",
    "Archaeotech Ruins",
    "Blinding Mists",
    "Celestial Phenomena",
    "Chaos Cultists",
    "Civil War",
    "Cold War",
    "Crumbling Arcologies",
    "Daemonic Corruption",
    "Dangerous Wildlife",
    "Desert World",
    "Deviant Religion",
    "Eugenic Cult",
    "Extreme Environment",
    "Factional Fragmentation",
    "Failed Paradise",
    "Flying Cities",
    "Forbidden Tech",
    "Foreign Control",
    "Freak Geology",
    "Freak Weather",
    "Freeport",
    "Friendly Xenos",
    "Frozen World",
    "Gold Rush",
    "Great Work",
    "Heavy Industry",
    "Heavy Mining",
    "Hereteks",
    "Holy War",
    "Hostile Biosphere",
    "Hostile Xenos",
    "Impending Doom",
    "Imperial Knights",
    "Important Shrine",
    "Inquisition Outpost",
    "Jungle World",
    "Libertines",
    "Local Specialty",
    "Local Tech",
    "Major Spaceyard",
    "Martial Law",
    "Mass Panic",
    "Minimal Contact",
    "Missionaries",
    "Mutant Hordes",
    "Naval Blockade",
    "Naval Outpost",
    "Navigator House",
    "Nomadic Cities",
    "Notable Local",
    "Ocean World",
    "Out of Contact",
    "Pandemic",
    "Pilgrimage Site",
    "Pocket Empire",
    "Police State",
    "Popular Uprising",
    "Powerful Criminals",
    "Powerful Nobles",
    "Primitive Xenos",
    "Prosperous",
    "Psyker Academy",
    "Psyker Cult",
    "Quarantined",
    "Radioactive",
    "Recently Rediscovered",
    "Schola Progenium",
    "Seagoing Cities",
    "Sealed Menace",
    "Secret Masters",
    "Sectarians",
    "Seismic Instability",
    "Separatists",
    "Silica Animus",
    "Sole Suppliers",
    "Sororitas Convent",
    "Space Hulks",
    "Strange Customs",
    "Strange Hatred",
    "Subsector Hegemon",
    "Tech-Priest Cult",
    "Test Site",
    "The Silent Trade",
    "Trade Hub",
    "Unmapped Wastes",
    "Vast Fortresses",
    "Verdant Ecology",
    "War Zone",
    "Warp Phenomena",
    "Witch Hunt",
    "Xeno Ruins",
    "Xenophiles",
    "Xenophobes",
    "Xenos Infiltrators",
    "Zombies",
];

