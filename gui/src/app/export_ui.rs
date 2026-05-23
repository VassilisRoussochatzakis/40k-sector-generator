//! Export UI: PNG export dialog + sector JSON bundle export. Split off
//! from `app/mod.rs` for readability — operates on the parent [`App`] state
//! through `impl` blocks here.

use std::path::PathBuf;

use egui::RichText;
use rfd::FileDialog;

use sectorforge::export;

use super::super::palette::TEXT_DIM;
use super::{App, PendingExport};

impl App {
    pub(super) fn draw_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_export.clone() else {
            return;
        };
        // HTML export skips the resolution dialog — it has no scale knob.
        if matches!(action, PendingExport::SectorHtml) {
            self.execute_html_export();
            self.pending_export = None;
            return;
        }
        let title = match &action {
            PendingExport::SectorPng => "Export Sector Map (PNG)".to_string(),
            PendingExport::AllSystemPngs => "Export All System Maps (PNG)".to_string(),
            PendingExport::SystemPng(id) => format!("Export System Map: {}", id),
            PendingExport::SectorHtml => unreachable!(),
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(RichText::new(&title).monospace().strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("Resolution").color(TEXT_DIM).monospace());
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.export_scale,
                        1,
                        RichText::new("720p").monospace(),
                    );
                    ui.selectable_value(
                        &mut self.export_scale,
                        2,
                        RichText::new("1440p").monospace(),
                    );
                    ui.selectable_value(&mut self.export_scale, 3, RichText::new("4K").monospace());
                    ui.selectable_value(
                        &mut self.export_scale,
                        5,
                        RichText::new("Ultra (5x)").monospace(),
                    );
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Theme").color(TEXT_DIM).monospace());
                egui::ComboBox::from_id_salt("png_export_theme")
                    .selected_text(RichText::new(&self.export_theme_name).monospace())
                    .show_ui(ui, |ui| {
                        for name in sectorforge::map_theme::BUILTIN_THEME_NAMES {
                            ui.selectable_value(
                                &mut self.export_theme_name,
                                (*name).to_string(),
                                RichText::new(*name).monospace(),
                            );
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("EXPORT").monospace()).clicked() {
                        confirm = true;
                    }
                    if ui.button(RichText::new("CANCEL").monospace()).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            let scale = self.export_scale.max(1);
            self.execute_png_export(action, scale);
            self.pending_export = None;
        } else if cancel {
            self.pending_export = None;
        }
    }

    pub(super) fn execute_png_export(&mut self, action: PendingExport, scale: u32) {
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
                let subs = if self.subsectors.is_empty() {
                    None
                } else {
                    Some(self.subsectors.as_slice())
                };
                let opts = sectorforge::bitmap::RenderOptions {
                    faction_fill: true,
                    heatmap: self.heatmap_mode,
                    theme: self.export_theme(),
                    route_view_mode: self.route_view_mode,
                };
                match sectorforge::bitmap::write_sector_png_to_with(&sector, &p, scale, subs, opts)
                {
                    Ok(()) => self.export_status = format!("exported {}", p),
                    Err(e) => self.export_status = format!("export failed: {}", e),
                }
            }
            PendingExport::AllSystemPngs => {
                let Some(dir) = FileDialog::new().pick_folder() else {
                    return;
                };
                let Ok(p) = camino::Utf8PathBuf::from_path_buf(dir) else {
                    self.export_status = "path is not valid utf-8".into();
                    return;
                };
                let sys_opts = sectorforge::system_map::SystemRenderOptions {
                    faction_fill: true,
                    theme: self.export_theme(),
                };
                match sectorforge::system_map::write_system_maps(&sector, &p, scale, sys_opts) {
                    Ok(()) => {
                        self.export_status = format!(
                            "exported {} system PNGs to {}/systems",
                            sector.systems.len(),
                            p
                        )
                    }
                    Err(e) => self.export_status = format!("export failed: {}", e),
                }
            }
            PendingExport::SystemPng(id) => {
                let Some(sys) = sector.systems.iter().find(|s| s.id == id).cloned() else {
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
                let sys_opts = sectorforge::system_map::SystemRenderOptions {
                    faction_fill: true,
                    theme: self.export_theme(),
                };
                match sectorforge::system_map::write_one_system_png(
                    &sys,
                    &sector.factions,
                    &p,
                    scale,
                    sys_opts,
                ) {
                    Ok(()) => self.export_status = format!("exported {}", p),
                    Err(e) => self.export_status = format!("export failed: {}", e),
                }
            }
            PendingExport::SectorHtml => unreachable!(),
        }
    }

    fn export_theme(&self) -> sectorforge::map_theme::MapTheme {
        let cfg = sectorforge::map_theme::MapThemeConfig::named(&self.export_theme_name);
        sectorforge::map_theme::resolve_map_theme(&cfg)
            .unwrap_or_else(|_| sectorforge::map_theme::MapTheme::gm_dark())
    }

    /// §11 NEW.md: write an interactive HTML map. Uses default
    /// [`HtmlConfig`] — GUI doesn't yet expose theme/observer pickers.
    pub(super) fn execute_html_export(&mut self) {
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
        match sectorforge::html_export::write_html_to(&sector, &p, &cfg) {
            Ok(()) => self.export_status = format!("exported {}", p),
            Err(e) => self.export_status = format!("export failed: {}", e),
        }
    }

    pub(super) fn export_sector_json(&mut self, _ctx: &egui::Context) {
        let sector = self.sector.clone();
        let data_dir_pb: Option<PathBuf> = self
            .data_editor
            .gen_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        if let Some(path) = FileDialog::new().pick_folder() {
            let Some(path) = camino::Utf8Path::from_path(&path) else {
                self.export_status = format!("export failed: non-UTF8 path {}", path.display());
                return;
            };
            match sector {
                Some(s) => {
                    let data_dir = data_dir_pb.as_deref().and_then(camino::Utf8Path::from_path);
                    let sector_subdir = path.join(s.id.as_ref());
                    match export::export_bundle(&s, data_dir, path) {
                        Ok(()) => {
                            self.export_status = match data_dir {
                                Some(_) => {
                                    format!("exported to {} (incl. data folder)", sector_subdir)
                                }
                                None => format!(
                                    "exported to {} (no data folder — project not loaded)",
                                    sector_subdir
                                ),
                            };
                        }
                        Err(e) => self.export_status = format!("export failed: {}", e),
                    }
                }
                None => self.export_status = "no sector to export".into(),
            }
        }
    }
}
