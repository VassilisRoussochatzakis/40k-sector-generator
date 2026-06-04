//! ECONOMY tab (§N1 / §N2) — Phase C §E1..§E7.
//!
//! §E1  per-world `ResourceVector` override (6-tuple signed sliders).
//! §E2  per-world `StrategicOutput` editor (10 strategic flags + reset).
//! §E3  per-system tithe / supply / strategic_priority override row.
//! §E4  stranded-world recompute on every edit; the inspector shows a "stranded"
//!      badge and the MAP panel draws a red ring around stranded systems.
//! §E5  `economy.toml` config editor: enabled / feed_stability /
//!      `by_world_type` / `by_tech_level` / `by_population` rows.
//! §E6  lifeline-lane visualiser: toggle highlights the top supplier→consumer
//!      dependency edges on the MAP route layer via `path_route_ids`.
//! §E7  trade-volume / food / tithe / supply heatmap mode picker. Drives
//!      [`BuilderState::map_heatmap_mode`] which the MAP panel consumes when no
//!      §C7 / §C8 control overlay is active.
//!
//! The panel never edits `sector.economy` directly — all mutations land in
//! [`BuilderState`] side-tables and `BuilderState::recompute_economy` rewrites
//! the live report.

use egui::{Color32, RichText, Ui};

use sectorforge::economy::{
    EconomyConfig, ResourceVector, StrategicOutput, StrategicPriority, SupplyRisk, TitheStatus,
    RESOURCE_KEYS, STRATEGIC_RESOURCE_KEYS,
};
use sectorforge::heatmap::HeatmapMode;
use sectorforge::ids::{SystemId, WorldId};
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::state::{BuilderTab, EntityRef};
use crate::builder::BuilderState;

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms while the tooltip names the underlying schema
/// field plus a plain-language note, so power users keep the mapping. Mirrors
/// the helper of the same name in `panels/factions.rs`.
fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [140.0, h],
            egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
        )
        .on_hover_text(help);
        add(ui);
    });
}

/// Human label for a `ResourceVector` component key (§E1). Falls back to the raw
/// key for any future field so nothing renders blank.
fn resource_human(key: &str) -> &'static str {
    match key {
        "ore" => "Ore",
        "promethium" => "Promethium",
        "foodstuffs" => "Foodstuffs",
        "manufactured" => "Manufactured goods",
        "archeotech" => "Archeotech",
        "recruits" => "Recruits",
        _ => "Resource",
    }
}

/// Human label for a `StrategicOutput` component key (§E2).
fn strategic_human(key: &str) -> &'static str {
    match key {
        "food" => "Food",
        "ore" => "Ore",
        "manufacturing" => "Manufacturing",
        "arms" => "Arms",
        "ships" => "Ships",
        "pilgrimage" => "Pilgrimage",
        "psyker_tithe" => "Psyker tithe",
        "manpower" => "Manpower",
        "knowledge" => "Knowledge",
        "xenos_value" => "Xenos value",
        _ => "Output",
    }
}

/// Friendly label for a heatmap mode shown in the §E7 picker; the raw mode key
/// goes on hover.
fn heatmap_human(mode: HeatmapMode) -> &'static str {
    match mode {
        HeatmapMode::Off => "Off",
        HeatmapMode::TradeVolume => "Trade volume",
        HeatmapMode::FoodOutput => "Food output",
        HeatmapMode::TitheStress => "Tithe stress",
        HeatmapMode::SupplyVulnerability => "Supply vulnerability",
        _ => mode.label(),
    }
}

const TITHE_STATES: &[TitheStatus] = &[
    TitheStatus::Surplus,
    TitheStatus::Adequate,
    TitheStatus::Strained,
    TitheStatus::Delinquent,
    TitheStatus::Failed,
    TitheStatus::Falsified,
];

const SUPPLY_RISKS: &[SupplyRisk] = &[
    SupplyRisk::Stable,
    SupplyRisk::Vulnerable,
    SupplyRisk::Disrupted,
    SupplyRisk::Collapsing,
];

const PRIORITIES: &[StrategicPriority] = &[
    StrategicPriority::Low,
    StrategicPriority::Local,
    StrategicPriority::Subsector,
    StrategicPriority::Sector,
    StrategicPriority::CrusadeLevel,
];

