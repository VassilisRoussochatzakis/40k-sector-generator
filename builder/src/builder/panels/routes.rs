//! ROUTES tab (§N1 / §N2) — Phase B §R1..§R7 route editor.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use egui::{Color32, Ui};

use sectorforge::ids::{self, FactionId, RouteId, SystemId};
use sectorforge::routes::{RouteCondition, RouteModifier, RouteRules};
use sectorforge::sector_model::{
    hex_distance, GeneratedRoute, GeneratedSector, GeneratedSystem, RouteStability, RouteType,
};
use sectorforge::worlds::{Government, NotableFeature, WorldType};

use crate::builder::command::BuilderCommand;
use crate::builder::preview::DEFAULT_DEBOUNCE_MS;
use crate::builder::state::{BuilderTab, ModalKind};
use crate::builder::BuilderState;

const ROUTE_STABILITIES: [RouteStability; 4] = [
    RouteStability::Stable,
    RouteStability::Unstable,
    RouteStability::Hazardous,
    RouteStability::Perilous,
];

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Routes");
    ui.add_space(4.0);
    show_summary(ui, state);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_route_picker(ui, state);
            ui.add_space(4.0);
            if let Some(idx) = selected_route_index(state) {
                show_route_inspector(ui, state, idx);
            } else {
                ui.colored_label(Color32::GRAY, "No route selected.");
            }

            ui.separator();
            show_bulk_ops(ui, state);
            ui.separator();
            show_route_rules_editor(ui, state);
            ui.separator();
            show_hidden_routes_panel(ui, state);
            ui.separator();
            show_ensure_connected(ui, state);
        });
}

fn show_summary(ui: &mut Ui, state: &BuilderState) {
    let components = route_component_count(&state.sector, &state.sector.routes);
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("routes: {}", state.sector.routes.len()));
        ui.label(format!("components: {components}"));
        ui.label(format!(
            "ensure_connected_graph: {}",
            state.config.generation.routes.ensure_connected_graph
        ));
        ui.label("MAP tool ADD ROUTE supports click-click or drag endpoint creation.");
    });
}

fn selected_route_index(state: &mut BuilderState) -> Option<usize> {
    if let Some(id) = &state.selected_route_id {
        if let Some(idx) = state.index.routes.get(id).copied() {
            return Some(idx);
        }
    }
    state.selected_route_id = state.sector.routes.first().map(|r| r.id.clone());
    state
        .selected_route_id
        .as_ref()
        .and_then(|id| state.index.routes.get(id).copied())
}

fn show_route_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("route:");
        let current = state.selected_route_id.clone();
        let label = current
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("route_picker")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for route in &state.sector.routes {
                    let text = format!(
                        "{}  {} -> {}  d={}",
                        route.id, route.from_system_id, route.to_system_id, route.distance
                    );
                    if ui
                        .selectable_label(current.as_ref() == Some(&route.id), text)
                        .clicked()
                    {
                        state.selected_route_id = Some(route.id.clone());
                    }
                }
            });

        if ui.button("Delete").clicked() {
            if let Some(id) = state.selected_route_id.clone() {
                let cmd = BuilderCommand::RemoveRoute { id, before: None };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("Delete route failed: {e}")));
                } else {
                    apply_ensure_connected_if_enabled(state);
                    state.selected_route_id = state.sector.routes.first().map(|r| r.id.clone());
                }
            }
        }
    });
}

