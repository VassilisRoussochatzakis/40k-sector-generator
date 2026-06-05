//! Main GUI application entry point and top-level layout router.
//!
//! §1.1 NEW.md: Unified sector/system view with feature tabs.
//! §15.2 GUIDE.md: App architecture and state management.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::dashboard::DashboardState;
use crate::data_editor::DataEditor;
use crate::editor::state::EditorState;
use crate::heatmap::HeatmapCache;
use crate::preset_gallery::PresetGalleryState;
use crate::route_planner::RoutePlannerState;
use crate::segmentum_view::SegmentumBundle;
use camino::Utf8PathBuf;
use sectorforge::heatmap::HeatmapMode;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::subsectors::Subsector;

use super::{dashboard, editor, factions_overview, info_panel, palette, preset_gallery};

mod types;
pub use types::*;

mod analytics_views;
mod editor_views;
mod layout;
mod lifecycle;
mod planner_view;
mod sector_view;
mod segmentum;
mod system_view;

mod export_ui;
mod factions_view;
mod regions_view;
mod relations_view;
mod trade_view;

pub struct App {
    /// Derived read snapshot of the editor's source-of-truth sector (F-S1).
    /// Rebuilt only by the frame bridge / explicit load — never mutated in place.
    /// Consumed by the App-level read views + the live map render; kept as an
    /// `Arc` so those reads clone cheaply into render closures.
    pub(super) sector: Option<Arc<GeneratedSector>>,
    pub(super) sector_source_path: Option<PathBuf>,
    /// Last `editor.revision` the derived `sector` snapshot was rebuilt from.
    pub(super) synced_revision: u64,
    pub(super) subsectors: Vec<Subsector>,
    pub(super) view: View,
    pub(super) sector_selected: Option<sectorforge::ids::SystemId>,
    pub(super) sector_selected_route: Option<sectorforge::ids::RouteId>,
    pub(super) sector_selected_subsector: Option<std::sync::Arc<str>>,
    pub(super) sector_selected_region: Option<String>,
    pub(super) map_edit_mode: bool,
    pub(super) pending_route_start: Option<sectorforge::ids::SystemId>,
    pub(super) sector_hex_size: f32,
    pub(super) sector_pan: egui::Vec2,
    pub(super) info_panel_open: bool,
    pub(super) system_side: f32,
    pub(super) system_layout: crate::system_view::SystemLayout,
    pub(super) editor: EditorState,
    pub(super) data_editor: DataEditor,
    pub(super) project_dir: Option<PathBuf>,
    pub(super) planner: RoutePlannerState,
    pub(super) planner_hex_size: f32,
    pub(super) planner_pan: egui::Vec2,
    pub(super) export_status: String,
    pub(super) export_job: Option<crate::jobs::JobHandle<ExportJobResult>>,
    pub(super) export_job_revision: u64,
    pub(super) pending_export: Option<PendingExport>,
    pub(super) sector_pick_export: bool,
    pub(super) export_scale: u32,
    pub(super) export_theme_name: String,
    pub(super) heatmap_mode: HeatmapMode,
    pub(super) heatmap_cache: HeatmapCache,
    pub(super) sector_map_cache: Option<crate::sector_view::SectorMapCache>,
    pub(super) sector_overview_cache: info_panel::SectorOverviewCache,
    pub(super) dashboard: DashboardState,
    pub(super) preset_gallery: PresetGalleryState,
    pub(super) segmentum: Option<Arc<SegmentumBundle>>,
    pub(super) segmentum_active_child: Option<std::sync::Arc<str>>,
    pub(super) segmentum_selected_link: Option<std::sync::Arc<str>>,
    pub(super) factions_mode: FactionsMode,
    pub(super) faction_designer: factions_overview::FactionDesignerState,
    pub(super) history_selected_event: Option<std::sync::Arc<str>>,
    pub(super) history_snapshots: Vec<(String, GeneratedSector)>,
    pub route_view_mode: sectorforge::sector_model::RouteViewMode,
    pub(super) theme: crate::theme::Theme,
    pub(super) applied_theme: Option<crate::theme::Theme>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            sector: None,
            sector_source_path: None,
            synced_revision: 0,
            subsectors: Vec::new(),
            view: View::Sector,
            sector_selected: None,
            sector_selected_route: None,
            sector_selected_subsector: None,
            sector_selected_region: None,
            map_edit_mode: false,
            pending_route_start: None,
            sector_hex_size: 40.0,
            sector_pan: egui::Vec2::ZERO,
            info_panel_open: true,
            system_side: 800.0,
            system_layout: crate::system_view::SystemLayout::default(),
            editor: EditorState::default(),
            data_editor: DataEditor::default(),
            project_dir: None,
            planner: RoutePlannerState::default(),
            planner_hex_size: 40.0,
            planner_pan: egui::Vec2::ZERO,
            export_status: String::new(),
            export_job: None,
            export_job_revision: 0,
            pending_export: None,
            sector_pick_export: false,
            export_scale: 2,
            export_theme_name: "gm_dark".into(),
            heatmap_mode: HeatmapMode::Off,
            heatmap_cache: HeatmapCache::default(),
            sector_map_cache: None,
            sector_overview_cache: info_panel::SectorOverviewCache::default(),
            dashboard: DashboardState::default(),
            preset_gallery: PresetGalleryState::default(),
            segmentum: None,
            segmentum_active_child: None,
            segmentum_selected_link: None,
            factions_mode: FactionsMode::Overview,
            faction_designer: factions_overview::FactionDesignerState::default(),
            history_selected_event: None,
            history_snapshots: Vec::new(),
            route_view_mode: sectorforge::sector_model::RouteViewMode::Detailed,
            theme: crate::theme::Theme::default(),
            applied_theme: None,
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
        Self {
            segmentum: Some(Arc::new(bundle)),
            view: View::Segmentum,
            ..Self::default()
        }
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
        if self.applied_theme != Some(self.theme) {
            self.theme.apply(ctx);
            self.applied_theme = Some(self.theme);
        }

