use egui::{Color32, RichText, ScrollArea, TopBottomPanel};

use super::{factions_overview, palette, App, TEXT, TEXT_DIM};

impl App {
    pub(super) fn draw_factions_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
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

        TopBottomPanel::top("factions_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let on_overview = matches!(self.factions_mode, super::FactionsMode::Overview);
                    let on_designer = matches!(self.factions_mode, super::FactionsMode::Designer);
                    if ui
                        .selectable_label(on_overview, RichText::new("OVERVIEW").monospace())
                        .clicked()
                    {
                        self.factions_mode = super::FactionsMode::Overview;
                    }
                    if ui
                        .selectable_label(on_designer, RichText::new("DESIGNER").monospace())
                        .clicked()
                    {
                        self.factions_mode = super::FactionsMode::Designer;
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("high-level faction state")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    match self.factions_mode {
                        super::FactionsMode::Overview => {
                            factions_overview::show_readonly(ui, &sector);
                        }
                        super::FactionsMode::Designer => {
                            factions_overview::show_designer(
                                ui,
                                &sector,
                                &mut self.faction_designer,
                                self.project_dir.as_deref(),
                            );
                        }
                    }
                });
            });
    }

    pub(super) fn draw_relations_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
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
                ui.label(
                    RichText::new("DIPLOMACY MATRIX")
                        .color(TEXT)
                        .monospace()
                        .strong(),
                );
                ui.label(
                    RichText::new("§5 NEW2.md — public/secret attitude + relation dimensions")
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.add_space(8.0);
                if sector.relations.pairs.is_empty() {
                    ui.label(
                        RichText::new(
                            "relations matrix is empty (need ≥2 factions or set \
                            inputs.relations in sectorforge.toml)",
                        )
                        .color(TEXT_DIM)
                        .monospace(),
                    );
                    return;
                }
                const COL_A: f32 = 220.0;
                const COL_B: f32 = 220.0;
                const COL_STANCE: f32 = 96.0;
                const COL_TREATY: f32 = 110.0;
                const COL_TENSION: f32 = 70.0;
                let header = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [COL_A, 0.0],
                            egui::Label::new(
                                RichText::new("A").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_B, 0.0],
                            egui::Label::new(
                                RichText::new("B").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_STANCE, 0.0],
                            egui::Label::new(
                                RichText::new("PUBLIC").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_STANCE, 0.0],
                            egui::Label::new(
                                RichText::new("SECRET").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_TREATY, 0.0],
                            egui::Label::new(
                                RichText::new("TREATY").color(TEXT_DIM).monospace().strong(),
                            ),
                        );
                        ui.add_sized(
                            [COL_TENSION, 0.0],
                            egui::Label::new(
                                RichText::new("TENSION")
                                    .color(TEXT_DIM)
                                    .monospace()
                                    .strong(),
                            ),
                        );
                        ui.label(RichText::new("CAUSE").color(TEXT_DIM).monospace().strong());
                    });
                };
                header(ui);
                ui.separator();
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
                let total = sector.relations.pairs.len();
                ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
                    ui,
                    row_h,
                    total,
                    |ui, range| {
                        for i in range {
                            let p = &sector.relations.pairs[i];
                            let public_color = stance_color(p.public_stance);
                            let secret_color = stance_color(p.secret_stance);
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [COL_A, row_h],
                                    egui::Label::new(RichText::new(&p.a).monospace()),
                                );
                                ui.add_sized(
                                    [COL_B, row_h],
                                    egui::Label::new(RichText::new(&p.b).monospace()),
                                );
                                ui.add_sized(
                                    [COL_STANCE, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:?}", p.public_attitude))
                                            .color(public_color)
                                            .monospace(),
                                    ),
                                );
                                ui.add_sized(
                                    [COL_STANCE, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:?}", p.secret_attitude))
                                            .color(secret_color)
                                            .monospace(),
                                    ),
                                );
                                ui.add_sized(
                                    [COL_TREATY, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:?}", p.treaty_status)).monospace(),
                                    ),
                                );
                                ui.add_sized(
                                    [COL_TENSION, row_h],
                                    egui::Label::new(
                                        RichText::new(format!("{:.0}", p.tension)).monospace(),
                                    ),
                                );
                                ui.label(RichText::new(&p.cause).color(TEXT_DIM).monospace());
                            });
                        }
                    },
                );
            });
    }

    pub(super) fn draw_regions_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
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
                        .num_columns(5)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("ID").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("NAME").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("KIND").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("HEXES").color(TEXT_DIM).monospace().strong());
                            ui.label(RichText::new("CENTRE").color(TEXT_DIM).monospace().strong());
                            ui.end_row();
                            for r in &sector.regions {
                                ui.label(RichText::new(&r.id).monospace());
                                ui.label(RichText::new(&r.name).monospace());
                                ui.label(
                                    RichText::new(format!("{:?}", r.kind))
                                        .color(Color32::from_rgb(220, 160, 60))
                                        .monospace(),
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

    pub(super) fn draw_trade_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
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
                            RichText::new(
                                "economy derivation disabled — set [economy].enabled = true",
                            )
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
                            for k in crate::economy::RESOURCE_KEYS {
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
                            for k in crate::economy::STRATEGIC_RESOURCE_KEYS {
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
                            s.supply_risk >= crate::economy::SupplyRisk::Disrupted
                                || matches!(
                                    s.tithe_status,
                                    crate::economy::TitheStatus::Delinquent
                                        | crate::economy::TitheStatus::Failed
                                        | crate::economy::TitheStatus::Falsified
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
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.tithe_status)).monospace(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.supply_risk)).monospace(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:?}", sy.strategic_priority))
                                            .monospace(),
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
}

fn stance_color(s: crate::relations::Stance) -> Color32 {
    use crate::relations::Stance;
    match s {
        Stance::Allied => Color32::from_rgb(90, 200, 110),
        Stance::Aligned => Color32::from_rgb(160, 220, 140),
        Stance::Neutral => Color32::from_rgb(190, 190, 190),
        Stance::Rival => Color32::from_rgb(240, 200, 90),
        Stance::Hostile => Color32::from_rgb(235, 130, 60),
        Stance::AtWar => Color32::from_rgb(235, 90, 90),
    }
}
