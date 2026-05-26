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

use crate::builder::state::{BuilderTab, EntityRef};
use crate::builder::BuilderState;

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
        "§E1..§E7 — per-world / per-system overrides, economy.toml editor, lifelines, heatmaps.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_sector_summary(ui, state);
            ui.separator();
            show_world_override_editor(ui, state);
            ui.separator();
            show_system_override_editor(ui, state);
            ui.separator();
            show_stranded_list(ui, state);
            ui.separator();
            show_lifeline_panel(ui, state);
            ui.separator();
            show_heatmap_picker(ui, state);
            ui.separator();
            show_economy_config_editor(ui, state);
        });
}

// ── header actions ──────────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Recompute economy").clicked() {
            state.recompute_economy();
        }
        let enabled = state.sector.economy.enabled;
        let badge = if enabled {
            RichText::new("derivation: ON").color(Color32::LIGHT_GREEN)
        } else {
            RichText::new("derivation: OFF — click Recompute")
                .color(Color32::from_rgb(220, 170, 80))
        };
        ui.label(badge);
        ui.label(format!(
            "worlds: {}  |  systems: {}  |  routes: {}  |  edges: {}",
            state.sector.economy.worlds.len(),
            state.sector.economy.systems.len(),
            state.sector.economy.routes.len(),
            state.sector.economy.dependency_edges.len(),
        ));
    });
}

fn show_sector_summary(ui: &mut Ui, state: &BuilderState) {
    let report = &state.sector.economy;
    ui.label(RichText::new("sector balance").strong());
    ui.horizontal_wrapped(|ui| {
        for k in RESOURCE_KEYS {
            let v = report.sector_balance.get(k);
            ui.label(resource_badge(k, v));
        }
    });
    ui.label(RichText::new("strategic output").strong());
    ui.horizontal_wrapped(|ui| {
        for k in STRATEGIC_RESOURCE_KEYS {
            let v = report.strategic_output.get(k);
            ui.label(strategic_badge(k, v));
        }
    });
}

