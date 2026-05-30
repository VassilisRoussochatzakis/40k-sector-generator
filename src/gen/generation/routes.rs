//! Public route graph generation, route classification, and union-find helpers
//! used to optionally connect disjoint components.

use std::collections::BTreeSet;
use std::sync::Arc;

use rand_chacha::ChaCha8Rng;

use crate::config::AppConfig;
use crate::ids;
use crate::routes::RouteRules;
use crate::sector_model::{
    hex_distance, GeneratedRoute, GeneratedSystem, RouteStability, RouteType,
};
use crate::taxonomy;

pub(super) fn generate_routes(
    config: &AppConfig,
    rules: &RouteRules,
    systems: &[GeneratedSystem],
    rng: &mut ChaCha8Rng,
) -> Vec<GeneratedRoute> {
    if systems.len() < 2 {
        return Vec::new();
    }
    let max_distance = config
        .generation
        .routes
        .max_route_distance
        .max(rules.max_distance);
    let density = config.generation.routes.route_density.clamp(0.0, 1.0);

    let pair_upper = systems.len().saturating_sub(1) * systems.len() / 2;
    let mut candidates: Vec<(usize, usize, f64, u32)> = Vec::with_capacity(pair_upper);
    for i in 0..systems.len() {
        for j in (i + 1)..systems.len() {
            let dist = hex_distance(systems[i].coord, systems[j].coord);
            if dist == 0 || dist > max_distance {
                continue;
            }
            let mut w = rules.default_weight;
            // Distance falloff.
            w *= 1.0 / f64::from(dist);

            let combined_tags: Vec<&Arc<str>> = systems[i]
                .worlds
                .iter()
                .chain(systems[j].worlds.iter())
                .flat_map(|wd| wd.tags.iter())
                .collect();

            if combined_tags.iter().any(|t| {
                let s = t.as_ref();
                s == "feature:trade_hub"
                    || s == "feature:freeport"
                    || s == "feature:major_spaceyard"
                    || s == "feature:administrative_hub"
                    || s == "feature:subsector_hegemon"
            }) {
                w *= 2.0;
            }
            if combined_tags.iter().any(|t| {
                let s = t.as_ref();
                s == "feature:warp_phenomena"
                    || s == "feature:quarantined"
                    || s == "feature:war_zone"
                    || s == "feature:daemonic_corruption"
            }) {
                w *= 0.25;
            }

            let (rt, _) = classify_route(&systems[i], &systems[j], dist, max_distance);

            // Apply config modifiers.
            for m in &rules.modifiers {
                if let Some(s) = &m.when.notable_feature {
                    let tag = format!("feature:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| t.as_ref() == tag) {
                        w *= m.multiplier;
                    }
                }
                if let Some(s) = &m.when.world_type {
                    let tag = format!("world_type:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| t.as_ref() == tag) {
                        w *= m.multiplier;
                    }
                }
                if let Some(s) = &m.when.government {
                    let tag = format!("gov:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| t.as_ref() == tag) {
                        w *= m.multiplier;
                    }
                }
                if let Some(s) = &m.when.route_type {
                    if RouteType::from_key(&taxonomy::to_snake_case(s)).is_some_and(|v| v == rt) {
                        w *= m.multiplier;
                    }
                }
            }

            if w.is_finite() && w > 0.0 {
                candidates.push((i, j, w, dist));
            }
        }
    }

    // Sort by descending weight for deterministic top selection.
    candidates.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    let total_pairs = candidates.len();
    let target_count = ((total_pairs as f64) * density).round() as usize;
    let target_count = target_count.max(systems.len().saturating_sub(1));

    let mut chosen: Vec<(usize, usize, u32, Vec<Arc<str>>)> = Vec::with_capacity(target_count);
    let mut chosen_set: BTreeSet<(usize, usize)> = BTreeSet::new();

    // Top-weight portion (deterministic).
    for (i, j, _, dist) in candidates.iter().take(target_count) {
        if chosen_set.insert((*i, *j)) {
            chosen.push((*i, *j, *dist, Vec::new()));
        }
    }

    // Connect isolated components if requested.
    if config.generation.routes.ensure_connected_graph {
        let mut parent: Vec<usize> = (0..systems.len()).collect();
        for (i, j, _, _) in &chosen {
            union(&mut parent, *i, *j);
        }
        let _ = rng; // RNG reserved for future stochastic edges
        for i in 0..systems.len() {
            for j in (i + 1)..systems.len() {
                if find(&mut parent, i) == find(&mut parent, j) {
                    continue;
                }
                let dist = hex_distance(systems[i].coord, systems[j].coord);
                if dist == 0 || dist > max_distance {
                    continue;
                }
                if chosen_set.insert((i, j)) {
                    chosen.push((i, j, dist, vec!["bridge".into()]));
                    union(&mut parent, i, j);
                }
            }
        }
    }

    let mut routes: Vec<GeneratedRoute> = chosen
        .into_iter()
        .map(|(i, j, dist, tags)| {
            let a = systems[i].id.clone();
            let b = systems[j].id.clone();
            let (from_id, to_id) = if a <= b { (a, b) } else { (b, a) };
            let (rt, stab) = classify_route(&systems[i], &systems[j], dist, max_distance);
            GeneratedRoute {
                id: ids::route_id(&from_id, &to_id),
                from_system_id: from_id,
                to_system_id: to_id,
                distance: dist,
                route_type: rt,
                stability: stab,
                tags,
                controls: Vec::new(),
            }
        })
        .collect();

    // Cap perilous routes at 10% of total. Excess downgraded to Hazardous,
    // shortest-first — so the downgrade can never leave a longer route safer
    // than a shorter one (preserves the short-is-safer invariant).
    let perilous_limit = ((routes.len() as f64) * 0.10).round() as usize;
    let perilous_count = routes
        .iter()
        .filter(|r| r.stability == RouteStability::Perilous)
        .count();
    if perilous_count > perilous_limit {
        let mut excess = perilous_count - perilous_limit;
        let mut perilous_idx: Vec<usize> = (0..routes.len())
            .filter(|&k| routes[k].stability == RouteStability::Perilous)
            .collect();
        // Shortest distance first; id as a deterministic tie-breaker.
        perilous_idx.sort_by(|&x, &y| {
            routes[x]
                .distance
                .cmp(&routes[y].distance)
                .then_with(|| routes[x].id.cmp(&routes[y].id))
        });
        for k in perilous_idx {
            if excess == 0 {
                break;
            }
            routes[k].stability = RouteStability::Hazardous;
            excess -= 1;
        }
    }

    routes.sort_by(|a, b| a.id.cmp(&b.id));
    routes
}

