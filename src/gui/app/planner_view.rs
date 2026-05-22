use egui::{Color32, RichText, ScrollArea, SidePanel};

use crate::{
    sector_model::GeneratedSector,
};

use super::{palette, App, TEXT, TEXT_DIM};
use crate::gui::route_planner::{self, Metric, PickTarget, Severity};

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

                draw_planner_panel(ui, app, &sector);
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(palette::BG))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                ui.add(
                    egui::Slider::new(&mut app.planner_hex_size, 20.0..=80.0).show_value(false),
                );
                ui.separator();
                if ui.button(RichText::new("CLEAR PLAN").monospace()).clicked() {
                    app.planner.clear();
                    recompute_plan(app);
                }
            });

            let path_routes = app.planner.highlighted_route_ids();
            let path_waypoints = app.planner.waypoint_set();

            ScrollArea::both().show(ui, |ui| {
                let (_resp, click) = crate::gui::sector_view::SectorView {
                    sector: &sector,
                    selected_system: app.planner.from.as_ref().map(|id| id.as_str()),
                    selected_route: None,
                    hex_size: app.planner_hex_size,
                    path_route_ids: Some(&path_routes),
                    path_waypoints: Some(&path_waypoints),
                    subsectors: Some(app.subsectors.as_slice()),
                    selected_subsector: None,
                    heatmap: None,
                    empty_hex_clicks: false,
                    route_view_mode: app.route_view_mode,
                }
                .show(ui);

                match click {
                    Some(crate::gui::sector_view::SectorClick::System(id)) => {
                        app.planner.click_system(id.as_str());
                        recompute_plan(app);
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

fn draw_planner_panel(ui: &mut egui::Ui, app: &mut App, sector: &GeneratedSector) {
    ui.label(
        RichText::new("ROUTE PLANNER")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    ui.add_space(6.0);

    let options: Vec<(crate::ids::SystemId, std::sync::Arc<str>)> = sector
        .systems
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();

    let mut dirty = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("FROM").color(TEXT_DIM).monospace());
        let armed = app.planner.picker == PickTarget::From;
        if ui
            .selectable_label(armed, RichText::new("◎ PICK").monospace())
            .on_hover_text("arm picker — next map click sets FROM")
            .clicked()
        {
            app.planner.picker = if armed {
                PickTarget::None
            } else {
                PickTarget::From
            };
        }
    });
    dirty |= system_combo(ui, "planner_from", &mut app.planner.from, &options);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("TO").color(TEXT_DIM).monospace());
        let armed = app.planner.picker == PickTarget::To;
        if ui
            .selectable_label(armed, RichText::new("◎ PICK").monospace())
            .on_hover_text("arm picker — next map click sets TO")
            .clicked()
        {
            app.planner.picker = if armed {
                PickTarget::None
            } else {
                PickTarget::To
            };
        }
    });
    dirty |= system_combo(ui, "planner_to", &mut app.planner.to, &options);

    if app.planner.picker != PickTarget::None {
        ui.add_space(4.0);
        let target = match app.planner.picker {
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
                app.planner.metric == Metric::Safest,
                RichText::new("SAFEST").monospace(),
            )
            .clicked()
        {
            app.planner.metric = Metric::Safest;
            dirty = true;
        }
        if ui
            .selectable_label(
                app.planner.metric == Metric::Shortest,
                RichText::new("SHORTEST").monospace(),
            )
            .clicked()
        {
            app.planner.metric = Metric::Shortest;
            dirty = true;
        }
        if ui
            .selectable_label(
                app.planner.metric == Metric::Strategic,
                RichText::new("STRATEGIC").monospace(),
            )
            .clicked()
        {
            app.planner.metric = Metric::Strategic;
            dirty = true;
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(RichText::new("PLAN").monospace()).clicked() {
            dirty = true;
        }
        if ui.button(RichText::new("CLEAR").monospace()).clicked() {
            app.planner.clear();
        }
        if let (Some(a), Some(b)) = (app.planner.from.clone(), app.planner.to.clone()) {
            if ui.button(RichText::new("SWAP").monospace()).clicked() {
                app.planner.from = Some(b);
                app.planner.to = Some(a);
                dirty = true;
            }
        }
    });

    if dirty {
        recompute_plan(app);
    }

    if !app.planner.status.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new(&app.planner.status)
                .color(Color32::from_rgb(235, 90, 90))
                .monospace(),
        );
    }

    ui.add_space(10.0);
    ui.separator();

    if let Some(plan) = app.planner.plan.clone() {
        let name_of = |id: &str| -> String {
            sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.to_string())
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
    } else if app.planner.from.is_some() || app.planner.to.is_some() {
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

fn recompute_plan(app: &mut App) {
    app.planner.status.clear();
    app.planner.plan = None;
    let Some(sector) = &app.sector else { return };
    let (Some(from), Some(to)) = (app.planner.from.clone(), app.planner.to.clone()) else {
        return;
    };
    if from == to {
        app.planner.status = "origin and destination are the same".to_string();
        return;
    }
    match route_planner::plan_route(sector, from.as_str(), to.as_str(), app.planner.metric) {
        Some(p) => app.planner.plan = Some(p),
        None => {
            app.planner.status =
                "no passable route — try the other metric or check for Perilous lanes"
                    .to_string();
        }
    }
}

fn system_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<crate::ids::SystemId>,
    options: &[(crate::ids::SystemId, std::sync::Arc<str>)],
) -> bool {
    let mut changed = false;
    let label = value
        .as_ref()
        .and_then(|sel| {
            options
                .iter()
                .find(|(oid, _)| oid == sel)
                .map(|(_, name)| name.to_string())
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
                    .selectable_label(sel, RichText::new(name.as_ref()).monospace())
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
