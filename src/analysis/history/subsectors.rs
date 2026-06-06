//! Per-subsector emission: capital naming via full clustering when cheap,
//! or deterministic sampled stand-in when sector exceeds the cap.

use crate::sector_model::GeneratedSystem;

use super::build::build_event;
use super::context::EmitContext;
use super::model::{
    EventKind, HistoryAnchor, HistoryConsequence, HistoryConsequenceKind, HistoryEntityKind,
    HistoryEntityRef, HistoryEvent,
};
use super::progress::HistoryProgress;

pub(super) fn emit_subsector_events(
    ctx: &EmitContext,
    out: &mut Vec<HistoryEvent>,
    progress: &mut impl FnMut(HistoryProgress),
) {
    if ctx.sector.systems.is_empty() {
        return;
    }
    let exact_cluster_count = ctx
        .sector
        .systems
        .len()
        .div_ceil(crate::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR as usize)
        .max(1);
    let cap = ctx.cfg.max_subsector_events;
    if cap == 0 {
        progress(HistoryProgress::SubsectorEventsStarted {
            exact_cluster_count,
            emitted_cap: cap,
            sampled: true,
        });
        progress(HistoryProgress::SubsectorEventsDone { events: 0 });
        return;
    }
    if exact_cluster_count > cap as usize {
        progress(HistoryProgress::SubsectorEventsStarted {
            exact_cluster_count,
            emitted_cap: cap,
            sampled: true,
        });
        emit_sampled_subsector_events(ctx, out, cap as usize);
        progress(HistoryProgress::SubsectorEventsDone {
            events: cap as usize,
        });
        return;
    }
    progress(HistoryProgress::SubsectorEventsStarted {
        exact_cluster_count,
        emitted_cap: cap,
        sampled: false,
    });
    let Ok(subsectors) = crate::subsectors::build_subsectors(
        ctx.sector,
        crate::subsectors::SubsectorConfig::default(),
    ) else {
        return;
    };
    for (i, sub) in subsectors.iter().enumerate() {
        let Some(cap_sys) = &sub.summary.subsector_capital_system_id else {
            continue;
        };
        let sys_name = ctx
            .sector
            .systems
            .iter()
            .find(|s| s.id == *cap_sys)
            .map(|s| s.name.as_ref())
            .unwrap_or(cap_sys.as_str());
        let text = match &sub.summary.subsector_capital_world_id {
            Some(wid) => format!(
                "{} was elevated around {sys_name}; {} became the recorded capital world.",
                sub.name, wid
            ),
            None => format!(
                "{sub} was charted as an administrative subsector around {sys_name}.",
                sub = sub.name
            ),
        };
        let anchor = HistoryAnchor::Subsector {
            subsector_id: sub.id.to_string(),
        };
        let mut ev = build_event(ctx, anchor, EventKind::Foundation, text, Vec::new(), 35, i);
        ev.entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: cap_sys.to_string(),
            role: Some("capital_system".into()),
        });
        if let Some(wid) = &sub.summary.subsector_capital_world_id {
            ev.entities.push(HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: wid.to_string(),
                role: Some("capital_world".into()),
            });
        }
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::SubsectorCapitalNamed,
            description: format!("{} gained a capital at {sys_name}.", sub.name),
            severity: 35,
            entity_id: Some(sub.id.to_string()),
        });
        out.push(ev);
    }
    progress(HistoryProgress::SubsectorEventsDone {
        events: subsectors.len(),
    });
}

fn emit_sampled_subsector_events(ctx: &EmitContext, out: &mut Vec<HistoryEvent>, cap: usize) {
    if cap == 0 || ctx.sector.systems.is_empty() {
        return;
    }
    let mut systems: Vec<&GeneratedSystem> = ctx.sector.systems.iter().collect();
    systems.sort_by(|a, b| {
        a.coord
            .r
            .cmp(&b.coord.r)
            .then_with(|| a.coord.q.cmp(&b.coord.q))
            .then_with(|| a.id.cmp(&b.id))
    });
    let emit_count = cap.min(systems.len());
    for i in 0..emit_count {
        let idx = (i * systems.len()) / emit_count;
        let sys = systems[idx];
        let capital_world = sys.worlds.iter().max_by(|a, b| {
            population_rank(&a.world.population.to_string())
                .cmp(&population_rank(&b.world.population.to_string()))
                .then_with(|| {
                    tech_rank(&a.world.tech_level.to_string())
                        .cmp(&tech_rank(&b.world.tech_level.to_string()))
                })
                .then_with(|| a.id.cmp(&b.id))
        });
        let subsector_id = format!("chronicle-subsector-{:04}", i + 1);
        let text = match capital_world {
            Some(world) => format!(
                "Chronicle surveyors grouped a vast administrative march around {}; {} became the recorded capital world.",
                sys.name, world.name
            ),
            None => format!(
                "Chronicle surveyors grouped a vast administrative march around {}.",
                sys.name
            ),
        };
        let anchor = HistoryAnchor::Subsector {
            subsector_id: subsector_id.clone(),
        };
        let mut ev = build_event(ctx, anchor, EventKind::Foundation, text, Vec::new(), 35, i);
        ev.entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: sys.id.to_string(),
            role: Some("capital_system".into()),
        });
        if let Some(world) = capital_world {
            ev.entities.push(HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: world.id.to_string(),
                role: Some("capital_world".into()),
            });
        }
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::SubsectorCapitalNamed,
            description: format!(
                "A sampled chronicle march gained a capital at {}.",
                sys.name
            ),
            severity: 35,
            entity_id: Some(subsector_id),
        });
        out.push(ev);
    }
}

fn population_rank(value: &str) -> i32 {
    match value {
        "Massive" => 6,
        "DenselyPopulated" => 5,
        "Standard" => 4,
        "LightlyPopulated" => 3,
        "SoleSettlement" => 2,
        "Minimal" => 1,
        "Uninhabited" => 0,
        _ => 0,
    }
}

fn tech_rank(value: &str) -> i32 {
    match value {
        "Archeotech" => 6,
        "High" => 5,
        "Imperial" => 4,
        "Standard" => 3,
        "Low" => 2,
        "Primitive" => 1,
        "None" => 0,
        _ => 0,
    }
}