/// Severity ladder used internally by route classification: `0` = safest.
/// The public [`RouteStability`] deliberately derives no `Ord`, so the ordering
/// lives here, next to the only code that needs to reason about "safer than".
/// `pub(crate)` so the hidden-route layers can shift a distance baseline by
/// whole tiers (e.g. an escorted black-ship lane one tier safer).
pub(crate) fn stability_from_level(level: u8) -> RouteStability {
    match level {
        0 => RouteStability::Stable,
        1 => RouteStability::Unstable,
        2 => RouteStability::Hazardous,
        _ => RouteStability::Perilous,
    }
}

/// Inverse of [`stability_from_level`]: the severity rank of a stability value
/// (`0` = safest). `pub(crate)` so region overlays can clamp a stability change
/// to a distance-derived floor without making a long route safer than a short
/// one.
pub(crate) fn stability_level(s: RouteStability) -> u8 {
    match s {
        RouteStability::Stable => 0,
        RouteStability::Unstable => 1,
        RouteStability::Hazardous => 2,
        RouteStability::Perilous => 3,
    }
}

/// Distance-only baseline danger level, **monotonically non-decreasing in
/// `dist`**: a shorter hop is never given a worse baseline than a longer one.
/// Banded relative to `max_dist` so the gradient scales with the configured
/// cap (a 1-2 hex jump is always the safest baseline; a hop at/over the cap is
/// always the worst). This is the core "short is safer than long" guarantee;
/// hazards in [`classify_route`] may only push the level *up* from here.
/// `pub(crate)` so the hidden-route layers share the exact same gradient.
///
/// The Stable band is deliberately generous — short hops (<= 2 hexes, or up to
/// half the cap) read as Stable baseline so that dense sectors keep a usable
/// backbone of safe lanes instead of degrading almost everything to Hazardous.
pub(crate) fn distance_base_level(dist: u32, max_dist: u32) -> u8 {
    let max_dist = max_dist.max(1);
    if dist <= 2 {
        0 // 1-2 hex jump: always Stable baseline
    } else if dist >= max_dist {
        3 // at or beyond the cap: worst baseline
    } else if 2 * dist <= max_dist {
        0 // <= 50% of cap: Stable
    } else if 4 * dist <= 3 * max_dist {
        1 // <= 75% of cap: Unstable
    } else {
        2 // 75-100% of cap: Hazardous
    }
}

fn classify_route(
    a: &GeneratedSystem,
    b: &GeneratedSystem,
    dist: u32,
    max_dist: u32,
) -> (RouteType, RouteStability) {
    let tags: Vec<&Arc<str>> = a
        .worlds
        .iter()
        .chain(b.worlds.iter())
        .flat_map(|w| w.tags.iter())
        .collect();
    let has = |tag: &str| tags.iter().any(|t| t.as_ref() == tag);

    // Distance sets the monotonic baseline; hazards can only raise danger,
    // never lower it. A longer route therefore can never be safer than a
    // shorter one carrying the same hazards.
    let mut level = distance_base_level(dist, max_dist);
    let war_zone = has("feature:war_zone");
    let warp = has("feature:warp_phenomena") || has("feature:daemonic_corruption");
    if war_zone {
        level = level.saturating_add(2);
    }
    if warp {
        level = level.saturating_add(1);
    }
    let stability = stability_from_level(level.min(3));

    // Hub-anchored, low-danger lanes read as Stable Warp Lanes; anything
    // hazardous or beyond half the cap is a Charted Passage.
    // `level` is still the pristine baseline here (this branch excludes
    // hazards, the only thing that mutates it).
    let route_type = if !war_zone
        && !warp
        && level <= 1
        && (has("feature:trade_hub") || has("feature:administrative_hub"))
    {
        RouteType::StableWarpLane
    } else {
        RouteType::ChartedPassage
    };
    (route_type, stability)
}

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] == i {
        return i;
    }
    let root = find(parent, parent[i]);
    parent[i] = root;
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}