        self.handle_preview_logic(ctx);
        self.handle_export_job();

        layout::TopBar::new(self).show(ctx);
        layout::MainView::new(self).show(ctx);

        self.draw_preset_gallery(ctx);
        self.draw_export_dialog(ctx);

        self.sync_derived_sector();
    }
}

impl App {
    /// Re-derive the App read snapshot (`self.sector`) + display subsectors and
    /// caches from the editor's source-of-truth sector whenever it advances
    /// (F-S1). Gated on the revision counter so an idle unsaved sector isn't
    /// deep-cloned every frame; auto-saves the source-of-truth on the change frame
    /// if enabled. Runs once per `update`; extracted so it is unit-testable
    /// without an egui context.
    pub(super) fn sync_derived_sector(&mut self) {
        if self.editor.sector.is_none() || self.editor.revision == self.synced_revision {
            return;
        }
        self.synced_revision = self.editor.revision;
        if let Some(sec) = &self.editor.sector {
            self.sector = Some(Arc::new(sec.clone()));
            let (subs, sub_err) = Self::build_display_subsectors(sec);
            self.subsectors = subs;
            if let Some(e) = sub_err {
                self.export_status = e;
            }
            self.sector_map_cache = Some(crate::sector_view::SectorMapCache::new(
                sec,
                &self.subsectors,
            ));
            self.dashboard.invalidate();
            self.heatmap_cache.invalidate();
            self.sector_overview_cache.invalidate();
        }

        // Auto-save the source-of-truth if enabled.
        if self.editor.auto_save && self.editor.dirty {
            if let Some(path_str) = self.editor.loaded_from.clone() {
                let path = PathBuf::from(path_str);
                match self.editor.sector.as_ref().map(serde_json::to_string_pretty) {
                    Some(Ok(text)) => match fs::write(&path, text) {
                        Ok(()) => {
                            self.editor.dirty = false;
                        }
                        Err(e) => {
                            self.export_status = format!("auto-save failed: {e}");
                        }
                    },
                    Some(Err(e)) => {
                        self.export_status = format!("auto-save serialize failed: {e}");
                    }
                    None => {}
                }
            }
        }
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
            let utf8_path = match Utf8PathBuf::from_path_buf(path.clone()) {
                Ok(p) => p,
                Err(orig) => {
                    self.export_status = format!("path is not UTF-8: {}", orig.display());
                    return;
                }
            };
            match sectorforge::load_sector_json(&utf8_path) {
                Ok(sector) => {
                    self.set_loaded_sector(sector, Some(path.to_string_lossy().to_string()));
                }
                Err(e) => {
                    self.export_status = format!("load failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::App;

    /// F-S1: `editor.sector` is the single source of truth; `App.sector` is a
    /// derived snapshot the bridge rebuilds only when the editor revision advances.
    #[test]
    fn editor_sector_is_single_source_of_truth() {
        use sectorforge::ids::system_id;
        use sectorforge::sector_model::{empty_sector, empty_system, HexCoord, SystemKind};
        use std::sync::Arc;

        // Loading seeds the derived snapshot and leaves the bridge in sync.
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        assert!(app.sector.is_some());
        assert_eq!(app.synced_revision, app.editor.revision);
        assert_eq!(app.sector.as_ref().unwrap().systems.len(), 0);
        let rev0 = app.editor.revision;

        // Mutating the source of truth bumps the revision but leaves the derived
        // snapshot untouched until the bridge runs.
        let sys = empty_system(
            system_id(1),
            1,
            "S1".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            None,
        );
        app.editor.sector.as_mut().unwrap().systems.push(sys);
        app.editor.mark_dirty();
        assert!(app.editor.revision > rev0);
        assert_eq!(app.sector.as_ref().unwrap().systems.len(), 0);

        // The bridge re-derives the snapshot from the source of truth.
        app.sync_derived_sector();
        assert_eq!(app.sector.as_ref().unwrap().systems.len(), 1);
        assert_eq!(app.synced_revision, app.editor.revision);

        // Idle (no revision change): the snapshot Arc is not rebuilt.
        let ptr = Arc::as_ptr(app.sector.as_ref().unwrap());
        app.sync_derived_sector();
        assert!(std::ptr::eq(ptr, Arc::as_ptr(app.sector.as_ref().unwrap())));
    }
}