const E7_MODES: &[HeatmapMode] = &[
    HeatmapMode::Off,
    HeatmapMode::TradeVolume,
    HeatmapMode::FoodOutput,
    HeatmapMode::TitheStress,
    HeatmapMode::SupplyVulnerability,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Economy");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "Tune what each world and system produces, find stranded worlds, and preview supply lines on the map.",
    );
    ui.separator();

    // §COLUMNS — top actions + sector-balance summary stay full-width, then the
    // editor surfaces split: per-world / per-system overrides + the economy.toml
    // editor on the LEFT, and the width-hungry preview surfaces (stranded list,
    // lifeline lanes, heatmap picker) on the RIGHT. Hand-assigned rather than
    // round-robin so the override editors stay grouped; collapses to one column
    // (`cols[if n>1 {1} else {0}]`) on a narrow window.
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_sector_summary(ui, state);
            ui.separator();

            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left column: the per-world / per-system override editors and
                    // the economy.toml config editor.
                    let left = &mut cols[0];
                    show_world_override_editor(left, state);
                    left.add_space(4.0);
                    show_system_override_editor(left, state);
                    left.add_space(4.0);
                    show_economy_config_editor(left, state);
                }
                {
                    // Right column — or the same single column when collapsed:
                    // the width-hungry preview surfaces.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    show_stranded_list(right, state);
                    right.add_space(4.0);
                    show_lifeline_panel(right, state);
                    right.add_space(4.0);
                    show_heatmap_picker(right, state);
                }
            });
        });
}

// ── header actions ──────────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("↺  Re-derive economy")
            .on_hover_text("Recompute every world and system figure from the current map and economy settings")
            .clicked()
        {
            state.recompute_economy();
        }
        let enabled = state.sector.economy.enabled;
        let badge = if enabled {
            RichText::new("● Figures up to date").color(Color32::LIGHT_GREEN)
        } else {
            RichText::new("● Not derived yet — click Re-derive")
                .color(Color32::from_rgb(220, 170, 80))
        };
        ui.label(badge).on_hover_text(
            "Whether the economy figures have been computed for this sector (schema: economy.enabled).",
        );
        ui.label(format!(
            "{} worlds · {} systems · {} routes · {} supply links",
            state.sector.economy.worlds.len(),
            state.sector.economy.systems.len(),
            state.sector.economy.routes.len(),
            state.sector.economy.dependency_edges.len(),
        ));
    });
}

fn show_sector_summary(ui: &mut Ui, state: &BuilderState) {
    let report = &state.sector.economy;
    ui.label(RichText::new("Sector balance").strong())
        .on_hover_text(
            "Net surplus (+) or shortfall (−) of each raw resource across the whole sector (schema: economy.sector_balance).",
        );
    ui.horizontal_wrapped(|ui| {
        for k in RESOURCE_KEYS {
            let v = report.sector_balance.get(k);
            ui.label(resource_badge(k, v))
                .on_hover_text(format!("{} (schema: {k})", resource_human(k)));
        }
    });
    ui.label(RichText::new("Strategic output").strong())
        .on_hover_text(
            "Sector-wide strategic production score per category, 0–100 (schema: economy.strategic_output).",
        );
    ui.horizontal_wrapped(|ui| {
        for k in STRATEGIC_RESOURCE_KEYS {
            let v = report.strategic_output.get(k);
            ui.label(strategic_badge(k, v))
                .on_hover_text(format!("{} (schema: {k})", strategic_human(k)));
        }
    });
}

fn resource_badge(key: &str, value: f32) -> RichText {
    let label = format!("{}: {value:+.0}", resource_human(key));
    let colour = if value >= 20.0 {
        Color32::LIGHT_GREEN
    } else if value <= -20.0 {
        Color32::LIGHT_RED
    } else {
        Color32::GRAY
    };
    RichText::new(label).color(colour).monospace()
}

fn strategic_badge(key: &str, value: f32) -> RichText {
    let label = format!("{}: {value:.0}", strategic_human(key));
    let colour = if value >= 80.0 {
        Color32::LIGHT_GREEN
    } else if value >= 35.0 {
        Color32::from_rgb(220, 200, 140)
    } else if value > 0.0 {
        Color32::GRAY
    } else {
        Color32::DARK_GRAY
    };
    RichText::new(label).color(colour).monospace()
}

// ── §E1 / §E2 per-world editor ──────────────────────────────────────────────

