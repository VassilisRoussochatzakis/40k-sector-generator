//! WORLD DATA tab: tabular editor for `key.csv` + `generator.csv`.
//!
//! Loads from a project dir (which must contain `sectorforge.toml` with an
//! `[inputs] world_data_dir = "..."` entry). Edits raw cell strings; saves
//! back by writing both CSVs in place.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, ScrollArea, Stroke, TextEdit};

use crate::worlds::parse_csv;

const COUNTER_COL: usize = 9;
const WEIGHT_COL: usize = 10;
const WORLD_TYPE_COL: usize = 1;

const BORDER_ERR: Color32 = Color32::from_rgb(235, 90, 90);
const BORDER_WARN: Color32 = Color32::from_rgb(240, 200, 90);
const BAR_BG: Color32 = Color32::from_rgb(40, 36, 52);
const BAR_FILL: Color32 = Color32::from_rgb(110, 160, 210);

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
    pub filter_world_type: String,
    pub filter_star_colour: String,
    pub filter_government: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellStatus {
    Ok,
    Warn,
    Err,
}

impl CellStatus {
    fn border(self) -> Option<Color32> {
        match self {
            CellStatus::Ok => None,
            CellStatus::Warn => Some(BORDER_WARN),
            CellStatus::Err => Some(BORDER_ERR),
        }
    }
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
        fs::write(key_path, key_text).map_err(|e| format!("write {}: {e}", key_path.display()))?;
        fs::write(gen_path, gen_text).map_err(|e| format!("write {}: {e}", gen_path.display()))?;
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
    let parsed: Mini =
        toml::from_str(toml_text).map_err(|e| format!("parse sectorforge.toml: {e}"))?;
    Ok(parsed.inputs.world_data_dir)
}

fn serialize_csv(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(&header.join(","));
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = (0..header.len())
            .map(|i| escape_cell(row.get(i).map_or("", |s| s.as_str())))
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

    let is_generator = matches!(editor.view, DataView::Generator);
    let key_sets = build_key_sets(&editor.key_rows);
    let totals = if is_generator {
        counter_weight_totals(&editor.gen_rows)
    } else {
        HashMap::new()
    };

    if is_generator {
        show_filter_bar(ui, editor, &key_sets);
        ui.add_space(4.0);
    }

    let filters = (
        editor.filter_world_type.clone(),
        editor.filter_star_colour.clone(),
        editor.filter_government.clone(),
    );

    let (header, rows) = match editor.view {
        DataView::Key => (KEY_HEADER, &mut editor.key_rows),
        DataView::Generator => (GEN_HEADER, &mut editor.gen_rows),
    };

    let mut any_change = false;
    ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut delete_row: Option<usize> = None;
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
                        if is_generator && !row_matches_filter(row, &filters) {
                            continue;
                        }
                        ui.label(
                            RichText::new(format!("{}", row_idx + 1))
                                .color(super::palette::TEXT_DIM)
                                .monospace(),
                        );

                        let prev_world_type = row.get(WORLD_TYPE_COL).cloned().unwrap_or_default();

                        for (col_idx, col_name) in header.iter().enumerate() {
                            let counter_val = row.get(COUNTER_COL).cloned().unwrap_or_default();
                            let world_type_val =
                                row.get(WORLD_TYPE_COL).cloned().unwrap_or_default();
                            let status =
                                cell_status(col_name, &row[col_idx], &key_sets, &world_type_val);
                            let bar = if is_generator && *col_name == "weight" {
                                let total = totals.get(&counter_val).copied().unwrap_or(0.0);
                                let w: f64 = row[col_idx].parse().unwrap_or(0.0);
                                if total > 0.0 {
                                    Some(((w / total) as f32, w / total))
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            let changed =
                                edit_cell(ui, row_idx, col_name, &mut row[col_idx], status, bar);
                            if changed {
                                any_change = true;
                            }
                        }

                        if is_generator {
                            let new_world_type =
                                row.get(WORLD_TYPE_COL).cloned().unwrap_or_default();
                            if new_world_type != prev_world_type && !new_world_type.is_empty() {
                                let counter_empty =
                                    row.get(COUNTER_COL).is_none_or(|s| s.is_empty());
                                if counter_empty {
                                    if row.len() <= COUNTER_COL {
                                        row.resize(COUNTER_COL + 1, String::new());
                                    }
                                    row[COUNTER_COL] = new_world_type;
                                    any_change = true;
                                }
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
        });

    if any_change {
        editor.dirty = true;
    }
}

fn show_filter_bar(
    ui: &mut egui::Ui,
    editor: &mut DataEditor,
    key_sets: &HashMap<&'static str, HashSet<String>>,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("filter:")
                .color(super::palette::TEXT_DIM)
                .monospace(),
        );
        filter_dropdown(ui, "world_type", &mut editor.filter_world_type, key_sets);
        filter_dropdown(ui, "star_colour", &mut editor.filter_star_colour, key_sets);
        filter_dropdown(ui, "government", &mut editor.filter_government, key_sets);
        if ui.small_button("clear").clicked() {
            editor.filter_world_type.clear();
            editor.filter_star_colour.clear();
            editor.filter_government.clear();
        }
    });
}

