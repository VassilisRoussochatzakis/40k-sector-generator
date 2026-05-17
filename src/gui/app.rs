//! Top-level eframe app: holds loaded sector + navigation state, dispatches
//! between sector view, system view, and edit view.

use std::path::PathBuf;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use crate::sector_model::{GeneratedSector, GeneratedSystem};

use super::data_editor::DataEditor;
use super::editor::{self, EditorState};
use super::info_panel;
use super::palette::{self, TEXT, TEXT_DIM};
use super::sector_view::SectorView;
use super::system_view::{SystemClick, SystemSelection, SystemView};

pub struct App {
    sector: Option<GeneratedSector>,
    view: View,
    sector_selected: Option<String>,
    sector_hex_size: f32,
    system_side: f32,
    editor: EditorState,
    data_editor: DataEditor,
    project_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum View {
    Sector,
    System {
        system_id: String,
        selection: SystemSelection,
    },
    Edit,
    Data,
}

impl App {
    pub fn new(sector: GeneratedSector) -> Self {
        Self {
            sector: Some(sector),
            view: View::Sector,
            sector_selected: None,
            sector_hex_size: 44.0,
            system_side: 700.0,
            editor: EditorState::default(),
            data_editor: DataEditor::default(),
            project_dir: None,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            sector: None,
            view: View::Edit,
            sector_selected: None,
            sector_hex_size: 44.0,
            system_side: 700.0,
            editor: EditorState::default(),
            data_editor: DataEditor::default(),
            project_dir: None,
        }
    }

    /// Set the project directory and try to preload world-data CSVs.
    pub fn with_project_dir(mut self, dir: PathBuf) -> Self {
        if let Err(e) = self.data_editor.load_from_project(&dir) {
            self.data_editor.status = format!("load failed: {e}");
        }
        self.project_dir = Some(dir);
        self
    }

    fn system_by_id(&self, id: &str) -> Option<&GeneratedSystem> {
        self.sector.as_ref()?.systems.iter().find(|s| s.id == id)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx);

        TopBottomPanel::top("nav")
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let on_sector = matches!(self.view, View::Sector);
                    let on_edit = matches!(self.view, View::Edit);
                    let on_data = matches!(self.view, View::Data);
                    let has_sector = self.sector.is_some();
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_sector,
                                RichText::new("SECTOR MAP").color(TEXT).monospace(),
                            ),
                        )
                        .clicked()
                    {
                        self.view = View::Sector;
                    }
                    if ui
                        .selectable_label(on_edit, RichText::new("EDIT").color(TEXT).monospace())
                        .clicked()
                    {
                        // Entering edit mode: copy current viewed sector into
                        // the editor if the editor is empty.
                        if self.editor.sector.is_none() {
                            if let Some(s) = self.sector.clone() {
                                self.editor.set_sector(s, None);
                            }
                        }
                        self.view = View::Edit;
                    }
                    if ui
                        .selectable_label(
                            on_data,
                            RichText::new("WORLD DATA").color(TEXT).monospace(),
                        )
                        .clicked()
                    {
                        self.view = View::Data;
                    }
                    if let View::System { system_id, .. } = &self.view {
                        ui.label(RichText::new("›").color(TEXT_DIM).monospace());
                        ui.label(
                            RichText::new(format!("SYSTEM {}", system_id.to_uppercase()))
                                .color(TEXT)
                                .monospace(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(s) = &self.sector {
                            ui.label(
                                RichText::new(format!(
                                    "{} - {} sys, {} worlds",
                                    s.id.to_uppercase(),
                                    s.systems.len(),
                                    s.manifest.world_count
                                ))
                                .color(TEXT_DIM)
                                .monospace(),
                            );
                        } else {
                            ui.label(
                                RichText::new("NO SECTOR LOADED")
                                    .color(TEXT_DIM)
                                    .monospace(),
                            );
                        }
                    });
                });
            });

        match self.view.clone() {
            View::Sector => self.draw_sector_layout(ctx),
            View::System { system_id, selection } => {
                self.draw_system_layout(ctx, &system_id, selection)
            }
            View::Edit => self.draw_edit_layout(ctx),
            View::Data => self.draw_data_layout(ctx),
        }
    }
}

