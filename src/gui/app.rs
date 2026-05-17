//! Top-level eframe app: holds loaded sector + navigation state, dispatches
//! between sector view, system view, and edit view.

use std::path::PathBuf;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use crate::sector_model::{GeneratedSector, GeneratedSystem};

use super::data_editor::DataEditor;
use super::editor::{self, EditorState};
use super::info_panel;
use super::palette::{self, TEXT, TEXT_DIM};
use super::route_planner::{self, Metric, PickTarget, RoutePlannerState, Severity};
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
    planner: RoutePlannerState,
    planner_hex_size: f32,
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
    Planner,
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
            planner: RoutePlannerState::default(),
            planner_hex_size: 44.0,
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
            planner: RoutePlannerState::default(),
            planner_hex_size: 44.0,
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
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(8.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let on_sector = matches!(self.view, View::Sector);
                    let on_edit = matches!(self.view, View::Edit);
                    let on_data = matches!(self.view, View::Data);
                    let on_planner = matches!(self.view, View::Planner);
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
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_planner,
                                RichText::new("ROUTE PLANNER").color(TEXT).monospace(),
                            ),
                        )
                        .clicked()
                    {
                        self.view = View::Planner;
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
            View::System {
                system_id,
                selection,
            } => self.draw_system_layout(ctx, &system_id, selection),
            View::Edit => self.draw_edit_layout(ctx),
            View::Data => self.draw_data_layout(ctx),
            View::Planner => self.draw_planner_layout(ctx),
        }
    }
}

