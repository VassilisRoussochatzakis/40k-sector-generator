use egui::{Color32, RichText, ScrollArea, SidePanel};

use crate::{
    sector_model::GeneratedSector,
};

use super::{palette, App, TEXT, TEXT_DIM};
use crate::gui::route_planner::{self, Metric, PickTarget, Severity};

impl App {
    pub(super) fn draw_planner_layout(&mut self, ctx: &egui::Context) {
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

        SidePanel::right("planner_info")
            .resizable(true)
            .default_width(360.0)
            .min_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new("NAV-PLANNER").color(TEXT).monospace().strong());
                    ui.label(
                        RichText::new("§3 NEXT — optimal warp routing")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                    ui.add_space(8.0);

                    self.draw_planner_panel(ui, &sector);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.planner_hex_size, 20.0..=80.0).show_value(false),
                    );
                    ui.separator();
                    if ui.button(RichText::new("CLEAR PLAN").monospace()).clicked() {
                        self.planner.clear();
                        self.recompute_plan();
                    }
                });

                let path_routes = self.planner.highlighted_route_ids();
                let path_waypoints = self.planner.waypoint_set();

                ScrollArea::both().show(ui, |ui| {
                    let (_resp, click) = crate::gui::sector_view::SectorView {
                        sector: &sector,
                        selected_system: self.planner.from.as_ref().map(|id| id.as_str()),
                        selected_route: None,
                        hex_size: self.planner_hex_size,
                        path_route_ids: Some(&path_routes),
                        path_waypoints: Some(&path_waypoints),
                        subsectors: Some(self.subsectors.as_slice()),
                        selected_subsector: None,
                        heatmap: None,
                        empty_hex_clicks: false,
                        route_view_mode: self.route_view_mode,
                    }
                    .show(ui);

                    match click {
                        Some(crate::gui::sector_view::SectorClick::System(id)) => {
                            self.planner.click_system(id.as_str());
                            self.recompute_plan();
                        }
                        Some(
                            crate::gui::sector_view::SectorClick::Route(_)
                            | crate::gui::sector_view::SectorClick::Subsector(_)
                            | crate::gui::sector_view::SectorClick::EmptyHex(_),
                        ) | None => {}
                    }
                });
            });
    }

    pub(super) fn draw_planner_panel(&mut self, ui: &mut egui::Ui, sector: &GeneratedSector) {
        ui.label(
            RichText::new("ROUTE PLANNER")
                .color(TEXT)
                .monospace()
                .strong(),
        );
        ui.add_space(6.0);

        let options: Vec<(crate::ids::SystemId, String)> = sector
            .systems
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();

        let mut dirty = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("FROM").color(TEXT_DIM).monospace());
            let armed = self.planner.picker == PickTarget::From;
            if ui
                .selectable_label(armed, RichText::new("◎ PICK").monospace())
                .on_hover_text("arm picker — next map click sets FROM")
                .clicked()
            {
                self.planner.picker = if armed {
                    PickTarget::None
                } else {
                    PickTarget::From
                };
            }
        });
        dirty |= system_combo(ui, "planner_from", &mut self.planner.from, &options);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("TO").color(TEXT_DIM).monospace());
            let armed = self.planner.picker == PickTarget::To;
            if ui
                .selectable_label(armed, RichText::new("◎ PICK").monospace())
                .on_hover_text("arm picker — next map click sets TO")
                .clicked()
            {
                self.planner.picker = if armed {
                    PickTarget::None
                } else {
                    PickTarget::To
                };
            }
        });
        dirty |= system_combo(ui, "planner_to", &mut self.planner.to, &options);

        if self.planner.picker != PickTarget::None {
            ui.add_space(4.0);
            let target = match self.planner.picker {
                PickTarget::From => "FROM",
                PickTarget::To => "TO",
                PickTarget::None => "",
            };
            ui.label(
                RichText::new(format!("◉ click a hex to set {}", target))
                    .color(palette::PATH_WAYPOINT)
                    .monospace(),
            );
        }

        ui.add_space(6.0);
        ui.label(RichText::new("METRIC").color(TEXT_DIM).monospace());
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Safest,
                    RichText::new("SAFEST").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Safest;
                dirty = true;
            }
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Shortest,
                    RichText::new("SHORTEST").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Shortest;
                dirty = true;
            }
            if ui
                .selectable_label(
                    self.planner.metric == Metric::Strategic,
                    RichText::new("STRATEGIC").monospace(),
                )
                .clicked()
            {
                self.planner.metric = Metric::Strategic;
                dirty = true;
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("PLAN").monospace()).clicked() {
                dirty = true;
            }
            if ui.button(RichText::new("CLEAR").monospace()).clicked() {
                self.planner.clear();
            }
            if let (Some(a), Some(b)) = (self.planner.from.clone(), self.planner.to.clone()) {
                if ui.button(RichText::new("SWAP").monospace()).clicked() {
                    self.planner.from = Some(b);
                    self.planner.to = Some(a);
                    dirty = true;
                }
            }
        });

        if dirty {
            self.recompute_plan();
        }

        if !self.planner.status.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(&self.planner.status)
                    .color(Color32::from_rgb(235, 90, 90))
                    .monospace(),
            );
        }

        ui.add_space(10.0);
        ui.separator();

        if let Some(plan) = self.planner.plan.clone() {
            let name_of = |id: &str| -> String {
                sector
                    .systems
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.to_string())
            };
            let metric_label = match plan.metric {
                Metric::Safest => "SAFEST",
                Metric::Shortest => "SHORTEST",
                Metric::Strategic => "STRATEGIC",
            };
            let cost_label = match plan.metric {
                Metric::Safest => format!("risk score {:.1}", plan.total_cost),
                Metric::Shortest => format!("{} hops", plan.total_cost as i64),
                Metric::Strategic => format!("strategic cost {:.1}", plan.total_cost),
            };
            ui.label(
                RichText::new(format!("PATH ({}) — {}", metric_label, cost_label))
                    .color(TEXT)
                    .monospace()
                    .strong(),
            );
            ui.add_space(4.0);
            for (i, hop) in plan.hops.iter().enumerate() {
                let prefix = if i == 0 {
                    "◉"
                } else if i == plan.hops.len() - 1 {
                    "◎"
                } else {
                    "→"
                };
                ui.label(
                    RichText::new(format!("{} {}", prefix, name_of(hop)))
                        .color(TEXT)
                        .monospace(),
                );
            }

            ui.add_space(8.0);
            if plan.hazards.is_empty() {
                ui.label(
                    RichText::new("no hazards along path")
                        .color(palette::stability_color(
                            crate::sector_model::RouteStability::Stable,
                        ))
                        .monospace(),
                );
            } else {
                ui.label(RichText::new("HAZARDS").color(TEXT_DIM).monospace());
                for h in &plan.hazards {
                    let color = match h.severity {
                        Severity::Danger => Color32::from_rgb(235, 90, 90),
                        Severity::Caution => Color32::from_rgb(240, 200, 90),
                        Severity::Info => TEXT_DIM,
                    };
                    ui.label(
                        RichText::new(format!(
                            "{}  {} ↔ {}",
                            severity_tag(h.severity),
                            name_of(&h.from),
                            name_of(&h.to)
                        ))
                        .color(color)
                        .monospace(),
                    );
                    ui.label(
                        RichText::new(format!("   {}", h.note))
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                }
            }
        } else if self.planner.from.is_some() || self.planner.to.is_some() {
            ui.label(
                RichText::new("pick both endpoints to plan a route")
                    .color(TEXT_DIM)
                    .monospace(),
            );
        } else {
            ui.label(
                RichText::new("click a system hex to set the origin")
                    .color(TEXT_DIM)
                    .monospace(),
            );
        }
    }

    pub(super) fn recompute_plan(&mut self) {
        self.planner.status.clear();
        self.planner.plan = None;
        let Some(sector) = &self.sector else { return };
        let (Some(from), Some(to)) = (self.planner.from.clone(), self.planner.to.clone()) else {
            return;
        };
        if from == to {
            self.planner.status = "origin and destination are the same".to_string();
            return;
        }
        match route_planner::plan_route(sector, from.as_str(), to.as_str(), self.planner.metric) {
            Some(p) => self.planner.plan = Some(p),
            None => {
                self.planner.status =
                    "no passable route — try the other metric or check for Perilous lanes"
                        .to_string();
            }
        }
    }
}

fn system_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<crate::ids::SystemId>,
    options: &[(crate::ids::SystemId, String)],
) -> bool {
    let mut changed = false;
    let label = value
        .as_ref()
        .and_then(|sel| {
            options
                .iter()
                .find(|(oid, _)| oid == sel)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| "—".to_string());
    egui::ComboBox::from_id_salt(id)
        .selected_text(RichText::new(label).monospace())
        .width(220.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value.is_none(), "— none —").clicked() && value.is_some() {
                *value = None;
                changed = true;
            }
            for (oid, name) in options {
                let sel = value.as_deref() == Some(oid.as_str());
                if ui
                    .selectable_label(sel, RichText::new(name).monospace())
                    .clicked()
                    && !sel
                {
                    *value = Some(oid.clone());
                    changed = true;
                }
            }
        });
    changed
}

fn severity_tag(s: Severity) -> &'static str {
    match s {
        Severity::Danger => "[!!]",
        Severity::Caution => "[!]",
        Severity::Info => "[·]",
    }
}
