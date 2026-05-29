use crate::palette::{TEXT, TEXT_DIM};
use crate::{palette, App};
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
                    RichText::new("TRADE & ECONOMY")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                ui.label(
                    RichText::new("§12 NEW.md — trade volume + resource balance")
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.add_space(8.0);
                if !sector.economy.enabled {
                    ui.label(
                        RichText::new("economy derivation disabled — set [economy].enabled = true")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                    return;
                }
                ui.label(
                    RichText::new("SECTOR BALANCE")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                egui::Grid::new("sector_balance")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in sectorforge::economy::RESOURCE_KEYS {
                            let v = sector.economy.sector_balance.get(k);
                            ui.label(RichText::new(*k).color(TEXT_DIM).monospace());
                            ui.label(RichText::new(format!("{:.1}", v)).monospace());
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new("STRATEGIC OUTPUT")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                egui::Grid::new("strategic_output")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in sectorforge::economy::STRATEGIC_RESOURCE_KEYS {
                            let v = sector.economy.strategic_output.get(k);
                            ui.label(RichText::new(*k).color(TEXT_DIM).monospace());
                            ui.label(RichText::new(format!("{:.1}", v)).monospace());
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
                            .color(Color32::from_rgb(235, 190, 90))
                            .monospace()
                            .strong(),
                    );
                    egui::Grid::new("tithe_supply_stress")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("SYSTEM").color(TEXT_DIM).monospace());
                            ui.label(RichText::new("TITHE").color(TEXT_DIM).monospace());
                            ui.label(RichText::new("SUPPLY").color(TEXT_DIM).monospace());
                            ui.label(RichText::new("PRIORITY").color(TEXT_DIM).monospace());
                            ui.end_row();
                            for sy in stressed.iter().take(12) {
                                ui.label(RichText::new(&sy.system_id).monospace());
                                ui.label(RichText::new(format!("{}", sy.tithe_status)).monospace());
                                ui.label(RichText::new(format!("{}", sy.supply_risk)).monospace());
                                ui.label(
                                    RichText::new(format!("{}", sy.strategic_priority)).monospace(),
                                );
                                ui.end_row();
                            }
                        });
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new("TOP TRADE LANES")
                        .color(TEXT)
                        .monospace()
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
                        ui.label(RichText::new("FROM").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("TO").color(TEXT_DIM).monospace().strong());
                        ui.label(RichText::new("VOLUME").color(TEXT_DIM).monospace().strong());
                        ui.label(
                            RichText::new("FRICTION")
                                .color(TEXT_DIM)
                                .monospace()
                                .strong(),
                        );
                        ui.end_row();
                        for r in routes.iter().take(20) {
                            ui.label(RichText::new(&r.from_system_id).monospace());
                            ui.label(RichText::new(&r.to_system_id).monospace());
                            ui.label(RichText::new(format!("{:.1}", r.volume)).monospace());
                            ui.label(RichText::new(format!("{:.2}", r.friction)).monospace());
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
                            .color(Color32::from_rgb(235, 90, 90))
                            .monospace()
                            .strong(),
                    );
                    for w in stranded {
                        ui.label(
                            RichText::new(format!(
                                "{} in {} — {}",
                                w.world_id,
                                w.system_id,
                                if w.shortages.is_empty() {
                                    "(systemic)".to_string()
                                } else {
                                    w.shortages.join(", ")
                                }
                            ))
                            .monospace(),
                        );
                    }
                }
            });
        });
}
