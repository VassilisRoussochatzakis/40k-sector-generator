use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use camino::Utf8PathBuf;
use rfd::FileDialog;

use sectorforge::sector_model::GeneratedSector;
use sectorforge::subsectors::{build_subsectors, SubsectorConfig};

use super::App;

impl App {
    pub(super) fn set_loaded_sector(
        &mut self,
        sector: GeneratedSector,
        source_path: Option<String>,
    ) {
        self.sector_source_path = source_path.as_ref().map(PathBuf::from);
        self.live_dirty = false;
        self.subsectors = build_subsectors(&sector, SubsectorConfig::default()).unwrap_or_default();
        self.sector_map_cache = Some(crate::sector_view::SectorMapCache::new(
            &sector,
            &self.subsectors,
        ));
        self.sector = Some(Arc::new(sector.clone()));
        self.sector_selected = None;
        self.sector_selected_route = None;
        self.sector_selected_subsector = None;
        self.editor.tool = crate::editor::state::SectorEditTool::Select;
        self.pending_route_start = None;
        self.history_selected_event = None;
        self.planner.clear();
        self.dashboard.invalidate();
        self.heatmap_cache.invalidate();
        self.sector_overview_cache.invalidate();

        // Auto-detect project dir if we're loading a sector from an "out/" folder
        if let Some(sp) = &self.sector_source_path {
            if let Some(parent) = sp.parent() {
                if parent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "out")
                    .unwrap_or(false)
                {
                    if let Some(project_root) = parent.parent() {
                        self.project_dir = Some(project_root.to_path_buf());
                    }
                }
            }
        }

        let mut input = None;
        if let Some(path) = &self.project_dir {
            if let Ok(utf8_path) = camino::Utf8PathBuf::from_path_buf(path.clone()) {
                if let Ok(pi) = sectorforge::input::load_project(&utf8_path) {
                    input = Some(pi);
                }
            }
        }

        // Always set the sector in the editor and clear dirty flag when explicitly loading
        self.editor.set_sector(sector, input, source_path);
        self.editor.dirty = false;

        self.zoom_to_fit();
    }

    pub(super) fn zoom_to_fit(&mut self) {
        let Some(sector) = self.sector.as_ref() else {
            return;
        };

        // Standard hex metrics used in SectorView
        let horiz_step = 3f32.sqrt();
        let vert_step = 1.5;

        // Approximate map dimensions for unit hex size (1.0)
        let w_units = sector.width as f32 * horiz_step;
        let h_units = sector.height as f32 * vert_step;

        // Target: fit into a reasonable area (e.g. 1000x1000) or use current ui if we had it.
        // Since we don't have UI size here, we use a heuristic or just center it.
        self.sector_hex_size = (800.0 / w_units.max(h_units).max(1.0)).clamp(5.0, 250.0);
        self.sector_pan = egui::Vec2::ZERO; // Reset pan
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

    pub(super) fn load_project_path(&mut self, path: Utf8PathBuf) {
        let std_path = path.clone().into_std_path_buf();
        self.project_dir = Some(std_path.clone());

        // Try load project input
        match sectorforge::input::load_project(&path) {
            Ok(input) => {
                // Check if sector.json exists
                let sector_path = path.join("out").join("sector.json");
                if sector_path.exists() {
                    match sectorforge::load_sector_json(&sector_path) {
                        Ok(sector) => {
                            self.set_loaded_sector(sector, Some(sector_path.to_string()));
                        }
                        Err(e) => {
                            self.export_status = format!("project load failed: {e}");
                        }
                    }
                } else {
                    // Try to generate it
                    match sectorforge::generation::generate(input.clone()) {
                        Ok(sector) => {
                            // Ensure out dir exists
                            let out_dir = path.join("out");
                            let _ = std::fs::create_dir_all(&out_dir);
                            if let Err(e) = sectorforge::write_sector_json(&sector_path, &sector) {
                                self.export_status =
                                    format!("failed to save generated sector: {e}");
                            }
                            self.set_loaded_sector(sector, Some(sector_path.to_string()));
                        }
                        Err(e) => {
                            self.export_status = format!("generation failed: {e}");
                        }
                    }
                }

                if let Err(e) = self.data_editor.load_from_project(&std_path) {
                    self.data_editor.status = format!("data load failed: {e}");
                }
            }
            Err(e) => {
                self.export_status = format!("failed to load project config: {e}");
            }
        }
    }

    pub(super) fn write_sector_to_path(&mut self, path: PathBuf) {
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "save failed: no sector to save".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        Self::refresh_live_manifest_counts(sector);
        let text = match serde_json::to_string_pretty(sector) {
            Ok(text) => text,
            Err(e) => {
                self.export_status = format!("save failed: encode: {e}");
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
                    let mut input = None;
                    if let Some(path) = &self.project_dir {
                        if let Ok(utf8_path) = camino::Utf8PathBuf::from_path_buf(path.clone()) {
                            if let Ok(pi) = sectorforge::input::load_project(&utf8_path) {
                                input = Some(pi);
                            }
                        }
                    }
                    self.editor.set_sector(
                        sector.as_ref().clone(),
                        input,
                        Some(path.to_string_lossy().to_string()),
                    );
                }
            }
            Err(e) => {
                self.export_status = format!("save failed: write {}: {}", path.display(), e);
            }
        }
    }

    pub(super) fn handle_preview_logic(&mut self, ctx: &egui::Context) {
        // 1. Debounce timer
        if let Some(timer) = self.editor.preview_timer {
            if ctx.input(|i| i.time) >= timer {
                self.editor.preview_timer = None;
                if let Some(input) = self.editor.project_input.clone() {
                    // Cancel existing job if any
                    self.editor.preview_job = None;

                    let ctx_clone = ctx.clone();
                    self.editor.preview_job = Some(crate::jobs::spawn_job(
                        "preview-gen",
                        "Generating preview...",
                        ctx_clone,
                        move |_job_ctx| {
                            // Run generation
                            match sectorforge::generation::generate(input) {
                                Ok(sector) => sector,
                                Err(_) => {
                                    // For preview, we might just want to return an empty sector or similar if it fails
                                    // but let's just return what we have or something.
                                    // Actually, we should probably handle errors in JobHandle.
                                    // For now, let's just return a default if it fails.
                                    // TODO: Proper error handling in Jobs
                                    panic!("Preview generation failed");
                                }
                            }
                        },
                    ));
                }
            }
        }

        // 2. Job completion
        if let Some(job) = &self.editor.preview_job {
            if let Ok(sector) = job.receiver.try_recv() {
                self.editor.preview_sector = Some(sector);
                self.editor.preview_job = None;
            }
        }
    }
}