fn show_route_inspector(ui: &mut Ui, state: &mut BuilderState, idx: usize) {
    let original = state.sector.routes[idx].clone();
    let mut draft = original.clone();

    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.heading(draft.id.to_string());
            if ui.link("from").clicked() {
                state.selected_system_id = Some(draft.from_system_id.clone());
                state.active_tab = BuilderTab::System;
            }
            if ui.link("to").clicked() {
                state.selected_system_id = Some(draft.to_system_id.clone());
                state.active_tab = BuilderTab::System;
            }
        });

        egui::CollapsingHeader::new("Identity / endpoints")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("route_identity_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("id");
                        let mut id_buf = draft.id.to_string();
                        if ui.text_edit_singleline(&mut id_buf).changed() {
                            draft.id = RouteId::new(id_buf.trim());
                        }
                        ui.end_row();

                        ui.label("from_system_id");
                        system_combo(ui, "route_from_combo", &mut draft.from_system_id, state);
                        ui.end_row();

                        ui.label("to_system_id");
                        system_combo(ui, "route_to_combo", &mut draft.to_system_id, state);
                        ui.end_row();

                        ui.label("route_type");
                        route_type_combo(ui, "route_type_combo", &mut draft.route_type);
                        ui.end_row();

                        ui.label("stability");
                        stability_combo(ui, "route_stability_combo", &mut draft.stability);
                        ui.end_row();
                    });

                let endpoints_changed = draft.from_system_id != original.from_system_id
                    || draft.to_system_id != original.to_system_id;
                if endpoints_changed {
                    canonicalize_route_endpoints(&mut draft);
                    if let Some(auto) = route_auto_distance(&state.sector, &draft) {
                        draft.distance = auto;
                    }
                    draft.controls = derive_controls(&draft, &state.sector);
                }
            });

        egui::CollapsingHeader::new("Distance")
            .default_open(true)
            .show(ui, |ui| {
                let auto = route_auto_distance(&state.sector, &draft);
                ui.horizontal(|ui| {
                    ui.label("distance");
                    let mut distance = i64::from(draft.distance);
                    if ui
                        .add(egui::DragValue::new(&mut distance).range(0..=999))
                        .changed()
                    {
                        draft.distance = distance.clamp(0, 999) as u32;
                    }
                    if let Some(auto) = auto {
                        ui.label(format!("auto={auto}"));
                        if ui.button("Use auto").clicked() {
                            draft.distance = auto;
                        }
                    }
                });
                if let Some(auto) = auto {
                    if draft.distance != auto {
                        ui.colored_label(
                            Color32::from_rgb(255, 190, 80),
                            "ROUTE_DISTANCE_MISMATCH will fire unless distance == hex_distance.",
                        );
                    }
                }
            });

        egui::CollapsingHeader::new("Tags")
            .default_open(false)
            .show(ui, |ui| show_tags_editor(ui, &mut draft));

        egui::CollapsingHeader::new("Route control")
            .default_open(false)
            .show(ui, |ui| show_controls_editor(ui, state, &mut draft));
    });

    if draft != original {
        let new_id = draft.id.clone();
        replace_route_at(state, idx, draft);
        state.selected_route_id = Some(new_id);
    }
}

fn show_tags_editor(ui: &mut Ui, route: &mut GeneratedRoute) {
    let mut remove = None;
    for (i, tag) in route.tags.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut text = tag.to_string();
            if ui.text_edit_singleline(&mut text).changed() {
                *tag = Arc::from(text.trim());
            }
            if ui.button("x").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        route.tags.remove(i);
    }
    if ui.button("Add tag").clicked() {
        route.tags.push(Arc::from("tag"));
    }
}