fn show_world_override_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Per-world production", |ui| {
        if state.sector.economy.worlds.is_empty() {
            ui_kit::placeholder(
                ui,
                "No world figures yet — press “↺ Re-derive economy” above to compute them.",
            );
            return;
        }

        let selected = state.selected_world_id.clone();
        let world_options: Vec<(WorldId, String)> = state
            .sector
            .economy
            .worlds
            .iter()
            .map(|w| {
                (
                    w.world_id.clone(),
                    format!("{} ({})", w.world_id, w.system_id),
                )
            })
            .collect();

        ui.horizontal_wrapped(|ui| {
            ui.label("World:");
            let label = selected
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "(none)".into());
            ui_kit::combo("econ_world_picker", label)
                .show_ui(ui, |ui| {
                    for (id, line) in &world_options {
                        let active = selected.as_ref() == Some(id);
                        if ui.selectable_label(active, line).clicked() {
                            state.selected_world_id = Some(id.clone());
                        }
                    }
                });
            if let Some(id) = state.selected_world_id.clone() {
                if ui
                    .button("Open in World tab  →")
                    .on_hover_text("Jump to the World tab for this world")
                    .clicked()
                {
                    if let Some((sys_idx, _)) = state.find_world_indices(&id) {
                        let sys_id = state.sector.systems[sys_idx].id.clone();
                        state.focus_entity(EntityRef::World {
                            system: sys_id,
                            world: id,
                        });
                    }
                }
            }
        });

        let Some(world_id) = state.selected_world_id.clone() else {
            ui_kit::placeholder(ui, "Pick a world above to tune its production.");
            return;
        };
        let Some(entry) = state
            .sector
            .economy
            .worlds
            .iter()
            .find(|w| w.world_id == world_id)
            .cloned()
        else {
            ui_kit::placeholder(
                ui,
                "That world has no economy figures — try re-deriving.",
            );
            return;
        };

        // §E1 row.
        egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Raw resources").strong())
            .on_hover_text(
                "Net production (+) or consumption (−) of each raw resource for this world, −100…100 (schema: ResourceVector).",
            );
        let pinned = state.world_economy_overrides.contains_key(&world_id);
        let mut vector = state
            .world_economy_overrides
            .get(&world_id)
            .cloned()
            .unwrap_or_else(|| entry.vector.clone());
        let mut changed = false;
        egui::Grid::new(("econ_world_vec", world_id.as_str()))
            .num_columns(2)
            .show(ui, |ui| {
                for key in RESOURCE_KEYS {
                    ui.label(resource_human(key))
                        .on_hover_text(format!("schema: {key}"));
                    let mut v = vector.get(key);
                    if ui
                        .add(egui::Slider::new(&mut v, -100.0..=100.0).fixed_decimals(0))
                        .changed()
                    {
                        set_vector_field(&mut vector, key, v);
                        changed = true;
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            let badge = if pinned {
                RichText::new("● Your values").color(Color32::LIGHT_GREEN)
            } else {
                RichText::new("● Auto-derived").color(Color32::GRAY)
            };
            ui.label(badge).on_hover_text(
                "“Your values” means this world is pinned to the figures you set; otherwise they follow the economy settings.",
            );
            if pinned
                && ui
                    .button("↺  Back to auto")
                    .on_hover_text("Drop your overrides and let these figures derive automatically")
                    .clicked()
            {
                state.world_economy_overrides.remove(&world_id);
                state.recompute_economy();
                return;
            }
            if changed {
                state.world_economy_overrides.insert(world_id.clone(), vector);
                state.recompute_economy();
            }
        });
        if entry.stranded {
            ui.colored_label(
                Color32::LIGHT_RED,
                format!("Stranded — shortages: {}", entry.shortages.join(", ")),
            );
        }
    });

        // §E2 row.
        egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Strategic output").strong())
            .on_hover_text(
                "How much this world contributes to each strategic category, 0…100 (schema: StrategicOutput).",
            );
        let pinned = state.world_strategic_overrides.contains_key(&world_id);
        let mut strat = state
            .world_strategic_overrides
            .get(&world_id)
            .copied()
            .unwrap_or(entry.strategic_output);
        let mut changed = false;
        egui::Grid::new(("econ_world_strat", world_id.as_str()))
            .num_columns(2)
            .show(ui, |ui| {
                for key in STRATEGIC_RESOURCE_KEYS {
                    ui.label(strategic_human(key))
                        .on_hover_text(format!("schema: {key}"));
                    let mut v = strat.get(key);
                    if ui
                        .add(egui::Slider::new(&mut v, 0.0..=100.0).fixed_decimals(0))
                        .changed()
                    {
                        set_strategic_field(&mut strat, key, v);
                        changed = true;
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            let badge = if pinned {
                RichText::new("● Your values").color(Color32::LIGHT_GREEN)
            } else {
                RichText::new("● Auto-derived").color(Color32::GRAY)
            };
            ui.label(badge).on_hover_text(
                "“Your values” means this world is pinned to the figures you set; otherwise they follow the economy settings.",
            );
            if pinned
                && ui
                    .button("↺  Back to auto")
                    .on_hover_text("Drop your overrides and let these figures derive automatically")
                    .clicked()
            {
                state.world_strategic_overrides.remove(&world_id);
                state.recompute_economy();
                return;
            }
            if changed {
                state.world_strategic_overrides.insert(world_id.clone(), strat);
                state.recompute_economy();
            }
        });
    });
    });
}

