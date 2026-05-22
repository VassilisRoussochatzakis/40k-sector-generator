//! Main GUI application entry point and top-level layout router.
//!
//! §1.1 NEW.md: Unified sector/system view with feature tabs.
//! §15.2 GUIDE.md: App architecture and state management.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::gui::dashboard::DashboardState;
use crate::gui::data_editor::DataEditor;
use crate::gui::editor::state::EditorState;
use crate::gui::heatmap::{HeatmapCache, HeatmapMode};
use crate::gui::preset_gallery::PresetGalleryState;
use crate::gui::route_planner::RoutePlannerState;
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::{dashboard, editor, factions_overview, info_panel, palette, preset_gallery};
use crate::gui::segmentum_view::SegmentumBundle;

mod types;
pub use types::*;

mod analytics_views;
mod editor_views;
mod lifecycle;
mod other_views;
mod planner_view;
mod sector_view;
mod segmentum;
mod system_view;
mod ui_helpers;
mod layout;

mod export_ui;
mod factions_view;
mod regions_view;
mod relations_view;
mod trade_view;


pub const TEXT: egui::Color32 = palette::TEXT;
pub const TEXT_DIM: egui::Color32 = palette::TEXT_DIM;

pub struct App {
    pub(super) sector: Option<Arc<GeneratedSector>>,
    pub(super) sector_source_path: Option<PathBuf>,
    pub(super) live_dirty: bool,
    pub(super) subsectors: Vec<Subsector>,
    pub(super) view: View,
    pub(super) sector_selected: Option<crate::ids::SystemId>,
    pub(super) sector_selected_route: Option<crate::ids::RouteId>,
    pub(super) sector_selected_subsector: Option<std::sync::Arc<str>>,
    pub(super) map_edit_mode: bool,
    pub(super) sector_edit_tool: SectorEditTool,
    pub(super) pending_route_start: Option<crate::ids::SystemId>,
    pub(super) sector_hex_size: f32,
    pub(super) system_side: f32,
    pub(super) editor: EditorState,
    pub(super) data_editor: DataEditor,
    pub(super) project_dir: Option<PathBuf>,
    pub(super) planner: RoutePlannerState,
    pub(super) planner_hex_size: f32,
    pub(super) export_status: String,
    pub(super) pending_export: Option<PendingExport>,
    pub(super) sector_pick_export: bool,
    pub(super) export_scale: u32,
    pub(super) export_theme_name: String,
    pub(super) heatmap_mode: HeatmapMode,
    pub(super) heatmap_cache: HeatmapCache,
    pub(super) sector_overview_cache: info_panel::SectorOverviewCache,
    pub(super) dashboard: DashboardState,
    pub(super) preset_gallery: PresetGalleryState,
    pub(super) segmentum: Option<Arc<SegmentumBundle>>,
    pub(super) segmentum_active_child: Option<std::sync::Arc<str>>,
    pub(super) segmentum_selected_link: Option<std::sync::Arc<str>>,
    pub(super) factions_mode: FactionsMode,
    pub(super) faction_designer: factions_overview::FactionDesignerState,
    pub(super) history_selected_event: Option<std::sync::Arc<str>>,
    pub route_view_mode: crate::sector_model::RouteViewMode,
}

impl Default for App {
    fn default() -> Self {
        Self {
            sector: None,
            sector_source_path: None,
            live_dirty: false,
            subsectors: Vec::new(),
            view: View::Sector,
            sector_selected: None,
            sector_selected_route: None,
            sector_selected_subsector: None,
            map_edit_mode: false,
            sector_edit_tool: SectorEditTool::Select,
            pending_route_start: None,
            sector_hex_size: 40.0,
            system_side: 800.0,
            editor: EditorState::default(),
            data_editor: DataEditor::default(),
            project_dir: None,
            planner: RoutePlannerState::default(),
            planner_hex_size: 40.0,
            export_status: String::new(),
            pending_export: None,
            sector_pick_export: false,
            export_scale: 2,
            export_theme_name: "gm_dark".into(),
            heatmap_mode: HeatmapMode::Off,
            heatmap_cache: HeatmapCache::default(),
            sector_overview_cache: info_panel::SectorOverviewCache::default(),
            dashboard: DashboardState::default(),
            preset_gallery: PresetGalleryState::default(),
            segmentum: None,
            segmentum_active_child: None,
            segmentum_selected_link: None,
            factions_mode: FactionsMode::Overview,
            faction_designer: factions_overview::FactionDesignerState::default(),
            history_selected_event: None,
            route_view_mode: crate::sector_model::RouteViewMode::Detailed,
        }
    }
}

impl App {
    pub fn new(sector: GeneratedSector) -> Self {
        let mut app = Self::default();
        app.set_loaded_sector(sector, None);
        app
    }

    pub fn new_with_source(sector: GeneratedSector, source_path: PathBuf) -> Self {
        let mut app = Self::default();
        app.set_loaded_sector(sector, Some(source_path.to_string_lossy().to_string()));
        app
    }

    pub fn new_segmentum(bundle: SegmentumBundle) -> Self {
        let mut app = Self::default();
        app.segmentum = Some(Arc::new(bundle));
        app.view = View::Segmentum;
        app
    }

    pub fn new_empty() -> Self {
        Self::default()
    }

    pub fn with_project_dir(mut self, dir: PathBuf) -> Self {
        self.project_dir = Some(dir.clone());
        if let Err(e) = self.data_editor.load_from_project(&dir) {
            self.data_editor.status = format!("load failed: {e}");
        }
        self
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ui_helpers::apply_theme(ctx);

        layout::TopBar::new(self).show(ctx);
        layout::MainView::new(self).show(ctx);

        self.draw_preset_gallery(ctx);
        self.draw_export_dialog(ctx);
    }
}

use rfd::FileDialog;

impl App {
    fn open_sector_dialog(&mut self) {
        let mut dialog = FileDialog::new().add_filter("Sector JSON", &["json"]);
        if let Some(dir) = self
            .sector_source_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| self.project_dir.as_ref().map(|p| p.join("out")))
        {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            match fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<GeneratedSector>(text.as_str()) {
                    Ok(sector) => {
                        self.set_loaded_sector(sector, Some(path.to_string_lossy().to_string()));
                    }
                    Err(e) => {
                        self.export_status = format!("load failed: parse error: {e}");
                    }
                },
                Err(e) => {
                    self.export_status = format!("load failed: read error: {e}");
                }
            }
        }
    }
}