impl App {
    fn draw_sector_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("no sector loaded")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            return;
        };
        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
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
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
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
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                editor::editor_toolbar(ui, &mut self.editor);
            });

        SidePanel::right("edit_inspector")
            .resizable(true)
            .default_width(360.0)
            .min_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
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
                        egui::Slider::new(&mut self.editor.hex_size, 20.0..=80.0).show_value(false),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| match self.editor.tab {
                    editor::state::Tab::Map | editor::state::Tab::Routes => {
                        editor::show_map(ui, &mut self.editor)
                    }
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
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
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

    fn draw_planner_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("no sector loaded")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            return;
        };

        SidePanel::right("planner_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(280.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    self.draw_planner_panel(ui, &sector);
                });
            });

        TopBottomPanel::bottom("planner_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.planner_hex_size, 20.0..=80.0)
                            .show_value(false),
                    );
                    ui.label(
                        RichText::new("click hexes to set FROM, then TO  ·  third click resets")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| {
                    let route_ids = self.planner.highlighted_route_ids();
                    let waypoints = self.planner.waypoint_set();
                    let selected = self.planner.to.as_deref().or(self.planner.from.as_deref());
                    let (_resp, click) = SectorView {
                        sector: &sector,
                        selected_system: selected,
                        hex_size: self.planner_hex_size,
                        path_route_ids: Some(&route_ids),
                        path_waypoints: Some(&waypoints),
                    }
                    .show(ui);
                    if let Some(c) = click {
                        self.planner.click_system(&c.system_id);
                        self.recompute_plan();
                    }
                });
            });
    }

    fn draw_planner_panel(&mut self, ui: &mut egui::Ui, sector: &GeneratedSector) {
        ui.label(
            RichText::new("ROUTE PLANNER")
                .color(TEXT)
                .monospace()
                .strong(),
        );
        ui.add_space(6.0);

        let options: Vec<(String, String)> = sector
            .systems
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();

        let mut dirty = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("FROM").color(TEXT_DIM).monospace());
            let armed = self.planner.picker == PickTarget::From;
            if ui
                .selectable_label(armed, RichText::new("◎ PICK").monospace())
                .on_hover_text("arm picker — next map click sets FROM")
                .clicked()
            {
                self.planner.picker = if armed {
                    PickTarget::None
                } else {
                    PickTarget::From
                };
            }
        });
        dirty |= system_combo(ui, "planner_from", &mut self.planner.from, &options);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("TO").color(TEXT_DIM).monospace());
            let armed = self.planner.picker == PickTarget::To;
            if ui
                .selectable_label(armed, RichText::new("◎ PICK").monospace())
                .on_hover_text("arm picker — next map click sets TO")
                .clicked()
            {
                self.planner.picker = if armed {
                    PickTarget::None
                } else {
                    PickTarget::To
                };
            }
        });
        dirty |= system_combo(ui, "planner_to", &mut self.planner.to, &options);

        if self.planner.picker != PickTarget::None {
            ui.add_space(4.0);
            let target = match self.planner.picker {
                PickTarget::From => "FROM",
                PickTarget::To => "TO",
                PickTarget::None => "",
            };
            ui.label(
                RichText::new(format!("◉ click a hex to set {}", target))
                    .color(palette::PATH_WAYPOINT)
                    .monospace(),
            );
        }

        ui.add_space(6.0);
        ui.label(RichText::new("METRIC").color(TEXT_DIM).monospace());
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Safest,
                    RichText::new("SAFEST").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Safest;
                dirty = true;
            }
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Shortest,
                    RichText::new("SHORTEST").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Shortest;
                dirty = true;
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("PLAN").monospace()).clicked() {
                dirty = true;
            }
            if ui.button(RichText::new("CLEAR").monospace()).clicked() {
                self.planner.clear();
            }
            if let (Some(a), Some(b)) = (self.planner.from.clone(), self.planner.to.clone()) {
                if ui.button(RichText::new("SWAP").monospace()).clicked() {
                    self.planner.from = Some(b);
                    self.planner.to = Some(a);
                    dirty = true;
                }
            }
        });

        if dirty {
            self.recompute_plan();
        }

        if !self.planner.status.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(&self.planner.status)
                    .color(Color32::from_rgb(235, 90, 90))
                    .monospace(),
            );
        }

        ui.add_space(10.0);
        ui.separator();

        if let Some(plan) = self.planner.plan.clone() {
            let name_of = |id: &str| -> String {
                sector
                    .systems
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.to_string())
            };
            let metric_label = match plan.metric {
                Metric::Safest => "SAFEST",
                Metric::Shortest => "SHORTEST",
            };
            let cost_label = match plan.metric {
                Metric::Safest => format!("risk score {:.1}", plan.total_cost),
                Metric::Shortest => format!("{} hops", plan.total_cost as i64),
            };
            ui.label(
                RichText::new(format!("PATH ({}) — {}", metric_label, cost_label))
                    .color(TEXT)
                    .monospace()
                    .strong(),
            );
            ui.add_space(4.0);
            for (i, hop) in plan.hops.iter().enumerate() {
                let prefix = if i == 0 {
                    "◉"
                } else if i == plan.hops.len() - 1 {
                    "◎"
                } else {
                    "→"
                };
                ui.label(
                    RichText::new(format!("{} {}", prefix, name_of(hop)))
                        .color(TEXT)
                        .monospace(),
                );
            }

            ui.add_space(8.0);
            if plan.hazards.is_empty() {
                ui.label(
                    RichText::new("no hazards along path")
                        .color(palette::stability_color(
                            crate::sector_model::RouteStability::Stable,
                        ))
                        .monospace(),
                );
            } else {
                ui.label(RichText::new("HAZARDS").color(TEXT_DIM).monospace());
                for h in &plan.hazards {
                    let color = match h.severity {
                        Severity::Danger => Color32::from_rgb(235, 90, 90),
                        Severity::Caution => Color32::from_rgb(240, 200, 90),
                        Severity::Info => TEXT_DIM,
                    };
                    ui.label(
                        RichText::new(format!(
                            "{}  {} ↔ {}",
                            severity_tag(h.severity),
                            name_of(&h.from),
                            name_of(&h.to)
                        ))
                        .color(color)
                        .monospace(),
                    );
                    ui.label(
                        RichText::new(format!("   {}", h.note))
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                }
            }
        } else if self.planner.from.is_some() || self.planner.to.is_some() {
            ui.label(
                RichText::new("pick both endpoints to plan a route")
                    .color(TEXT_DIM)
                    .monospace(),
            );
        } else {
            ui.label(
                RichText::new("click a system hex to set the origin")
                    .color(TEXT_DIM)
                    .monospace(),
            );
        }
    }

    fn recompute_plan(&mut self) {
        self.planner.status.clear();
        self.planner.plan = None;
        let Some(sector) = &self.sector else { return };
        let (Some(from), Some(to)) = (self.planner.from.clone(), self.planner.to.clone()) else {
            return;
        };
        if from == to {
            self.planner.status = "origin and destination are the same".to_string();
            return;
        }
        match route_planner::plan_route(sector, &from, &to, self.planner.metric) {
            Some(p) => self.planner.plan = Some(p),
            None => {
                self.planner.status =
                    "no passable route — try the other metric or check for Lost lanes".to_string();
            }
        }
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
                        egui::Slider::new(&mut self.sector_hex_size, 20.0..=80.0).show_value(false),
                    );
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let (_resp, click) = SectorView {
                sector: &sector,
                selected_system: self.sector_selected.as_deref(),
                hex_size: self.sector_hex_size,
                path_route_ids: None,
                path_waypoints: None,
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
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0).show_value(false),
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

fn system_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    options: &[(String, String)],
) -> bool {
    let mut changed = false;
    let label = value
        .as_ref()
        .and_then(|sel| {
            options
                .iter()
                .find(|(oid, _)| oid == sel)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| "—".to_string());
    egui::ComboBox::from_id_salt(id)
        .selected_text(RichText::new(label).monospace())
        .width(220.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "— none —").clicked() {
                if value.is_some() {
                    *value = None;
                    changed = true;
                }
            }
            for (oid, name) in options {
                let sel = value.as_deref() == Some(oid.as_str());
                if ui
                    .selectable_label(sel, RichText::new(name).monospace())
                    .clicked()
                    && !sel
                {
                    *value = Some(oid.clone());
                    changed = true;
                }
            }
        });
    changed
}

fn severity_tag(s: Severity) -> &'static str {
    match s {
        Severity::Danger => "[!!]",
        Severity::Caution => "[!]",
        Severity::Info => "[·]",
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