fn set_vector_field(v: &mut ResourceVector, key: &str, value: f32) {
    match key {
        "ore" => v.ore = value,
        "promethium" => v.promethium = value,
        "foodstuffs" => v.foodstuffs = value,
        "manufactured" => v.manufactured = value,
        "archeotech" => v.archeotech = value,
        "recruits" => v.recruits = value,
        _ => {}
    }
}

fn set_strategic_field(s: &mut StrategicOutput, key: &str, value: f32) {
    match key {
        "food" => s.food = value,
        "ore" => s.ore = value,
        "manufacturing" => s.manufacturing = value,
        "arms" => s.arms = value,
        "ships" => s.ships = value,
        "pilgrimage" => s.pilgrimage = value,
        "psyker_tithe" => s.psyker_tithe = value,
        "manpower" => s.manpower = value,
        "knowledge" => s.knowledge = value,
        "xenos_value" => s.xenos_value = value,
        _ => {}
    }
}

// ── §E3 per-system editor ───────────────────────────────────────────────────

fn show_system_override_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Per-system tithe, supply & priority", |ui| {
        if state.sector.economy.systems.is_empty() {
            ui_kit::placeholder(
                ui,
                "No system figures yet — re-derive the economy to populate this table.",
            );
            return;
        }
        egui::Grid::new("econ_system_overrides")
            .num_columns(7)
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("System").strong());
                ui.label(RichText::new("Tithe").strong())
                    .on_hover_text("Tithe-grade owed to the Administratum (schema: tithe_status).");
                ui.label(RichText::new("Supply").strong())
                    .on_hover_text("How exposed the system's supply lines are (schema: supply_risk).");
                ui.label(RichText::new("Priority").strong())
                    .on_hover_text("Strategic importance to high command (schema: strategic_priority).");
                ui.label(RichText::new("Surplus").strong())
                    .on_hover_text("Resources this system produces a surplus of (schema: surplus_resources).");
                ui.label(RichText::new("Shortage").strong())
                    .on_hover_text("Resources this system is short on (schema: shortage_resources).");
                ui.label(RichText::new("Actions").strong());
                ui.end_row();

                let systems: Vec<_> = state.sector.economy.systems.clone();
                for sy in &systems {
                    let id = sy.system_id.clone();
                    ui.monospace(id.as_str());

                    // tithe
                    let mut tithe = sy.tithe_status;
                    let active_tithe = state.system_tithe_overrides.contains_key(&id);
                    ui_kit::combo(("tithe_cb", id.as_str()), tithe_label(tithe)).show_ui(
                        ui,
                        |ui| {
                            for t in TITHE_STATES {
                                ui.selectable_value(&mut tithe, *t, tithe_label(*t));
                            }
                        },
                    );
                    if tithe != sy.tithe_status {
                        state.system_tithe_overrides.insert(id.clone(), tithe);
                        state.recompute_economy();
                    }

                    // supply
                    let mut supply = sy.supply_risk;
                    let active_supply = state.system_supply_overrides.contains_key(&id);
                    ui_kit::combo(("supply_cb", id.as_str()), supply_label(supply)).show_ui(
                        ui,
                        |ui| {
                            for r in SUPPLY_RISKS {
                                ui.selectable_value(&mut supply, *r, supply_label(*r));
                            }
                        },
                    );
                    if supply != sy.supply_risk {
                        state.system_supply_overrides.insert(id.clone(), supply);
                        state.recompute_economy();
                    }

                    // strategic priority
                    let mut prio = sy.strategic_priority;
                    let active_prio = state.system_priority_overrides.contains_key(&id);
                    ui_kit::combo(("prio_cb", id.as_str()), priority_label(prio)).show_ui(
                        ui,
                        |ui| {
                            for p in PRIORITIES {
                                ui.selectable_value(&mut prio, *p, priority_label(*p));
                            }
                        },
                    );
                    if prio != sy.strategic_priority {
                        state.system_priority_overrides.insert(id.clone(), prio);
                        state.recompute_economy();
                    }

                    ui.label(if sy.surplus_resources.is_empty() {
                        "—".to_string()
                    } else {
                        sy.surplus_resources.join(",")
                    });
                    ui.label(if sy.shortage_resources.is_empty() {
                        "—".to_string()
                    } else {
                        sy.shortage_resources.join(",")
                    });

                    ui.horizontal(|ui| {
                        if (active_tithe || active_supply || active_prio)
                            && ui
                                .button(RichText::new("↺  Back to auto").color(Color32::LIGHT_RED))
                                .on_hover_text("Drop your tithe / supply / priority overrides for this system")
                                .clicked()
                        {
                            state.system_tithe_overrides.remove(&id);
                            state.system_supply_overrides.remove(&id);
                            state.system_priority_overrides.remove(&id);
                            state.recompute_economy();
                        }
                        if ui
                            .button("Open  →")
                            .on_hover_text("Jump to the System tab for this system")
                            .clicked()
                        {
                            state.focus_entity(EntityRef::System(id.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
    });
}

// ── §E4 stranded list ───────────────────────────────────────────────────────

fn show_stranded_list(ui: &mut Ui, state: &mut BuilderState) {
    let stranded: Vec<_> = state
        .sector
        .economy
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .cloned()
        .collect();
    let title = format!("Stranded worlds ({})", stranded.len());
    ui_kit::section(ui, &title, |ui| {
        ui.label(
            RichText::new("Worlds that can't meet their own needs. The map marks each one's system with a red ring.")
                .color(Color32::DARK_GRAY),
        );
        if stranded.is_empty() {
            ui.colored_label(Color32::DARK_GREEN, "None — every world is supplied.");
            return;
        }
        for w in &stranded {
            ui.horizontal(|ui| {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    format!(
                        "● {} in {} — shortages: {}",
                        w.world_id,
                        w.system_id,
                        if w.shortages.is_empty() {
                            "(systemic)".into()
                        } else {
                            w.shortages.join(", ")
                        }
                    ),
                );
                if ui
                    .button("Open  →")
                    .on_hover_text("Jump to the World tab for this stranded world")
                    .clicked()
                {
                    let sys_id = w.system_id.clone();
                    state.focus_entity(EntityRef::World {
                        system: sys_id,
                        world: w.world_id.clone(),
                    });
                }
            });
        }
    });
}

// ── §E6 lifeline-lane panel ─────────────────────────────────────────────────

fn show_lifeline_panel(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Supply lines (lifelines)", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut state.economy_highlight_lifelines,
                "💡  Show supply lines on the map",
            )
            .on_hover_text("Highlight the busiest supplier → consumer routes on the map's route layer.");
            ui.label("Min. importance:")
                .on_hover_text("Only show supply lines scoring at least this high (schema: economy_lifeline_min_score).");
            ui.add(
                egui::DragValue::new(&mut state.economy_lifeline_min_score)
                    .range(0.0..=200.0)
                    .speed(1.0),
            );
            if ui
                .button("Open map  →")
                .on_hover_text("Jump to the Map tab to see the highlighted supply lines")
                .clicked()
            {
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
        });
        let mut edges: Vec<_> = state
            .sector
            .economy
            .dependency_edges
            .iter()
            .filter(|e| e.score >= state.economy_lifeline_min_score)
            .cloned()
            .collect();
        edges.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if edges.is_empty() {
            ui_kit::placeholder(
                ui,
                "No supply lines this important — lower the minimum to see more.",
            );
            return;
        }
        let mut focus_route: Option<sectorforge::ids::RouteId> = None;
        for e in edges.iter().take(20) {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{} → {} ({})  importance {:.1}  ·  {} supply",
                    e.from_system_id,
                    e.to_system_id,
                    resource_human(&e.resource),
                    e.score,
                    supply_label(e.risk)
                ));
                if let Some(route_id) = e.route_id.as_ref() {
                    if ui
                        .small_button("Focus route")
                        .on_hover_text("Select and centre this route on the map")
                        .clicked()
                    {
                        focus_route = Some(route_id.clone());
                    }
                }
            });
        }
        if let Some(rid) = focus_route {
            state.focus_entity(EntityRef::Route(rid));
        }
    });
}

