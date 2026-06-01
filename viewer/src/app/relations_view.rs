use crate::{palette, App};
use egui::{RichText, ScrollArea};

pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = app.sector.clone() else {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::chrome_bg()))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("no sector loaded")
                        .color(palette::chrome_text_dim())
                        .monospace(),
                );
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
            ui.label(
                RichText::new("DIPLOMACY MATRIX")
                    .color(palette::chrome_text())
                    .monospace()
                    .strong(),
            );
            ui.label(
                RichText::new("§5 NEW2.md — public/secret attitude + relation dimensions")
                    .color(palette::chrome_text_dim())
                    .monospace(),
            );
            ui.add_space(8.0);
            if sector.relations.pairs.is_empty() {
                ui.label(
                    RichText::new("no relations defined")
                        .color(palette::chrome_text_dim())
                        .monospace(),
                );
            } else {
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
                let total = sector.relations.pairs.len();
                ScrollArea::vertical().show_rows(ui, row_h, total, |ui, range| {
                    for i in range {
                        let p = &sector.relations.pairs[i];
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} ↔ {}", p.a, p.b)).monospace());
                            ui.label(
                                RichText::new(format!(
                                    "{:?}/{:?}",
                                    p.public_attitude, p.secret_attitude
                                ))
                                .monospace(),
                            );
                        });
                    }
                });
            }
        });
}
