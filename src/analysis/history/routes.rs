//! Per-route emission: warp-instability hazards, concealed-passage discoveries,
//! pirate/interdictor control consolidation.

use std::cmp::Ordering;

use crate::sector_model::{GeneratedRoute, RouteStability, RouteType};

use super::build::build_event;
use super::context::EmitContext;
use super::model::{EventKind, HistoryAnchor, HistoryEvent};

pub(super) fn emit_route_events(
    ctx: &EmitContext,
    route: &GeneratedRoute,
    out: &mut Vec<HistoryEvent>,
) {
    let from = ctx
        .system_names
        .get(route.from_system_id.as_str())
        .copied()
        .unwrap_or(route.from_system_id.as_str());
    let to = ctx
        .system_names
        .get(route.to_system_id.as_str())
        .copied()
        .unwrap_or(route.to_system_id.as_str());

    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();
    if matches!(
        route.stability,
        RouteStability::Unstable | RouteStability::Hazardous | RouteStability::Perilous
    ) {
        let kind = if matches!(route.stability, RouteStability::Perilous) {
            EventKind::WarpStormSurge
        } else {
            EventKind::Discovery
        };
        let weight = (kind.base_weight() as u32
            + match route.stability {
                RouteStability::Stable => 0,
                RouteStability::Unstable => 10,
                RouteStability::Hazardous => 25,
                RouteStability::Perilous => 35,
            })
        .min(100) as u8;
        buf.push((
            kind,
            format!(
                "Navigators marked the lane between {from} and {to} as {:?}; later charts record it as {:?}.",
                route.route_type, route.stability
            ),
            Vec::new(),
            weight,
        ));
    }

    if matches!(
        route.route_type,
        RouteType::SecretPassage
            | RouteType::Webway
            | RouteType::BlackShip
            | RouteType::SmugglingLane
    ) {
        let kind = match route.route_type {
            RouteType::Webway => EventKind::AeldariActivity,
            RouteType::BlackShip => EventKind::ImperialMandateGranted,
            RouteType::SmugglingLane | RouteType::SecretPassage => EventKind::Discovery,
            _ => EventKind::Discovery,
        };
        buf.push((
            kind,
            format!(
                "A concealed passage linking {from} and {to} entered restricted charts as {:?}.",
                route.route_type
            ),
            Vec::new(),
            kind.base_weight(),
        ));
    }

    if let Some(c) = route
        .controls
        .iter()
        .filter(|c| c.interdiction >= 60.0 || c.piracy >= 60.0)
        .max_by(|a, b| {
            (a.interdiction + a.piracy)
                .partial_cmp(&(b.interdiction + b.piracy))
                .unwrap_or(Ordering::Equal)
        })
    {
        let kind = EventKind::Blockade;
        buf.push((
            kind,
            format!(
                "{} became the dominant threat along the {from}-{to} route.",
                c.faction_id
            ),
            vec![c.faction_id.clone()],
            kind.base_weight(),
        ));
    }

    if buf.len() as u32 > ctx.cfg.max_events_per_route {
        buf.sort_by_key(|b| std::cmp::Reverse(b.3));
        buf.truncate(ctx.cfg.max_events_per_route as usize);
    }
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::Route {
            route_id: route.id.clone(),
            from_system_id: route.from_system_id.clone(),
            to_system_id: route.to_system_id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}