fn resource_badge(key: &str, value: f32) -> RichText {
    let label = format!("{key}: {value:+.0}");
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
    let label = format!("{key}: {value:.0}");
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
    ui.label(RichText::new("§E1 / §E2 — per-world overrides").strong());
    if state.sector.economy.worlds.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "No per-world economy rows. Run Recompute first.",
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
        ui.label("world:");
        let label = selected
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("econ_world_picker")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (id, line) in &world_options {
                    let active = selected.as_ref() == Some(id);
                    if ui.selectable_label(active, line).clicked() {
                        state.selected_world_id = Some(id.clone());
                    }
                }
            });
        if let Some(id) = state.selected_world_id.clone() {
            if ui.button("→ WORLD inspector").clicked() {
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
        ui.colored_label(Color32::GRAY, "Pick a world to edit overrides.");
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
        ui.colored_label(Color32::GRAY, "World not in economy report.");
        return;
    };

    // §E1 row.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("§E1 — resource vector (ore / promethium / foodstuffs / manufactured / archeotech / recruits)").italics());
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
                    ui.label(*key);
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
                RichText::new("override pinned").color(Color32::LIGHT_GREEN)
            } else {
                RichText::new("derived (no override)").color(Color32::GRAY)
            };
            ui.label(badge);
            if pinned && ui.button("Clear override").clicked() {
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
                format!("STRANDED — shortages: {}", entry.shortages.join(", ")),
            );
        }
    });

    // §E2 row.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("§E2 — strategic output (food, ore, manufacturing, arms, ships, pilgrimage, psyker_tithe, manpower, knowledge, xenos_value)").italics());
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
                    ui.label(*key);
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
                RichText::new("override pinned").color(Color32::LIGHT_GREEN)
            } else {
                RichText::new("derived (no override)").color(Color32::GRAY)
            };
            ui.label(badge);
            if pinned && ui.button("Clear override").clicked() {
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
    ui.label(RichText::new("§E3 — per-system tithe / supply / strategic priority").strong());
    if state.sector.economy.systems.is_empty() {
        ui.colored_label(Color32::GRAY, "No per-system economy rows.");
        return;
    }
    egui::Grid::new("econ_system_overrides")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new("system").strong());
            ui.label(RichText::new("tithe").strong());
            ui.label(RichText::new("supply").strong());
            ui.label(RichText::new("priority").strong());
            ui.label(RichText::new("surplus").strong());
            ui.label(RichText::new("shortage").strong());
            ui.label(RichText::new("actions").strong());
            ui.end_row();

            let systems: Vec<_> = state.sector.economy.systems.clone();
            for sy in &systems {
                let id = sy.system_id.clone();
                ui.monospace(id.as_str());

                // tithe
                let mut tithe = sy.tithe_status;
                let active_tithe = state.system_tithe_overrides.contains_key(&id);
                egui::ComboBox::from_id_salt(("tithe_cb", id.as_str()))
                    .selected_text(tithe_label(tithe))
                    .show_ui(ui, |ui| {
                        for t in TITHE_STATES {
                            ui.selectable_value(&mut tithe, *t, tithe_label(*t));
                        }
                    });
                if tithe != sy.tithe_status {
                    state.system_tithe_overrides.insert(id.clone(), tithe);
                    state.recompute_economy();
                }

                // supply
                let mut supply = sy.supply_risk;
                let active_supply = state.system_supply_overrides.contains_key(&id);
                egui::ComboBox::from_id_salt(("supply_cb", id.as_str()))
                    .selected_text(supply_label(supply))
                    .show_ui(ui, |ui| {
                        for r in SUPPLY_RISKS {
                            ui.selectable_value(&mut supply, *r, supply_label(*r));
                        }
                    });
                if supply != sy.supply_risk {
                    state.system_supply_overrides.insert(id.clone(), supply);
                    state.recompute_economy();
                }

                // strategic priority
                let mut prio = sy.strategic_priority;
                let active_prio = state.system_priority_overrides.contains_key(&id);
                egui::ComboBox::from_id_salt(("prio_cb", id.as_str()))
                    .selected_text(priority_label(prio))
                    .show_ui(ui, |ui| {
                        for p in PRIORITIES {
                            ui.selectable_value(&mut prio, *p, priority_label(*p));
                        }
                    });
                if prio != sy.strategic_priority {
                    state.system_priority_overrides.insert(id.clone(), prio);
                    state.recompute_economy();
                }

                ui.label(if sy.surplus_resources.is_empty() {
                    "—".to_string()
                } else {
                    sy.surplus_resources.join(", ")
                });
                ui.label(if sy.shortage_resources.is_empty() {
                    "—".to_string()
                } else {
                    sy.shortage_resources.join(", ")
                });

                ui.horizontal(|ui| {
                    if (active_tithe || active_supply || active_prio)
                        && ui
                            .button(RichText::new("× clear").color(Color32::LIGHT_RED))
                            .clicked()
                    {
                        state.system_tithe_overrides.remove(&id);
                        state.system_supply_overrides.remove(&id);
                        state.system_priority_overrides.remove(&id);
                        state.recompute_economy();
                    }
                    if ui.button("→ SYSTEM").clicked() {
                        state.focus_entity(EntityRef::System(id.clone()));
                    }
                });
                ui.end_row();
            }
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
    ui.label(
        RichText::new(format!(
            "§E4 — stranded worlds ({}) — MAP draws red ring on each system",
            stranded.len()
        ))
        .strong(),
    );
    if stranded.is_empty() {
        ui.colored_label(Color32::DARK_GREEN, "No stranded worlds.");
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
            if ui.button("→ WORLD").clicked() {
                let sys_id = w.system_id.clone();
                state.focus_entity(EntityRef::World {
                    system: sys_id,
                    world: w.world_id.clone(),
                });
            }
        });
    }
}

// ── §E6 lifeline-lane panel ─────────────────────────────────────────────────

fn show_lifeline_panel(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§E6 — lifeline lanes").strong());
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(
            &mut state.economy_highlight_lifelines,
            "highlight lifeline routes on MAP",
        );
        ui.label("min score:");
        ui.add(
            egui::DragValue::new(&mut state.economy_lifeline_min_score)
                .range(0.0..=200.0)
                .speed(1.0),
        );
        if ui.button("→ MAP").clicked() {
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
        ui.colored_label(
            Color32::GRAY,
            "No dependency edges above the score threshold.",
        );
        return;
    }
    let mut focus_route: Option<sectorforge::ids::RouteId> = None;
    for e in edges.iter().take(20) {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} → {} ({:>4})  score {:.1}  risk {:?}",
                e.from_system_id, e.to_system_id, e.resource, e.score, e.risk
            ));
            if let Some(route_id) = e.route_id.as_ref() {
                if ui.small_button("focus route").clicked() {
                    focus_route = Some(route_id.clone());
                }
            }
        });
    }
    if let Some(rid) = focus_route {
        state.focus_entity(EntityRef::Route(rid));
    }
}

