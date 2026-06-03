//! Export UI: PNG export dialog + sector JSON bundle export. Split off
//! from `app/mod.rs` for readability; operates on the parent [`App`] state
//! through `impl` blocks here.

use std::{fs, path::PathBuf};

use egui::RichText;
use rfd::FileDialog;

use sectorforge::export;

use super::super::palette;
use super::{App, ExportJobResult, PendingExport};

const SECTOR_PNG_JOB_ID: &str = "export-sector-png";
const ALL_SYSTEM_PNG_JOB_ID: &str = "export-all-system-pngs";
const SYSTEM_PNG_JOB_ID: &str = "export-system-png";
const SECTOR_SVG_JOB_ID: &str = "export-sector-svg";
const SECTOR_HTML_JOB_ID: &str = "export-sector-html";
const BUNDLE_JOB_ID: &str = "export-bundle";

impl App {
    pub(super) fn draw_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_export.clone() else {
            return;
        };
        if self.export_job.is_some() {
            self.export_status = "export already running".into();
            self.pending_export = None;
            return;
        }
        // HTML and SVG export skip the resolution dialog; vector output has
        // no scale knob, and HTML has its own self-contained renderer.
        if matches!(action, PendingExport::SectorHtml) {
            self.execute_html_export(ctx);
            self.pending_export = None;
            return;
        }
        if matches!(action, PendingExport::SectorSvg) {
            self.execute_svg_export(ctx);
            self.pending_export = None;
            return;
        }
        let title = match &action {
            PendingExport::SectorPng => "Export Sector Map (PNG)".to_string(),
            PendingExport::AllSystemPngs => "Export All System Maps (PNG)".to_string(),
            PendingExport::SystemPng(id) => format!("Export System Map: {}", id),
            PendingExport::SectorHtml | PendingExport::SectorSvg => unreachable!(),
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(RichText::new(&title).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("Resolution").color(palette::chrome_text_dim()));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.export_scale, 1, RichText::new("720p"));
                    ui.selectable_value(&mut self.export_scale, 2, RichText::new("1440p"));
                    ui.selectable_value(&mut self.export_scale, 3, RichText::new("4K"));
                    ui.selectable_value(&mut self.export_scale, 5, RichText::new("Ultra (5x)"));
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Theme").color(palette::chrome_text_dim()));
                crate::ui_kit::combo("png_export_theme", RichText::new(&self.export_theme_name))
                    .show_ui(ui, |ui| {
                        for name in sectorforge::map_theme::BUILTIN_THEME_NAMES {
                            ui.selectable_value(
                                &mut self.export_theme_name,
                                (*name).to_string(),
                                RichText::new(*name),
                            );
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("EXPORT")).clicked() {
                        confirm = true;
                    }
                    if ui.button(RichText::new("CANCEL")).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            let scale = self.export_scale.max(1);
            self.execute_png_export(ctx, action, scale);
            self.pending_export = None;
        } else if cancel {
            self.pending_export = None;
        }
    }

    pub(super) fn handle_export_job(&mut self) {
        let completed = self
            .export_job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(ExportJobResult::Failed(
                    format!("export failed: worker disconnected ({})", job.description),
                )),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = completed {
            self.export_status = result.into_status();
            self.export_job = None;
        } else if let Some(job) = self.export_job.as_ref() {
            let pct = (job.progress().clamp(0.0, 1.0) * 100.0).round();
            let detail = job.status().map(|s| format!(" — {s}")).unwrap_or_default();
            self.export_status = if job.is_cancelled() {
                format!("cancelling {} ({pct:.0}%){detail}", job.description)
            } else {
                format!("{} ({pct:.0}%){detail}", job.description)
            };
        }
    }

    pub(super) fn cancellable_export_running(&self) -> bool {
        self.export_job
            .as_ref()
            .map(|job| job.id == ALL_SYSTEM_PNG_JOB_ID && !job.is_cancelled())
            .unwrap_or(false)
    }

