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

    // Cap perilous routes at 10% of total. Excess downgraded to Hazardous.
    let perilous_limit = ((routes.len() as f64) * 0.10).round() as usize;
    if routes
        .iter()
        .filter(|r| r.stability == RouteStability::Perilous)
        .count()
        > perilous_limit
    {
        let remaining = std::cell::Cell::new(
            routes
                .iter()
                .filter(|r| r.stability == RouteStability::Perilous)
                .count()
                .saturating_sub(perilous_limit),
        );
        for r in &mut routes {
            if r.stability == RouteStability::Perilous && remaining.get() > 0 {
                r.stability = RouteStability::Hazardous;
                remaining.set(remaining.get() - 1);
            }
        }
    }

    routes.sort_by(|a, b| a.id.cmp(&b.id));
    routes
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
    if has("feature:warp_phenomena") || has("feature:daemonic_corruption") {
        if dist >= max_dist - 2 && dist < max_dist {
            return (RouteType::ChartedPassage, RouteStability::Perilous);
        }
        return (RouteType::ChartedPassage, RouteStability::Hazardous);
    }
    if has("feature:war_zone") {
        return (RouteType::ChartedPassage, RouteStability::Perilous);
    }
    if dist >= max_dist {
        return (RouteType::ChartedPassage, RouteStability::Unstable);
    }
    if has("feature:trade_hub") || has("feature:administrative_hub") {
        return (RouteType::StableWarpLane, RouteStability::Stable);
    }
    (RouteType::ChartedPassage, RouteStability::Stable)
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