fn show_controls_editor(ui: &mut Ui, state: &BuilderState, route: &mut GeneratedRoute) {
    ui.horizontal(|ui| {
        if ui.button("Re-derive controls").clicked() {
            route.controls = derive_controls(route, &state.sector);
        }
        ui.label(format!("{} faction row(s)", route.controls.len()));
    });
    let mut remove = None;
    egui::Grid::new("route_controls_grid")
        .striped(true)
        .num_columns(8)
        .show(ui, |ui| {
            ui.label("faction");
            ui.label("patrol");
            ui.label("toll");
            ui.label("interdiction");
            ui.label("piracy");
            ui.label("secrecy");
            ui.label("confidence");
            ui.label("");
            ui.end_row();

            for (i, control) in route.controls.iter_mut().enumerate() {
                faction_combo(
                    ui,
                    format!("route_control_faction_{i}"),
                    &mut control.faction_id,
                    state,
                );
                percent_drag(ui, &mut control.patrol);
                percent_drag(ui, &mut control.toll);
                percent_drag(ui, &mut control.interdiction);
                percent_drag(ui, &mut control.piracy);
                percent_drag(ui, &mut control.secrecy);
                percent_drag(ui, &mut control.confidence);
                if ui.button("x").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        route.controls.remove(i);
    }
    if ui.button("Add control row").clicked() {
        let fid = state
            .sector
            .factions
            .first()
            .map(|f| f.id.clone())
            .unwrap_or_else(|| FactionId::new("faction"));
        route
            .controls
            .push(sectorforge::route_control::RouteControl {
                faction_id: fid,
                ..Default::default()
            });
    }
}

fn percent_drag(ui: &mut Ui, value: &mut f32) {
    ui.add(
        egui::DragValue::new(value)
            .speed(0.5)
            .range(0.0..=100.0)
            .max_decimals(1),
    );
}

fn system_combo(ui: &mut Ui, id: &str, value: &mut SystemId, state: &BuilderState) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .show_ui(ui, |ui| {
            for sys in &state.sector.systems {
                ui.selectable_value(value, sys.id.clone(), format!("{} — {}", sys.id, sys.name));
            }
        });
}

fn faction_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut FactionId,
    state: &BuilderState,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .show_ui(ui, |ui| {
            for faction in &state.sector.factions {
                ui.selectable_value(
                    value,
                    faction.id.clone(),
                    format!("{} — {}", faction.id, faction.name),
                );
            }
        });
}

fn route_type_combo(ui: &mut Ui, id: &str, value: &mut RouteType) -> bool {
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.editor_label())
        .show_ui(ui, |ui| {
            for option in RouteType::ALL {
                ui.selectable_value(value, option, option.editor_label());
            }
        });
    *value != before
}

fn stability_combo(ui: &mut Ui, id: &str, value: &mut RouteStability) -> bool {
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .selected_text(stability_label(*value))
        .show_ui(ui, |ui| {
            for option in ROUTE_STABILITIES {
                ui.selectable_value(value, option, stability_label(option));
            }
        });
    *value != before
}

fn stability_label(value: RouteStability) -> &'static str {
    match value {
        RouteStability::Stable => "stable",
        RouteStability::Unstable => "unstable",
        RouteStability::Hazardous => "hazardous",
        RouteStability::Perilous => "perilous",
    }
}

fn canonicalize_route_endpoints(route: &mut GeneratedRoute) {
    if route.to_system_id < route.from_system_id {
        std::mem::swap(&mut route.from_system_id, &mut route.to_system_id);
    }
    route.id = ids::route_id(&route.from_system_id, &route.to_system_id);
}

fn route_auto_distance(sector: &GeneratedSector, route: &GeneratedRoute) -> Option<u32> {
    let a = system_by_id(sector, &route.from_system_id)?;
    let b = system_by_id(sector, &route.to_system_id)?;
    Some(hex_distance(a.coord, b.coord))
}

fn system_by_id<'a>(sector: &'a GeneratedSector, id: &SystemId) -> Option<&'a GeneratedSystem> {
    sector.systems.iter().find(|s| s.id == *id)
}

fn derive_controls(
    route: &GeneratedRoute,
    sector: &GeneratedSector,
) -> Vec<sectorforge::route_control::RouteControl> {
    let systems_by_id: BTreeMap<&str, &GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();
    sectorforge::route_control::derive_route_controls(route, &systems_by_id, &sector.factions)
}

fn replace_route_at(state: &mut BuilderState, idx: usize, route: GeneratedRoute) {
    let mut routes = state.sector.routes.clone();
    if idx < routes.len() {
        routes[idx] = route;
        routes.sort_by(|a, b| a.id.cmp(&b.id));
        replace_routes(state, routes);
    }
}

fn replace_routes(state: &mut BuilderState, mut routes: Vec<GeneratedRoute>) {
    if state.config.generation.routes.ensure_connected_graph {
        routes = ensure_connected_routes(state, routes).0;
    }
    let cmd = BuilderCommand::ReplaceRoutes {
        before: Vec::new(),
        after: routes,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Route update failed: {e}")));
    }
}

