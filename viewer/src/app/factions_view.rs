use crate::app::{App, FactionsMode};
use crate::{factions_overview, palette};
use egui::{RichText, ScrollArea, TopBottomPanel};

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = super::require_sector(app, ctx) else {
        return;
    };

    TopBottomPanel::top("factions_toolbar")
        .frame(
            egui::Frame::none()
                .fill(crate::app::palette::chrome_panel())
                .inner_margin(6.0),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let on_overview = matches!(app.factions_mode, FactionsMode::Overview);
                let on_designer = matches!(app.factions_mode, FactionsMode::Designer);
                if ui
                    .selectable_label(on_overview, RichText::new("OVERVIEW"))
                    .clicked()
                {
                    app.factions_mode = FactionsMode::Overview;
                }
                if ui
                    .selectable_label(on_designer, RichText::new("DESIGNER"))
                    .clicked()
                {
                    app.factions_mode = FactionsMode::Designer;
                }
                ui.separator();
                ui.label(
                    RichText::new("high-level faction state").color(palette::chrome_text_dim()),
                );
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(palette::chrome_bg())
                .inner_margin(14.0),
        )
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