// ── §E7 heatmap picker ──────────────────────────────────────────────────────

fn show_heatmap_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§E7 — MAP heatmap (trade volume / food / tithe / supply)").strong());
    ui.horizontal_wrapped(|ui| {
        ui.label("mode:");
        let current = state.map_heatmap_mode;
        egui::ComboBox::from_id_salt("econ_heatmap_mode")
            .selected_text(current.label())
            .show_ui(ui, |ui| {
                for mode in E7_MODES {
                    ui.selectable_value(&mut state.map_heatmap_mode, *mode, mode.label());
                }
            });
        if ui.button("→ MAP").clicked() {
            state.focus_entity(EntityRef::Tab(BuilderTab::Map));
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Overridden by §C7/§C8 when a control overlay is on.",
        );
    });
}

// ── §E5 economy.toml editor ─────────────────────────────────────────────────

fn show_economy_config_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§E5 — economy.toml editor").strong());
    if state.data_catalogs.economy.is_none() {
        ui.horizontal(|ui| {
            ui.colored_label(Color32::DARK_GRAY, "No economy catalog loaded.");
            if ui.button("create defaults").clicked() {
                state.data_catalogs.economy = Some(EconomyConfig {
                    enabled: true,
                    ..EconomyConfig::default()
                });
                if state.config.inputs.economy.is_none() {
                    state.config.inputs.economy = Some("data/worlds/economy.toml".into());
                }
                state.dirty = true;
            }
        });
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

    egui::Grid::new("econ_cfg").num_columns(2).show(ui, |ui| {
        ui.label("enabled");
        changed |= ui.checkbox(&mut cfg.enabled, "").changed();
        ui.end_row();
        ui.label("feed_stability");
        changed |= ui.checkbox(&mut cfg.feed_stability, "").changed();
        ui.end_row();
    });

    show_world_type_rows(ui, &mut cfg, &mut changed);
    show_tech_rows(ui, &mut cfg, &mut changed);
    show_pop_rows(ui, &mut cfg, &mut changed);

    ui.horizontal(|ui| {
        if ui.button("Save economy.toml").clicked() {
            save_clicked = true;
        }
        if ui.button("Apply & recompute").clicked() {
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
}

fn show_world_type_rows(ui: &mut Ui, cfg: &mut EconomyConfig, changed: &mut bool) {
    ui.collapsing(
        format!("by_world_type ({} rows)", cfg.by_world_type.len()),
        |ui| {
            let mut remove: Option<String> = None;
            for (key, vec) in cfg.by_world_type.iter_mut() {
                ui.label(RichText::new(key).monospace());
                egui::Grid::new(("econ_wt", key.as_str()))
                    .num_columns(2)
                    .show(ui, |ui| {
                        for k in RESOURCE_KEYS {
                            ui.label(*k);
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
                    .button(RichText::new("× remove row").color(Color32::LIGHT_RED))
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
                let (buf, resp) = crate::builder::panels::persistent_singleline(ui, key, "");
                ui.label("+ world_type:");
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
        format!("by_tech_level ({})", cfg.by_tech_level.len()),
        |ui| {
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
                        .button(RichText::new("×").color(Color32::LIGHT_RED))
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
        format!("by_population ({})", cfg.by_population.len()),
        |ui| {
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
                        .button(RichText::new("×").color(Color32::LIGHT_RED))
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
    }
}

fn supply_label(r: SupplyRisk) -> &'static str {
    match r {
        SupplyRisk::Stable => "Stable",
        SupplyRisk::Vulnerable => "Vulnerable",
        SupplyRisk::Disrupted => "Disrupted",
        SupplyRisk::Collapsing => "Collapsing",
    }
}

fn priority_label(p: StrategicPriority) -> &'static str {
    match p {
        StrategicPriority::Low => "Low",
        StrategicPriority::Local => "Local",
        StrategicPriority::Subsector => "Subsector",
        StrategicPriority::Sector => "Sector",
        StrategicPriority::CrusadeLevel => "Crusade",
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