    pub(super) fn cancel_export_job(&mut self) {
        if let Some(job) = self.export_job.as_ref() {
            if job.id == ALL_SYSTEM_PNG_JOB_ID {
                job.cancel();
                self.export_status = "cancelling all-system PNG export".into();
            }
        }
    }

    pub(super) fn execute_png_export(
        &mut self,
        ctx: &egui::Context,
        action: PendingExport,
        scale: u32,
    ) {
        let Some(sector) = self.sector.clone() else {
            self.export_status = "no sector to export".into();
            return;
        };
        match action {
            PendingExport::SectorPng => {
                let default_name = format!("{}-sector.png", sector.id);
                let Some(path) = FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .set_file_name(&default_name)
                    .save_file()
                else {
                    return;
                };
                let Ok(p) = camino::Utf8PathBuf::from_path_buf(path) else {
                    self.export_status = "path is not valid utf-8".into();
                    return;
                };
                let subsectors = self.subsectors.clone();
                let opts = sector_render_options(self);
                let status_path = p.clone();
                self.start_export_job(
                    SECTOR_PNG_JOB_ID,
                    "sector PNG export",
                    ctx,
                    move |job_ctx| {
                        job_ctx.set_progress(0.05);
                        let subs = if subsectors.is_empty() {
                            None
                        } else {
                            Some(subsectors.as_slice())
                        };
                        match sectorforge::bitmap::write_sector_png_to_with(
                            &sector, &p, scale, subs, opts,
                        ) {
                            Ok(()) => {
                                job_ctx.set_progress(1.0);
                                ExportJobResult::Completed(format!("exported {}", status_path))
                            }
                            Err(e) => ExportJobResult::Failed(format!("export failed: {}", e)),
                        }
                    },
                );
            }
            PendingExport::AllSystemPngs => {
                let Some(dir) = FileDialog::new().pick_folder() else {
                    return;
                };
                let Ok(p) = camino::Utf8PathBuf::from_path_buf(dir) else {
                    self.export_status = "path is not valid utf-8".into();
                    return;
                };
                let sys_opts = system_render_options(self);
                let systems_dir = p.join("systems");
                let status_dir = systems_dir.clone();
                self.start_export_job(
                    ALL_SYSTEM_PNG_JOB_ID,
                    "all-system PNG export",
                    ctx,
                    move |job_ctx| {
                        job_ctx.set_progress(0.0);
                        if job_ctx.is_cancelled() {
                            return ExportJobResult::Cancelled(
                                "cancelled all-system PNG export".into(),
                            );
                        }
                        if let Err(e) = fs::create_dir_all(&systems_dir) {
                            return ExportJobResult::Failed(format!(
                                "export failed: create {}: {}",
                                systems_dir, e
                            ));
                        }
                        let total = sector.systems.len();
                        if total == 0 {
                            job_ctx.set_progress(1.0);
                            return ExportJobResult::Completed(format!(
                                "exported 0 system PNGs to {}",
                                status_dir
                            ));
                        }
                        for (idx, sys) in sector.systems.iter().enumerate() {
                            if job_ctx.is_cancelled() {
                                return ExportJobResult::Cancelled(format!(
                                    "cancelled all-system PNG export after {}/{} systems",
                                    idx, total
                                ));
                            }
                            let path = systems_dir.join(format!("{}.png", sys.id));
                            if let Err(e) = sectorforge::system_map::write_one_system_png(
                                sys,
                                &sector.factions,
                                &path,
                                scale,
                                sys_opts.clone(),
                            ) {
                                return ExportJobResult::Failed(format!("export failed: {}", e));
                            }
                            job_ctx.set_progress((idx + 1) as f32 / total as f32);
                        }
                        ExportJobResult::Completed(format!(
                            "exported {} system PNGs to {}",
                            total, status_dir
                        ))
                    },
                );
            }
            PendingExport::SystemPng(id) => {
                let Some(sys) = sector.systems.iter().find(|s| s.id == id) else {
                    self.export_status = format!("system {} not found", id);
                    return;
                };
                let default_name = format!("{}.png", sys.id);
                let Some(path) = FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .set_file_name(&default_name)
                    .save_file()
                else {
                    return;
                };
                let Ok(p) = camino::Utf8PathBuf::from_path_buf(path) else {
                    self.export_status = "path is not valid utf-8".into();
                    return;
                };
                let sys_opts = system_render_options(self);
                let status_path = p.clone();
                self.start_export_job(
                    SYSTEM_PNG_JOB_ID,
                    "system PNG export",
                    ctx,
                    move |job_ctx| {
                        job_ctx.set_progress(0.05);
                        let Some(sys) = sector.systems.iter().find(|s| s.id == id) else {
                            return ExportJobResult::Failed(format!("system {} not found", id));
                        };
                        match sectorforge::system_map::write_one_system_png(
                            sys,
                            &sector.factions,
                            &p,
                            scale,
                            sys_opts,
                        ) {
                            Ok(()) => {
                                job_ctx.set_progress(1.0);
                                ExportJobResult::Completed(format!("exported {}", status_path))
                            }
                            Err(e) => ExportJobResult::Failed(format!("export failed: {}", e)),
                        }
                    },
                );
            }
            PendingExport::SectorHtml | PendingExport::SectorSvg => unreachable!(),
        }
    }

