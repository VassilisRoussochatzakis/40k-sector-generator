use crate::{palette, App};
use egui::{RichText, ScrollArea};

// VAPP-4: this view is an intentional read-only summary dump of the relation pairs
// (public/secret attitude via `{:?}`). No filter/sort/selection is provided by design — it is a
// glanceable diplomacy overview, not an editor. Leave behaviour as-is unless a feature is requested.
pub fn ui(app: &mut App, ctx: &egui::Context) {
    let Some(sector) = super::require_sector(app, ctx) else {
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
                    .strong(),
            );
            ui.label(
                RichText::new("§5 NEW2.md — public/secret attitude + relation dimensions")
                    .color(palette::chrome_text_dim()),
            );
            ui.add_space(8.0);
            if sector.relations.pairs.is_empty() {
                ui.label(RichText::new("no relations defined").color(palette::chrome_text_dim()));
            } else {
                let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
                let total = sector.relations.pairs.len();
                ScrollArea::vertical().show_rows(ui, row_h, total, |ui, range| {
                    for i in range {
                        let p = &sector.relations.pairs[i];
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} ↔ {}", p.a, p.b)));
                            ui.label(RichText::new(format!(
                                "{:?}/{:?}",
                                p.public_attitude, p.secret_attitude
                            )));
                        });
                    }
                });
            }
        });
}
