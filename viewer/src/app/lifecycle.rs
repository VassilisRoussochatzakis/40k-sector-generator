use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use camino::Utf8PathBuf;
use rfd::FileDialog;

use crate::editor::PreviewJobResult;

use sectorforge::sector_model::GeneratedSector;
use sectorforge::subsectors::{build_subsectors, SubsectorConfig};

use super::App;

impl App {
    /// TF-E-2: build the display subsectors for a sector, surfacing any
    /// [`SubsectorBuildError`](sectorforge::subsectors::SubsectorBuildError) as a
    /// status string instead of silently collapsing to an empty `Vec` (the old
    /// `.unwrap_or_default()` swallow). Mirrors the builder's `last_subsector_error`
    /// slot; the viewer routes it through the shared sticky `export_status` line.
    pub(super) fn build_display_subsectors(
        sector: &GeneratedSector,
    ) -> (Vec<sectorforge::subsectors::Subsector>, Option<String>) {
        match build_subsectors(sector, SubsectorConfig::default()) {
            Ok(subs) => (subs, None),
            Err(e) => (Vec::new(), Some(format!("subsectors: {e}"))),
        }
    }

    pub(super) fn set_loaded_sector(
        &mut self,
        sector: GeneratedSector,
        source_path: Option<String>,
    ) {
        self.sector_source_path = source_path.as_ref().map(PathBuf::from);
        let (subs, sub_err) = Self::build_display_subsectors(&sector);
        self.subsectors = subs;
        if let Some(e) = sub_err {
            self.export_status = e;
        }
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
        // `self.sector` was seeded directly above; mark the bridge in sync so it
        // doesn't redundantly re-derive the snapshot we just built.
        self.synced_revision = self.editor.revision;

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
        let Some(sector) = self.editor.sector.as_ref() else {
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
        // Serialize the single source of truth (F-S1). `refresh_live_manifest_counts`
        // covers editor-panel edits that mutated the sector without going through
        // `mark_live_sector_dirty`.
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "save failed: no sector to save".into();
            return;
        };
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
                self.editor.dirty = false;
                self.editor.loaded_from = Some(path.to_string_lossy().to_string());
                self.export_status = format!("saved {}", path.display());
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
                    if let Some(job) = &self.editor.preview_job {
                        job.cancel();
                    }

                    let ctx_clone = ctx.clone();
                    let revision = self.editor.preview_revision;
                    self.editor.preview_job = Some(crate::jobs::spawn_job(
                        "preview-gen",
                        revision,
                        "Generating preview...",
                        ctx_clone,
                        move |job_ctx| {
                            let result = sectorforge::generation::generate_with_progress_and_cancel(
                                input,
                                |event| job_ctx.set_progress(preview_progress(&event)),
                                || job_ctx.is_cancelled(),
                            );
                            match result {
                                Ok(sector) => PreviewJobResult::Ready(sector),
                                Err(sectorforge::SectorError::GenerationCancelled) => {
                                    PreviewJobResult::Cancelled
                                }
                                Err(err) => {
                                    PreviewJobResult::Failed(format!("preview failed: {err}"))
                                }
                            }
                        },
                    ));
                }
            }
        }

        // 2. Job completion
        let completed =
            self.editor
                .preview_job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(result) => Some((job.revision, result)),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some((
                        job.revision,
                        PreviewJobResult::Failed("preview worker disconnected".to_string()),
                    )),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                });
        if let Some((revision, result)) = completed {
            self.editor.apply_preview_result(revision, result);
            self.editor.preview_job = None;
        }
    }
}

fn preview_progress(event: &sectorforge::SectorProgress) -> f32 {
    match event {
        sectorforge::SectorProgress::WorldPoolBuilt { .. } => 0.05,
        sectorforge::SectorProgress::SystemsPlaced { .. } => 0.1,
        sectorforge::SectorProgress::RegionsBuilt { .. } => 0.12,
        sectorforge::SectorProgress::SystemBuilt { current, total, .. } => {
            0.12 + fraction(*current, *total) * 0.28
        }
        sectorforge::SectorProgress::FactionsAssigned { .. }
        | sectorforge::SectorProgress::FactionsAggregated { .. } => 0.42,
        sectorforge::SectorProgress::RoutesGenerated { .. }
        | sectorforge::SectorProgress::RegionEffectsApplied { .. }
        | sectorforge::SectorProgress::HiddenRoutesApplied { .. }
        | sectorforge::SectorProgress::RouteControlsDerived { .. } => 0.55,
        sectorforge::SectorProgress::RouteControlsProgress { current, total } => {
            0.45 + fraction(*current, *total) * 0.08
        }
        sectorforge::SectorProgress::SystemStateDerived { current, total } => {
            0.55 + fraction(*current, *total) * 0.1
        }
        sectorforge::SectorProgress::ManifestBuilt { .. } => 0.66,
        sectorforge::SectorProgress::InfluenceFieldAnchorsProjected { current, total, .. }
        | sectorforge::SectorProgress::InfluenceFieldCellsResolved { current, total, .. } => {
            0.7 + fraction(*current, *total) * 0.1
        }
        sectorforge::SectorProgress::ChronicleSystemsScanned { current, total, .. }
        | sectorforge::SectorProgress::ChronicleRoutesScanned { current, total, .. } => {
            0.86 + fraction(*current, *total) * 0.08
        }
        sectorforge::SectorProgress::Complete { .. } => 1.0,
        _ => 0.7,
    }
}

fn fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::{fraction, preview_progress};
    use sectorforge::SectorProgress;

    #[test]
    fn fraction_zero_total_is_zero() {
        assert_eq!(fraction(0, 0), 0.0);
        assert_eq!(fraction(5, 0), 0.0);
    }

    #[test]
    fn fraction_partial_is_ratio() {
        assert_eq!(fraction(1, 2), 0.5);
        assert_eq!(fraction(3, 4), 0.75);
        assert_eq!(fraction(0, 4), 0.0);
    }

    #[test]
    fn preview_progress_anchors_are_monotonic_at_known_points() {
        let world_pool = preview_progress(&SectorProgress::WorldPoolBuilt {
            rows: 0,
            candidates: 0,
            excluded: 0,
        });
        let systems = preview_progress(&SectorProgress::SystemsPlaced {
            total: 0,
            width: 0,
            height: 0,
        });
        let regions = preview_progress(&SectorProgress::RegionsBuilt { count: 0 });
        let routes = preview_progress(&SectorProgress::RoutesGenerated { routes: 0 });
        let manifest = preview_progress(&SectorProgress::ManifestBuilt {
            systems: 0,
            worlds: 0,
            routes: 0,
        });
        let complete = preview_progress(&SectorProgress::Complete {
            systems: 0,
            worlds: 0,
            routes: 0,
        });

        assert!(world_pool < systems);
        assert!(systems < regions);
        assert!(regions < routes);
        assert!(routes < manifest);
        assert!(manifest < complete);
        assert_eq!(complete, 1.0);
    }

    #[test]
    fn preview_progress_system_built_scales_with_fraction() {
        let mid = preview_progress(&SectorProgress::SystemBuilt {
            current: 5,
            total: 10,
            worlds: 1,
        });
        let full = preview_progress(&SectorProgress::SystemBuilt {
            current: 10,
            total: 10,
            worlds: 1,
        });
        assert!(mid < full);
        assert!(mid > 0.12);
    }
}
