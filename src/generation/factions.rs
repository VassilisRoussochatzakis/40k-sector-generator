//! Faction assignment per system/world + post-pass aggregation into
//! [`GeneratedFaction`] roll-ups.

use std::collections::{BTreeMap, BTreeSet};

use rand_chacha::ChaCha8Rng;

use crate::control;
use crate::factions::FactionDef;
use crate::rng::{self, weighted_index};
use crate::sector_model::{
    DominanceState, FactionInfluence, GeneratedFaction, GeneratedForce, GeneratedSubfaction,
    GeneratedSystem, GeneratedWorld, WorldFactionPresence,
};

/// Spec §10.9: at most this many primary factions per system.
const PRIMARY_FACTION_LIMIT: usize = 3;

pub(super) fn assign_factions(
    systems: &mut [GeneratedSystem],
    factions: &[FactionDef],
    rng: &mut ChaCha8Rng,
) {
    if factions.is_empty() {
        return;
    }
    assign_factions_inner(systems, factions, rng);
    // Post-pass: derive per-world claims + multi-winner snapshots, then roll up
    // to system-level state classification. Pure, deterministic.
    // Build a temporary top-faction catalog for stability derivation.
    let stability_factions: Vec<crate::sector_model::GeneratedFaction> =
        build_faction_groups(factions)
            .iter()
            .map(|g| crate::sector_model::GeneratedFaction {
                id: g.id.clone(),
                name: g.name.clone().into(),
                kind: g.kind.clone().into(),
                disposition: g.disposition.clone().into(),
                subfactions: Vec::new(),
                system_presence: vec![],
                world_presence: vec![],
                power: crate::sector_model::PowerProfile::default(),
            })
            .collect();
    for sys in systems.iter_mut() {
        for world in &mut sys.worlds {
            world.claims = control::derive_world_claims(world);
            world.control = control::derive_world_control(world);
            world.stability = crate::stability::derive_world_stability(world, &stability_factions);
        }
        sys.control = control::derive_system_control(sys);
        sys.stability = crate::stability::derive_system_stability(sys);
    }
}

/// Apply faction assignment to one or more systems. Public so the standalone
/// system generator can reuse the same logic the sector generator does.
pub fn assign_factions_for_systems(
    systems: &mut [GeneratedSystem],
    factions: &[FactionDef],
    seed: &str,
    discriminator: &str,
) {
    if factions.is_empty() {
        return;
    }
    let mut rng = rng::stage_rng(seed, "factions", discriminator);
    assign_factions(systems, factions, &mut rng);
}

