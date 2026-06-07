//! Chronicle renders (`world_history`, `system_history`) + event-mention
//! predicates. Split verbatim from `info_panel.rs` (AREA_F F8, by section).


use egui::Ui;

use sectorforge::sector_model::GeneratedSector;


use super::{dim, section, short};

pub fn world_history(ui: &mut Ui, sector: &GeneratedSector, world_id: &str) {
    let mut hits: Vec<_> = sector
        .chronicle
        .events
        .iter()
        .filter(|e| event_mentions_world(e, world_id))
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    ui.add_space(8.0);
    section(ui, "HISTORY");
    for e in hits {
        dim(
            ui,
            &format!(
                "{}  {:?}  {}",
                e.date,
                e.kind,
                short(&e.summary.to_uppercase(), 42)
            ),
        );
    }
}

pub(super) fn system_history(ui: &mut Ui, sector: &GeneratedSector, system_id: &str) {
    let mut hits: Vec<_> = sector
        .chronicle
        .events
        .iter()
        .filter(|e| event_mentions_system(e, system_id))
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    ui.add_space(8.0);
    section(ui, "LOCAL HISTORY");
    for e in hits.iter().take(8) {
        dim(
            ui,
            &format!(
                "{}  {:?}  {}",
                e.date,
                e.kind,
                short(&e.summary.to_uppercase(), 42)
            ),
        );
    }
}

fn event_mentions_world(e: &sectorforge::history::HistoryEvent, world_id: &str) -> bool {
    (match &e.anchor {
        sectorforge::history::HistoryAnchor::World { world_id: wid, .. } => wid == world_id,
        _ => false,
    }) || e.entities.iter().any(|x| {
        matches!(x.kind, sectorforge::history::HistoryEntityKind::World) && x.id == world_id
    })
}

fn event_mentions_system(e: &sectorforge::history::HistoryEvent, system_id: &str) -> bool {
    (match &e.anchor {
        sectorforge::history::HistoryAnchor::System { system_id: sid } => sid == system_id,
        sectorforge::history::HistoryAnchor::World { system_id: sid, .. } => sid == system_id,
        sectorforge::history::HistoryAnchor::Route {
            from_system_id,
            to_system_id,
            ..
        } => from_system_id == system_id || to_system_id == system_id,
        _ => false,
    }) || e.entities.iter().any(|x| {
        matches!(x.kind, sectorforge::history::HistoryEntityKind::System) && x.id == system_id
    })
}

#[cfg(test)]
mod tests {
    use super::{event_mentions_system, event_mentions_world};
    use sectorforge::history::{
        EventKind, HistoryAnchor, HistoryEntityKind, HistoryEntityRef, HistoryEvent,
    };

    /// `HistoryEvent` has no `Default`; spell out every field (model.rs:24-56).
    fn base_event(anchor: HistoryAnchor) -> HistoryEvent {
        HistoryEvent {
            id: "ev-1".into(),
            date: "M41.001".into(),
            era_id: String::new(),
            era_label: String::new(),
            relative_year: 0,
            anchor,
            kind: EventKind::Foundation,
            summary: String::new(),
            narrative: String::new(),
            factions: Vec::new(),
            entities: Vec::new(),
            consequences: Vec::new(),
            weight: 10,
            manual: false,
        }
    }

    #[test]
    fn world_anchor_matches_world_id() {
        let e = base_event(HistoryAnchor::World {
            system_id: "sys-0001".into(),
            world_id: "sys-0001-w01".into(),
        });
        assert!(event_mentions_world(&e, "sys-0001-w01"));
        assert!(!event_mentions_world(&e, "sys-0001-w99"));
    }

    #[test]
    fn world_anchor_surfaces_in_system_view() {
        // A World anchor carries its system_id, so the system-history filter
        // surfaces it via the `World { system_id: sid, .. }` arm (history.rs:76).
        let e = base_event(HistoryAnchor::World {
            system_id: "sys-0001".into(),
            world_id: "sys-0001-w01".into(),
        });
        assert!(event_mentions_system(&e, "sys-0001"));
        assert!(!event_mentions_system(&e, "sys-9999"));
    }

    #[test]
    fn route_anchor_matches_both_endpoints() {
        let e = base_event(HistoryAnchor::Route {
            route_id: "route-x".into(),
            from_system_id: "sys-0001".into(),
            to_system_id: "sys-0002".into(),
        });
        assert!(event_mentions_system(&e, "sys-0001"));
        assert!(event_mentions_system(&e, "sys-0002"));
        assert!(!event_mentions_system(&e, "sys-0003"));
        // A Route anchor has no World arm and no entities → never a world hit.
        assert!(!event_mentions_world(&e, "sys-0001"));
        assert!(!event_mentions_world(&e, "sys-0001-w01"));
    }

    #[test]
    fn entities_fallback_respects_world_kind() {
        // Sector anchor → only the entities list can produce a match.
        let mut e = base_event(HistoryAnchor::Sector);
        e.entities = vec![HistoryEntityRef {
            kind: HistoryEntityKind::World,
            id: "sys-0001-w01".into(),
            role: None,
        }];
        assert!(event_mentions_world(&e, "sys-0001-w01"));

        // Same id but the wrong kind: the `matches!(x.kind, ..World)` guard fails.
        let mut e2 = base_event(HistoryAnchor::Sector);
        e2.entities = vec![HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: "sys-0001-w01".into(),
            role: None,
        }];
        assert!(!event_mentions_world(&e2, "sys-0001-w01"));
    }

    #[test]
    fn entities_fallback_respects_system_kind() {
        // System-kind entity surfaces in the system filter.
        let mut e = base_event(HistoryAnchor::Sector);
        e.entities = vec![HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: "sys-0001".into(),
            role: None,
        }];
        assert!(event_mentions_system(&e, "sys-0001"));

        // World-kind entity with a system-looking id does NOT match the system
        // filter (kind guard fails for system lookup).
        let mut e2 = base_event(HistoryAnchor::Sector);
        e2.entities = vec![HistoryEntityRef {
            kind: HistoryEntityKind::World,
            id: "sys-0001".into(),
            role: None,
        }];
        assert!(!event_mentions_system(&e2, "sys-0001"));
    }
}

