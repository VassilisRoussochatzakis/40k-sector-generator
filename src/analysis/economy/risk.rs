//! Supply-risk and tithe-status classifiers: turn the derived strategic output,
//! dependency edges, blockade/conflict state, and resilience into the
//! per-world / per-system `SupplyRisk` and `TitheStatus` tiers.

use crate::sector_model::{GeneratedRoute, GeneratedWorld, RouteStability};

use super::config::{
    DependencyEdge, SupplyRisk, SystemEconomy, TitheStatus, WorldEconomy, SELF_SUFFICIENCY_OUTPUT,
    SUPPLY_RESILIENCE_SAFE,
};
use super::derive::strategic_needs_for_world;

pub(super) fn import_risk(r: &GeneratedRoute, friction: f32, score: f32) -> SupplyRisk {
    if score < 10.0 || r.stability == RouteStability::Perilous {
        SupplyRisk::Collapsing
    } else if score < 20.0 || friction < 0.35 || r.stability == RouteStability::Hazardous {
        SupplyRisk::Disrupted
    } else if score < 35.0 || friction < 0.70 || r.stability == RouteStability::Unstable {
        SupplyRisk::Vulnerable
    } else {
        SupplyRisk::Stable
    }
}

pub(super) fn system_supply_risk(
    sy: &SystemEconomy,
    sys_ref: Option<&crate::sector_model::GeneratedSystem>,
    deps: &[DependencyEdge],
) -> SupplyRisk {
    let mut risk = if sy.shortage_resources.len() >= 2 {
        SupplyRisk::Disrupted
    } else if sy.shortage_resources.is_empty() {
        SupplyRisk::Stable
    } else {
        SupplyRisk::Vulnerable
    };
    if sys_ref.map(|s| s.blockade.under_blockade).unwrap_or(false) {
        risk = risk.max(SupplyRisk::Disrupted);
    }
    if let Some(sys) = sys_ref {
        for world in &sys.worlds {
            for resource in strategic_needs_for_world(&world.world.world_type) {
                if sy.strategic_output.get(resource) >= SELF_SUFFICIENCY_OUTPUT {
                    continue;
                }
                let incoming: Vec<&DependencyEdge> = deps
                    .iter()
                    .filter(|e| e.to_system_id == sy.system_id && e.resource == *resource)
                    .collect();
                if incoming.is_empty() {
                    risk = risk.max(if *resource == "food" {
                        SupplyRisk::Collapsing
                    } else {
                        SupplyRisk::Disrupted
                    });
                } else if let Some(best) = incoming.iter().map(|e| e.risk).min() {
                    risk = risk.max(best);
                }
            }
        }
    }
    risk
}

pub(super) fn world_supply_risk(we: &WorldEconomy, system_risk: SupplyRisk) -> SupplyRisk {
    let base = if we.stranded || we.shortages.len() >= 2 {
        SupplyRisk::Collapsing
    } else if !we.shortages.is_empty() {
        SupplyRisk::Disrupted
    } else {
        SupplyRisk::Stable
    };
    let mut risk = base.max(system_risk);
    if we.supply_resilience >= SUPPLY_RESILIENCE_SAFE {
        risk = lower_risk(risk);
    }
    risk
}

fn lower_risk(risk: SupplyRisk) -> SupplyRisk {
    match risk {
        SupplyRisk::Collapsing => SupplyRisk::Disrupted,
        SupplyRisk::Disrupted => SupplyRisk::Vulnerable,
        SupplyRisk::Vulnerable => SupplyRisk::Stable,
        SupplyRisk::Stable => SupplyRisk::Stable,
    }
}

pub(super) fn world_tithe_status(w: &GeneratedWorld, we: &WorldEconomy) -> TitheStatus {
    let stress = (w.conflict.intensity as f32).mul_add(
        0.55,
        w.stability.corruption.mul_add(
            0.25,
            w.stability
                .famine_or_resource_stress
                .mul_add(0.5, w.stability.rebellion_risk * 0.35),
        ),
    ) + if w.control.contested { 20.0 } else { 0.0 }
        + if we.stranded { 30.0 } else { 0.0 };
    let reliability = we.strategic_output.weighted_priority_score() - stress;
    if w.control.hidden_master.is_some() && reliability >= 25.0 {
        return TitheStatus::Falsified;
    }
    tithe_from_reliability(reliability)
}

pub(super) fn system_tithe_status(
    sy: &SystemEconomy,
    world_count: usize,
    sys_ref: Option<&crate::sector_model::GeneratedSystem>,
) -> TitheStatus {
    let avg_score = sy.strategic_output.weighted_priority_score() / world_count.max(1) as f32;
    let stress = sys_ref
        .map(|s| {
            (s.conflict.intensity as f32).mul_add(
                0.5,
                s.stability
                    .famine_or_resource_stress
                    .mul_add(0.4, s.stability.rebellion_risk * 0.25),
            ) + if s.blockade.under_blockade { 25.0 } else { 0.0 }
        })
        .unwrap_or(0.0);
    tithe_from_reliability(avg_score - stress)
}

fn tithe_from_reliability(reliability: f32) -> TitheStatus {
    match reliability {
        r if r >= 160.0 => TitheStatus::Surplus,
        r if r >= 75.0 => TitheStatus::Adequate,
        r if r >= 40.0 => TitheStatus::Strained,
        r if r >= 15.0 => TitheStatus::Delinquent,
        _ => TitheStatus::Failed,
    }
}
