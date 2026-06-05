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

