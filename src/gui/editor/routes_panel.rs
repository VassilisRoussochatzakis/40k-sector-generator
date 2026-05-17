//! Routes list editor. Each row: from, to, type, stability, distance, delete.

use egui::{RichText, Ui};

use crate::sector_model::{HexCoord, RouteStability, RouteType};

use super::enums::{ROUTE_STABILITIES, ROUTE_TYPES};
use super::state::{empty_route, EditorState, RouteEndpoint};
use super::ui_helpers::{combo_kv, dim, label, mono, section};

pub fn show_routes(ui: &mut Ui, state: &mut EditorState) {
    let current_pick = state.route_pick;
    let Some(sector) = state.sector.as_mut() else {
        dim(ui, "no sector loaded");
        state.route_pick = None;
        return;
    };
    section(ui, &format!("ROUTES ({})", sector.routes.len()));
    dim(ui, "tip: click PICK then click a system on the map");

    let system_options: Vec<String> = sector.systems.iter().map(|s| s.id.clone()).collect();
    if system_options.len() < 2 {
        dim(ui, "need ≥2 systems to add routes");
    }
    let system_labels: Vec<(String, String)> = sector
        .systems
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();
    let opt_kv: Vec<(&str, &str)> = system_labels
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()))
        .collect();

    let coord_lookup: std::collections::HashMap<String, HexCoord> = sector
        .systems
        .iter()
        .map(|s| (s.id.clone(), s.coord))
        .collect();

    let mut dirty = false;
    let mut remove_idx: Option<usize> = None;
    let mut new_pick: Option<Option<(usize, RouteEndpoint)>> = None;

    for (i, route) in sector.routes.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            label(ui, "FROM");
            if combo_kv(
                ui,
                &format!("r_from_{i}"),
                &mut route.from_system_id,
                &opt_kv,
            ) {
                dirty = true;
            }
            let from_active = current_pick == Some((i, RouteEndpoint::From));
            let from_btn = if from_active { "PICKING…" } else { "PICK" };
            if ui
                .small_button(RichText::new(from_btn).font(mono(10.0)))
                .clicked()
            {
                new_pick = Some(if from_active {
                    None
                } else {
                    Some((i, RouteEndpoint::From))
                });
            }
            label(ui, "TO");
            if combo_kv(ui, &format!("r_to_{i}"), &mut route.to_system_id, &opt_kv) {
                dirty = true;
            }
            let to_active = current_pick == Some((i, RouteEndpoint::To));
            let to_btn = if to_active { "PICKING…" } else { "PICK" };
            if ui
                .small_button(RichText::new(to_btn).font(mono(10.0)))
                .clicked()
            {
                new_pick = Some(if to_active {
                    None
                } else {
                    Some((i, RouteEndpoint::To))
                });
            }
        });
        ui.horizontal(|ui| {
            let mut rtype = route_type_str(route.route_type).to_string();
            label(ui, "TYPE");
            if combo_kv(ui, &format!("r_type_{i}"), &mut rtype, ROUTE_TYPES) {
                if let Some(rt) = route_type_from_str(&rtype) {
                    route.route_type = rt;
                    dirty = true;
                }
            }
            let mut stab = route_stab_str(route.stability).to_string();
            label(ui, "STAB");
            if combo_kv(ui, &format!("r_stab_{i}"), &mut stab, ROUTE_STABILITIES) {
                if let Some(rs) = route_stab_from_str(&stab) {
                    route.stability = rs;
                    dirty = true;
                }
            }
            label(ui, "DIST");
            let mut d = route.distance as i32;
            if ui.add(egui::DragValue::new(&mut d).range(0..=99)).changed() {
                route.distance = d.max(0) as u32;
                dirty = true;
            }
            if ui
                .small_button(RichText::new("x").font(mono(11.0)))
                .clicked()
            {
                remove_idx = Some(i);
            }
        });
        ui.separator();
    }

    if let Some(i) = remove_idx {
        sector.routes.remove(i);
        dirty = true;
        // Clear/reindex pick after deletion.
        new_pick = Some(match current_pick {
            Some((idx, _)) if idx == i => None,
            Some((idx, ep)) if idx > i => Some((idx - 1, ep)),
            other => other,
        });
    }

    if system_options.len() >= 2
        && ui
            .button(RichText::new("+ ADD ROUTE").font(mono(12.0)))
            .clicked()
    {
        let from = system_options[0].clone();
        let to = system_options[1].clone();
        let mut route = empty_route(from.clone(), to.clone());
        if let (Some(a), Some(b)) = (coord_lookup.get(&from), coord_lookup.get(&to)) {
            route.distance = crate::sector_model::hex_distance(*a, *b);
        }
        sector.routes.push(route);
        dirty = true;
    }

    // Refresh IDs after potential FROM/TO changes so they stay unique-ish.
    if dirty {
        for r in &mut sector.routes {
            r.id = crate::ids::route_id(&r.from_system_id, &r.to_system_id);
        }
        state.mark_dirty();
    }
    if let Some(p) = new_pick {
        state.route_pick = p;
    }
}

fn route_type_str(rt: RouteType) -> &'static str {
    match rt {
        RouteType::StableWarpLane => "stable_warp_lane",
        RouteType::ChartedPassage => "charted_passage",
        RouteType::DangerousPassage => "dangerous_passage",
        RouteType::SecretPassage => "secret_passage",
    }
}

fn route_type_from_str(s: &str) -> Option<RouteType> {
    Some(match s {
        "stable_warp_lane" => RouteType::StableWarpLane,
        "charted_passage" => RouteType::ChartedPassage,
        "dangerous_passage" => RouteType::DangerousPassage,
        "secret_passage" => RouteType::SecretPassage,
        _ => return None,
    })
}

fn route_stab_str(rs: RouteStability) -> &'static str {
    match rs {
        RouteStability::Stable => "stable",
        RouteStability::Unstable => "unstable",
        RouteStability::Hazardous => "hazardous",
        RouteStability::Lost => "lost",
    }
}

fn route_stab_from_str(s: &str) -> Option<RouteStability> {
    Some(match s {
        "stable" => RouteStability::Stable,
        "unstable" => RouteStability::Unstable,
        "hazardous" => RouteStability::Hazardous,
        "lost" => RouteStability::Lost,
        _ => return None,
    })
}
