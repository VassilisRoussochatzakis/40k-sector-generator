use crate::app::palette;
use crate::app::{App, View};
use egui::{Color32, RichText, TopBottomPanel};
use sectorforge::sector_model::RouteViewMode;

pub struct TopBar<'a> {
    app: &'a mut App,
}

impl<'a> TopBar<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }

    pub fn show(self, ctx: &egui::Context) {
        draw_top_bar(self.app, ctx);
    }
}

pub struct MainView<'a> {
    app: &'a mut App,
}

impl<'a> MainView<'a> {
    pub fn new(app: &'a mut App) -> Self {
        Self { app }
    }

    pub fn show(self, ctx: &egui::Context) {
        draw_main_view(self.app, ctx);
    }
}

fn draw_top_bar(app: &mut App, ctx: &egui::Context) {
    TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::none()
                .fill(palette::chrome_panel())
                .inner_margin(6.0),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                crate::theme::menu(ui, &mut app.theme);
                ui.separator();
                if ui
                    .selectable_label(matches!(app.view, View::Sector), "SECTOR")
                    .clicked()
                {
                    app.view = View::Sector;
                }
                if ui
                    .selectable_label(matches!(app.view, View::System { .. }), "SYSTEM")
                    .clicked()
                {
                    if let Some(id) = app.sector_selected.clone() {
                        app.view = View::System {
                            system_id: id,
                            selection: crate::system_view::SystemSelection::None,
                        };
                    }
                }
                if ui
                    .selectable_label(matches!(app.view, View::Segmentum), "SEGMENTUM")
                    .clicked()
                {
                    app.view = View::Segmentum;
                }
                ui.separator();
                if ui
                    .selectable_label(matches!(app.view, View::Dashboard), "DASHBOARD")
                    .clicked()
                {
                    app.view = View::Dashboard;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Factions), "FACTIONS")
                    .clicked()
                {
                    app.view = View::Factions;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Relations), "RELATIONS")
                    .clicked()
                {
                    app.view = View::Relations;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Trade), "TRADE")
                    .clicked()
                {
                    app.view = View::Trade;
                }
                if ui
                    .selectable_label(matches!(app.view, View::History), "HISTORY")
                    .clicked()
                {
                    app.view = View::History;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Regions), "REGIONS")
                    .clicked()
                {
                    app.view = View::Regions;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Planner), "PLANNER")
                    .clicked()
                {
                    app.view = View::Planner;
                }

                ui.separator();
                if ui
                    .selectable_label(matches!(app.view, View::Edit), "EDIT-MAP")
                    .clicked()
                {
                    app.view = View::Edit;
                }
                if ui
                    .selectable_label(matches!(app.view, View::Data), "DATA-RAW")
                    .clicked()
                {
                    app.view = View::Data;
                }
            });

            ui.separator();

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("ROUTE VIEW").color(palette::chrome_text_dim()));
                if ui
                    .selectable_label(app.route_view_mode == RouteViewMode::TopLevel, "TOP-LEVEL")
                    .on_hover_text("Group routes by Warp / Webway / etc.")
                    .clicked()
                {
                    app.route_view_mode = RouteViewMode::TopLevel;
                    app.editor.route_view_mode = RouteViewMode::TopLevel;
                }
                if ui
                    .selectable_label(app.route_view_mode == RouteViewMode::Detailed, "DETAILED")
                    .on_hover_text("Show all specialized route types")
                    .clicked()
                {
                    app.route_view_mode = RouteViewMode::Detailed;
                    app.editor.route_view_mode = RouteViewMode::Detailed;
                }

                ui.separator();
                if ui.button("NEW").clicked() {
                    app.preset_gallery.open = true;
                }
                if ui.button("OPEN").clicked() {
                    app.open_sector_dialog();
                }
                if ui
                    .add_enabled(app.sector.is_some(), egui::Button::new("SAVE"))
                    .clicked()
                {
                    app.save_sector_to_source();
                }
                let export_ready = app.sector.is_some() && app.export_job.is_none();
                if ui
                    .add_enabled(export_ready, egui::Button::new("EXPORT PNG"))
                    .clicked()
                {
                    app.pending_export = Some(crate::app::PendingExport::SectorPng);
                }
                if ui
                    .add_enabled(export_ready, egui::Button::new("EXPORT SVG"))
                    .clicked()
                {
                    app.pending_export = Some(crate::app::PendingExport::SectorSvg);
                }
                if ui
                    .add_enabled(export_ready, egui::Button::new("EXPORT HTML"))
                    .clicked()
                {
                    app.pending_export = Some(crate::app::PendingExport::SectorHtml);
                }
                if ui
                    .add_enabled(export_ready, egui::Button::new("EXPORT SYSTEMS"))
                    .clicked()
                {
                    app.pending_export = Some(crate::app::PendingExport::AllSystemPngs);
                }
                if ui
                    .add_enabled(export_ready, egui::Button::new("SAVE & EXPORT (PNG)"))
                    .clicked()
                {
                    app.save_sector_to_source();
                    app.pending_export = Some(crate::app::PendingExport::SectorPng);
                    // VAPP-1: renamed from "SAVE & EXPORT ALL" — pending_export holds one variant
                    // drained once per frame, each format opens its own blocking file dialog, PNG
                    // needs a multi-frame resolution dialog, and only one export_job runs at a time.
                    // True multi-format export would need an async job queue; out of scope here, so
                    // the label is now honest about doing PNG only (SVG/HTML have their own buttons).
                }
                if ui
                    .add_enabled(export_ready, egui::Button::new("EXPORT BUNDLE"))
                    .clicked()
                {
                    app.export_sector_json(ctx);
                }
                if app.cancellable_export_running()
                    && ui.add(egui::Button::new("CANCEL EXPORT")).clicked()
                {
                    app.cancel_export_job();
                }

                if !app.export_status.is_empty() {
                    ui.label(
                        RichText::new(&app.export_status).color(Color32::from_rgb(235, 200, 90)),
                    );
                }
            });
        });
}

fn draw_main_view(app: &mut App, ctx: &egui::Context) {
    match app.view.clone() {
        View::Sector => app.draw_sector_layout(ctx),
        View::System {
            system_id,
            selection,
        } => app.draw_system_layout(ctx, &system_id, selection),
        View::Edit => app.draw_edit_layout(ctx),
        View::Data => app.draw_data_layout(ctx),
        View::Planner => crate::app::planner_view::ui(app, ctx),
        View::Dashboard => app.draw_dashboard_layout(ctx),
        View::Factions => crate::app::factions_view::ui(app, ctx),
        View::Relations => crate::app::relations_view::ui(app, ctx),
        View::Regions => crate::app::regions_view::ui(app, ctx),
        View::Trade => crate::app::trade_view::ui(app, ctx),
        View::History => app.draw_history_layout(ctx),
        View::Segmentum => app.draw_segmentum_layout(ctx),
    }
}
