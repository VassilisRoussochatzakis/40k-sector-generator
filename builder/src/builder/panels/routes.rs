//! ROUTES tab (§N1 / §N2) — Phase B §R1..§R7 route editor.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{self, FactionId, RouteId, SystemId};
use sectorforge::routes::{RouteCondition, RouteModifier, RouteRules};
use sectorforge::sector_model::{
    hex_distance, GeneratedRoute, GeneratedSector, GeneratedSystem, RouteStability, RouteType,
};
use sectorforge::worlds::{Government, NotableFeature, WorldType};
use sectorforge_gui_core::{
    card, palette,
    ui_kit::{self, labeled},
};

use crate::builder::command::BuilderCommand;
use crate::builder::preview::DEFAULT_DEBOUNCE_MS;
use crate::builder::state::{EntityRef, ModalKind};
use crate::builder::BuilderState;

const ROUTE_STABILITIES: [RouteStability; 4] = [
    RouteStability::Stable,
    RouteStability::Unstable,
    RouteStability::Hazardous,
    RouteStability::Perilous,
];

pub(crate) fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Routes");
    ui.label(
        RichText::new("Travel lanes between systems — their danger, who patrols them, and how the network stays connected.")
            .color(Color32::DARK_GRAY),
    );
    ui.add_space(4.0);
    // §COLUMNS — the summary line (route / component / hop counts) stays
    // full-width on top, above the master-detail split.
    show_summary(ui, state);
    ui.separator();

    // §COLUMNS — master-detail: a persistent route roster on the left rail
    // (`routes_roster`); the route editor + the sector-wide R4..R7 tools fill
    // the rest. Replaces the combo-picker + single-column stack.
    egui::SidePanel::left("routes_roster")
        .resizable(true)
        .default_width(260.0)
        .width_range(200.0..=460.0)
        .show_inside(ui, |ui| show_route_roster(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if let Some(idx) = selected_route_index(state) {
                    show_route_inspector(ui, state, idx);
                } else {
                    ui_kit::placeholder(
                        ui,
                        "Pick a route on the left to edit it — or draw a new one on the Map tab.",
                    );
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
    });
}

fn show_summary(ui: &mut Ui, state: &BuilderState) {
    let components =
        sectorforge::routes::route_component_count(&state.sector.systems, &state.sector.routes);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{} route(s)", state.sector.routes.len())).strong());
        ui.label(
            RichText::new(format!("· {components} connected group(s)")).color(Color32::DARK_GRAY),
        )
        .on_hover_text(
            "How many separate clusters the systems form (schema: routes graph components). \
                 1 means every system can be reached from every other.",
        );
        if state.config.generation.routes.ensure_connected_graph {
            ui.label(RichText::new("· auto-connect on").color(Color32::DARK_GRAY))
                .on_hover_text(
                    "New edits automatically add bridge routes so the network stays in one piece \
                     (schema: ensure_connected_graph). Toggle under \"Keep everything reachable\".",
                );
        }
    });
    ui.label(
        RichText::new("Tip: draw new routes on the Map tab — click one system then another, or drag between them.")
            .small()
            .color(Color32::DARK_GRAY),
    );
}

fn selected_route_index(state: &mut BuilderState) -> Option<usize> {
    if let Some(id) = &state.selection.route_id {
        if let Some(idx) = state.index.routes.get(id).copied() {
            return Some(idx);
        }
    }
    state.selection.route_id = state.sector.routes.first().map(|r| r.id.clone());
    state
        .selection
        .route_id
        .as_ref()
        .and_then(|id| state.index.routes.get(id).copied())
}

