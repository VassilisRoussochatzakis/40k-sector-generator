use crate::gui::palette::{TEXT, TEXT_DIM};
use crate::gui::{palette, App};
use egui::{Color32, RichText, ScrollArea};

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = app.sector.clone() else {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("no sector loaded")
                        .color(TEXT_DIM)
                        .monospace(),
                );
            });
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    RichText::new("WARP REGIONS")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                ui.label(
                    RichText::new("§5 NEW.md — regional warp phenomena overlay")
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.add_space(8.0);
                if sector.regions.is_empty() {
                    ui.label(
                        RichText::new(
                            "no regions configured — enable in regions.toml or \
                            sectorforge.toml",
                        )
                        .color(TEXT_DIM)
                        .monospace(),
                    );
                    return;
                }
                egui::Grid::new("regions_grid")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("ID").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("NAME").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("KIND").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("DESCRIPTION").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("HEXES").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("CENTRE").color(TEXT_DIM).monospace().strong());
                        ui.end_row();
                        for r in sector.regions.iter() {
                            ui.label(RichText::new(&r.id).monospace());
                            ui.label(RichText::new(&r.name).monospace());
                            ui.label(
                                RichText::new(r.kind.label())
                                    .color(Color32::from_rgb(220, 160, 60))
                                    .monospace(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(r.kind.description())
                                        .color(TEXT_DIM)
                                        .monospace(),
                                )
                                .wrap(),
                            );
                            ui.label(RichText::new(r.hexes.len().to_string()).monospace());
                            ui.label(
                                RichText::new(format!("({},{})", r.centre.q, r.centre.r))
                                    .monospace(),
                            );
                            ui.end_row();
                        }
                    });
            });
        });
}