// ── §E7 heatmap picker ──────────────────────────────────────────────────────

fn show_heatmap_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Map heatmap", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Shade map by:").on_hover_text(
                "Tint the map by an economy figure (schema: map_heatmap_mode).",
            );
            let current = state.map_heatmap_mode;
            ui_kit::combo("econ_heatmap_mode", heatmap_human(current)).show_ui(ui, |ui| {
                for mode in E7_MODES {
                    ui.selectable_value(&mut state.map_heatmap_mode, *mode, heatmap_human(*mode))
                        .on_hover_text(format!("key: {}", mode.as_slug()));
                }
            });
            if ui
                .button("Open map  →")
                .on_hover_text("Jump to the Map tab to see the shading")
                .clicked()
            {
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
            ui.colored_label(
                Color32::DARK_GRAY,
                "A control overlay on the map takes precedence over this.",
            );
        });
    });
}

// ── §E5 economy.toml editor ─────────────────────────────────────────────────

fn show_economy_config_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Economy rules", |ui| {
        if state.data_catalogs.economy.is_none() {
            ui_kit::placeholder(
                ui,
                "No economy rules loaded — create a starter set to tune how production is derived.",
            );
            ui.add_space(4.0);
            if ui
                .button("➕  Create starter rules")
                .on_hover_text("Start a new economy.toml with sensible defaults you can edit")
                .clicked()
            {
                state.data_catalogs.economy = Some(EconomyConfig {
                    enabled: true,
                    ..EconomyConfig::default()
                });
                if state.config.inputs.economy.is_none() {
                    state.config.inputs.economy = Some("data/worlds/economy.toml".into());
                }
                state.dirty = true;
            }
            return;
        }
        let mut cfg = state
            .data_catalogs
            .economy
            .as_ref()
            .expect("checked above")
            .clone();
        let mut changed = false;
        let mut save_clicked = false;
        let mut recompute_clicked = false;

        labeled(
            ui,
            "Economy on",
            "Turn the economy derivation on or off for this sector (schema: enabled).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.enabled, "").changed();
            },
        );
        labeled(
            ui,
            "Feed stability",
            "Let economic strain feed back into route stability (schema: feed_stability).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.feed_stability, "").changed();
            },
        );

        show_world_type_rows(ui, &mut cfg, &mut changed);
        show_tech_rows(ui, &mut cfg, &mut changed);
        show_pop_rows(ui, &mut cfg, &mut changed);

        ui.horizontal(|ui| {
            if ui
                .button("💾  Save rules")
                .on_hover_text("Write these economy rules back to economy.toml")
                .clicked()
            {
                save_clicked = true;
            }
            if ui
                .button("↺  Apply & re-derive")
                .on_hover_text("Save in memory and recompute every world and system figure")
                .clicked()
            {
                recompute_clicked = true;
            }
        });

        if changed {
            state.data_catalogs.economy = Some(cfg);
            state.dirty = true;
            if let Some(rel) = state.config.inputs.economy.clone() {
                state.dirty_files.insert(rel);
            }
            state.mark_validation_dirty();
        }
        if save_clicked {
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save economy.toml failed: {e}"
                )));
            }
        }
        if recompute_clicked {
            state.recompute_economy();
        }
    });
}