/// §COLUMNS — left-rail route roster (master pane). A vertical selectable list
/// of every route with a delete affordance at the top; selection is pure view
/// state, set directly. Replaces the combo picker that hid the list.
fn show_route_roster(ui: &mut Ui, state: &mut BuilderState) {
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{} route(s)", state.sector.routes.len())).strong());
        if ui
            .add_enabled(
                state.selection.route_id.is_some(),
                egui::Button::new("🗑  Delete"),
            )
            .on_hover_text("Remove the selected route")
            .clicked()
        {
            if let Some(id) = state.selection.route_id.clone() {
                let cmd = BuilderCommand::RemoveRoute { id, before: None };
                if let Err(e) = state.run(cmd) {
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("Delete route failed: {e}")));
                } else {
                    apply_ensure_connected_if_enabled(state);
                    state.selection.route_id = state.sector.routes.first().map(|r| r.id.clone());
                }
            }
        }
    });
    ui.separator();

    if state.sector.routes.is_empty() {
        ui_kit::placeholder(
            ui,
            "No routes yet. Draw one on the Map tab — click one system, then another.",
        );
        return;
    }
    let current = state.selection.route_id.clone();
    let mut pick: Option<RouteId> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for route in &state.sector.routes {
                let sel = current.as_ref() == Some(&route.id);
                // §BEAUTY: animated selectable plate; the stability dot keeps its
                // meaning-carrying colour and tooltip inside the plate content.
                let (resp, _) = card::selectable_plate(ui, ("route_row", &route.id), sel, |ui| {
                    ui.label(RichText::new("●").color(palette::stability_color(route.stability)))
                        .on_hover_text(format!(
                            "Travel danger: {}",
                            stability_label(route.stability)
                        ));
                    ui.label(RichText::new(format!(
                        "{} → {}  ({} hops)",
                        route.from_system_id, route.to_system_id, route.distance
                    )));
                });
                if resp
                    .on_hover_text(format!("Route id: {}", route.id))
                    .clicked()
                {
                    pick = Some(route.id.clone());
                }
            }
        });
    if let Some(id) = pick {
        state.selection.route_id = Some(id);
    }
}

fn show_route_inspector(ui: &mut Ui, state: &mut BuilderState, idx: usize) {
    let original = state.sector.routes[idx].clone();
    let mut draft = original.clone();

    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.heading(draft.id.to_string());
            ui.label(RichText::new("Jump to endpoint:").color(Color32::DARK_GRAY));
            if sectorforge_gui_core::entity_link(ui, "from", false)
                .on_hover_text("Open the system this route starts at")
                .clicked()
            {
                state.focus_entity(EntityRef::System(draft.from_system_id.clone()));
            }
            if sectorforge_gui_core::entity_link(ui, "to", false)
                .on_hover_text("Open the system this route ends at")
                .clicked()
            {
                state.focus_entity(EntityRef::System(draft.to_system_id.clone()));
            }
        });

        ui_kit::collapsing_section(
            ui,
            "route_identity_endpoints",
            "Endpoints & type",
            true,
            |ui| {
                labeled(
                    ui,
                    "Route id",
                    "Unique identifier (schema: id). Derived from the two endpoints; usually leave as-is.",
                    |ui| {
                        let mut id_buf = draft.id.to_string();
                        if ui.text_edit_singleline(&mut id_buf).changed() {
                            draft.id = RouteId::new(id_buf.trim());
                        }
                    },
                );
                labeled(
                    ui,
                    "From",
                    "System this route starts at (schema: from_system_id).",
                    |ui| system_combo(ui, "route_from_combo", &mut draft.from_system_id, state),
                );
                labeled(
                    ui,
                    "To",
                    "System this route ends at (schema: to_system_id).",
                    |ui| system_combo(ui, "route_to_combo", &mut draft.to_system_id, state),
                );
                labeled(
                    ui,
                    "Lane type",
                    "What kind of travel lane this is (schema: route_type). Sets how it's drawn on the map.",
                    |ui| {
                        route_type_combo(ui, "route_type_combo", &mut draft.route_type);
                    },
                );
                labeled(
                    ui,
                    "Travel danger",
                    "How risky the crossing is (schema: stability). Higher danger slows or blocks safe passage.",
                    |ui| {
                        stability_combo(ui, "route_stability_combo", &mut draft.stability);
                    },
                );

                let endpoints_changed = draft.from_system_id != original.from_system_id
                    || draft.to_system_id != original.to_system_id;
                if endpoints_changed {
                    canonicalize_route_endpoints(&mut draft);
                    if let Some(auto) = route_auto_distance(&state.sector, &draft) {
                        draft.distance = auto;
                    }
                    draft.controls = derive_controls(&draft, &state.sector);
                }
            },
        );

        ui_kit::collapsing_section(ui, "route_distance", "Length", true, |ui| {
            let auto = route_auto_distance(&state.sector, &draft);
            labeled(
                ui,
                "Length (hops)",
                "Travel distance in hexes (schema: distance). Should match the straight-line gap between endpoints.",
                |ui| {
                    let mut distance = i64::from(draft.distance);
                    if ui
                        .add(egui::DragValue::new(&mut distance).range(0..=999))
                        .changed()
                    {
                        draft.distance = distance.clamp(0, 999) as u32;
                    }
                    if let Some(auto) = auto {
                        ui.label(RichText::new(format!("straight-line: {auto}")).color(Color32::DARK_GRAY));
                        if ui
                            .button("Use straight-line")
                            .on_hover_text("Set length to the hex distance between the two systems")
                            .clicked()
                        {
                            draft.distance = auto;
                        }
                    }
                },
            );
            if let Some(auto) = auto {
                if draft.distance != auto {
                    ui.colored_label(
                        palette::warning(),
                        format!(
                            "⚠  Length doesn't match the {auto}-hop gap between these systems — \
                             validation will flag it until they agree."
                        ),
                    );
                }
            }
        });

        ui_kit::collapsing_section(ui, "route_tags", "Tags", false, |ui| {
            show_tags_editor(ui, &mut draft)
        });

        ui_kit::collapsing_section(ui, "route_control", "Who controls this lane", false, |ui| {
            show_controls_editor(ui, state, &mut draft)
        });
    });

    if draft != original {
        let new_id = draft.id.clone();
        replace_route_at(state, idx, draft);
        state.selection.route_id = Some(new_id);
    }
}

