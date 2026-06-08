//! Event construction primitives: date/era/id synthesis, entity refs,
//! consequence attachment. Used by every emit-family module.

use crate::rng::stage_rng;

use super::config::HistoryConfig;
use super::context::EmitContext;
use super::labels::kind_slug;
use super::model::{
    EventKind, HistoryAnchor, HistoryConsequence, HistoryConsequenceKind, HistoryEntityKind,
    HistoryEntityRef, HistoryEvent,
};

pub(super) fn build_event(
    ctx: &EmitContext,
    anchor: HistoryAnchor,
    kind: EventKind,
    text: String,
    factions: Vec<crate::ids::FactionId>,
    weight: u8,
    ordinal: usize,
) -> HistoryEvent {
    let mut rng = stage_rng(
        &ctx.sector.seed,
        "history-event",
        &format!(
            "{}:{kind:?}:{ordinal}{}",
            super::anchor_key(&anchor),
            ctx.reroll_suffix,
        ),
    );
    let date = synthesise_date(&mut rng, ctx.cfg, kind, ordinal);
    let (era_id, era_label, relative_year) = synthesise_era(&mut rng, ctx.cfg, kind);
    let mut entities = entities_for_anchor(&anchor);
    for faction_id in &factions {
        entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::Faction,
            id: faction_id.to_string(),
            role: Some("participant".into()),
        });
    }
    HistoryEvent {
        id: event_id(&anchor, kind, ordinal),
        date,
        era_id,
        era_label,
        relative_year,
        anchor,
        kind,
        summary: text.clone(),
        narrative: text,
        factions,
        entities,
        consequences: consequences_for(kind),
        weight,
        manual: false,
    }
}

/// Date is `M{epoch}.{ddd}` where `epoch` is scaled by topo rank so that
/// foundation events fall in the start epoch and post-conflict events fall
/// in the end epoch. `ordinal` shifts the final ddd block within the
/// chosen epoch so multiple events at the same anchor are ordered.
fn synthesise_date(
    rng: &mut impl rand::Rng,
    cfg: &HistoryConfig,
    kind: EventKind,
    ordinal: usize,
) -> String {
    let span = cfg.epoch_end.saturating_sub(cfg.epoch_start).max(1);
    let normalised = (kind.topo_rank() as f32 / 70.0).clamp(0.0, 1.0);
    let epoch = cfg.epoch_start + (normalised * span as f32).round() as u32;
    let base = (kind.topo_rank() % 70) * 14;
    let jitter: u32 = rng.gen_range(0..40);
    let ddd = (base + jitter + ordinal as u32 * 5).min(999);
    format!("M{epoch}.{ddd:03}")
}

fn synthesise_era(
    rng: &mut impl rand::Rng,
    cfg: &HistoryConfig,
    kind: EventKind,
) -> (String, String, i32) {
    let era = cfg
        .eras
        .iter()
        .find(|e| e.allowed_events.is_empty() || e.allowed_events.contains(&kind))
        .or_else(|| cfg.eras.first());
    let Some(era) = era else {
        return ("unlabelled".into(), "Unlabelled".into(), 0);
    };
    let lo = era.relative_start.min(era.relative_end);
    let hi = era.relative_start.max(era.relative_end);
    let year = if lo == hi { lo } else { rng.gen_range(lo..=hi) };
    (era.id.clone(), era.label.clone(), year)
}

pub(super) fn event_id(anchor: &HistoryAnchor, kind: EventKind, ordinal: usize) -> String {
    match anchor {
        HistoryAnchor::Sector => format!("evt-sector-{}-{ordinal}", kind_slug(kind)),
        HistoryAnchor::System { system_id } => {
            format!("evt-{system_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Route { route_id, .. } => {
            format!("evt-{route_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Subsector { subsector_id } => {
            format!("evt-{subsector_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Region { region_id } => {
            format!("evt-{region_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("evt-{system_id}-{world_id}-{}-{ordinal}", kind_slug(kind)),
    }
}

fn entities_for_anchor(anchor: &HistoryAnchor) -> Vec<HistoryEntityRef> {
    match anchor {
        HistoryAnchor::Sector => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Sector,
            id: "sector".into(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::System { system_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: system_id.to_string(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Route {
            route_id,
            from_system_id,
            to_system_id,
        } => vec![
            HistoryEntityRef {
                kind: HistoryEntityKind::Route,
                id: route_id.to_string(),
                role: Some("anchor".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: from_system_id.to_string(),
                role: Some("route_endpoint".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: to_system_id.to_string(),
                role: Some("route_endpoint".into()),
            },
        ],
        HistoryAnchor::Subsector { subsector_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Subsector,
            id: subsector_id.clone(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Region { region_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Region,
            id: region_id.clone(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::World {
            system_id,
            world_id,
        } => vec![
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: system_id.to_string(),
                role: Some("parent_system".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: world_id.to_string(),
                role: Some("anchor".into()),
            },
        ],
    }
}

fn consequences_for(kind: EventKind) -> Vec<HistoryConsequence> {
    use EventKind::*;
    let (ck, text, severity) = match kind {
        Foundation => (
            HistoryConsequenceKind::WorldSettled,
            "settlement record became part of the sector rolls",
            20,
        ),
        Discovery | AeldariActivity | TauContact => (
            HistoryConsequenceKind::RegionRecorded,
            "restricted charts gained a new strategic note",
            35,
        ),
        ImperialMandateGranted | Consecration | CommercialCharter | DynasticClaim => (
            HistoryConsequenceKind::ClaimEstablished,
            "legal memory now affects present claims",
            45,
        ),
        Annexation | Reconquest => (
            HistoryConsequenceKind::ControlShift,
            "present control diverged from older settlement patterns",
            70,
        ),
        Secession | Uprising | Purge | CultExposed | ChaosIncursion | OrkWaaagh
        | NecronAwakening | TyranidContact => (
            HistoryConsequenceKind::ConflictEscalated,
            "local grudges and emergency powers persist",
            80,
        ),
        QuarantineDeclared => (
            HistoryConsequenceKind::QuarantineDeclared,
            "access restrictions and sealed records persist",
            75,
        ),
        Blockade => (
            HistoryConsequenceKind::BlockadeCreated,
            "void-control imbalance shaped later route politics",
            75,
        ),
        WarpStormSurge => (
            HistoryConsequenceKind::RouteHazard,
            "navigation risk entered formal route doctrine",
            70,
        ),
    };
    vec![HistoryConsequence {
        kind: ck,
        description: text.into(),
        severity,
        entity_id: None,
    }]
}