// ── R4 bulk ops ──────────────────────────────────────────────────────────────

fn show_bulk_ops(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("Bulk operations")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Predicate");
            egui::Grid::new("route_bulk_predicate_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("route_type");
                    optional_route_type_combo(
                        ui,
                        "bulk_filter_type",
                        &mut state.route_bulk_filter_type,
                    );
                    ui.end_row();
                    ui.label("stability");
                    optional_stability_combo(
                        ui,
                        "bulk_filter_stability",
                        &mut state.route_bulk_filter_stability,
                    );
                    ui.end_row();
                    ui.label("tag contains");
                    ui.text_edit_singleline(&mut state.route_bulk_filter_tag);
                    ui.end_row();
                    ui.label("crosses region");
                    let region_options: Vec<(String, String)> = state
                        .sector
                        .regions
                        .iter()
                        .map(|region| (region.id.clone(), region.name.clone()))
                        .collect();
                    optional_region_combo(
                        ui,
                        "bulk_filter_region",
                        &mut state.route_bulk_filter_region,
                        &region_options,
                    );
                    ui.end_row();
                });

            let matching = state
                .sector
                .routes
                .iter()
                .filter(|route| route_matches_bulk(state, route))
                .count();
            ui.label(format!("{matching} route(s) match"));

            ui.separator();
            ui.label("Action");
            ui.horizontal_wrapped(|ui| {
                ui.label("type");
                route_type_combo(ui, "bulk_set_type", &mut state.route_bulk_set_type);
                if ui.button("Set matching type").clicked() {
                    apply_bulk_routes(state, BulkRouteAction::SetType(state.route_bulk_set_type));
                }
                ui.label("stability");
                stability_combo(
                    ui,
                    "bulk_set_stability",
                    &mut state.route_bulk_set_stability,
                );
                if ui.button("Set matching stability").clicked() {
                    apply_bulk_routes(
                        state,
                        BulkRouteAction::SetStability(state.route_bulk_set_stability),
                    );
                }
            });
        });
}

enum BulkRouteAction {
    SetType(RouteType),
    SetStability(RouteStability),
}

fn apply_bulk_routes(state: &mut BuilderState, action: BulkRouteAction) {
    let mut routes = state.sector.routes.clone();
    let mut changed = 0usize;
    for route in &mut routes {
        if !route_matches_bulk(state, route) {
            continue;
        }
        match action {
            BulkRouteAction::SetType(value) => {
                if route.route_type != value {
                    route.route_type = value;
                    changed += 1;
                }
            }
            BulkRouteAction::SetStability(value) => {
                if route.stability != value {
                    route.stability = value;
                    changed += 1;
                }
            }
        }
    }
    if changed == 0 {
        state.modal = Some(ModalKind::Message("No matching routes changed.".into()));
        return;
    }
    replace_routes(state, routes);
}

fn route_matches_bulk(state: &BuilderState, route: &GeneratedRoute) -> bool {
    if state
        .route_bulk_filter_type
        .is_some_and(|filter| filter != route.route_type)
    {
        return false;
    }
    if state
        .route_bulk_filter_stability
        .is_some_and(|filter| filter != route.stability)
    {
        return false;
    }
    let tag_filter = state.route_bulk_filter_tag.trim();
    if !tag_filter.is_empty()
        && !route
            .tags
            .iter()
            .any(|tag| tag.as_ref().contains(tag_filter))
    {
        return false;
    }
    if let Some(region_id) = &state.route_bulk_filter_region {
        if !route_crosses_region(&state.sector, route, region_id) {
            return false;
        }
    }
    true
}