    pub(super) fn execute_svg_export(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            self.export_status = "no sector to export".into();
            return;
        };
        let default_name = format!("{}-sector.svg", sector.id);
        let Some(path) = FileDialog::new()
            .add_filter("SVG", &["svg"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };
        let Ok(p) = camino::Utf8PathBuf::from_path_buf(path) else {
            self.export_status = "path is not valid utf-8".into();
            return;
        };
        let subsectors = self.subsectors.clone();
        let opts = sector_render_options(self);
        let status_path = p.clone();
        self.start_export_job(
            SECTOR_SVG_JOB_ID,
            "sector SVG export",
            ctx,
            move |job_ctx| {
                job_ctx.set_progress(0.05);
                let subs = if subsectors.is_empty() {
                    None
                } else {
                    Some(subsectors.as_slice())
                };
                match sectorforge::svg_export::write_sector_svg_to_with(&sector, &p, subs, &opts) {
                    Ok(()) => {
                        job_ctx.set_progress(1.0);
                        ExportJobResult::Completed(format!("exported {}", status_path))
                    }
                    Err(e) => ExportJobResult::Failed(format!("export failed: {}", e)),
                }
            },
        );
    }

    fn export_theme(&self) -> sectorforge::map_theme::MapTheme {
        let cfg = sectorforge::map_theme::MapThemeConfig::named(&self.export_theme_name);
        sectorforge::map_theme::resolve_map_theme(&cfg)
            .unwrap_or_else(|_| sectorforge::map_theme::MapTheme::gm_dark())
    }

    /// §11 NEW.md: write an interactive HTML map. Uses default
    /// [`HtmlConfig`] because the GUI does not expose theme/observer pickers yet.
    pub(super) fn execute_html_export(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            self.export_status = "no sector to export".into();
            return;
        };
        let default_name = format!("{}-sector.html", sector.id);
        let Some(path) = FileDialog::new()
            .add_filter("HTML", &["html"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };
        let Ok(p) = camino::Utf8PathBuf::from_path_buf(path) else {
            self.export_status = "path is not valid utf-8".into();
            return;
        };
        let cfg = sectorforge::config::HtmlConfig::default();
        let status_path = p.clone();
        self.start_export_job(
            SECTOR_HTML_JOB_ID,
            "sector HTML export",
            ctx,
            move |job_ctx| {
                job_ctx.set_progress(0.05);
                match sectorforge::html_export::write_html_to(&sector, &p, &cfg) {
                    Ok(()) => {
                        job_ctx.set_progress(1.0);
                        ExportJobResult::Completed(format!("exported {}", status_path))
                    }
                    Err(e) => ExportJobResult::Failed(format!("export failed: {}", e)),
                }
            },
        );
    }

