use crate::app::App;
use crate::factions_overview;
use egui::ScrollArea;

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = super::require_sector(app, ctx) else {
        return;
    };

    super::page(ctx, |ui| {
        ScrollArea::vertical().show(ui, |ui| {
            factions_overview::show_readonly(ui, &sector);
        });
    });
}
