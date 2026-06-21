//! Derived tension scalar: the per-pair co-occurrence accumulator
//! (`CooccurStats`), the walk that builds it from worlds/systems/routes
//! (`build_cooccurrence`), and the `tension_of` scalar that feeds the
//! "Factions at war" digest and the tension heatmap.

use std::collections::{BTreeMap, BTreeSet};

use super::config::Stance;
use super::derive::canonical_pair_idx;
use crate::sector_model::{GeneratedFaction, GeneratedSector};
use crate::FxMap;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct CooccurStats {
    pub(super) contested_worlds: u32,
    pub(super) same_system_worlds: u32,
    pub(super) active_warzones: u32,
    pub(super) claim_conflicts: u32,
    pub(super) hidden_overlap: u32,
    pub(super) route_competition: f32,
    pub(super) economic_dependency: f32,
    pub(super) military_pressure: f32,
    pub(super) covert_activity: f32,
}

pub(super) fn build_cooccurrence(
    sector: &GeneratedSector,
    idx: &FxMap<&str, u32>,
) -> BTreeMap<(u32, u32), CooccurStats> {
    let mut out: BTreeMap<(u32, u32), CooccurStats> = BTreeMap::new();
    for sys in &sector.systems {
        for world in sys.worlds.iter() {
            for i in 0..world.factions.len() {
                for j in (i + 1)..world.factions.len() {
                    let pa = &world.factions[i];
                    let pb = &world.factions[j];
                    let a = pa.faction_id.as_str();
                    let b = pb.faction_id.as_str();
                    bump_cooccur(&mut out, idx, a, b, |s| {
                        s.same_system_worlds += 1;
                        s.economic_dependency +=
                            pa.dimensions.economic.min(pb.dimensions.economic).mul_add(
                                0.06,
                                pa.dimensions.logistics.min(pb.dimensions.logistics) * 0.04,
                            );
                        s.military_pressure += (pa.dimensions.military + pa.dimensions.orbital)
                            .max(pb.dimensions.military + pb.dimensions.orbital)
                            * 0.04;
                        s.covert_activity += pa.dimensions.covert.max(pb.dimensions.covert) * 0.05;
                        if matches!(pa.influence, crate::sector_model::FactionInfluence::Hidden)
                            || matches!(pb.influence, crate::sector_model::FactionInfluence::Hidden)
                            || pa.dimensions.visibility < 25.0
                            || pb.dimensions.visibility < 25.0
                        {
                            s.hidden_overlap += 1;
                        }
                    });
                    if world.control.contested {
                        bump_cooccur(&mut out, idx, a, b, |s| s.contested_worlds += 1);
                    }
                }
            }
            for i in 0..world.claims.len() {
                for j in (i + 1)..world.claims.len() {
                    bump_cooccur(
                        &mut out,
                        idx,
                        world.claims[i].faction_id.as_str(),
                        world.claims[j].faction_id.as_str(),
                        |s| s.claim_conflicts += 1,
                    );
                }
            }
        }
        // Active warzone at the system level adds heat between every co-located
        // pair in the system.
        if let Some(crate::sector_model::SystemState::Warzone) = sys.control.state {
            let mut sys_ids: BTreeSet<&str> = BTreeSet::new();
            for w in sys.worlds.iter() {
                for p in &w.factions {
                    sys_ids.insert(p.faction_id.as_str());
                }
            }
            let ids: Vec<&str> = sys_ids.into_iter().collect();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    bump_cooccur(&mut out, idx, ids[i], ids[j], |s| s.active_warzones += 1);
                }
            }
        }
    }
    for route in &sector.routes {
        for i in 0..route.controls.len() {
            for j in (i + 1)..route.controls.len() {
                let a = &route.controls[i];
                let b = &route.controls[j];
                bump_cooccur(
                    &mut out,
                    idx,
                    a.faction_id.as_str(),
                    b.faction_id.as_str(),
                    |s| {
                        s.route_competition += a.piracy.min(b.piracy).mul_add(
                            0.25,
                            a.interdiction.min(b.interdiction).mul_add(
                                0.25,
                                a.patrol
                                    .min(b.patrol)
                                    .mul_add(0.15, a.toll.min(b.toll) * 0.20),
                            ),
                        );
                        s.economic_dependency += a.toll.min(b.toll) * 0.20;
                        s.military_pressure += (a.patrol + a.interdiction + a.piracy)
                            .max(b.patrol + b.interdiction + b.piracy)
                            * 0.10;
                        s.covert_activity += a
                            .secrecy
                            .max(b.secrecy)
                            .mul_add(0.10, a.piracy.max(b.piracy) * 0.10);
                        if a.secrecy.max(b.secrecy) >= 65.0 {
                            s.hidden_overlap += 1;
                        }
                    },
                );
            }
        }
    }
    out
}

fn bump_cooccur<F>(
    out: &mut BTreeMap<(u32, u32), CooccurStats>,
    idx: &FxMap<&str, u32>,
    a: &str,
    b: &str,
    f: F,
) where
    F: FnOnce(&mut CooccurStats),
{
    if a == b {
        return;
    }
    // Skip ids not in the faction catalogue: such entries were never read by the
    // lookup path (which only queries `sector.factions` ids), so dropping them
    // is observably identical to the old string-keyed map.
    let (Some(&ia), Some(&ib)) = (idx.get(a), idx.get(b)) else {
        return;
    };
    let entry = out.entry(canonical_pair_idx(ia, ib)).or_default();
    f(entry);
}

/// Resolve the co-occurrence stats for an id pair through the faction index,
/// returning the default (all-zero) accumulator when either id is absent or the
/// pair never co-occurred. Shared by `tension_of` and `build_relation` (B6).
pub(super) fn cooccur_stats(
    cooccur: &BTreeMap<(u32, u32), CooccurStats>,
    idx: &FxMap<&str, u32>,
    a_id: &str,
    b_id: &str,
) -> CooccurStats {
    idx.get(a_id)
        .zip(idx.get(b_id))
        .and_then(|(&ia, &ib)| cooccur.get(&canonical_pair_idx(ia, ib)))
        .copied()
        .unwrap_or_default()
}

pub(super) fn tension_of(
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    stance: Stance,
    cooccur: &BTreeMap<(u32, u32), CooccurStats>,
    idx: &FxMap<&str, u32>,
) -> f32 {
    let stats = cooccur_stats(cooccur, idx, &a.id, &b.id);
    let stance_bonus = match stance {
        Stance::AtWar => 40.0,
        Stance::Hostile => 25.0,
        Stance::Rival => 12.0,
        Stance::Neutral => 0.0,
        Stance::Aligned => -5.0,
        Stance::Allied => -10.0,
    };
    let raw = stats.covert_activity.mul_add(
        0.1,
        stats.military_pressure.mul_add(
            0.2,
            stats.route_competition.mul_add(
                0.4,
                (stats.claim_conflicts as f32).mul_add(
                    6.0,
                    (stats.same_system_worlds as f32).mul_add(
                        1.5,
                        (stats.active_warzones as f32).mul_add(
                            10.0,
                            (stats.contested_worlds as f32).mul_add(8.0, stance_bonus),
                        ),
                    ),
                ),
            ),
        ),
    );
    raw.clamp(0.0, 100.0)
}
