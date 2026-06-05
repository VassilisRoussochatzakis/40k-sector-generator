use crate::{palette, App};
use egui::{RichText, ScrollArea};

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
                    RichText::new("TRADE & ECONOMY")
                        .color(palette::chrome_text())
                        .strong(),
                );
                ui.label(
                    RichText::new("§12 NEW.md — trade volume + resource balance")
                        .color(palette::chrome_text_dim()),
                );
                ui.add_space(8.0);
                if !sector.economy.enabled {
                    ui.label(
                        RichText::new("economy derivation disabled — set [economy].enabled = true")
                            .color(palette::chrome_text_dim()),
                    );
                    return;
                }
                ui.label(
                    RichText::new("SECTOR BALANCE")
                        .color(palette::chrome_text())
                        .strong(),
                );
                egui::Grid::new("sector_balance")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in sectorforge::economy::RESOURCE_KEYS {
                            let v = sector.economy.sector_balance.get(k);
                            ui.label(RichText::new(*k).color(palette::chrome_text_dim()));
                            ui.label(RichText::new(format!("{:.1}", v)));
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new("STRATEGIC OUTPUT")
                        .color(palette::chrome_text())
                        .strong(),
                );
                egui::Grid::new("strategic_output")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in sectorforge::economy::STRATEGIC_RESOURCE_KEYS {
                            let v = sector.economy.strategic_output.get(k);
                            ui.label(RichText::new(*k).color(palette::chrome_text_dim()));
                            ui.label(RichText::new(format!("{:.1}", v)));
                            ui.end_row();
                        }
                    });
                let stressed: Vec<_> = sector
                    .economy
                    .systems
                    .iter()
                    .filter(|s| {
                        s.supply_risk >= sectorforge::economy::SupplyRisk::Disrupted
                            || matches!(
                                s.tithe_status,
                                sectorforge::economy::TitheStatus::Delinquent
                                    | sectorforge::economy::TitheStatus::Failed
                                    | sectorforge::economy::TitheStatus::Falsified
                            )
                    })
                    .collect();
                if !stressed.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("TITHE / SUPPLY STRESS")
                            .color(palette::warning())
                            .strong(),
                    );
                    egui::Grid::new("tithe_supply_stress")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("SYSTEM").color(palette::chrome_text_dim()));
                            ui.label(RichText::new("TITHE").color(palette::chrome_text_dim()));
                            ui.label(RichText::new("SUPPLY").color(palette::chrome_text_dim()));
                            ui.label(RichText::new("PRIORITY").color(palette::chrome_text_dim()));
                            ui.end_row();
                            for sy in stressed.iter().take(12) {
                                ui.label(RichText::new(&sy.system_id));
                                ui.label(RichText::new(format!("{}", sy.tithe_status)));
                                ui.label(RichText::new(format!("{}", sy.supply_risk)));
                                ui.label(RichText::new(format!("{}", sy.strategic_priority)));
                                ui.end_row();
                            }
                        });
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new("TOP TRADE LANES")
                        .color(palette::chrome_text())
                        .strong(),
                );
                let mut routes: Vec<_> = sector.economy.routes.iter().collect();
                routes.sort_by(|a, b| {
                    b.volume
                        .partial_cmp(&a.volume)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                egui::Grid::new("trade_routes")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("FROM")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("TO")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("VOLUME")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.label(
                            RichText::new("FRICTION")
                                .color(palette::chrome_text_dim())
                                .strong(),
                        );
                        ui.end_row();
                        for r in routes.iter().take(20) {
                            ui.label(RichText::new(&r.from_system_id));
                            ui.label(RichText::new(&r.to_system_id));
                            ui.label(RichText::new(format!("{:.1}", r.volume)));
                            ui.label(RichText::new(format!("{:.2}", r.friction)));
                            ui.end_row();
                        }
                    });
                let stranded: Vec<_> = sector
                    .economy
                    .worlds
                    .iter()
                    .filter(|w| w.stranded)
                    .collect();
                if !stranded.is_empty() {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("STRANDED WORLDS")
                            .color(palette::danger())
                            .strong(),
                    );
                    for w in stranded {
                        ui.label(RichText::new(format!(
                            "{} in {} — {}",
                            w.world_id,
                            w.system_id,
                            if w.shortages.is_empty() {
                                "(systemic)".to_string()
                            } else {
                                w.shortages.join(", ")
                            }
                        )));
                    }
                }
            });
        });
}