fn route_crosses_region(sector: &GeneratedSector, route: &GeneratedRoute, region_id: &str) -> bool {
    let Some(a) = system_by_id(sector, &route.from_system_id).map(|s| s.coord) else {
        return false;
    };
    let Some(b) = system_by_id(sector, &route.to_system_id).map(|s| s.coord) else {
        return false;
    };
    let d = hex_distance(a, b);
    sector
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .is_some_and(|region| {
            region
                .hexes
                .iter()
                .any(|h| hex_distance(a, *h) + hex_distance(*h, b) == d)
        })
}

fn optional_route_type_combo(ui: &mut Ui, id: &str, value: &mut Option<RouteType>) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.map_or("(any)", RouteType::editor_label))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "(any)");
            for option in RouteType::ALL {
                ui.selectable_value(value, Some(option), option.editor_label());
            }
        });
}

fn optional_stability_combo(ui: &mut Ui, id: &str, value: &mut Option<RouteStability>) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.map_or("(any)", stability_label))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "(any)");
            for option in ROUTE_STABILITIES {
                ui.selectable_value(value, Some(option), stability_label(option));
            }
        });
}

fn optional_region_combo(
    ui: &mut Ui,
    id: &str,
    value: &mut Option<String>,
    regions: &[(String, String)],
) {
    let selected = value.as_deref().unwrap_or("(any)");
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "(any)");
            for (region_id, region_name) in regions {
                ui.selectable_value(
                    value,
                    Some(region_id.clone()),
                    format!("{region_id} — {region_name}"),
                );
            }
        });
}

// ── R5 route-rules editor ───────────────────────────────────────────────────

fn show_route_rules_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("Route rules")
        .default_open(false)
        .show(ui, |ui| {
            if state.config.inputs.route_rules.is_none() {
                ui.colored_label(
                    Color32::from_rgb(255, 190, 80),
                    "No [inputs].route_rules path; edits stay in memory until a path exists.",
                );
            }

            let mut changed = false;
            {
                let rules = state
                    .data_catalogs
                    .route_rules
                    .get_or_insert_with(RouteRules::default);
                egui::Grid::new("route_rules_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("default_weight");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut rules.default_weight)
                                    .speed(0.1)
                                    .range(0.01..=1000.0),
                            )
                            .changed();
                        ui.end_row();
                        ui.label("max_distance");
                        changed |= ui
                            .add(egui::DragValue::new(&mut rules.max_distance).range(1..=64))
                            .changed();
                        ui.end_row();
                        ui.label("prefer_populated_worlds");
                        changed |= ui
                            .checkbox(&mut rules.prefer_populated_worlds, "")
                            .changed();
                        ui.end_row();
                        ui.label("prefer_trade_hubs");
                        changed |= ui.checkbox(&mut rules.prefer_trade_hubs, "").changed();
                        ui.end_row();
                        ui.label("avoid_warp_phenomena");
                        changed |= ui.checkbox(&mut rules.avoid_warp_phenomena, "").changed();
                        ui.end_row();
                    });

                ui.separator();
                changed |= show_route_modifiers(ui, rules);
            }

            if changed {
                mark_route_rules_changed(ui, state);
            }
            show_preview_status(ui, state);
        });
}

fn show_route_modifiers(ui: &mut Ui, rules: &mut RouteRules) -> bool {
    let mut changed = false;
    let mut remove = None;
    egui::Grid::new("route_modifiers_grid")
        .striped(true)
        .num_columns(6)
        .show(ui, |ui| {
            ui.label("notable_feature");
            ui.label("world_type");
            ui.label("government");
            ui.label("route_type");
            ui.label("multiplier");
            ui.label("");
            ui.end_row();

            for (i, modifier) in rules.modifiers.iter_mut().enumerate() {
                changed |= optional_world_value_combo(
                    ui,
                    format!("route_mod_feature_{i}"),
                    &mut modifier.when.notable_feature,
                    feature_options(),
                );
                changed |= optional_world_value_combo(
                    ui,
                    format!("route_mod_world_type_{i}"),
                    &mut modifier.when.world_type,
                    world_type_options(),
                );
                changed |= optional_world_value_combo(
                    ui,
                    format!("route_mod_government_{i}"),
                    &mut modifier.when.government,
                    government_options(),
                );
                changed |= optional_route_type_key_combo(
                    ui,
                    format!("route_mod_route_type_{i}"),
                    &mut modifier.when.route_type,
                );
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut modifier.multiplier)
                            .speed(0.05)
                            .range(0.01..=100.0),
                    )
                    .changed();
                if ui.button("x").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        rules.modifiers.remove(i);
        changed = true;
    }
    if ui.button("Add modifier").clicked() {
        rules.modifiers.push(RouteModifier {
            when: RouteCondition::default(),
            multiplier: 1.0,
        });
        changed = true;
    }
    changed
}

