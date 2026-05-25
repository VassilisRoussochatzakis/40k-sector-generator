//! Deterministic sector chronicle generator (§1 NEW2.md).
//!
//! A `history` derivation pass over a finished `GeneratedSector` that walks
//! every world's claim list, dominance, archetype state, and conflict, and
//! emits a dated chronological list of in-universe events explaining how
//! the present configuration came to be.
//!
//! Pure derivation: no extra RNG draws affect other stages. The stage RNG
//! is seeded from `blake3("sectorforge:{seed}:history:{anchor_id}")`,
//! mirroring the existing per-stage RNG scheme. Same sector ⇒ same
//! chronicle, byte-stable.
//!
//! Output is intentionally narrative-source: lines a GM or writer can paste
//! into session notes. Calendar notation is `M{epoch}.{ddd}` by default.

use std::collections::BTreeMap;

use crate::sector_model::GeneratedSector;

mod build;
mod config;
mod context;
mod labels;
mod markdown;
mod model;
mod progress;
mod regions;
mod routes;
mod rules;
mod subsectors;
mod systems;
mod worlds;

#[cfg(test)]
mod tests;

pub use config::{HistoryConfig, HistoryEra, HistoryEventRule, HistoryFile};
pub use markdown::{render_markdown, write_report};
pub use model::{
    EventKind, HistoryAnchor, HistoryConsequence, HistoryConsequenceKind, HistoryEntityKind,
    HistoryEntityRef, HistoryEvent, HistoryReport, SectorChronicle,
};
pub use progress::HistoryProgress;

use context::EmitContext;
use progress::should_report_history_progress;

#[must_use]
pub fn derive(sector: &GeneratedSector) -> HistoryReport {
    derive_with(sector, &HistoryConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &HistoryConfig) -> HistoryReport {
    derive_with_progress(sector, cfg, |_| {})
}

pub fn derive_with_progress(
    sector: &GeneratedSector,
    cfg: &HistoryConfig,
    mut progress: impl FnMut(HistoryProgress),
) -> HistoryReport {
    if !cfg.enabled {
        return HistoryReport {
            sector_id: sector.id.to_string(),
            seed: sector.seed.to_string(),
            eras: cfg.eras.clone(),
            events: Vec::new(),
        };
    }
    let world_count: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    progress(HistoryProgress::Started {
        systems: sector.systems.len(),
        worlds: world_count,
        routes: sector.routes.len(),
        max_subsector_events: cfg.max_subsector_events,
    });

    let faction_names: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_ref()))
        .collect();
    let system_names: BTreeMap<&str, &str> = sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_ref()))
        .collect();

    let ctx = EmitContext {
        cfg,
        sector,
        faction_names: &faction_names,
        system_names: &system_names,
    };

    let mut events: Vec<HistoryEvent> = Vec::new();

    subsectors::emit_subsector_events(&ctx, &mut events, &mut progress);
    regions::emit_region_events(&ctx, &mut events);

    // Per-system + per-world events.
    let system_total = sector.systems.len();
    for (idx, sys) in sector.systems.iter().enumerate() {
        systems::emit_system_events(&ctx, sys, &mut events);
        for world in &sys.worlds {
            worlds::emit_world_events(&ctx, sys, world, &mut events);
        }
        let current = idx + 1;
        if should_report_history_progress(current, system_total) {
            progress(HistoryProgress::SystemsScanned {
                current,
                total: system_total,
                events: events.len(),
            });
        }
    }
    let route_total = sector.routes.len();
    for (idx, route) in sector.routes.iter().enumerate() {
        routes::emit_route_events(&ctx, route, &mut events);
        let current = idx + 1;
        if should_report_history_progress(current, route_total) {
            progress(HistoryProgress::RoutesScanned {
                current,
                total: route_total,
                events: events.len(),
            });
        }
    }
    rules::apply_event_rules(&ctx, &mut events);
    progress(HistoryProgress::EventRulesApplied {
        events: events.len(),
    });

    // Stable sort: epoch date then anchor then kind rank. Dates were chosen
    // so that the topo rank already orders events within an anchor; sorting
    // by date alone yields the final chronology.
    progress(HistoryProgress::SortingStarted {
        events: events.len(),
    });
    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| anchor_key(&a.anchor).cmp(&anchor_key(&b.anchor)))
            .then_with(|| a.kind.topo_rank().cmp(&b.kind.topo_rank()))
            .then_with(|| a.id.cmp(&b.id))
    });
    progress(HistoryProgress::Complete {
        events: events.len(),
    });

    HistoryReport {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        eras: cfg.eras.clone(),
        events,
    }
}

pub(crate) fn anchor_key(a: &HistoryAnchor) -> String {
    match a {
        HistoryAnchor::Sector => "0:sector".into(),
        HistoryAnchor::System { system_id } => format!("1:{system_id}"),
        HistoryAnchor::Route { route_id, .. } => format!("2:{route_id}"),
        HistoryAnchor::Subsector { subsector_id } => format!("3:{subsector_id}"),
        HistoryAnchor::Region { region_id } => format!("4:{region_id}"),
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("5:{system_id}:{world_id}"),
    }
}
