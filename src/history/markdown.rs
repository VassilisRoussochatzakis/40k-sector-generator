//! Narrative-source Markdown rendering of a chronicle + paired JSON writer.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::Utf8Path;

use crate::errors::SectorError;

use super::config::HistoryConfig;
use super::model::{HistoryAnchor, HistoryEvent, HistoryReport};

#[must_use]
pub fn render_markdown(report: &HistoryReport, cfg: &HistoryConfig) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Sector Chronicle — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal events: **{}**", report.events.len());

    // Key events digest.
    let mut keyed: Vec<&HistoryEvent> = report.events.iter().collect();
    keyed.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.date.cmp(&b.date))
            .then_with(|| a.id.cmp(&b.id))
    });
    if !keyed.is_empty() {
        let _ = writeln!(s, "\n## Key events");
        let n = (cfg.key_events_top_n as usize).min(keyed.len());
        for e in keyed.iter().take(n) {
            let _ = writeln!(
                s,
                "- **{}** · _{}_ ({:?}, weight {}): {}",
                e.date, e.era_label, e.kind, e.weight, e.narrative
            );
        }
    }

    // Group remaining events by anchor for the chronicle proper.
    let mut by_system: BTreeMap<crate::ids::SystemId, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_route: BTreeMap<crate::ids::RouteId, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_subsector: BTreeMap<String, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_region: BTreeMap<String, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_world: BTreeMap<(crate::ids::SystemId, crate::ids::WorldId), Vec<&HistoryEvent>> =
        BTreeMap::new();
    let mut sector_events: Vec<&HistoryEvent> = Vec::new();
    for e in &report.events {
        match &e.anchor {
            HistoryAnchor::Sector => sector_events.push(e),
            HistoryAnchor::System { system_id } => {
                by_system.entry(system_id.clone()).or_default().push(e)
            }
            HistoryAnchor::Route { route_id, .. } => {
                by_route.entry(route_id.clone()).or_default().push(e)
            }
            HistoryAnchor::Subsector { subsector_id } => by_subsector
                .entry(subsector_id.clone())
                .or_default()
                .push(e),
            HistoryAnchor::Region { region_id } => {
                by_region.entry(region_id.clone()).or_default().push(e)
            }
            HistoryAnchor::World {
                system_id,
                world_id,
            } => by_world
                .entry((system_id.clone(), world_id.clone()))
                .or_default()
                .push(e),
        }
    }

    if !sector_events.is_empty() {
        let _ = writeln!(s, "\n## Sector-wide events");
        for e in &sector_events {
            let _ = writeln!(s, "- **{}** — {}", e.date, e.narrative);
        }
    }

    if !by_system.is_empty() {
        let _ = writeln!(s, "\n## System chronicles");
        for (sys_id, evs) in &by_system {
            let _ = writeln!(s, "\n### {sys_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_route.is_empty() {
        let _ = writeln!(s, "\n## Route chronicles");
        for (route_id, evs) in &by_route {
            let _ = writeln!(s, "\n### {route_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_subsector.is_empty() {
        let _ = writeln!(s, "\n## Subsector chronicles");
        for (subsector_id, evs) in &by_subsector {
            let _ = writeln!(s, "\n### {subsector_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_region.is_empty() {
        let _ = writeln!(s, "\n## Region chronicles");
        for (region_id, evs) in &by_region {
            let _ = writeln!(s, "\n### {region_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_world.is_empty() {
        let _ = writeln!(s, "\n## World chronicles");
        for ((sys_id, world_id), evs) in &by_world {
            let _ = writeln!(s, "\n### {sys_id} · {world_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    s
}

fn write_event_line(s: &mut String, e: &HistoryEvent) {
    let entity_ids: Vec<&str> = e.entities.iter().map(|x| x.id.as_str()).collect();
    let consequences: Vec<&str> = e
        .consequences
        .iter()
        .map(|x| x.description.as_str())
        .collect();
    let _ = writeln!(
        s,
        "- **{}** · _{}_ ({:?}): {}",
        e.date, e.era_label, e.kind, e.narrative
    );
    if !entity_ids.is_empty() {
        let _ = writeln!(s, "  - refs: `{}`", entity_ids.join("`, `"));
    }
    if !consequences.is_empty() {
        let _ = writeln!(s, "  - consequences: {}", consequences.join("; "));
    }
}

/// Write `history.md` + `history.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written, and
/// [`SectorError::ExportFailed`] if the report cannot be serialised.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &HistoryReport,
    cfg: &HistoryConfig,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "history", &render_markdown(report, cfg), report)
}
