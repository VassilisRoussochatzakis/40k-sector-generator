//! Per-cluster summary aggregation, faction-control tallies, ownership
//! resolution, and capital selection (spec §10.4 / §10.5).
//!
//! All entry points here are reached from
//! [`super::build_subsectors`]. Internal scoring helpers stay private.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use crate::sector_model::{
    FactionInfluence, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld,
};

use super::{
    ControlDenominator, FactionControlSummary, ScoredId, Subsector, SubsectorConfig,
    SubsectorSummary,
};

// ── Ownership resolution (spec §10.4.1) ────────────────────────────────────────

#[derive(Debug, Clone)]
pub(super) struct SystemOwnership {
    /// Some(faction_id) when a faction has a clear lead; None = contested.
    pub(super) owner: Option<crate::ids::FactionId>,
    /// Per-faction owned-world counts within this system.
    pub(super) owned_worlds_by_faction: BTreeMap<crate::ids::FactionId, Vec<crate::ids::WorldId>>,
    /// True if the system has at least one inhabited world.
    pub(super) inhabited: bool,
    /// Other factions that contributed signal but did not win ownership.
    pub(super) contested_with: BTreeSet<crate::ids::FactionId>,
}

pub(super) fn resolve_system_owners(
    sector: &GeneratedSector,
) -> BTreeMap<crate::ids::SystemId, SystemOwnership> {
    let mut out = BTreeMap::new();
    for sys in &sector.systems {
        let mut scores: BTreeMap<crate::ids::FactionId, i32> = BTreeMap::new();
        let mut owned_worlds_by_faction: BTreeMap<crate::ids::FactionId, Vec<crate::ids::WorldId>> =
            BTreeMap::new();
        let mut inhabited = false;

        // Find capital-like and highest-pop worlds (sec §10.4.1 bonuses).
        let mut max_pop_world: Option<(&GeneratedWorld, i32)> = None;
        for w in sector.get_worlds_for_system(sys) {
            let rank = population_rank(&w.world.population);
            if rank > 0 {
                inhabited = true;
            }
            if max_pop_world.map(|(_, r)| rank > r).unwrap_or(true) {
                max_pop_world = Some((w, rank));
            }
        }

        for f in &sys.primary_factions {
            *scores.entry(f.clone()).or_default() += 3;
        }
        for w in &sector.get_worlds_for_system(sys) {
            // Infer per-world owner: faction with strongest influence, tied = no clear owner.
            let inferred_owner = infer_world_owner(w);
            if let Some(ref owner) = inferred_owner {
                *scores.entry(owner.clone()).or_default() += 8;
                owned_worlds_by_faction
                    .entry(owner.clone())
                    .or_default()
                    .push(w.id.clone());
            }
            for p in &w.factions {
                *scores.entry(p.faction_id.clone()).or_default() +=
                    ownership_influence_weight(p.influence);
            }
            // Capital-like world bonus.
            let capital_like =
                any_capital_like(w.tags.iter().chain(w.world.notable_features.iter()));
            if capital_like {
                if let Some(owner) = inferred_owner.as_ref() {
                    *scores.entry(owner.clone()).or_default() += 4;
                }
            }
            if let Some((mpw, _)) = max_pop_world {
                if mpw.id == w.id {
                    if let Some(owner) = inferred_owner.as_ref() {
                        *scores.entry(owner.clone()).or_default() += 3;
                    }
                }
            }
        }

        // Pick top two for clear-lead test.
        let mut ranked: Vec<(crate::ids::FactionId, i32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let (owner, contested_with) = match ranked.as_slice() {
            [] => (None, BTreeSet::new()),
            [(id, s)] if *s >= 6 => (Some(id.clone()), BTreeSet::new()),
            [_] => (None, BTreeSet::new()),
            [(id_a, sa), (id_b, sb), ..] => {
                if *sa >= 6 && *sa >= *sb + 3 {
                    (Some(id_a.clone()), BTreeSet::new())
                } else {
                    let mut others = BTreeSet::new();
                    others.insert(id_a.clone());
                    others.insert(id_b.clone());
                    (None, others)
                }
            }
        };

        out.insert(
            sys.id.clone(),
            SystemOwnership {
                owner,
                owned_worlds_by_faction,
                inhabited,
                contested_with,
            },
        );
    }
    out
}

fn infer_world_owner(world: &GeneratedWorld) -> Option<crate::ids::FactionId> {
    if world.factions.is_empty() {
        return None;
    }
    let mut best: Option<(&str, u8)> = None;
    let mut tied = false;
    for p in &world.factions {
        let rank = influence_rank_inclusive(p.influence);
        match best {
            None => best = Some((p.faction_id.as_str(), rank)),
            Some((_, br)) if rank > br => {
                best = Some((p.faction_id.as_str(), rank));
                tied = false;
            }
            Some((_, br)) if rank == br => tied = true,
            _ => {}
        }
    }
    if tied {
        return None;
    }
    best.map(|(id, _)| crate::ids::FactionId::new(id))
}

fn ownership_influence_weight(i: FactionInfluence) -> i32 {
    match i {
        FactionInfluence::Dominant => 6,
        FactionInfluence::Significant => 3,
        FactionInfluence::Minor => 1,
        FactionInfluence::Hidden => 1,
    }
}

fn influence_rank_inclusive(i: FactionInfluence) -> u8 {
    match i {
        FactionInfluence::Dominant => 4,
        FactionInfluence::Significant => 3,
        FactionInfluence::Minor => 2,
        FactionInfluence::Hidden => 1,
    }
}

// ── Summary aggregation (spec §10) ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn populate_summary(
    sector: &GeneratedSector,
    cell: &mut Subsector,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    route_by_id: &BTreeMap<&str, &GeneratedRoute>,
    route_degree: &BTreeMap<&str, u32>,
    stable_route_degree: &BTreeMap<&str, u32>,
    owners: &BTreeMap<crate::ids::SystemId, SystemOwnership>,
    config: &SubsectorConfig,
) {
    let mut summary = SubsectorSummary {
        system_count: cell.system_ids.len() as u32,
        internal_route_count: cell.route_ids_internal.len() as u32,
        border_route_count: cell.route_ids_border.len() as u32,
        ..Default::default()
    };

    let mut dominant_scores: BTreeMap<crate::ids::FactionId, i32> = BTreeMap::new();

    let mut total_worlds: u32 = 0;
    for sid in &cell.system_ids {
        let Some(&sys) = sys_by_id.get(sid.as_str()) else {
            continue;
        };
        total_worlds += sector.get_worlds_for_system(sys).len() as u32;
        *summary
            .star_colour_counts
            .entry(sys.star.colour_name.clone())
            .or_default() += 1;
        for tag in &sys.tags {
            *summary.tag_counts.entry(tag.clone()).or_default() += 1;
        }
        for pf in &sys.primary_factions {
            *dominant_scores.entry(pf.clone()).or_default() += 2;
        }
        for w in &sector.get_worlds_for_system(sys) {
            *summary
                .world_type_counts
                .entry(w.world.world_type.clone())
                .or_default() += 1;
            *summary
                .population_counts
                .entry(w.world.population.clone())
                .or_default() += 1;
            *summary
                .tech_level_counts
                .entry(w.world.tech_level.clone())
                .or_default() += 1;
            *summary
                .government_counts
                .entry(w.world.government.clone())
                .or_default() += 1;
            for feat in &w.world.notable_features {
                *summary.feature_counts.entry(feat.clone()).or_default() += 1;
            }
            for tag in &w.tags {
                *summary.tag_counts.entry(tag.clone()).or_default() += 1;
            }
            for p in &w.factions {
                *dominant_scores.entry(p.faction_id.clone()).or_default() +=
                    dominant_influence_weight(p.influence);
            }
        }
    }
    summary.world_count = total_worlds;

    // Aggregate route counts via internal + border routes touching this cell.
    let mut routes_seen: BTreeSet<&str> = BTreeSet::new();
    for rid in cell
        .route_ids_internal
        .iter()
        .chain(cell.route_ids_border.iter())
    {
        if !routes_seen.insert(rid.as_str()) {
            continue;
        }
        let Some(&r) = route_by_id.get(rid.as_str()) else {
            continue;
        };
        let rt_key = format!("{:?}", r.route_type);
        let rs_key = format!("{:?}", r.stability);
        *summary.route_type_counts.entry(rt_key).or_default() += 1;
        *summary.route_stability_counts.entry(rs_key).or_default() += 1;
        for t in &r.tags {
            *summary.tag_counts.entry(t.clone()).or_default() += 1;
        }
    }

    // Dominant factions (top N, default 3).
    let mut dominant: Vec<ScoredId> = dominant_scores
        .into_iter()
        .map(|(id, score)| ScoredId { id, score })
        .collect();
    dominant.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    dominant.truncate(3);
    summary.dominant_factions = dominant;

    // Faction control. Build per-faction ownership tallies from member systems.
    let (faction_control, controlling) =
        build_faction_control(sector, cell, sys_by_id, owners, config);
    summary.faction_control = faction_control;
    summary.controlling_faction_id = controlling;

    // Primary and capital systems.
    summary.primary_system_id = pick_primary_system(sector, cell, sys_by_id, route_degree);
    let (cap_sys, cap_world) = pick_capital(
        sector,
        cell,
        sys_by_id,
        route_degree,
        stable_route_degree,
        owners,
        summary.controlling_faction_id.as_deref(),
    );
    summary.subsector_capital_system_id = cap_sys;
    summary.subsector_capital_world_id = cap_world;

    cell.summary = summary;
}