fn optional_world_value_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut Option<String>,
    options: Vec<(String, String)>,
) -> bool {
    let before = value.clone();
    let selected = value.as_deref().unwrap_or("(any)");
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "(any)");
            for (stored, label) in options {
                ui.selectable_value(value, Some(stored), label);
            }
        });
    *value != before
}

fn optional_route_type_key_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut Option<String>,
) -> bool {
    let before = value.clone();
    let selected = value.as_deref().unwrap_or("(any)");
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "(any)");
            for option in RouteType::ALL {
                ui.selectable_value(value, Some(option.key().to_string()), option.editor_label());
            }
        });
    *value != before
}

fn world_type_options() -> Vec<(String, String)> {
    WorldType::VARIANTS
        .iter()
        .map(|v| (format!("{v:?}"), v.display_name().to_string()))
        .collect()
}

fn government_options() -> Vec<(String, String)> {
    Government::VARIANTS
        .iter()
        .map(|v| (format!("{v:?}"), v.display_name().to_string()))
        .collect()
}

fn feature_options() -> Vec<(String, String)> {
    NotableFeature::VARIANTS
        .iter()
        .map(|v| (format!("{v:?}"), v.display_name().to_string()))
        .collect()
}

fn mark_route_rules_changed(ui: &Ui, state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = &state.config.inputs.route_rules {
        state.dirty_files.insert(rel.clone());
    }
    state.mark_validation_dirty();
    let now = ui.ctx().input(|i| i.time);
    state
        .preview
        .schedule(now, DEFAULT_DEBOUNCE_MS as f64 / 1000.0);
}

fn show_preview_status(ui: &mut Ui, state: &BuilderState) {
    if state.preview.timer.is_some() {
        ui.label("preview scheduled");
    } else if state.preview.job.is_some() {
        ui.label("preview running");
    } else if let Some(sector) = &state.preview.sector {
        ui.label(format!("preview ready: {} routes", sector.routes.len()));
    } else if let Some(err) = &state.preview.error {
        ui.colored_label(Color32::LIGHT_RED, err);
    }
}

// ── R6 hidden routes ────────────────────────────────────────────────────────