fn assign_factions_inner(
    systems: &mut [GeneratedSystem],
    factions: &[FactionDef],
    rng: &mut ChaCha8Rng,
) {
    if factions.is_empty() {
        return;
    }
    let groups = build_faction_groups(factions);
    let subgroups = build_subfaction_groups(&groups);
    // Stable faction/sub-faction order from first appearance in the source catalog.
    let catalog_order: BTreeMap<crate::ids::FactionId, usize> =
        groups.iter().map(|g| (g.id.clone(), g.order)).collect();
    let sub_catalog_order: BTreeMap<crate::ids::FactionId, usize> =
        subgroups.iter().map(|g| (g.id.clone(), g.order)).collect();

    for sys in systems.iter_mut() {
        // Per-system accumulator: overall faction_id -> (score, world_appearances)
        let mut scores: BTreeMap<crate::ids::FactionId, (f64, usize)> = BTreeMap::new();
        for world in &mut sys.worlds {
            let pop_tag = world
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let max_factions: usize = match pop_tag.as_ref() {
                "population:uninhabited" => 0,
                "population:minimal" | "population:lightly_populated" => 1,
                "population:sole_settlement" => 2,
                _ => 3,
            };
            if max_factions == 0 {
                continue;
            }

            let mut weighted: Vec<(&SubfactionGroup<'_>, f64)> = subgroups
                .iter()
                .map(|g| {
                    let g = *g;
                    let w = g
                        .members
                        .iter()
                        .map(|f| faction_weight_for_world(f, world))
                        .fold(0.0_f64, f64::max);
                    (g, w)
                })
                .collect();

            let mut chosen: BTreeSet<crate::ids::FactionId> = BTreeSet::new();
            let influences = [
                FactionInfluence::Dominant,
                FactionInfluence::Significant,
                FactionInfluence::Minor,
            ];

            for inf in influences.iter().take(max_factions) {
                if weighted.is_empty() {
                    break;
                }
                let pairs: Vec<(&SubfactionGroup<'_>, f64)> =
                    weighted.iter().map(|(g, w)| (*g, *w)).collect();
                let idx = match weighted_index(&pairs, rng, "faction") {
                    Ok(i) => i,
                    Err(_) => break,
                };
                let g = weighted[idx].0;
                if chosen.insert(g.id.clone()) {
                    let f = match choose_force(g, world, rng) {
                        Some(f) => f,
                        None => {
                            weighted.remove(idx);
                            continue;
                        }
                    };
                    let dims = control::presence_dimensions(
                        &g.kind,
                        &f.default_disposition,
                        *inf,
                        Some(f),
                        world,
                    );
                    let dominance = DominanceState::from_score(dims.local_control_score());
                    let intel_confidence = dims.visibility.round().clamp(0.0, 100.0) as u8;
                    world.factions.push(WorldFactionPresence {
                        faction_id: g.faction_id.clone(),
                        subfaction_id: Some(g.id.clone()),
                        subfaction_name: Some(g.name.clone().into()),
                        force_id: Some(f.id.clone()),
                        force_name: Some(f.name.clone().into()),
                        influence: *inf,
                        relationship_to_government: f.default_disposition.clone().into(),
                        dimensions: dims,
                        dominance,
                        intel_confidence,
                    });
                    let entry = scores.entry(g.faction_id.clone()).or_insert((0.0, 0));
                    entry.0 += inf.weight();
                    entry.1 += 1;
                }
                weighted.remove(idx);
            }
            // Sort world.factions deterministically: by influence rank then catalog order.
            world.factions.sort_by(|a, b| {
                influence_rank(b.influence)
                    .cmp(&influence_rank(a.influence))
                    .then_with(|| {
                        catalog_order
                            .get(&a.faction_id)
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(
                                &catalog_order
                                    .get(&b.faction_id)
                                    .copied()
                                    .unwrap_or(usize::MAX),
                            )
                    })
                    .then_with(|| a.faction_id.cmp(&b.faction_id))
                    .then_with(|| {
                        sub_catalog_order
                            .get(a.subfaction_id.as_ref().unwrap_or(&a.faction_id))
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(
                                &sub_catalog_order
                                    .get(b.subfaction_id.as_ref().unwrap_or(&b.faction_id))
                                    .copied()
                                    .unwrap_or(usize::MAX),
                            )
                    })
                    .then_with(|| a.subfaction_id.cmp(&b.subfaction_id))
                    .then_with(|| a.force_id.cmp(&b.force_id))
            });
        }
        // Spec §10.9: primary factions = top by score, ties broken by world
        // appearances, then catalog order, then faction id.
        let mut entries: Vec<(crate::ids::FactionId, f64, usize)> =
            scores.into_iter().map(|(id, (s, n))| (id, s, n)).collect();
        entries.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| {
                    catalog_order
                        .get(&a.0)
                        .copied()
                        .unwrap_or(usize::MAX)
                        .cmp(&catalog_order.get(&b.0).copied().unwrap_or(usize::MAX))
                })
                .then_with(|| a.0.cmp(&b.0))
        });
        entries.truncate(PRIMARY_FACTION_LIMIT);
        sys.primary_factions = entries.into_iter().map(|(id, _, _)| id).collect();
    }
}

#[derive(Debug)]
struct FactionGroup<'a> {
    id: crate::ids::FactionId,
    name: String,
    kind: String,
    disposition: String,
    order: usize,
    subfactions: Vec<SubfactionGroup<'a>>,
}