fn dominant_influence_weight(i: FactionInfluence) -> i32 {
    match i {
        FactionInfluence::Dominant => 3,
        FactionInfluence::Significant => 2,
        FactionInfluence::Minor => 1,
        FactionInfluence::Hidden => 1,
    }
}

fn build_faction_control(
    sector: &GeneratedSector,
    cell: &Subsector,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    owners: &BTreeMap<crate::ids::SystemId, SystemOwnership>,
    config: &SubsectorConfig,
) -> (Vec<FactionControlSummary>, Option<crate::ids::FactionId>) {
    // Tallies per faction inside this subsector.
    #[derive(Default)]
    struct Tally {
        owned_systems: u32,
        owned_inhabited_systems: u32,
        owned_worlds: u32,
        contested_systems: u32,
    }
    let mut tallies: BTreeMap<crate::ids::FactionId, Tally> = BTreeMap::new();

    let mut inhabited_system_count: u32 = 0;
    let mut world_count: u32 = 0;

    for sid in &cell.system_ids {
        let Some(&sys) = sys_by_id.get(sid.as_str()) else {
            continue;
        };
        world_count += sector.get_worlds_for_system(sys).len() as u32;
        let Some(o) = owners.get(sid) else { continue };
        if o.inhabited {
            inhabited_system_count += 1;
        }
        if let Some(owner) = &o.owner {
            let t = tallies.entry(owner.clone()).or_default();
            t.owned_systems += 1;
            if o.inhabited {
                t.owned_inhabited_systems += 1;
            }
            let owned = o
                .owned_worlds_by_faction
                .get(owner)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            t.owned_worlds += owned;
        } else {
            for c in &o.contested_with {
                let t = tallies.entry(c.clone()).or_default();
                t.contested_systems += 1;
            }
        }
    }

    let use_inhabited = config.control_denominator == ControlDenominator::InhabitedSystems;
    let eligible_system_count: u32 = if use_inhabited && inhabited_system_count > 0 {
        inhabited_system_count
    } else {
        cell.system_ids.len() as u32
    };

    let mut rows: Vec<FactionControlSummary> = tallies
        .into_iter()
        .map(|(id, t)| {
            let primary_share_num = if use_inhabited && inhabited_system_count > 0 {
                t.owned_inhabited_systems
            } else {
                t.owned_systems
            };
            let sys_share = basis_points(t.owned_systems, eligible_system_count);
            let inh_share = basis_points(t.owned_inhabited_systems, inhabited_system_count);
            let w_share = basis_points(t.owned_worlds, world_count);
            let primary_share = basis_points(primary_share_num, eligible_system_count);
            let control_score = (t.owned_systems as i32) * 100
                + (t.owned_inhabited_systems as i32) * 50
                + (t.owned_worlds as i32) * 10
                + (primary_share as i32) / 100;
            FactionControlSummary {
                faction_id: id,
                owned_system_count: t.owned_systems,
                owned_inhabited_system_count: t.owned_inhabited_systems,
                owned_world_count: t.owned_worlds,
                system_share_basis_points: sys_share,
                inhabited_system_share_basis_points: inh_share,
                world_share_basis_points: w_share,
                control_score,
                control_tier: String::new(),
                contested_system_count: t.contested_systems,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        b.control_score
            .cmp(&a.control_score)
            .then_with(|| {
                let pa = primary_share(a, use_inhabited);
                let pb = primary_share(b, use_inhabited);
                pb.cmp(&pa)
            })
            .then_with(|| b.owned_system_count.cmp(&a.owned_system_count))
            .then_with(|| a.faction_id.cmp(&b.faction_id))
    });

    // Tier assignment using primary denominator.
    let leader_primary = rows
        .first()
        .map(|r| primary_share(r, use_inhabited))
        .unwrap_or(0);
    let runner_primary = rows
        .get(1)
        .map(|r| primary_share(r, use_inhabited))
        .unwrap_or(0);
    if let Some(first) = rows.first_mut() {
        first.control_tier = control_tier(leader_primary, runner_primary).to_string();
    }
    for r in rows.iter_mut().skip(1) {
        r.control_tier = "presence".to_string();
        let p = primary_share(r, use_inhabited);
        if p == 0 {
            r.control_tier = "trace".to_string();
        }
    }

    // Truncate to top N, then append tracked factions with zero rows if absent.
    if config.faction_control_top_n < rows.len() {
        rows.truncate(config.faction_control_top_n);
    }
    for tid in &config.tracked_faction_ids {
        if !rows.iter().any(|r| r.faction_id.as_str() == tid.as_str()) {
            rows.push(FactionControlSummary {
                faction_id: crate::ids::FactionId::new(tid.as_str()),
                owned_system_count: 0,
                owned_inhabited_system_count: 0,
                owned_world_count: 0,
                system_share_basis_points: 0,
                inhabited_system_share_basis_points: 0,
                world_share_basis_points: 0,
                control_score: 0,
                control_tier: "trace".to_string(),
                contested_system_count: 0,
            });
        }
    }
    // Final deterministic sort after possible tracked-faction appends.
    rows.sort_by(|a, b| {
        b.control_score
            .cmp(&a.control_score)
            .then_with(|| {
                let pa = primary_share(a, use_inhabited);
                let pb = primary_share(b, use_inhabited);
                pb.cmp(&pa)
            })
            .then_with(|| b.owned_system_count.cmp(&a.owned_system_count))
            .then_with(|| a.faction_id.cmp(&b.faction_id))
    });

    let controlling = rows
        .first()
        .filter(|r| matches!(r.control_tier.as_str(), "absolute" | "clear" | "plurality"))
        .map(|r| r.faction_id.clone());

    (rows, controlling)
}

fn primary_share(r: &FactionControlSummary, use_inhabited: bool) -> u32 {
    if use_inhabited {
        r.inhabited_system_share_basis_points
    } else {
        r.system_share_basis_points
    }
}

fn control_tier(leader_bp: u32, runner_bp: u32) -> &'static str {
    let lead = leader_bp.saturating_sub(runner_bp);
    if leader_bp >= 7_500 {
        "absolute"
    } else if leader_bp >= 6_000 {
        "clear"
    } else if leader_bp >= 4_000 && lead >= 1_500 {
        "plurality"
    } else if leader_bp >= 2_500 {
        "contested"
    } else if leader_bp >= 1_000 {
        "presence"
    } else {
        "trace"
    }
}