fn show_world_type_rows(ui: &mut Ui, cfg: &mut EconomyConfig, changed: &mut bool) {
    ui.collapsing(
        format!("Resource bonus by world type ({})", cfg.by_world_type.len()),
        |ui| {
            ui.label(
                RichText::new("Per world type, how much it adds to or removes from each raw resource (schema: by_world_type).")
                    .color(Color32::DARK_GRAY),
            );
            let mut remove: Option<String> = None;
            for (key, vec) in cfg.by_world_type.iter_mut() {
                ui.label(RichText::new(key).monospace());
                egui::Grid::new(("econ_wt", key.as_str()))
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in RESOURCE_KEYS {
                            ui.label(resource_human(k))
                                .on_hover_text(format!("schema: {k}"));
                            let mut v = vec.get(k);
                            if ui
                                .add(
                                    egui::DragValue::new(&mut v)
                                        .range(-100.0..=100.0)
                                        .speed(0.5),
                                )
                                .changed()
                            {
                                set_vector_field(vec, k, v);
                                *changed = true;
                            }
                            ui.end_row();
                        }
                    });
                if ui
                    .button(RichText::new("🗑  Remove").color(Color32::LIGHT_RED))
                    .on_hover_text(format!("Remove the “{key}” row"))
                    .clicked()
                {
                    remove = Some(key.clone());
                }
                ui.separator();
            }
            if let Some(k) = remove {
                cfg.by_world_type.remove(&k);
                *changed = true;
            }
            ui.horizontal(|ui| {
                let key = egui::Id::new("economy_world_type_new_buf");
                ui.label("Add world type:");
                let (buf, resp) = crate::builder::panels::persistent_singleline(ui, key, "");
                if (resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !buf.is_empty())
                    && !cfg.by_world_type.contains_key(&buf)
                {
                    cfg.by_world_type.insert(buf, ResourceVector::default());
                    *changed = true;
                    crate::builder::panels::persistent_text_clear(ui, key);
                }
            });
        },
    );
}

