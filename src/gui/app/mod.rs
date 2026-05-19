//! Top-level eframe app: holds loaded sector + navigation state, dispatches
//! between sector view, system view, and edit view.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use crate::{
    sector_model::{GeneratedSector, GeneratedSystem},
    subsectors::{build_subsectors, Subsector, SubsectorConfig},
};

use super::dashboard::{self, DashboardState};
use super::data_editor::DataEditor;
use super::editor::{self, EditorState};
use super::heatmap::{self, HeatmapMode};
use super::info_panel;
use super::palette::{self, TEXT, TEXT_DIM};
use super::preset_gallery::{self, PresetGalleryState};
use super::route_planner::{self, Metric, PickTarget, RoutePlannerState, Severity};
use super::sector_view::{SectorClick, SectorView};
use super::segmentum_view::{self, SegmentumAction, SegmentumBundle};
use super::system_view::{SystemClick, SystemSelection, SystemView};

pub struct App {
    sector: Option<Arc<GeneratedSector>>,
    subsectors: Vec<Subsector>,
    view: View,
    sector_selected: Option<String>,
    sector_selected_subsector: Option<String>,
    sector_hex_size: f32,
    system_side: f32,
    editor: EditorState,
    data_editor: DataEditor,
    project_dir: Option<PathBuf>,
    planner: RoutePlannerState,
    planner_hex_size: f32,
    export_status: String,
    pending_export: Option<PendingExport>,
    sector_pick_export: bool,
    export_scale: u32,
    heatmap_mode: HeatmapMode,
    dashboard: DashboardState,
    preset_gallery: PresetGalleryState,
    segmentum: Option<Arc<SegmentumBundle>>,
    segmentum_active_child: Option<String>,
    segmentum_selected_link: Option<String>,
}

#[derive(Debug, Clone)]
enum PendingExport {
    SectorPng,
    AllSystemPngs,
    SystemPng(String),
    /// §11 NEW.md: self-contained interactive HTML map.
    SectorHtml,
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
    Dashboard,
    Relations,
    Regions,
    Trade,
    Segmentum,
}

impl Default for App {
    fn default() -> Self {
        Self {
            sector: None,
            subsectors: Vec::new(),
            view: View::Edit,
            sector_selected: None,
            sector_selected_subsector: None,
            sector_hex_size: 44.0,
            system_side: 700.0,
            editor: EditorState::default(),
            data_editor: DataEditor::default(),
            project_dir: None,
            planner: RoutePlannerState::default(),
            planner_hex_size: 44.0,
            export_status: String::new(),
            pending_export: None,
            sector_pick_export: false,
            export_scale: 2,
            heatmap_mode: HeatmapMode::Off,
            dashboard: DashboardState::default(),
            preset_gallery: PresetGalleryState::default(),
            segmentum: None,
            segmentum_active_child: None,
            segmentum_selected_link: None,
        }
    }
}

impl App {
    pub fn new(sector: GeneratedSector) -> Self {
        let mut app = Self::default();
        app.set_loaded_sector(sector, None);
        app.view = View::Sector;
        app
    }

    pub fn new_segmentum(bundle: SegmentumBundle) -> Self {
        let first_child = bundle.children.first().map(|c| c.id.clone());
        let mut app = Self {
            segmentum: Some(Arc::new(bundle)),
            view: View::Segmentum,
            ..Self::default()
        };
        if let Some(id) = first_child {
            app.set_active_segmentum_child(&id);
        }
        app.view = View::Segmentum;
        app
    }

