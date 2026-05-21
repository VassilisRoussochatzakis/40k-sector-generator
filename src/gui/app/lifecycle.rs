use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rfd::FileDialog;

use crate::{
    sector_model::GeneratedSector,
    subsectors::{build_subsectors, SubsectorConfig},
};

use super::App;

impl App {
    pub(super) fn set_loaded_sector(&mut self, sector: GeneratedSector, source_path: Option<String>) {
        self.sector_source_path = source_path.as_ref().map(PathBuf::from);
        self.live_dirty = false;
        self.subsectors = build_subsectors(&sector, SubsectorConfig::default()).unwrap_or_default();
        self.sector = Some(Arc::new(sector.clone()));
        self.sector_selected = None;
        self.sector_selected_route = None;
        self.sector_selected_subsector = None;
        self.sector_edit_tool = super::SectorEditTool::Select;
        self.pending_route_start = None;
        self.history_selected_event = None;
        self.planner.clear();
        self.dashboard.invalidate();
        self.heatmap_cache.invalidate();
        self.sector_overview_cache.invalidate();
        if !self.editor.dirty {
            self.editor.set_sector(sector, source_path);
        }
    }

    pub(super) fn save_sector_to_source(&mut self) {
        if let Some(path) = self.sector_source_path.clone() {
            self.write_sector_to_path(path);
        } else {
            self.save_sector_as();
        }
    }

    pub(super) fn save_sector_as(&mut self) {
        let Some(sector) = self.sector.as_ref() else {
            self.export_status = "no sector to save".into();
            return;
        };
        let mut dialog = FileDialog::new()
            .add_filter("Sector JSON", &["json"])
            .set_file_name("sector.json");
        if let Some(dir) = self
            .sector_source_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| self.project_dir.as_ref().map(|p| p.join("out")))
        {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        if path.file_name().is_none() {
            self.export_status = format!("save failed: invalid path for {}", sector.id);
            return;
        }
        self.write_sector_to_path(path);
    }

    pub(super) fn write_sector_to_path(&mut self, path: PathBuf) {
        let text = match self.sector.as_mut() {
            Some(sector) => {
                let sector = Arc::make_mut(sector);
                Self::refresh_live_manifest_counts(sector);
                serde_json::to_string_pretty(sector).map_err(|e| format!("encode: {e}"))
            }
            None => Err("no sector to save".into()),
        };
        let text = match text {
            Ok(text) => text,
            Err(e) => {
                self.export_status = format!("save failed: {e}");
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                self.export_status = format!("save failed: mkdir {}: {}", parent.display(), e);
                return;
            }
        }
        match fs::write(&path, text) {
            Ok(()) => {
                self.sector_source_path = Some(path.clone());
                self.live_dirty = false;
                self.export_status = format!("saved {}", path.display());
                if let Some(sector) = self.sector.as_ref() {
                    self.editor.set_sector(
                        sector.as_ref().clone(),
                        Some(path.to_string_lossy().to_string()),
                    );
                }
            }
            Err(e) => {
                self.export_status = format!("save failed: write {}: {}", path.display(), e);
            }
        }
    }
}