fn show_tech_rows(ui: &mut Ui, cfg: &mut EconomyConfig, changed: &mut bool) {
    ui.collapsing(
        format!("Output multiplier by tech level ({})", cfg.by_tech_level.len()),
        |ui| {
            ui.label(
                RichText::new("Per tech level, a multiplier on that world's output (schema: by_tech_level).")
                    .color(Color32::DARK_GRAY),
            );
            let mut remove: Option<String> = None;
            for (key, v) in cfg.by_tech_level.iter_mut() {
                ui.horizontal(|ui| {
                    ui.monospace(key);
                    if ui
                        .add(egui::DragValue::new(v).range(0.0..=4.0).speed(0.05))
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .button(RichText::new("🗑").color(Color32::LIGHT_RED))
                        .on_hover_text(format!("Remove the “{key}” row"))
                        .clicked()
                    {
                        remove = Some(key.clone());
                    }
                });
            }
            if let Some(k) = remove {
                cfg.by_tech_level.remove(&k);
                *changed = true;
            }
        },
    );
}

fn show_pop_rows(ui: &mut Ui, cfg: &mut EconomyConfig, changed: &mut bool) {
    ui.collapsing(
        format!("Output multiplier by population ({})", cfg.by_population.len()),
        |ui| {
            ui.label(
                RichText::new("Per population band, a multiplier on that world's output (schema: by_population).")
                    .color(Color32::DARK_GRAY),
            );
            let mut remove: Option<String> = None;
            for (key, v) in cfg.by_population.iter_mut() {
                ui.horizontal(|ui| {
                    ui.monospace(key);
                    if ui
                        .add(egui::DragValue::new(v).range(0.0..=4.0).speed(0.05))
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .button(RichText::new("🗑").color(Color32::LIGHT_RED))
                        .on_hover_text(format!("Remove the “{key}” row"))
                        .clicked()
                    {
                        remove = Some(key.clone());
                    }
                });
            }
            if let Some(k) = remove {
                cfg.by_population.remove(&k);
                *changed = true;
            }
        },
    );
}

// ── label helpers ───────────────────────────────────────────────────────────

fn tithe_label(t: TitheStatus) -> &'static str {
    match t {
        TitheStatus::Surplus => "Surplus",
        TitheStatus::Adequate => "Adequate",
        TitheStatus::Strained => "Strained",
        TitheStatus::Delinquent => "Delinquent",
        TitheStatus::Failed => "Failed",
        TitheStatus::Falsified => "Falsified",
        _ => "UNKNOWN",
    }
}

fn supply_label(r: SupplyRisk) -> &'static str {
    match r {
        SupplyRisk::Stable => "Stable",
        SupplyRisk::Vulnerable => "Vulnerable",
        SupplyRisk::Disrupted => "Disrupted",
        SupplyRisk::Collapsing => "Collapsing",
        _ => "UNKNOWN",
    }
}

fn priority_label(p: StrategicPriority) -> &'static str {
    match p {
        StrategicPriority::Low => "Low",
        StrategicPriority::Local => "Local",
        StrategicPriority::Subsector => "Subsector",
        StrategicPriority::Sector => "Sector",
        StrategicPriority::CrusadeLevel => "Crusade",
        _ => "UNKNOWN",
    }
}

