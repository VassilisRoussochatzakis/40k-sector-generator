use crate::gui::app::{App, FactionsMode};
use crate::gui::{factions_overview, palette};
use egui::{RichText, ScrollArea, TopBottomPanel};

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = app.sector.clone() else {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("no sector loaded")
                        .color(palette::TEXT_DIM)
                        .monospace(),
                );
            });
        return;
    };

    TopBottomPanel::top("factions_toolbar")
        .frame(
            egui::Frame::none()
                .fill(palette::PANEL_BG)
                .inner_margin(6.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let on_overview = matches!(app.factions_mode, FactionsMode::Overview);
                let on_designer = matches!(app.factions_mode, FactionsMode::Designer);
                if ui
                    .selectable_label(on_overview, RichText::new("OVERVIEW").monospace())
                    .clicked()
                {
                    app.factions_mode = FactionsMode::Overview;
                }
                if ui
                    .selectable_label(on_designer, RichText::new("DESIGNER").monospace())
                    .clicked()
                {
                    app.factions_mode = FactionsMode::Designer;
                }
                ui.separator();
                ui.label(
                    RichText::new("high-level faction state")
                        .color(palette::TEXT_DIM)
                        .monospace(),
                );
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| match app.factions_mode {
                FactionsMode::Overview => {
                    factions_overview::show_readonly(ui, &sector);
                }
                FactionsMode::Designer => {
                    factions_overview::show_designer(
                        ui,
                        &sector,
                        &mut app.faction_designer,
                        app.project_dir.as_deref(),
                    );
                }
            });
        });
}