fn show_tags_editor(ui: &mut Ui, route: &mut GeneratedRoute) {
    ui.label(
        RichText::new("Free-form labels used by bulk operations and filters (schema: tags).")
            .small()
            .color(Color32::DARK_GRAY),
    );
    if route.tags.is_empty() {
        ui_kit::placeholder(ui, "No tags yet — add one to group or filter this route.");
    }
    let mut remove = None;
    for (i, tag) in route.tags.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut text = tag.to_string();
            if ui.text_edit_singleline(&mut text).changed() {
                *tag = Arc::from(text.trim());
            }
            if ui
                .small_button("×")
                .on_hover_text("Remove this tag")
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        route.tags.remove(i);
    }
    if ui
        .button("➕  Add tag")
        .on_hover_text("Add a new label to this route")
        .clicked()
    {
        route.tags.push(Arc::from("tag"));
    }
}

fn show_controls_editor(ui: &mut Ui, state: &BuilderState, route: &mut GeneratedRoute) {
    ui.label(
        RichText::new(
            "Each row is one faction's grip on this lane, as a 0–100% rating (schema: controls).",
        )
        .small()
        .color(Color32::DARK_GRAY),
    );
    ui.horizontal(|ui| {
        if ui
            .button("↺  Re-derive from systems")
            .on_hover_text("Recompute every row from the factions present at the two endpoints")
            .clicked()
        {
            route.controls = derive_controls(route, &state.sector);
        }
        ui.label(
            RichText::new(format!("{} faction row(s)", route.controls.len()))
                .color(Color32::DARK_GRAY),
        );
    });
    if route.controls.is_empty() {
        ui_kit::placeholder(
            ui,
            "No controlling factions — add a row, or re-derive from the endpoint systems.",
        );
    }
    let mut remove = None;
    egui::Grid::new("route_controls_grid")
        .striped(true)
        .num_columns(8)
        .show(ui, |ui| {
            control_header(
                ui,
                "Faction",
                "Which faction this row describes (schema: faction_id).",
            );
            control_header(
                ui,
                "Patrol",
                "How heavily the faction patrols this lane (schema: patrol).",
            );
            control_header(
                ui,
                "Toll",
                "How aggressively it charges passage tolls (schema: toll).",
            );
            control_header(
                ui,
                "Blockade",
                "How likely it is to interdict / blockade traffic (schema: interdiction).",
            );
            control_header(
                ui,
                "Piracy",
                "How much piracy preys on this lane (schema: piracy).",
            );
            control_header(
                ui,
                "Secrecy",
                "How secret the faction keeps this lane (schema: secrecy).",
            );
            control_header(
                ui,
                "Confidence",
                "How sure we are of this faction's grip (schema: confidence).",
            );
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
                if ui
                    .small_button("×")
                    .on_hover_text("Remove this faction row")
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        route.controls.remove(i);
    }
    if ui
        .button("➕  Add faction row")
        .on_hover_text("Add a controlling faction to this lane")
        .clicked()
    {
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

/// Grid column header with a hover tooltip naming the underlying schema field.
fn control_header(ui: &mut Ui, label: &str, help: &str) {
    ui.label(RichText::new(label).color(palette::chrome_text_dim()))
        .on_hover_text(help);
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
    ui_kit::combo(id, value.to_string()).show_ui(ui, |ui| {
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
    ui_kit::combo(id, value.to_string()).show_ui(ui, |ui| {
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
    ui_kit::combo(id, value.editor_label()).show_ui(ui, |ui| {
        for option in RouteType::ALL {
            ui.selectable_value(value, option, option.editor_label())
                .on_hover_text(format!("schema key: {}", option.as_slug()));
        }
    });
    *value != before
}

fn stability_combo(ui: &mut Ui, id: &str, value: &mut RouteStability) -> bool {
    let before = *value;
    ui_kit::combo(id, stability_label(*value)).show_ui(ui, |ui| {
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
        _ => "unknown",
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
        state.feedback.modal = Some(ModalKind::Message(format!("Route update failed: {e}")));
    }
}

// ── R4 bulk ops ──────────────────────────────────────────────────────────────

fn show_bulk_ops(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "route_bulk_operations",
        "Edit many routes at once",
        false,
        |ui| {
            ui.label(
                RichText::new("1. Pick which routes to match")
                    .strong()
                    .color(Color32::DARK_GRAY),
            );
            labeled(
                ui,
                "Lane type is",
                "Only match routes of this lane type (schema: route_type). \"(any)\" matches all.",
                |ui| {
                    optional_route_type_combo(
                        ui,
                        "bulk_filter_type",
                        &mut state.route_bulk.filter_type,
                    )
                },
            );
            labeled(
                ui,
                "Travel danger is",
                "Only match routes with this danger level (schema: stability). \"(any)\" matches all.",
                |ui| {
                    optional_stability_combo(
                        ui,
                        "bulk_filter_stability",
                        &mut state.route_bulk.filter_stability,
                    )
                },
            );
            labeled(
                ui,
                "Tag contains",
                "Only match routes carrying a tag with this text (schema: tags). Blank matches all.",
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.route_bulk.filter_tag)
                            .hint_text("e.g. bridge"),
                    );
                },
            );
            let region_options: Vec<(String, String)> = state
                .sector
                .regions
                .iter()
                .map(|region| (region.id.clone(), region.name.clone()))
                .collect();
            labeled(
                ui,
                "Crosses region",
                "Only match routes whose path passes through this warp region. \"(any)\" matches all.",
                |ui| {
                    optional_region_combo(
                        ui,
                        "bulk_filter_region",
                        &mut state.route_bulk.filter_region,
                        &region_options,
                    )
                },
            );

            let matching = state
                .sector
                .routes
                .iter()
                .filter(|route| route_matches_bulk(state, route))
                .count();
            ui.label(RichText::new(format!("{matching} route(s) match")).strong());

            ui.separator();
            ui.label(
                RichText::new("2. Apply a change to the matches")
                    .strong()
                    .color(Color32::DARK_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label("Set lane type to");
                route_type_combo(ui, "bulk_set_type", &mut state.route_bulk.set_type);
                if ui
                    .button("Apply")
                    .on_hover_text("Set the lane type on every matching route")
                    .clicked()
                {
                    apply_bulk_routes(state, BulkRouteAction::SetType(state.route_bulk.set_type));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Set travel danger to");
                stability_combo(
                    ui,
                    "bulk_set_stability",
                    &mut state.route_bulk.set_stability,
                );
                if ui
                    .button("Apply")
                    .on_hover_text("Set the travel danger on every matching route")
                    .clicked()
                {
                    apply_bulk_routes(
                        state,
                        BulkRouteAction::SetStability(state.route_bulk.set_stability),
                    );
                }
            });
        },
    );
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
        state.feedback.modal = Some(ModalKind::Message("No matching routes changed.".into()));
        return;
    }
    replace_routes(state, routes);
}

fn route_matches_bulk(state: &BuilderState, route: &GeneratedRoute) -> bool {
    if state
        .route_bulk
        .filter_type
        .is_some_and(|filter| filter != route.route_type)
    {
        return false;
    }
    if state
        .route_bulk
        .filter_stability
        .is_some_and(|filter| filter != route.stability)
    {
        return false;
    }
    let tag_filter = state.route_bulk.filter_tag.trim();
    if !tag_filter.is_empty()
        && !route
            .tags
            .iter()
            .any(|tag| tag.as_ref().contains(tag_filter))
    {
        return false;
    }
    if let Some(region_id) = &state.route_bulk.filter_region {
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
    ui_kit::combo(id, value.map_or("(any)", RouteType::editor_label)).show_ui(ui, |ui| {
        ui.selectable_value(value, None, "(any)");
        for option in RouteType::ALL {
            ui.selectable_value(value, Some(option), option.editor_label());
        }
    });
}

fn optional_stability_combo(ui: &mut Ui, id: &str, value: &mut Option<RouteStability>) {
    ui_kit::combo(id, value.map_or("(any)", stability_label)).show_ui(ui, |ui| {
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
    ui_kit::combo(id, selected).show_ui(ui, |ui| {
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
    ui_kit::collapsing_section(ui, "route_rules", "How routes are generated", false, |ui| {
        ui.label(
            RichText::new("Tuning the generator uses when it draws routes for you.")
                .small()
                .color(Color32::DARK_GRAY),
        );
        if state.config.inputs.route_rules.is_none() {
            ui.colored_label(
                palette::warning(),
                "No save file is set for these rules yet — changes apply now but won't be saved until the project has a route-rules file.",
            );
        }

        let mut changed = false;
        {
            let rules = state
                .data_catalogs
                .route_rules
                .get_or_insert_with(RouteRules::default);
            labeled(
                ui,
                "Base likelihood",
                "Baseline weight for considering any route (schema: default_weight). Higher = more routes drawn.",
                |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut rules.default_weight)
                                .speed(0.1)
                                .range(0.01..=1000.0),
                        )
                        .changed();
                },
            );
            labeled(
                ui,
                "Max length (hops)",
                "Longest route the generator will draw, in hexes (schema: max_distance).",
                |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut rules.max_distance).range(1..=64))
                        .changed();
                },
            );
            labeled(
                ui,
                "Favour populated worlds",
                "Prefer linking systems with populated worlds (schema: prefer_populated_worlds).",
                |ui| {
                    changed |= ui
                        .checkbox(&mut rules.prefer_populated_worlds, "")
                        .changed();
                },
            );
            labeled(
                ui,
                "Favour trade hubs",
                "Prefer linking trade-hub systems (schema: prefer_trade_hubs).",
                |ui| {
                    changed |= ui.checkbox(&mut rules.prefer_trade_hubs, "").changed();
                },
            );
            labeled(
                ui,
                "Avoid warp hazards",
                "Steer routes around warp phenomena where possible (schema: avoid_warp_phenomena).",
                |ui| {
                    changed |= ui.checkbox(&mut rules.avoid_warp_phenomena, "").changed();
                },
            );

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
    ui.label(
        RichText::new(
            "Optional rules that nudge route likelihood when a system matches (schema: modifiers). \
             Leave a column at \"(any)\" to ignore it.",
        )
        .small()
        .color(Color32::DARK_GRAY),
    );
    if rules.modifiers.is_empty() {
        ui_kit::placeholder(
            ui,
            "No modifiers yet — add one to make certain worlds attract or repel routes.",
        );
    }
    let mut changed = false;
    let mut remove = None;
    egui::Grid::new("route_modifiers_grid")
        .striped(true)
        .num_columns(6)
        .show(ui, |ui| {
            control_header(
                ui,
                "Notable feature",
                "Match systems with this notable feature (schema: when.notable_feature).",
            );
            control_header(
                ui,
                "World type",
                "Match systems with this world type (schema: when.world_type).",
            );
            control_header(
                ui,
                "Government",
                "Match systems with this government (schema: when.government).",
            );
            control_header(
                ui,
                "Lane type",
                "Match only this lane type (schema: when.route_type).",
            );
            control_header(
                ui,
                "Multiplier",
                "Multiply route likelihood when the row matches (schema: multiplier). >1 attracts, <1 repels.",
            );
            ui.label("");
            ui.end_row();

            for (i, modifier) in rules.modifiers.iter_mut().enumerate() {
                changed |= optional_enum_combo(
                    ui,
                    format!("route_mod_feature_{i}"),
                    &mut modifier.when.notable_feature,
                    NotableFeature::VARIANTS,
                    |v| v.display_name(),
                );
                changed |= optional_enum_combo(
                    ui,
                    format!("route_mod_world_type_{i}"),
                    &mut modifier.when.world_type,
                    WorldType::VARIANTS,
                    |v| v.display_name(),
                );
                changed |= optional_enum_combo(
                    ui,
                    format!("route_mod_government_{i}"),
                    &mut modifier.when.government,
                    Government::VARIANTS,
                    |v| v.display_name(),
                );
                changed |= optional_enum_combo(
                    ui,
                    format!("route_mod_route_type_{i}"),
                    &mut modifier.when.route_type,
                    &RouteType::ALL,
                    |v| v.editor_label(),
                );
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut modifier.multiplier)
                            .speed(0.05)
                            .range(0.01..=100.0),
                    )
                    .changed();
                if ui.small_button("×").on_hover_text("Remove this modifier").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        rules.modifiers.remove(i);
        changed = true;
    }
    if ui
        .button("➕  Add modifier")
        .on_hover_text("Add a rule that nudges route likelihood for matching systems")
        .clicked()
    {
        rules.modifiers.push(RouteModifier {
            when: RouteCondition::default(),
            multiplier: 1.0,
        });
        changed = true;
    }
    changed
}

/// A `(any)`-or-one-variant dropdown over a typed `RouteCondition` field. The
/// stored value is the domain enum itself (`Option<T>`), so an out-of-enum
/// value is unrepresentable — there is no string to mis-type (P10). Editing the
/// rules in place follows the existing route-rules direct-write path (the panel
/// has never routed route_rules through the command bus); `mark_route_rules_changed`
/// flags the dirty file + schedules the preview.
fn optional_enum_combo<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut Option<T>,
    variants: &[T],
    label: impl Fn(&T) -> &'static str,
) -> bool {
    let before = value.clone();
    let selected = value.as_ref().map_or("(any)", &label);
    ui_kit::combo(id, selected).show_ui(ui, |ui| {
        ui.selectable_value(value, None, "(any)");
        for variant in variants {
            ui.selectable_value(value, Some(variant.clone()), label(variant));
        }
    });
    *value != before
}

fn mark_route_rules_changed(ui: &Ui, state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = &state.config.inputs.route_rules {
        state.dirty_files.insert(rel.clone());
    }
    state.mark_validation_dirty();
    let now = ui.ctx().input(|i| i.time);
    state
        .generation
        .preview
        .schedule(now, DEFAULT_DEBOUNCE_MS as f64 / 1000.0);
}

fn show_preview_status(ui: &mut Ui, state: &BuilderState) {
    if state.generation.preview.timer.is_some() {
        ui.label(RichText::new("Preview update queued…").color(Color32::DARK_GRAY));
    } else if state.generation.preview.job.is_some() {
        ui.label(RichText::new("Updating preview…").color(Color32::DARK_GRAY));
    } else if let Some(sector) = &state.generation.preview.sector {
        ui.label(
            RichText::new(format!("Preview ready: {} route(s).", sector.routes.len()))
                .color(Color32::DARK_GRAY),
        );
    } else if let Some(err) = &state.generation.preview.error {
        ui.colored_label(palette::danger(), err);
    }
}

// ── R6 hidden routes ────────────────────────────────────────────────────────

fn show_hidden_routes_panel(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "route_hidden_routes",
        "Hidden lanes (webway, black-ship, smuggling)",
        false,
        |ui| {
            ui.label(
            RichText::new("Generate covert links between chosen systems — they bypass the normal route network.")
                .small()
                .color(Color32::DARK_GRAY),
        );
            labeled(
                ui,
                "Lane kind",
                "Which covert lane type to build (schema: route_type).",
                |ui| hidden_kind_combo(ui, &mut state.hidden_routes.kind),
            );
            labeled(
                ui,
                "Links per system",
                "Connect each system to this many nearest neighbours (schema: k_nearest).",
                |ui| {
                    ui.add(egui::DragValue::new(&mut state.hidden_routes.k_nearest).range(1..=16));
                },
            );
            ui.checkbox(
                &mut state.hidden_routes.exclude_blackout,
                "Skip Blackout regions",
            )
            .on_hover_text("Don't route covert lanes through Blackout warp regions");

            ui.add_space(2.0);
            ui.label(
                RichText::new("Endpoints to link:")
                    .small()
                    .color(Color32::DARK_GRAY),
            );
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Use map selection")
                    .on_hover_text("Use the systems currently selected on the Map tab")
                    .clicked()
                {
                    state.hidden_routes.endpoints = state.selection.systems.clone();
                }
                if ui
                    .button("Select all")
                    .on_hover_text("Use every system in the sector")
                    .clicked()
                {
                    state.hidden_routes.endpoints =
                        state.sector.systems.iter().map(|s| s.id.clone()).collect();
                }
                if ui
                    .button("Clear")
                    .on_hover_text("Deselect every endpoint")
                    .clicked()
                {
                    state.hidden_routes.endpoints.clear();
                }
                ui.label(
                    RichText::new(format!("{} selected", state.hidden_routes.endpoints.len()))
                        .color(Color32::DARK_GRAY),
                );
            });

            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for sys in &state.sector.systems {
                        let mut selected = state.hidden_routes.endpoints.contains(&sys.id);
                        if ui
                            .checkbox(&mut selected, format!("{} — {}", sys.id, sys.name))
                            .changed()
                        {
                            if selected {
                                state.hidden_routes.endpoints.insert(sys.id.clone());
                            } else {
                                state.hidden_routes.endpoints.remove(&sys.id);
                            }
                        }
                    }
                });

            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("➕  Build hidden lanes")
                    .on_hover_text("Create the covert links between the selected endpoints")
                    .clicked()
                {
                    let cfg = sectorforge::hidden_routes::HiddenRoutesConfig {
                        kind: state.hidden_routes.kind,
                        endpoints: state.hidden_routes.endpoints.iter().cloned().collect(),
                        k_nearest: state.hidden_routes.k_nearest.max(1),
                        exclude_blackout_regions: state.hidden_routes.exclude_blackout,
                    };
                    let new_routes = sectorforge::hidden_routes::configured_hidden_routes(
                        &state.sector.systems,
                        &state.sector.factions,
                        state.sector.regions.as_ref(),
                        &state.sector.routes,
                        &cfg,
                    );
                    if new_routes.is_empty() {
                        state.feedback.modal = Some(ModalKind::Message(
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
                if ui
                    .button("🗑  Remove lanes of this kind")
                    .on_hover_text("Delete every route of the selected lane kind")
                    .clicked()
                {
                    let kind = state.hidden_routes.kind;
                    let mut routes = state.sector.routes.clone();
                    let before = routes.len();
                    routes.retain(|route| route.route_type != kind);
                    if routes.len() == before {
                        state.feedback.modal = Some(ModalKind::Message(
                            "No matching hidden routes found.".into(),
                        ));
                    } else {
                        replace_routes(state, routes);
                    }
                }
            });
        },
    );
}

fn hidden_kind_combo(ui: &mut Ui, value: &mut RouteType) {
    ui_kit::combo("hidden_route_kind", value.editor_label()).show_ui(ui, |ui| {
        for option in [
            RouteType::Webway,
            RouteType::BlackShip,
            RouteType::SmugglingLane,
        ] {
            ui.selectable_value(value, option, option.editor_label())
                .on_hover_text(format!("schema key: {}", option.as_slug()));
        }
    });
}

// ── R7 ensure-connected connector ───────────────────────────────────────────

fn show_ensure_connected(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "route_ensure_connected",
        "Keep everything reachable",
        false,
        |ui| {
            ui.label(
                RichText::new(
                    "Bridge routes link otherwise-isolated clusters so no system is stranded.",
                )
                .small()
                .color(Color32::DARK_GRAY),
            );
            let mut enabled = state.config.generation.routes.ensure_connected_graph;
            if ui
                .checkbox(&mut enabled, "Auto-connect after every edit")
                .on_hover_text(
                    "Automatically add bridge routes whenever an edit splits the network \
                     (schema: ensure_connected_graph).",
                )
                .changed()
            {
                state.config.generation.routes.ensure_connected_graph = enabled;
                state.dirty = true;
                state.mark_validation_dirty();
                if enabled {
                    apply_ensure_connected_if_enabled(state);
                }
            }
            let components = sectorforge::routes::route_component_count(
                &state.sector.systems,
                &state.sector.routes,
            );
            let added = ensure_connected_routes(state, state.sector.routes.clone()).1;
            if components <= 1 {
                ui.label(
                    RichText::new("Everything is reachable — one connected network.")
                        .color(Color32::DARK_GRAY),
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "{components} separate clusters — connecting would add {added} bridge route(s)."
                    ))
                    .color(palette::warning()),
                );
            }
            if ui
                .button("▶  Connect now")
                .on_hover_text(
                    "Add the bridge routes needed to join every cluster into one network",
                )
                .clicked()
            {
                let (routes, added) = ensure_connected_routes(state, state.sector.routes.clone());
                if added == 0 {
                    state.feedback.modal =
                        Some(ModalKind::Message("Route graph already connected.".into()));
                } else {
                    replace_routes(state, routes);
                }
            }
        },
    );
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

/// Thin builder-side adapter over [`sectorforge::routes::ensure_connected_routes`]
/// (relocated to the lib in REVIEW P15): unpacks the lib-pure inputs the
/// algorithm needs from the sector + config so panel call sites stay terse.
fn ensure_connected_routes(
    state: &BuilderState,
    routes: Vec<GeneratedRoute>,
) -> (Vec<GeneratedRoute>, usize) {
    sectorforge::routes::ensure_connected_routes(
        &state.sector.systems,
        &state.sector.factions,
        routes,
        state.config.generation.routes.max_route_distance,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    // The union-find + MST connectivity algorithm itself now lives in the lib
    // (`sectorforge::routes`, REVIEW P15) and is unit-tested there. This test
    // only covers the builder-side adapter: that `&BuilderState` is unpacked
    // into the lib call correctly (systems + factions + config distance).
    #[test]
    fn ensure_connected_adapter_unpacks_state() {
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
        assert_eq!(
            sectorforge::routes::route_component_count(&state.sector.systems, &routes),
            1
        );
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