// ── helpers used by the MAP panel ───────────────────────────────────────────

/// §E4 — set of systems where at least one world is stranded. The MAP panel
/// draws a red ring around each system in the returned set.
#[must_use]
pub fn stranded_system_ids(state: &BuilderState) -> std::collections::BTreeSet<SystemId> {
    state
        .sector
        .economy
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| w.system_id.clone())
        .collect()
}

/// §E6 — route ids carrying the top supplier→consumer dependency edges (above
/// the `min_score` threshold). MAP feeds these into
/// [`sectorforge_gui_core::sector_view::SectorView::path_route_ids`].
#[must_use]
pub fn lifeline_route_ids(
    state: &BuilderState,
) -> std::collections::HashSet<sectorforge::ids::RouteId> {
    if !state.economy_highlight_lifelines {
        return std::collections::HashSet::new();
    }
    state
        .sector
        .economy
        .dependency_edges
        .iter()
        .filter(|e| e.score >= state.economy_lifeline_min_score)
        .filter_map(|e| e.route_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::economy::{EconomyConfig, ResourceVector, StrategicOutput};
    use sectorforge::sector_model::HexCoord;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    #[test]
    fn stranded_set_starts_empty_on_blank_sector() {
        let state = blank();
        assert!(stranded_system_ids(&state).is_empty());
    }

    #[test]
    fn lifeline_set_empty_when_toggle_off() {
        let state = blank();
        assert!(lifeline_route_ids(&state).is_empty());
    }

    #[test]
    fn world_override_pins_vector_through_recompute() {
        let mut state = blank();
        let sys_id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Sys")
            .unwrap();
        let world_id = state.sector.add_world_to_system(&sys_id, "World").unwrap();
        state.recompute_economy();
        let override_vec = ResourceVector {
            ore: 99.0,
            foodstuffs: -42.0,
            ..ResourceVector::default()
        };
        state
            .world_economy_overrides
            .insert(world_id.clone(), override_vec.clone());
        state.recompute_economy();
        let row = state
            .sector
            .economy
            .worlds
            .iter()
            .find(|w| w.world_id == world_id)
            .expect("override world should appear in report");
        assert_eq!(row.vector.ore, 99.0);
        assert_eq!(row.vector.foodstuffs, -42.0);
    }

    #[test]
    fn system_overrides_pin_after_recompute() {
        let mut state = blank();
        let sys_id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Sys")
            .unwrap();
        let _ = state.sector.add_world_to_system(&sys_id, "World").unwrap();
        state.recompute_economy();
        state
            .system_tithe_overrides
            .insert(sys_id.clone(), TitheStatus::Falsified);
        state
            .system_supply_overrides
            .insert(sys_id.clone(), SupplyRisk::Collapsing);
        state
            .system_priority_overrides
            .insert(sys_id.clone(), StrategicPriority::CrusadeLevel);
        state.recompute_economy();
        let row = state
            .sector
            .economy
            .systems
            .iter()
            .find(|s| s.system_id == sys_id)
            .unwrap();
        assert_eq!(row.tithe_status, TitheStatus::Falsified);
        assert_eq!(row.supply_risk, SupplyRisk::Collapsing);
        assert_eq!(row.strategic_priority, StrategicPriority::CrusadeLevel);
    }

    #[test]
    fn strategic_override_replaces_derived_output() {
        let mut state = blank();
        let sys_id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Sys")
            .unwrap();
        let world_id = state.sector.add_world_to_system(&sys_id, "World").unwrap();
        state.recompute_economy();
        let strat = StrategicOutput {
            food: 75.0,
            arms: 25.0,
            ..StrategicOutput::default()
        };
        state
            .world_strategic_overrides
            .insert(world_id.clone(), strat);
        state.recompute_economy();
        let row = state
            .sector
            .economy
            .worlds
            .iter()
            .find(|w| w.world_id == world_id)
            .unwrap();
        assert_eq!(row.strategic_output.food, 75.0);
        assert_eq!(row.strategic_output.arms, 25.0);
    }

    #[test]
    fn recompute_with_disabled_catalog_still_enables_derivation() {
        let mut state = blank();
        state.data_catalogs.economy = Some(EconomyConfig::default());
        let sys_id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Sys")
            .unwrap();
        let _ = state.sector.add_world_to_system(&sys_id, "World").unwrap();
        state.recompute_economy();
        assert!(state.sector.economy.enabled);
    }
}