fn basis_points(numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        0
    } else {
        ((numerator as u64 * 10_000 + denominator as u64 / 2) / denominator as u64) as u32
    }
}

// ── Primary system + capital selection (spec §10.5) ────────────────────────────

fn pick_primary_system(
    sector: &GeneratedSector,
    cell: &Subsector,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    route_degree: &BTreeMap<&str, u32>,
) -> Option<crate::ids::SystemId> {
    let mut best: Option<(i32, usize, &str)> = None;
    for sid in &cell.system_ids {
        let Some(&sys) = sys_by_id.get(sid.as_str()) else {
            continue;
        };
        let deg = route_degree.get(sid.as_str()).copied().unwrap_or(0) as i32;
        let worlds = sector.get_worlds_for_system(sys);
        let max_pop = worlds
            .iter()
            .map(|w| population_rank(&w.world.population))
            .max()
            .unwrap_or(0);
        let max_tech = worlds
            .iter()
            .map(|w| tech_rank(&w.world.tech_level))
            .max()
            .unwrap_or(0);
        // ... (rest of logic)
        let score = deg * 4
            + max_pop * 3
            + max_tech * 2
            + sector.get_worlds_for_system(sys).len() as i32
            + sys.primary_factions.len() as i32;
        let cand = (score, sys.index, sys.id.as_str());
        if best
            .map(|(bs, bi, bid)| {
                score > bs
                    || (score == bs && sys.index < bi)
                    || (score == bs && sys.index == bi && sys.id.as_str() < bid)
            })
            .unwrap_or(true)
        {
            best = Some(cand);
        }
    }
    best.map(|(_, _, id)| crate::ids::SystemId::new(id))
}