#[derive(Debug)]
struct SubfactionGroup<'a> {
    faction_id: crate::ids::FactionId,
    id: crate::ids::FactionId,
    name: String,
    kind: String,
    disposition: String,
    order: usize,
    members: Vec<&'a FactionDef>,
}

fn build_faction_groups(factions: &[FactionDef]) -> Vec<FactionGroup<'_>> {
    let mut members: BTreeMap<crate::ids::FactionId, Vec<&FactionDef>> = BTreeMap::new();
    let mut order: BTreeMap<crate::ids::FactionId, usize> = BTreeMap::new();
    for (idx, f) in factions.iter().enumerate() {
        let top_id = f.top_faction_id();
        order.entry(top_id.clone()).or_insert(idx);
        members.entry(top_id).or_default().push(f);
    }

    let mut groups: Vec<FactionGroup<'_>> = members
        .into_iter()
        .map(|(top_id, group_members)| {
            let disposition = representative_disposition(&group_members);
            let name = group_members
                .first()
                .map(|f| f.top_faction_name())
                .unwrap_or_else(|| {
                    crate::factions::display_name_from_id(top_id.as_str()).into_owned()
                });
            FactionGroup {
                id: top_id.clone(),
                name,
                kind: top_id.to_string(),
                disposition,
                order: order.get(&top_id).copied().unwrap_or(usize::MAX),
                subfactions: build_subfactions_for_group(&top_id, group_members),
            }
        })
        .collect();
    groups.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    groups
}

fn build_subfactions_for_group<'a>(
    top_id: &crate::ids::FactionId,
    members: Vec<&'a FactionDef>,
) -> Vec<SubfactionGroup<'a>> {
    let mut grouped: BTreeMap<crate::ids::FactionId, Vec<&FactionDef>> = BTreeMap::new();
    let mut order: BTreeMap<crate::ids::FactionId, usize> = BTreeMap::new();
    for (idx, f) in members.into_iter().enumerate() {
        let sub_id = f.subfaction_id();
        order.entry(sub_id.clone()).or_insert(idx);
        grouped.entry(sub_id).or_default().push(f);
    }

    let mut subfactions: Vec<SubfactionGroup<'a>> = grouped
        .into_iter()
        .map(|(sub_id, mut group_members)| {
            group_members.sort_by(|a, b| a.id.cmp(&b.id));
            let first = group_members.first().copied();
            SubfactionGroup {
                faction_id: top_id.clone(),
                id: sub_id.clone(),
                name: first.map(FactionDef::subfaction_name).unwrap_or_else(|| {
                    crate::factions::display_name_from_id(sub_id.as_str()).into_owned()
                }),
                kind: first
                    .map(|f| f.kind.clone())
                    .unwrap_or_else(|| sub_id.to_string()),
                disposition: representative_disposition(&group_members),
                order: order.get(&sub_id).copied().unwrap_or(usize::MAX),
                members: group_members,
            }
        })
        .collect();
    subfactions.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    subfactions
}

fn build_subfaction_groups<'a>(groups: &'a [FactionGroup<'a>]) -> Vec<&'a SubfactionGroup<'a>> {
    groups.iter().flat_map(|g| g.subfactions.iter()).collect()
}

fn representative_disposition(members: &[&FactionDef]) -> String {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for f in members {
        *totals.entry(f.default_disposition.as_str()).or_default() += f.weight.max(0.0);
    }
    totals
        .into_iter()
        .max_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(d, _)| d.to_string())
        .unwrap_or_default()
}

fn faction_weight_for_world(f: &FactionDef, world: &GeneratedWorld) -> f64 {
    let mut w = f.weight;
    if f.preferred_world_types
        .iter()
        .any(|s| s == world.world.world_type.as_ref())
    {
        w *= 1.5;
    }
    if f.preferred_governments
        .iter()
        .any(|s| s == world.world.government.as_ref())
    {
        w *= 1.4;
    }
    let feat_hits = f
        .preferred_notable_features
        .iter()
        .filter(|s| {
            world
                .world
                .notable_features
                .iter()
                .any(|nf| nf.as_ref() == *s)
        })
        .count();
    if feat_hits > 0 {
        w *= 1.3_f64.powi(feat_hits as i32);
    }
    w
}