fn filter_dropdown(
    ui: &mut egui::Ui,
    column: &str,
    selected: &mut String,
    key_sets: &HashMap<&'static str, HashSet<String>>,
) {
    let label = if selected.is_empty() {
        format!("{column}: any")
    } else {
        format!("{column}: {selected}")
    };
    let popup_id = ui.make_persistent_id(("filter_popup", column));
    let btn = ui.small_button(label);
    if btn.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &btn,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(180.0);
            ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                if ui.selectable_label(selected.is_empty(), "any").clicked() {
                    selected.clear();
                    ui.memory_mut(|m| m.close_popup());
                }
                let empty = HashSet::new();
                let set = key_sets.get(column).unwrap_or(&empty);
                let mut opts: Vec<&String> = set.iter().collect();
                opts.sort();
                for opt in opts {
                    if ui.selectable_label(selected == opt, opt).clicked() {
                        *selected = opt.clone();
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });
        },
    );
}

fn row_matches_filter(row: &[String], filters: &(String, String, String)) -> bool {
    let (wt, sc, gov) = filters;
    if !wt.is_empty() && row.get(WORLD_TYPE_COL).map(|s| s.as_str()) != Some(wt.as_str()) {
        return false;
    }
    if !sc.is_empty() && row.first().map(|s| s.as_str()) != Some(sc.as_str()) {
        return false;
    }
    if !gov.is_empty() && row.get(7).map(|s| s.as_str()) != Some(gov.as_str()) {
        return false;
    }
    true
}

fn build_key_sets(key_rows: &[Vec<String>]) -> HashMap<&'static str, HashSet<String>> {
    let mut out: HashMap<&'static str, HashSet<String>> = HashMap::new();
    for (col_idx, col_name) in KEY_HEADER.iter().enumerate() {
        let set: HashSet<String> = key_rows
            .iter()
            .filter_map(|r| r.get(col_idx))
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        out.insert(*col_name, set);
    }
    out
}

fn counter_weight_totals(gen_rows: &[Vec<String>]) -> HashMap<String, f64> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    for row in gen_rows {
        let counter = row.get(COUNTER_COL).cloned().unwrap_or_default();
        if counter.is_empty() {
            continue;
        }
        let weight: f64 = row
            .get(WEIGHT_COL)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        if weight > 0.0 {
            *totals.entry(counter).or_insert(0.0) += weight;
        }
    }
    totals
}

fn cell_status(
    column: &str,
    value: &str,
    key_sets: &HashMap<&'static str, HashSet<String>>,
    row_world_type: &str,
) -> CellStatus {
    if value.is_empty() {
        return CellStatus::Ok;
    }
    match column {
        "weight" => match value.parse::<f64>() {
            Ok(w) if w >= 0.0 => CellStatus::Ok,
            _ => CellStatus::Err,
        },
        "counter" => {
            let world_set = key_sets.get("world_type");
            let in_world_types = world_set.is_none_or(|s| s.contains(value));
            if !in_world_types {
                return CellStatus::Err;
            }
            if !row_world_type.is_empty() && row_world_type != value {
                CellStatus::Warn
            } else {
                CellStatus::Ok
            }
        }
        _ => {
            let in_set = key_sets.get(column).is_none_or(|s| s.contains(value));
            if in_set {
                CellStatus::Ok
            } else {
                CellStatus::Err
            }
        }
    }
}

fn edit_cell(
    ui: &mut egui::Ui,
    row_idx: usize,
    column: &str,
    value: &mut String,
    status: CellStatus,
    bar: Option<(f32, f64)>,
) -> bool {
    let options = enum_options(column);
    let mut changed = false;
    ui.horizontal(|ui| {
        let width = if bar.is_some() {
            60.0
        } else if options.is_some() {
            130.0
        } else {
            110.0
        };
        let resp = ui.add(
            TextEdit::singleline(value)
                .id(egui::Id::new(("data_cell", row_idx, column)))
                .desired_width(width),
        );
        if resp.changed() {
            changed = true;
        }
        if let Some(border) = status.border() {
            ui.painter()
                .rect_stroke(resp.rect.expand(1.0), 2.0, Stroke::new(1.0, border));
            let hover = match status {
                CellStatus::Err => "not in key.csv",
                CellStatus::Warn => "does not match this row's world_type",
                CellStatus::Ok => "",
            };
            if !hover.is_empty() {
                resp.on_hover_text(hover);
            }
        }
        if let Some(opts) = options {
            let popup_id = ui.make_persistent_id(("data_popup", row_idx, column));
            let btn = ui.small_button("▼");
            if btn.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            egui::popup::popup_below_widget(
                ui,
                popup_id,
                &btn,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(180.0);
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        if ui.selectable_label(value.is_empty(), "—").clicked() {
                            value.clear();
                            changed = true;
                            ui.memory_mut(|m| m.close_popup());
                        }
                        for opt in opts {
                            if ui.selectable_label(value == opt, *opt).clicked() {
                                *value = (*opt).to_string();
                                changed = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                        }
                    });
                },
            );
        }
        if let Some((frac, ratio)) = bar {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(60.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, BAR_BG);
            let fill_w = rect.width() * frac.clamp(0.0, 1.0);
            let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
            ui.painter().rect_filled(fill_rect, 2.0, BAR_FILL);
            ui.label(
                RichText::new(format!("{:>4.1}%", ratio * 100.0))
                    .color(super::palette::TEXT_DIM)
                    .monospace()
                    .small(),
            );
        }
    });
    changed
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
