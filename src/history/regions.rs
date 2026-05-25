//! Per-region emission: warp phenomena → chronicle entries.

use super::build::build_event;
use super::context::EmitContext;
use super::model::{
    EventKind, HistoryAnchor, HistoryConsequence, HistoryConsequenceKind, HistoryEvent,
};

pub(super) fn emit_region_events(ctx: &EmitContext, out: &mut Vec<HistoryEvent>) {
    for (i, reg) in ctx.sector.regions.iter().enumerate() {
        let (kind, text, weight) = match reg.kind {
            crate::regions::RegionConditionKind::WarpStorm => (
                EventKind::WarpStormSurge,
                format!(
                    "{} swelled into a warp-storm region across {} charted hexes.",
                    reg.name,
                    reg.hexes.len()
                ),
                75,
            ),
            crate::regions::RegionConditionKind::Turbulence => (
                EventKind::WarpStormSurge,
                format!(
                    "{} became a region of persistent immaterial turbulence.",
                    reg.name
                ),
                60,
            ),
            crate::regions::RegionConditionKind::CalmCorridor => (
                EventKind::Discovery,
                format!("Navigators confirmed {} as a rare calm corridor.", reg.name),
                45,
            ),
            crate::regions::RegionConditionKind::Blackout => (
                EventKind::Discovery,
                format!("Astropathic silence spread through {}.", reg.name),
                65,
            ),
            crate::regions::RegionConditionKind::Anomaly => (
                EventKind::Discovery,
                format!("Surveyors catalogued {} as a persistent anomaly.", reg.name),
                55,
            ),
            crate::regions::RegionConditionKind::NecropolisDrift => (
                EventKind::Discovery,
                format!("Vast debris and dead worlds were charted in {}.", reg.name),
                55,
            ),
            crate::regions::RegionConditionKind::BeaconChain => (
                EventKind::Discovery,
                format!(
                    "Ancient navigation pylons were mapped aligning {}.",
                    reg.name
                ),
                60,
            ),
            crate::regions::RegionConditionKind::EmpyricBleed => (
                EventKind::WarpStormSurge,
                format!(
                    "Unnatural empyric phenomena began bleeding into {}.",
                    reg.name
                ),
                65,
            ),
        };
        let anchor = HistoryAnchor::Region {
            region_id: reg.id.clone(),
        };
        let mut ev = build_event(ctx, anchor, kind, text, Vec::new(), weight, i);
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::RegionRecorded,
            description: format!("{:?} effects entered the sector chronicle.", reg.kind),
            severity: weight,
            entity_id: Some(reg.id.clone()),
        });
        out.push(ev);
    }
}