fn choose_force<'a>(
    group: &SubfactionGroup<'a>,
    world: &GeneratedWorld,
    rng: &mut ChaCha8Rng,
) -> Option<&'a FactionDef> {
    let weighted: Vec<(&FactionDef, f64)> = group
        .members
        .iter()
        .map(|f| (*f, faction_weight_for_world(f, world)))
        .collect();
    let idx = weighted_index(&weighted, rng, "force").ok()?;
    Some(weighted[idx].0)
}

fn influence_rank(i: FactionInfluence) -> u8 {
    match i {
        FactionInfluence::Dominant => 3,
        FactionInfluence::Significant => 2,
        FactionInfluence::Minor => 1,
        FactionInfluence::Hidden => 0,
    }
}

pub(super) fn aggregate_factions(
    systems: &[GeneratedSystem],
    factions: &[FactionDef],
) -> Vec<GeneratedFaction> {
    if factions.is_empty() {
        return Vec::new();
    }
    let mut by_id: BTreeMap<crate::ids::FactionId, GeneratedFaction> = BTreeMap::new();
    for g in build_faction_groups(factions) {
        by_id.insert(
            g.id.clone(),
            GeneratedFaction {
                id: g.id.clone(),
                name: g.name.into(),
                kind: g.kind.into(),
                disposition: g.disposition.into(),
                subfactions: g
                    .subfactions
                    .iter()
                    .map(|sf| GeneratedSubfaction {
                        id: sf.id.clone(),
                        name: sf.name.clone().into(),
                        disposition: sf.disposition.clone().into(),
                        forces: sf
                            .members
                            .iter()
                            .map(|f| GeneratedForce {
                                id: f.id.clone(),
                                name: f.name.clone().into(),
                                disposition: f.default_disposition.clone().into(),
                                system_presence: Vec::new(),
                                world_presence: Vec::new(),
                            })
                            .collect(),
                        system_presence: Vec::new(),
                        world_presence: Vec::new(),
                    })
                    .collect(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: Default::default(),
            },
        );
    }
    for sys in systems {
        for world in &sys.worlds {
            for p in &world.factions {
                if let Some(gf) = by_id.get_mut(&p.faction_id) {
                    gf.world_presence.push(world.id.clone());
                    if !gf.system_presence.contains(&sys.id) {
                        gf.system_presence.push(sys.id.clone());
                    }
                    if let Some(sub_id) = &p.subfaction_id {
                        if let Some(sf) = gf.subfactions.iter_mut().find(|sf| sf.id == *sub_id) {
                            sf.world_presence.push(world.id.clone());
                            if !sf.system_presence.contains(&sys.id) {
                                sf.system_presence.push(sys.id.clone());
                            }
                            if let Some(force_id) = &p.force_id {
                                if let Some(force) =
                                    sf.forces.iter_mut().find(|force| force.id == *force_id)
                                {
                                    force.world_presence.push(world.id.clone());
                                    if !force.system_presence.contains(&sys.id) {
                                        force.system_presence.push(sys.id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut v: Vec<GeneratedFaction> = by_id.into_values().collect();
    for f in &mut v {
        f.system_presence.sort();
        f.system_presence.dedup();
        f.world_presence.sort();
        f.world_presence.dedup();
        for sf in &mut f.subfactions {
            sf.system_presence.sort();
            sf.system_presence.dedup();
            sf.world_presence.sort();
            sf.world_presence.dedup();
            for force in &mut sf.forces {
                force.system_presence.sort();
                force.system_presence.dedup();
                force.world_presence.sort();
                force.world_presence.dedup();
            }
        }
    }
    let power = control::aggregate_faction_power(systems);
    control::apply_faction_power(&mut v, &power);
    v
}