    pub(super) fn export_sector_json(&mut self, ctx: &egui::Context) {
        if self.export_job.is_some() {
            self.export_status = "export already running".into();
            return;
        }
        let Some(sector) = self.sector.clone() else {
            self.export_status = "no sector to export".into();
            return;
        };
        let data_dir_pb: Option<PathBuf> = self
            .data_editor
            .worlds_toml_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let Some(path) = FileDialog::new().pick_folder() else {
            return;
        };
        let Ok(output_dir) = camino::Utf8PathBuf::from_path_buf(path) else {
            self.export_status = "export failed: non-UTF8 output path".into();
            return;
        };
        let data_dir = match data_dir_pb {
            Some(path) => match camino::Utf8PathBuf::from_path_buf(path) {
                Ok(path) => Some(path),
                Err(_) => {
                    self.export_status = "export failed: non-UTF8 data path".into();
                    return;
                }
            },
            None => None,
        };
        let data_dir_present = data_dir.is_some();
        let sector_subdir = output_dir.join(sector.id.as_ref());
        self.start_export_job(BUNDLE_JOB_ID, "sector bundle export", ctx, move |job_ctx| {
            job_ctx.set_progress(0.05);
            job_ctx.set_status("writing sector.json…");
            let mut on_progress = |ev: sectorforge::ExportProgress| {
                use sectorforge::ExportProgress as E;
                match ev {
                    E::JsonProgress { bytes_written } => {
                        job_ctx.set_progress(0.1);
                        job_ctx.set_status(format!(
                            "writing sector.json… {}",
                            sectorforge_gui_core::human_bytes(bytes_written)
                        ));
                    }
                    E::PerSystemJson { current, total } => {
                        job_ctx.set_status(format!("per-system json {current}/{total}"));
                    }
                    _ => {}
                }
            };
            let result = export::export_bundle_with_progress(
                &sector,
                data_dir.as_deref(),
                &output_dir,
                &mut on_progress,
            );
            match result {
                Ok(()) => {
                    job_ctx.set_progress(1.0);
                    let status = if data_dir_present {
                        format!("exported to {} (incl. data folder)", sector_subdir)
                    } else {
                        format!(
                            "exported to {} (no data folder - project not loaded)",
                            sector_subdir
                        )
                    };
                    ExportJobResult::Completed(status)
                }
                Err(e) => ExportJobResult::Failed(format!("export failed: {}", e)),
            }
        });
    }

    fn start_export_job<F>(&mut self, id: &str, description: &str, ctx: &egui::Context, f: F)
    where
        F: FnOnce(crate::jobs::JobContext) -> ExportJobResult + Send + 'static,
    {
        if self.export_job.is_some() {
            self.export_status = "export already running".into();
            return;
        }
        self.export_job_revision = self.export_job_revision.wrapping_add(1);
        self.export_status = format!("{description} queued");
        self.export_job = Some(crate::jobs::spawn_job(
            id,
            self.export_job_revision,
            description,
            ctx.clone(),
            f,
        ));
    }
}

fn sector_render_options(app: &App) -> sectorforge::bitmap::RenderOptions {
    sectorforge::bitmap::RenderOptions {
        faction_fill: true,
        heatmap: app.heatmap_mode,
        theme: app.export_theme(),
        route_view_mode: app.route_view_mode,
    }
}

fn system_render_options(app: &App) -> sectorforge::system_map::SystemRenderOptions {
    sectorforge::system_map::SystemRenderOptions {
        faction_fill: true,
        theme: app.export_theme(),
    }
}