    pub fn new_empty() -> Self {
        Self::default()
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

    fn set_loaded_sector(&mut self, sector: GeneratedSector, source_path: Option<String>) {
        self.subsectors = build_subsectors(&sector, SubsectorConfig::default()).unwrap_or_default();
        self.sector = Some(Arc::new(sector.clone()));
        self.sector_selected = None;
        self.sector_selected_subsector = None;
        self.planner.clear();
        self.dashboard.invalidate();
        if !self.editor.dirty {
            self.editor.set_sector(sector, source_path);
        }
    }

    fn set_active_segmentum_child(&mut self, child_id: &str) -> bool {
        let Some(bundle) = self.segmentum.clone() else {
            return false;
        };
        let Some(child) = bundle.child(child_id) else {
            self.export_status = format!("segmentum child '{}' not found", child_id);
            return false;
        };
        self.segmentum_active_child = Some(child_id.to_string());
        self.set_loaded_sector(
            child.sector.clone(),
            Some(child.sector_path.as_str().to_string()),
        );
        true
    }

    fn handle_segmentum_action(&mut self, action: SegmentumAction) {
        match action {
            SegmentumAction::OpenChild(child_id) => {
                if self.set_active_segmentum_child(&child_id) {
                    self.view = View::Sector;
                }
            }
            SegmentumAction::OpenSystem {
                child_id,
                system_id,
            } => {
                if self.set_active_segmentum_child(&child_id) {
                    self.sector_selected = Some(system_id.clone());
                    self.view = View::System {
                        system_id,
                        selection: SystemSelection::None,
                    };
                }
            }
        }
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
                    let has_segmentum = self.segmentum.is_some();
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
                    let on_segmentum = matches!(self.view, View::Segmentum);
                    if ui
                        .add_enabled(
                            has_segmentum,
                            egui::SelectableLabel::new(
                                on_segmentum,
                                RichText::new("SEGMENTUM").color(TEXT).monospace(),
                            ),
                        )
                        .clicked()
                    {
                        self.view = View::Segmentum;
                    }
                    if ui
                        .selectable_label(on_edit, RichText::new("EDIT").color(TEXT).monospace())
                        .clicked()
                    {
                        // Entering edit mode: copy current viewed sector into
                        // the editor if the editor is empty.
                        if self.editor.sector.is_none() {
                            if let Some(s) = self.sector.as_ref() {
                                self.editor.set_sector((**s).clone(), None);
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
                    let on_dashboard = matches!(self.view, View::Dashboard);
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_dashboard,
                                RichText::new("DASHBOARD").color(TEXT).monospace(),
                            ),
                        )
                        .on_hover_text("Analytics dashboard (§8 NEW.md)")
                        .clicked()
                    {
                        self.view = View::Dashboard;
                    }
                    let on_relations = matches!(self.view, View::Relations);
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_relations,
                                RichText::new("DIPLOMACY").color(TEXT).monospace(),
                            ),
                        )
                        .on_hover_text("Inter-faction stance matrix (§4 NEW.md)")
                        .clicked()
                    {
                        self.view = View::Relations;
                    }
                    let on_regions = matches!(self.view, View::Regions);
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_regions,
                                RichText::new("REGIONS").color(TEXT).monospace(),
                            ),
                        )
                        .on_hover_text("Regional warp phenomena overlay (§5 NEW.md)")
                        .clicked()
                    {
                        self.view = View::Regions;
                    }
                    let on_trade = matches!(self.view, View::Trade);
                    if ui
                        .add_enabled(
                            has_sector,
                            egui::SelectableLabel::new(
                                on_trade,
                                RichText::new("TRADE").color(TEXT).monospace(),
                            ),
                        )
                        .on_hover_text("Economy & trade volumes (§12 NEW.md)")
                        .clicked()
                    {
                        self.view = View::Trade;
                    }
                    if ui
                        .button(RichText::new("NEW…").color(TEXT).monospace())
                        .on_hover_text("Scaffold a fresh project from a preset (§9 NEW.md)")
                        .clicked()
                    {
                        self.preset_gallery.open = !self.preset_gallery.open;
                    }
                    if let Some(bundle) = self.segmentum.clone() {
                        ui.menu_button(RichText::new("CHILD ▾").color(TEXT).monospace(), |ui| {
                            for child in &bundle.children {
                                let active = self.segmentum_active_child.as_deref()
                                    == Some(child.id.as_str());
                                let label = format!(
                                    "{}  {} sys",
                                    child.id.to_uppercase(),
                                    child.sector.systems.len()
                                );
                                if ui
                                    .selectable_label(active, RichText::new(label).monospace())
                                    .clicked()
                                {
                                    ui.close_menu();
                                    if self.set_active_segmentum_child(&child.id) {
                                        self.view = View::Sector;
                                    }
                                }
                            }
                        });
                    }
                    let systems_list: Vec<(String, String)> = self
                        .sector
                        .as_ref()
                        .map(|s| {
                            s.systems
                                .iter()
                                .map(|x| (x.id.clone(), x.name.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    ui.add_enabled_ui(has_sector, |ui| {
                        ui.menu_button(RichText::new("EXPORT ▾").color(TEXT).monospace(), |ui| {
                            if ui
                                .button(RichText::new("Bundle (JSON / MD / CSV)").monospace())
                                .on_hover_text(
                                    "Save all JSONs, markdown, CSVs, and a copy of the data \
                                         folder (no images) to a folder",
                                )
                                .clicked()
                            {
                                ui.close_menu();
                                self.export_sector_json(ctx);
                            }
                            if ui
                                .button(RichText::new("Sector Map PNG").monospace())
                                .clicked()
                            {
                                ui.close_menu();
                                self.pending_export = Some(PendingExport::SectorPng);
                            }
                            if ui
                                .button(RichText::new("All System Maps PNG").monospace())
                                .clicked()
                            {
                                ui.close_menu();
                                self.pending_export = Some(PendingExport::AllSystemPngs);
                            }
                            if ui
                                .button(RichText::new("Interactive HTML").monospace())
                                .clicked()
                            {
                                ui.close_menu();
                                self.pending_export = Some(PendingExport::SectorHtml);
                            }
                            ui.menu_button(
                                RichText::new("Single System Map PNG ▸").monospace(),
                                |ui| {
                                    if ui
                                        .button(RichText::new("Pick from sector map").monospace())
                                        .clicked()
                                    {
                                        ui.close_menu();
                                        self.sector_pick_export = true;
                                        self.view = View::Sector;
                                    }
                                    ui.separator();
                                    ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                                        for (id, name) in &systems_list {
                                            if ui.button(RichText::new(name).monospace()).clicked()
                                            {
                                                ui.close_menu();
                                                self.pending_export =
                                                    Some(PendingExport::SystemPng(id.clone()));
                                            }
                                        }
                                    });
                                },
                            );
                        });
                    });
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
                            let prefix = self
                                .segmentum_active_child
                                .as_ref()
                                .map(|id| format!("{} / ", id.to_uppercase()))
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(format!(
                                    "{}{} - {} sys, {} worlds",
                                    prefix,
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
                        if !self.export_status.is_empty() {
                            ui.label(
                                RichText::new(&self.export_status)
                                    .color(egui::Color32::from_rgb(235, 200, 90))
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
            View::Dashboard => self.draw_dashboard_layout(ctx),
            View::Relations => self.draw_relations_layout(ctx),
            View::Regions => self.draw_regions_layout(ctx),
            View::Trade => self.draw_trade_layout(ctx),
            View::Segmentum => self.draw_segmentum_layout(ctx),
        }

        self.draw_preset_gallery(ctx);
        self.draw_export_dialog(ctx);
    }
}

impl App {
    fn draw_segmentum_layout(&mut self, ctx: &egui::Context) {
        let Some(bundle) = self.segmentum.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("no segmentum loaded")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            return;
        };

        let mut action = None;
        SidePanel::right("segmentum_info")
            .resizable(true)
            .default_width(380.0)
            .min_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    let next = segmentum_view::show_side_panel(
                        ui,
                        &bundle,
                        self.segmentum_active_child.as_deref(),
                        &mut self.segmentum_selected_link,
                    );
                    if action.is_none() {
                        action = next;
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| {
                    let next = segmentum_view::show_overview(
                        ui,
                        &bundle,
                        self.segmentum_active_child.as_deref(),
                        &mut self.segmentum_selected_link,
                    );
                    if action.is_none() {
                        action = next;
                    }
                });
            });

        if let Some(action) = action {
            self.handle_segmentum_action(action);
        }
    }

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
                        if let (Some(sys), Some(sector)) =
                            (self.system_by_id(sel), self.sector.as_ref())
                        {
                            info_panel::system_summary(ui, sys, sector);
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

        self.draw_subsector_popup(ctx, &sector);
    }

    fn draw_subsector_popup(&mut self, ctx: &egui::Context, sector: &GeneratedSector) {
        let Some(sub_id) = self.sector_selected_subsector.clone() else {
            return;
        };
        let Some(sub) = self.subsectors.iter().find(|s| s.id == sub_id).cloned() else {
            self.sector_selected_subsector = None;
            return;
        };
        let mut open = true;
        let title = format!("SUBSECTOR {} - {}", sub.label, sub.name);
        egui::Window::new(RichText::new(&title).monospace().strong())
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(340.0)
            .default_height(520.0)
            .anchor(egui::Align2::RIGHT_TOP, [-360.0, 60.0])
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    info_panel::subsector_summary(ui, &sub, sector);
                });
            });
        if !open {
            self.sector_selected_subsector = None;
        }
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
                        let sector = self.sector.as_ref().expect("sector loaded");
                        info_panel::system_summary(ui, sys, sector);
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

    fn draw_preset_gallery(&mut self, ctx: &egui::Context) {
        if !self.preset_gallery.open {
            return;
        }
        let mut open = true;
        egui::Window::new(
            RichText::new("NEW PROJECT FROM PRESET")
                .monospace()
                .strong(),
        )
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(620.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            preset_gallery::show(ui, &mut self.preset_gallery);
        });
        if !open {
            self.preset_gallery.open = false;
        }
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

    fn draw_dashboard_layout(&mut self, ctx: &egui::Context) {
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
        TopBottomPanel::top("dashboard_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("RECOMPUTE").monospace())
                        .on_hover_text("re-run analytics on the current sector")
                        .clicked()
                    {
                        self.dashboard.invalidate();
                    }
                    ui.label(
                        RichText::new("§8 NEW.md — analytics dashboard")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    dashboard::show(ui, &sector, &mut self.dashboard);
                });
            });
    }

    fn draw_relations_layout(&mut self, ctx: &egui::Context) {
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
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("DIPLOMACY MATRIX")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                ui.label(
                    RichText::new("§4 NEW.md — inter-faction stance + tension")
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.add_space(8.0);
                if sector.relations.pairs.is_empty() {
                    ui.label(
                        RichText::new(
                            "relations matrix is empty (need ≥2 factions or set \
                            inputs.relations in sectorforge.toml)",
                        )
                        .color(TEXT_DIM)
                        .monospace(),
                    );
                    return;
                }
                // Fixed column widths so virtualized rows align with the
                // header without a Grid (Grid is incompatible with
                // ScrollArea::show_rows virtualization).
                const COL_A: f32 = 220.0;
                const COL_B: f32 = 220.0;
                const COL_STANCE: f32 = 90.0;
                const COL_TENSION: f32 = 70.0;
                let header = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [COL_A, 0.0],
                            egui::Label::new(
                                RichText::new("A").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_B, 0.0],
                            egui::Label::new(
                                RichText::new("B").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_STANCE, 0.0],
                            egui::Label::new(
                                RichText::new("STANCE").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_TENSION, 0.0],
                            egui::Label::new(
                                RichText::new("TENSION")
                                    .color(TEXT_DIM)
                                    .monospace()
                                    .strong(),
                            ),
                        );
                        ui.label(RichText::new("CAUSE").color(TEXT_DIM).monospace().strong());
                    });
                };
                header(ui);
                ui.separator();
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
                let total = sector.relations.pairs.len();
                ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
                    ui,
                    row_h,
                    total,
                    |ui, range| {
                        for i in range {
                            let p = &sector.relations.pairs[i];
                            let stance_color = stance_color(p.stance);
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [COL_A, row_h],
                                    egui::Label::new(RichText::new(&p.a).monospace()),
                                );
                                ui.add_sized(
                                    [COL_B, row_h],
                                    egui::Label::new(RichText::new(&p.b).monospace()),
                                );
                                ui.add_sized(
                                    [COL_STANCE, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:?}", p.stance))
                                            .color(stance_color)
                                            .monospace(),
                                    ),
                                );
                                ui.add_sized(
                                    [COL_TENSION, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:.0}", p.tension)).monospace(),
                                    ),
                                );
                                ui.label(RichText::new(&p.cause).color(TEXT_DIM).monospace());
                            });
                        }
                    },
                );
            });
    }

    fn draw_regions_layout(&mut self, ctx: &egui::Context) {
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
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(
                        RichText::new("WARP REGIONS")
                            .color(TEXT)
                            .monospace()
                            .strong(),
                    );
                    ui.label(
                        RichText::new("§5 NEW.md — regional warp phenomena overlay")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    if sector.regions.is_empty() {
                        ui.label(
                            RichText::new(
                                "no regions configured — enable in regions.toml or \
                                sectorforge.toml",
                            )
                            .color(TEXT_DIM)
                            .monospace(),
                        );
                        return;
                    }
                    egui::Grid::new("regions_grid")
                        .num_columns(5)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("ID").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("NAME").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("KIND").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("HEXES").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("CENTRE").color(TEXT_DIM).monospace().strong());
                            ui.end_row();
                            for r in &sector.regions {
                                ui.label(RichText::new(&r.id).monospace());
                                ui.label(RichText::new(&r.name).monospace());
                                ui.label(
                                    RichText::new(format!("{:?}", r.kind))
                                        .color(Color32::from_rgb(220, 160, 60))
                                        .monospace(),
                                );
                                ui.label(RichText::new(r.hexes.len().to_string()).monospace());
                                ui.label(
                                    RichText::new(format!("({},{})", r.centre.q, r.centre.r))
                                        .monospace(),
                                );
                                ui.end_row();
                            }
                        });
                });
            });
    }

    fn draw_trade_layout(&mut self, ctx: &egui::Context) {
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
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(
                        RichText::new("TRADE & ECONOMY")
                            .color(TEXT)
                            .monospace()
                            .strong(),
                    );
                    ui.label(
                        RichText::new("§12 NEW.md — trade volume + resource balance")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    if !sector.economy.enabled {
                        ui.label(
                            RichText::new(
                                "economy derivation disabled — set [economy].enabled = true",
                            )
                            .color(TEXT_DIM)
                            .monospace(),
                        );
                        return;
                    }
                    ui.label(
                        RichText::new("SECTOR BALANCE")
                            .color(TEXT)
                            .monospace()
                            .strong(),
                    );
                    egui::Grid::new("sector_balance")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for k in crate::economy::RESOURCE_KEYS {
                                let v = sector.economy.sector_balance.get(k);
                                ui.label(RichText::new(*k).color(TEXT_DIM).monospace());
                                ui.label(RichText::new(format!("{:.1}", v)).monospace());
                                ui.end_row();
                            }
                        });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("STRATEGIC OUTPUT")
                            .color(TEXT)
                            .monospace()
                            .strong(),
                    );
                    egui::Grid::new("strategic_output")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for k in crate::economy::STRATEGIC_RESOURCE_KEYS {
                                let v = sector.economy.strategic_output.get(k);
                                ui.label(RichText::new(*k).color(TEXT_DIM).monospace());
                                ui.label(RichText::new(format!("{:.1}", v)).monospace());
                                ui.end_row();
                            }
                        });
                    let stressed: Vec<_> = sector
                        .economy
                        .systems
                        .iter()
                        .filter(|s| {
                            s.supply_risk >= crate::economy::SupplyRisk::Disrupted
                                || matches!(
                                    s.tithe_status,
                                    crate::economy::TitheStatus::Delinquent
                                        | crate::economy::TitheStatus::Failed
                                        | crate::economy::TitheStatus::Falsified
                                )
                        })
                        .collect();
                    if !stressed.is_empty() {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("TITHE / SUPPLY STRESS")
                                .color(Color32::from_rgb(235, 190, 90))
                                .monospace()
                                .strong(),
                        );
                        egui::Grid::new("tithe_supply_stress")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(RichText::new("SYSTEM").color(TEXT_DIM).monospace());
                                ui.label(RichText::new("TITHE").color(TEXT_DIM).monospace());
                                ui.label(RichText::new("SUPPLY").color(TEXT_DIM).monospace());
                                ui.label(RichText::new("PRIORITY").color(TEXT_DIM).monospace());
                                ui.end_row();
                                for sy in stressed.iter().take(12) {
                                    ui.label(RichText::new(&sy.system_id).monospace());
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.tithe_status)).monospace(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.supply_risk)).monospace(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.strategic_priority))
                                            .monospace(),
                                    );
                                    ui.end_row();
                                }
                            });
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("TOP TRADE LANES")
                            .color(TEXT)
                            .monospace()
                            .strong(),
                    );
                    let mut routes: Vec<_> = sector.economy.routes.iter().collect();
                    routes.sort_by(|a, b| {
                        b.volume
                            .partial_cmp(&a.volume)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    egui::Grid::new("trade_routes")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("FROM").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("TO").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("VOLUME").color(TEXT_DIM).monospace().strong());
                            ui.label(
                                RichText::new("FRICTION")
                                    .color(TEXT_DIM)
                                    .monospace()
                                    .strong(),
                            );
                            ui.end_row();
                            for r in routes.iter().take(20) {
                                ui.label(RichText::new(&r.from_system_id).monospace());
                                ui.label(RichText::new(&r.to_system_id).monospace());
                                ui.label(RichText::new(format!("{:.1}", r.volume)).monospace());
                                ui.label(RichText::new(format!("{:.2}", r.friction)).monospace());
                                ui.end_row();
                            }
                        });
                    let stranded: Vec<_> = sector
                        .economy
                        .worlds
                        .iter()
                        .filter(|w| w.stranded)
                        .collect();
                    if !stranded.is_empty() {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("STRANDED WORLDS")
                                .color(Color32::from_rgb(235, 90, 90))
                                .monospace()
                                .strong(),
                        );
                        for w in stranded {
                            ui.label(
                                RichText::new(format!(
                                    "{} in {} — {}",
                                    w.world_id,
                                    w.system_id,
                                    if w.shortages.is_empty() {
                                        "(systemic)".to_string()
                                    } else {
                                        w.shortages.join(", ")
                                    }
                                ))
                                .monospace(),
                            );
                        }
                    }
                });
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
                        subsectors: None,
                        selected_subsector: None,
                        heatmap: None,
                    }
                    .show(ui);
                    if let Some(SectorClick::System(id)) = click {
                        self.planner.click_system(&id);
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
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Strategic,
                    RichText::new("STRATEGIC").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Strategic;
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
                Metric::Strategic => "STRATEGIC",
            };
            let cost_label = match plan.metric {
                Metric::Safest => format!("risk score {:.1}", plan.total_cost),
                Metric::Shortest => format!("{} hops", plan.total_cost as i64),
                Metric::Strategic => format!("strategic cost {:.1}", plan.total_cost),
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
                    "no passable route — try the other metric or check for Perilous lanes"
                        .to_string();
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
                    ui.separator();
                    ui.label(RichText::new("HEATMAP").color(TEXT_DIM).monospace());
                    egui::ComboBox::from_id_salt("sector_heatmap")
                        .selected_text(
                            RichText::new(self.heatmap_mode.label())
                                .monospace()
                                .color(TEXT),
                        )
                        .show_ui(ui, |ui| {
                            for &m in HeatmapMode::ALL {
                                let sel = m == self.heatmap_mode;
                                if ui
                                    .selectable_label(sel, RichText::new(m.label()).monospace())
                                    .clicked()
                                    && !sel
                                {
                                    self.heatmap_mode = m;
                                }
                            }
                        });
                    if self.sector_pick_export {
                        ui.label(
                            RichText::new("◉ click a system hex to pick for PNG export")
                                .color(Color32::from_rgb(235, 200, 90))
                                .monospace(),
                        );
                        if ui
                            .button(RichText::new("CANCEL PICK").monospace())
                            .clicked()
                        {
                            self.sector_pick_export = false;
                        }
                    }
                });
            });
        let heatmap = if matches!(self.heatmap_mode, HeatmapMode::Off) {
            None
        } else {
            Some(heatmap::compute(&sector, self.heatmap_mode))
        };
        ScrollArea::both().show(ui, |ui| {
            let (_resp, click) = SectorView {
                sector: &sector,
                selected_system: self.sector_selected.as_deref(),
                hex_size: self.sector_hex_size,
                path_route_ids: None,
                path_waypoints: None,
                subsectors: Some(self.subsectors.as_slice()),
                selected_subsector: self.sector_selected_subsector.as_deref(),
                heatmap: heatmap.as_ref(),
            }
            .show(ui);
            match click {
                Some(SectorClick::System(id)) => {
                    if self.sector_pick_export {
                        self.sector_pick_export = false;
                        self.pending_export = Some(PendingExport::SystemPng(id));
                    } else if self.sector_selected.as_deref() == Some(id.as_str()) {
                        self.view = View::System {
                            system_id: id,
                            selection: SystemSelection::None,
                        };
                    } else {
                        self.sector_selected = Some(id);
                        self.sector_selected_subsector = None;
                    }
                }
                Some(SectorClick::Subsector(id)) => {
                    if self.sector_pick_export {
                        // empty hexes are not valid export targets
                    } else if self.sector_selected_subsector.as_deref() == Some(id.as_str()) {
                        self.sector_selected_subsector = None;
                    } else {
                        self.sector_selected_subsector = Some(id);
                        self.sector_selected = None;
                    }
                }
                None => {}
            }
        });
    }

    fn show_system(&mut self, ui: &mut egui::Ui, system_id: &str, selection: SystemSelection) {
        let sys_id_owned = system_id.to_string();
        TopBottomPanel::bottom("system_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0).show_value(false),
                    );
                    if ui
                        .button(RichText::new("EXPORT MAP PNG").monospace())
                        .on_hover_text("export this system's map to a PNG")
                        .clicked()
                    {
                        self.pending_export = Some(PendingExport::SystemPng(sys_id_owned));
                    }
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

mod export_ui;

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
            if ui.selectable_label(value.is_none(), "— none —").clicked() && value.is_some() {
                *value = None;
                changed = true;
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

fn stance_color(s: crate::relations::Stance) -> Color32 {
    use crate::relations::Stance;
    match s {
        Stance::Allied => Color32::from_rgb(90, 200, 110),
        Stance::Aligned => Color32::from_rgb(160, 220, 140),
        Stance::Neutral => Color32::from_rgb(190, 190, 190),
        Stance::Rival => Color32::from_rgb(240, 200, 90),
        Stance::Hostile => Color32::from_rgb(235, 130, 60),
        Stance::AtWar => Color32::from_rgb(235, 90, 90),
    }
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