fn show_hidden_routes_panel(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("Hidden routes")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("kind");
                hidden_kind_combo(ui, &mut state.hidden_route_kind);
                ui.label("k_nearest");
                ui.add(egui::DragValue::new(&mut state.hidden_route_k_nearest).range(1..=16));
                ui.checkbox(
                    &mut state.hidden_route_exclude_blackout,
                    "exclude Blackout regions",
                );
            });

            ui.horizontal_wrapped(|ui| {
                if ui.button("Use selected systems").clicked() {
                    state.hidden_route_endpoints = state.selected_systems.clone();
                }
                if ui.button("All systems").clicked() {
                    state.hidden_route_endpoints =
                        state.sector.systems.iter().map(|s| s.id.clone()).collect();
                }
                if ui.button("Clear").clicked() {
                    state.hidden_route_endpoints.clear();
                }
                ui.label(format!("endpoints: {}", state.hidden_route_endpoints.len()));
            });

            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for sys in &state.sector.systems {
                        let mut selected = state.hidden_route_endpoints.contains(&sys.id);
                        if ui
                            .checkbox(&mut selected, format!("{} — {}", sys.id, sys.name))
                            .changed()
                        {
                            if selected {
                                state.hidden_route_endpoints.insert(sys.id.clone());
                            } else {
                                state.hidden_route_endpoints.remove(&sys.id);
                            }
                        }
                    }
                });

            ui.horizontal_wrapped(|ui| {
                if ui.button("Build hidden routes").clicked() {
                    let cfg = sectorforge::hidden_routes::HiddenRoutesConfig {
                        kind: state.hidden_route_kind,
                        endpoints: state.hidden_route_endpoints.iter().cloned().collect(),
                        k_nearest: state.hidden_route_k_nearest.max(1),
                        exclude_blackout_regions: state.hidden_route_exclude_blackout,
                    };
                    let new_routes = sectorforge::hidden_routes::configured_hidden_routes(
                        &state.sector.systems,
                        &state.sector.factions,
                        state.sector.regions.as_ref(),
                        &state.sector.routes,
                        &cfg,
                    );
                    if new_routes.is_empty() {
                        state.modal = Some(ModalKind::Message(
                            "No hidden routes added; endpoints may be too few or already linked."
                                .into(),
                        ));
                    } else {
                        let mut routes = state.sector.routes.clone();
                        routes.extend(new_routes);
                        routes.sort_by(|a, b| a.id.cmp(&b.id));
                        replace_routes(state, routes);
                    }
                }
                if ui.button("Remove hidden routes of kind").clicked() {
                    let kind = state.hidden_route_kind;
                    let mut routes = state.sector.routes.clone();
                    let before = routes.len();
                    routes.retain(|route| route.route_type != kind);
                    if routes.len() == before {
                        state.modal = Some(ModalKind::Message(
                            "No matching hidden routes found.".into(),
                        ));
                    } else {
                        replace_routes(state, routes);
                    }
                }
            });
        });
}

fn hidden_kind_combo(ui: &mut Ui, value: &mut RouteType) {
    egui::ComboBox::from_id_salt("hidden_route_kind")
        .selected_text(value.editor_label())
        .show_ui(ui, |ui| {
            for option in [
                RouteType::Webway,
                RouteType::BlackShip,
                RouteType::SmugglingLane,
            ] {
                ui.selectable_value(value, option, option.editor_label());
            }
        });
}

// ── R7 ensure-connected connector ───────────────────────────────────────────

fn show_ensure_connected(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("Ensure connected")
        .default_open(false)
        .show(ui, |ui| {
            let mut enabled = state.config.generation.routes.ensure_connected_graph;
            if ui
                .checkbox(&mut enabled, "ensure_connected_graph")
                .changed()
            {
                state.config.generation.routes.ensure_connected_graph = enabled;
                state.dirty = true;
                state.mark_validation_dirty();
                if enabled {
                    apply_ensure_connected_if_enabled(state);
                }
            }
            let components = route_component_count(&state.sector, &state.sector.routes);
            let added = ensure_connected_routes(state, state.sector.routes.clone()).1;
            ui.label(format!(
                "components={components}; connector would add {added} bridge route(s)"
            ));
            if ui.button("Run connector now").clicked() {
                let (routes, added) = ensure_connected_routes(state, state.sector.routes.clone());
                if added == 0 {
                    state.modal = Some(ModalKind::Message("Route graph already connected.".into()));
                } else {
                    replace_routes(state, routes);
                }
            }
        });
}

fn apply_ensure_connected_if_enabled(state: &mut BuilderState) {
    if !state.config.generation.routes.ensure_connected_graph {
        return;
    }
    let (routes, added) = ensure_connected_routes(state, state.sector.routes.clone());
    if added > 0 {
        replace_routes(state, routes);
    }
}