pub(super) fn pick_capital(
    sector: &GeneratedSector,
    cell: &Subsector,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    route_degree: &BTreeMap<&str, u32>,
    stable_route_degree: &BTreeMap<&str, u32>,
    owners: &BTreeMap<crate::ids::SystemId, SystemOwnership>,
    controlling_faction: Option<&str>,
) -> (Option<crate::ids::SystemId>, Option<crate::ids::WorldId>) {
    struct Cand {
        sys_score: i32,
        world_score: i32,
        max_pop: i32,
        max_prosperity: i32,
        stable_deg: u32,
        sys_index: usize,
        sys_id: crate::ids::SystemId,
        world_id: Option<crate::ids::WorldId>,
    }
    type CandKey<'a> = (
        Reverse<i32>,
        Reverse<i32>,
        Reverse<i32>,
        Reverse<i32>,
        Reverse<u32>,
        usize,
        &'a str,
    );
    impl Cand {
        fn key(&self) -> CandKey<'_> {
            (
                Reverse(self.sys_score),
                Reverse(self.world_score),
                Reverse(self.max_pop),
                Reverse(self.max_prosperity),
                Reverse(self.stable_deg),
                self.sys_index,
                self.sys_id.as_str(),
            )
        }
    }
    let mut best: Option<Cand> = None;

    for sid in &cell.system_ids {
        let Some(&sys) = sys_by_id.get(sid.as_str()) else {
            continue;
        };
        let deg = route_degree.get(sid.as_str()).copied().unwrap_or(0) as i32;
        let stable_deg = stable_route_degree.get(sid.as_str()).copied().unwrap_or(0) as i32;
        let max_pop = sector
            .get_worlds_for_system(sys)
            .iter()
            .map(|w| population_rank(&w.world.population))
            .max()
            .unwrap_or(0);
        let max_tech = sector
            .get_worlds_for_system(sys)
            .iter()
            .map(|w| tech_rank(&w.world.tech_level))
            .max()
            .unwrap_or(0);
        let max_prosperity = sector
            .get_worlds_for_system(sys)
            .iter()
            .map(|w| inferred_prosperity_rank(sys, w, deg, stable_deg))
            .max()
            .unwrap_or(0);
        let inhabited_worlds = sector
            .get_worlds_for_system(sys)
            .iter()
            .filter(|w| population_rank(&w.world.population) > 0)
            .count();

        // Best world inside this system.
        let mut best_world: Option<(i32, &GeneratedWorld)> = None;
        for w in &sector.get_worlds_for_system(sys) {
            let score = score_world_as_capital(sys, w, controlling_faction, stable_deg);
            if best_world.map(|(s, _)| score > s).unwrap_or(true) {
                best_world = Some((score, w));
            }
        }
        let (world_score, world_id) = match best_world {
            Some((s, w)) if s >= 0 => (s, Some(w.id.clone())),
            Some((s, _)) => (s, None),
            None => (0, None),
        };

        // System-level adjustments.
        let mut sys_score = world_score
            + max_pop * 10
            + max_prosperity * 8
            + deg * 4
            + stable_deg * 3
            + max_tech * 3;
        if let Some(o) = owners.get(sid) {
            if let (Some(cf), Some(owner)) = (controlling_faction, o.owner.as_ref()) {
                if owner == cf {
                    sys_score += 6;
                }
            }
        }
        if inhabited_worlds >= 2 {
            sys_score += 3;
        }
        if sector.get_worlds_for_system(sys).len() >= 4 {
            sys_score += 2;
        }
        let only_unstable = deg > 0 && stable_deg == 0;
        if only_unstable {
            sys_score -= 3;
        }
        let has_hazard = sys
            .tags
            .iter()
            .chain(sector.get_worlds_for_system(sys).iter().flat_map(|w| w.tags.iter()))
            .any(|t| {
                let l = t.to_ascii_lowercase();
                l.contains("hazard")
                    || l.contains("ruin")
                    || l.contains("quarantine")
                    || l.contains("blockade")
                    || l.contains("war_zone")
            });
        if has_hazard {
            sys_score -= 5;
        }

        // Frontier rule: if no inhabited worlds and sys_score < 0, skip capital selection.
        if inhabited_worlds == 0 && sys_score < 0 {
            continue;
        }

        let cand = Cand {
            sys_score,
            world_score,
            max_pop,
            max_prosperity,
            stable_deg: stable_deg as u32,
            sys_index: sys.index,
            sys_id: sys.id.clone(),
            world_id,
        };
        if best.as_ref().is_none_or(|b| cand.key() < b.key()) {
            best = Some(cand);
        }
    }

    match best {
        Some(c) => (Some(c.sys_id), c.world_id),
        None => (None, None),
    }
}

