use crate::{palette, App};
use egui::{Color32, RichText, ScrollArea};

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = app.sector.clone() else {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::chrome_bg()))
            .show(ctx, |ui| {
                ui.label(RichText::new("no sector loaded").color(palette::chrome_text_dim()));
            });
        return;
    };
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(palette::chrome_bg())
                .inner_margin(14.0),
        )
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    RichText::new("WARP REGIONS")
                        .color(palette::chrome_text())
                        .strong(),
                );
                ui.label(
                    RichText::new("§5 NEW.md — regional warp phenomena overlay")
                        .color(palette::chrome_text_dim()),
                );
                ui.add_space(8.0);
                if sector.regions.is_empty() {
                    ui.label(
                        RichText::new(
                            "no regions configured — enable in regions.toml or \
                            sectorforge.toml",
                        )
                        .color(palette::chrome_text_dim()),
                    );
                    return;
                }
                egui::Grid::new("regions_grid")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("ID")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("NAME")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("KIND")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("DESCRIPTION")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("HEXES")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("CENTRE")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.end_row();
                        for r in sector.regions.iter() {
                            ui.label(RichText::new(&r.id));
                            ui.label(RichText::new(&r.name));
                            ui.label(
                                // Data-viz: region-kind hue, not a UI status color (AREA_F F5).
                                RichText::new(r.kind.label())
                                    .color(Color32::from_rgb(220, 160, 60)),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(r.kind.description())
                                        .color(palette::chrome_text_dim()),
                                )
                                .wrap(),
                            );
                            ui.label(RichText::new(r.hexes.len().to_string()));
                            ui.label(RichText::new(format!("({},{})", r.centre.q, r.centre.r)));
                            ui.end_row();
                        }
                    });
            });
        });
}