fn ensure_connected_routes(
    state: &BuilderState,
    mut routes: Vec<GeneratedRoute>,
) -> (Vec<GeneratedRoute>, usize) {
    let n = state.sector.systems.len();
    if n < 2 {
        return (routes, 0);
    }

    let index: BTreeMap<SystemId, usize> = state
        .sector
        .systems
        .iter()
        .enumerate()
        .map(|(i, sys)| (sys.id.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut existing: BTreeSet<(SystemId, SystemId)> = BTreeSet::new();
    for route in &routes {
        if let (Some(&a), Some(&b)) = (
            index.get(&route.from_system_id),
            index.get(&route.to_system_id),
        ) {
            union(&mut parent, a, b);
        }
        let (lo, hi) = ordered_pair(&route.from_system_id, &route.to_system_id);
        existing.insert((lo, hi));
    }

    let mut candidates = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &state.sector.systems[i];
            let b = &state.sector.systems[j];
            let dist = hex_distance(a.coord, b.coord);
            candidates.push((dist, a.id.clone(), b.id.clone(), i, j));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut added = 0usize;
    for (dist, a_id, b_id, a_idx, b_idx) in candidates {
        if find_root(&mut parent, a_idx) == find_root(&mut parent, b_idx) {
            continue;
        }
        let (from, to) = ordered_pair(&a_id, &b_id);
        if existing.contains(&(from.clone(), to.clone())) {
            continue;
        }
        let stability = if dist <= state.config.generation.routes.max_route_distance {
            RouteStability::Stable
        } else {
            RouteStability::Unstable
        };
        let mut route = GeneratedRoute {
            id: ids::route_id(&from, &to),
            from_system_id: from.clone(),
            to_system_id: to.clone(),
            distance: dist,
            route_type: RouteType::ChartedPassage,
            stability,
            tags: vec![Arc::from("bridge")],
            controls: Vec::new(),
        };
        route.controls = derive_controls(&route, &state.sector);
        routes.push(route);
        existing.insert((from, to));
        union(&mut parent, a_idx, b_idx);
        added += 1;
    }
    routes.sort_by(|a, b| a.id.cmp(&b.id));
    (routes, added)
}

fn route_component_count(sector: &GeneratedSector, routes: &[GeneratedRoute]) -> usize {
    let n = sector.systems.len();
    if n == 0 {
        return 0;
    }
    let index: BTreeMap<SystemId, usize> = sector
        .systems
        .iter()
        .enumerate()
        .map(|(i, sys)| (sys.id.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    for route in routes {
        if let (Some(&a), Some(&b)) = (
            index.get(&route.from_system_id),
            index.get(&route.to_system_id),
        ) {
            union(&mut parent, a, b);
        }
    }
    let mut roots = BTreeSet::new();
    for i in 0..n {
        roots.insert(find_root(&mut parent, i));
    }
    roots.len()
}

fn ordered_pair(a: &SystemId, b: &SystemId) -> (SystemId, SystemId) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find_root(parent, a);
    let rb = find_root(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    #[test]
    fn ensure_connected_adds_bridge_between_components() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 0 }, "B")
            .unwrap();
        let c = state
            .sector
            .add_system(HexCoord { q: 7, r: 7 }, "C")
            .unwrap();
        state
            .sector
            .add_route(&a, &b, RouteType::ChartedPassage, RouteStability::Stable)
            .unwrap();
        let (routes, added) = ensure_connected_routes(&state, state.sector.routes.clone());
        assert_eq!(added, 1);
        assert_eq!(route_component_count(&state.sector, &routes), 1);
        assert!(routes
            .iter()
            .any(|r| (r.from_system_id == c || r.to_system_id == c)
                && r.tags.iter().any(|t| t.as_ref() == "bridge")));
    }

    #[test]
    fn route_region_predicate_uses_hex_line() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 0 }, "B")
            .unwrap();
        state
            .sector
            .add_route(&a, &b, RouteType::ChartedPassage, RouteStability::Stable)
            .unwrap();
        state.sector.regions = Arc::new(vec![sectorforge::regions::WarpRegion {
            id: "reg-1".into(),
            name: "R".into(),
            kind: sectorforge::regions::RegionConditionKind::Turbulence,
            hexes: vec![HexCoord { q: 1, r: 0 }],
            centre: HexCoord { q: 1, r: 0 },
        }]);
        assert!(route_crosses_region(
            &state.sector,
            &state.sector.routes[0],
            "reg-1"
        ));
    }
}