fn score_world_as_capital(
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    controlling_faction: Option<&str>,
    stable_deg: i32,
) -> i32 {
    let pop = population_rank(&w.world.population);
    let prosperity = inferred_prosperity_rank(sys, w, 0, stable_deg);
    let tech = tech_rank(&w.world.tech_level);
    let mut score = pop * 8 + prosperity * 7 + tech * 4;

    let capital_like = any_capital_like(w.tags.iter().chain(w.world.notable_features.iter()));
    if capital_like {
        score += 10;
    }
    let gov = &w.world.government;
    if matches!(
        gov.as_str(),
        "Republic" | "Corporate" | "Imperial" | "Federation" | "Theocracy"
    ) {
        score += 3;
    }
    if let Some(cf) = controlling_faction {
        let owner = infer_world_owner(w);
        if owner.as_deref() == Some(cf) {
            score += 3;
        }
    }
    if stable_deg > 0 {
        score += 2;
    }
    if pop == 0 {
        score -= 20;
    }
    score
}

fn inferred_prosperity_rank(
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    route_degree: i32,
    stable_deg: i32,
) -> i32 {
    let pop = population_rank(&w.world.population);
    let tech = tech_rank(&w.world.tech_level);
    let lower = |s: &str| s.to_ascii_lowercase();
    let mut bonus = 0;
    let tokens = w
        .tags
        .iter()
        .chain(w.world.notable_features.iter())
        .chain(sys.tags.iter())
        .map(|t| lower(t))
        .collect::<Vec<_>>();
    if tokens.iter().any(|t| {
        t.contains("trade")
            || t.contains("market")
            || t.contains("commerce")
            || t.contains("port")
            || t.contains("hub")
    }) {
        bonus += 1;
    }
    if tokens.iter().any(|t| {
        t.contains("industrial")
            || t.contains("shipyard")
            || t.contains("mining")
            || t.contains("manufacturing")
    }) {
        bonus += 1;
    }
    if route_degree >= 3 {
        bonus += 1;
    }
    if stable_deg >= 1 {
        bonus += 1;
    }
    let penalty = if tokens.iter().any(|t| {
        t.contains("ruin")
            || t.contains("hazard")
            || t.contains("warzone")
            || t.contains("war_zone")
            || t.contains("quarantine")
            || t.contains("blockade")
    }) {
        2
    } else {
        0
    };
    (pop + tech + bonus - penalty).clamp(0, 6)
}

/// Tag contains a "capital-like" keyword (system seat / admin world etc.).
fn is_capital_like_tag(tag: &str) -> bool {
    let l = tag.to_ascii_lowercase();
    l.contains("capital") || l.contains("admin") || l.contains("bureaucratic") || l.contains("seat")
}

/// Any tag in `iter` matches `is_capital_like_tag`.
fn any_capital_like<'a, I: IntoIterator<Item = &'a String>>(iter: I) -> bool {
    iter.into_iter().any(|t| is_capital_like_tag(t))
}

pub(super) fn population_rank(value: &str) -> i32 {
    match value {
        "Uninhabited" => 0,
        "Minimal" => 1,
        "SoleSettlement" => 2,
        "LightlyPopulated" => 3,
        "DenselyPopulated" => 4,
        "ExtremelyDense" => 5,
        _ => 0,
    }
}

pub(super) fn tech_rank(value: &str) -> i32 {
    match value {
        "Primitive" => 0,
        "Low" => 1,
        "Standard" => 2,
        "High" => 3,
        "Archaeotech" | "XenoHybrid" => 4,
        _ => 0,
    }
}