impl App {
    fn draw_sector_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(RichText::new("no sector loaded").color(TEXT_DIM).monospace());
                });
            return;
        };
        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(sel) = self.sector_selected.as_deref() {
                        if let Some(sys) = self.system_by_id(sel) {
                            info_panel::system_summary(ui, sys);
                            ui.add_space(10.0);
                            if ui
                                .button(RichText::new("OPEN SYSTEM →").monospace())
                                .clicked()
                            {
                                self.view = View::System {
                                    system_id: sys.id.clone(),
                                    selection: SystemSelection::None,
                                };
                            }
                            ui.separator();
                        }
                    }
                    info_panel::sector_overview(ui, &sector);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| self.show_sector(ui));
    }

    fn draw_system_layout(
        &mut self,
        ctx: &egui::Context,
        system_id: &str,
        selection: SystemSelection,
    ) {
        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    let sys_opt = self.system_by_id(system_id).cloned();
                    if let Some(sys) = sys_opt.as_ref() {
                        match selection {
                            SystemSelection::World(idx) => {
                                if let Some(w) = sys.worlds.iter().find(|w| w.index == idx) {
                                    info_panel::world_detail(ui, w);
                                    ui.add_space(10.0);
                                }
                            }
                            SystemSelection::Star => {
                                info_panel::star_detail(ui, sys);
                                ui.add_space(10.0);
                            }
                            SystemSelection::None => {}
                        }
                        ui.separator();
                        info_panel::system_summary(ui, sys);
                        ui.add_space(10.0);
                        if ui
                            .button(RichText::new("← BACK TO SECTOR").monospace())
                            .clicked()
                        {
                            self.view = View::Sector;
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| self.show_system(ui, system_id, selection));
    }

    fn draw_edit_layout(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("edit_toolbar")
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(6.0))
            .show(ctx, |ui| {
                editor::editor_toolbar(ui, &mut self.editor);
            });

        SidePanel::right("edit_inspector")
            .resizable(true)
            .default_width(360.0)
            .min_width(300.0)
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    use editor::state::Selection as Sel;
                    let sel = self.editor.selection.clone();
                    match self.editor.tab {
                        editor::state::Tab::Map => match sel {
                            Sel::World { .. } => editor::show_world_inspector(ui, &mut self.editor),
                            Sel::System(_) => editor::show_system_inspector(ui, &mut self.editor),
                            Sel::None => {
                                ui.label(
                                    RichText::new("click a hex to add or select a system")
                                        .color(TEXT_DIM)
                                        .monospace(),
                                );
                            }
                        },
                        editor::state::Tab::Routes => editor::show_routes(ui, &mut self.editor),
                        editor::state::Tab::Factions => editor::show_factions(ui, &mut self.editor),
                        editor::state::Tab::Settings => editor::show_settings(ui, &mut self.editor),
                    }
                });
            });

        TopBottomPanel::bottom("edit_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.editor.hex_size, 20.0..=80.0)
                            .show_value(false),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| match self.editor.tab {
                    editor::state::Tab::Map => editor::show_map(ui, &mut self.editor),
                    _ => {
                        ui.label(
                            RichText::new("(switch to MAP tab to view the hex grid)")
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    }
                });
            });

        editor::draw_dialog(ctx, &mut self.editor);
    }

    fn draw_data_layout(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("data_toolbar")
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let can_reload = self.project_dir.is_some();
                    if ui
                        .add_enabled(
                            can_reload,
                            egui::Button::new(RichText::new("RELOAD").monospace()),
                        )
                        .on_hover_text("re-read CSVs from disk (discards unsaved edits)")
                        .clicked()
                    {
                        if let Some(dir) = self.project_dir.clone() {
                            if let Err(e) = self.data_editor.load_from_project(&dir) {
                                self.data_editor.status = format!("load failed: {e}");
                            }
                        }
                    }
                    let can_save = self.data_editor.key_path.is_some();
                    if ui
                        .add_enabled(
                            can_save,
                            egui::Button::new(RichText::new("SAVE").monospace()),
                        )
                        .clicked()
                    {
                        if let Err(e) = self.data_editor.save() {
                            self.data_editor.status = format!("save failed: {e}");
                        }
                    }
                    if let Some(dir) = &self.project_dir {
                        ui.label(
                            RichText::new(dir.display().to_string())
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "no project loaded — pass --project <dir> when launching",
                            )
                            .color(TEXT_DIM)
                            .monospace(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(&self.data_editor.status)
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(8.0))
            .show(ctx, |ui| {
                crate::gui::data_editor::show(ui, &mut self.data_editor);
            });
    }

    fn show_sector(&mut self, ui: &mut egui::Ui) {
        let Some(sector) = self.sector.clone() else {
            return;
        };
        TopBottomPanel::bottom("sector_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.sector_hex_size, 20.0..=80.0)
                            .show_value(false),
                    );
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let (_resp, click) = SectorView {
                sector: &sector,
                selected_system: self.sector_selected.as_deref(),
                hex_size: self.sector_hex_size,
            }
            .show(ui);
            if let Some(c) = click {
                if self.sector_selected.as_deref() == Some(c.system_id.as_str()) {
                    self.view = View::System {
                        system_id: c.system_id,
                        selection: SystemSelection::None,
                    };
                } else {
                    self.sector_selected = Some(c.system_id);
                }
            }
        });
    }

    fn show_system(&mut self, ui: &mut egui::Ui, system_id: &str, selection: SystemSelection) {
        TopBottomPanel::bottom("system_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0)
                            .show_value(false),
                    );
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let sys_clone = self.system_by_id(system_id).cloned();
            let Some(sys) = sys_clone else {
                ui.label(RichText::new("system not found").color(Color32::RED));
                return;
            };
            let (_resp, click) = SystemView {
                system: &sys,
                selected: selection,
                side: self.system_side,
            }
            .show(ui);
            if let Some(c) = click {
                let new_sel = match c {
                    SystemClick::Star => SystemSelection::Star,
                    SystemClick::World(i) => SystemSelection::World(i),
                };
                self.view = View::System {
                    system_id: system_id.to_string(),
                    selection: new_sel,
                };
            }
        });
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = palette::PANEL_BG;
    visuals.window_fill = palette::PANEL_BG;
    visuals.extreme_bg_color = palette::BG;
    visuals.widgets.noninteractive.bg_fill = palette::PANEL_BG;
    visuals.widgets.inactive.bg_fill = palette::HEX_EMPTY;
    visuals.widgets.hovered.bg_fill = palette::HEX_OUTLINE;
    visuals.widgets.active.bg_fill = palette::SELECTION;
    visuals.override_text_color = Some(TEXT);
    ctx.set_visuals(visuals);
}
