//! Public route graph generation, route classification, and union-find helpers
//! used to optionally connect disjoint components.

use std::collections::BTreeSet;
use std::sync::Arc;

use rand_chacha::ChaCha8Rng;

use crate::config::{AppConfig, StabilityTargets};
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

    // Legacy balancing: cap perilous routes at 10% of total. Excess downgraded
    // to Hazardous, shortest-first — so the downgrade can never leave a longer
    // route safer than a shorter one (preserves the short-is-safer invariant).
    // Skipped when `stability_targets` is configured: the final quantile
    // rebalance (run after the region + hidden layers) then owns the whole
    // public-route stability distribution instead.
    if config.generation.routes.stability_targets.is_none() {
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

/// Cumulative cut indices `[c0, c1, c2]` partitioning `n` rank positions into
/// the four stability tiers (`[0,c0)` Stable, `[c0,c1)` Unstable, `[c1,c2)`
/// Hazardous, `[c2,n)` Perilous) per the target weights. Weights are treated as
/// relative (normalised by their sum) with negatives clamped to `0`; cuts are
/// clamped non-decreasing so the bands can never invert.
fn stability_cut_indices(n: usize, t: StabilityTargets) -> [usize; 3] {
    let w = [
        t.stable.max(0.0),
        t.unstable.max(0.0),
        t.hazardous.max(0.0),
        t.perilous.max(0.0),
    ];
    let sum: f64 = w.iter().sum();
    if !sum.is_finite() || sum <= 0.0 {
        // Degenerate weights: leave everything in the safest tier rather than
        // emit garbage cuts.
        return [n, n, n];
    }
    let nf = n as f64;
    let cut = |frac: f64| -> usize { (nf * frac / sum).round() as usize };
    let c0 = cut(w[0]).min(n);
    let c1 = cut(w[0] + w[1]).clamp(c0, n);
    let c2 = cut(w[0] + w[1] + w[2]).clamp(c1, n);
    [c0, c1, c2]
}

/// §route-rebalance: re-bucket **public** route stabilities to approach the
/// configured [`StabilityTargets`] mix, independent of how many warp-storm
/// regions the sector rolled (the original failure mode — a storm-heavy
/// `regions.toml` could force nearly every lane to Perilous, leaving almost no
/// Stable backbone).
///
/// Public routes (`!route_type.is_hidden()`) are ranked safest-first by the
/// pre-rebalance danger ordering — existing stability tier, then distance, then
/// id — and the rank positions are partitioned into the four tiers by
/// [`stability_cut_indices`]. Because that ranking key is non-decreasing in
/// distance for a fixed hazard/region configuration, and the partition is
/// monotonic in rank, a shorter route is never assigned a *less* safe tier than
/// a longer route carrying the same hazards: the "short is safer than long"
/// invariant holds by construction.
///
/// Hidden lanes (webway / black-ship / smuggling) are untouched — they keep
/// their own per-type stability. Finally, any route promoted to Perilous that
/// is the sole navigable link between its endpoints is capped back to Hazardous
/// (the same connectivity rule the region overlay uses), tagged
/// `rebalance:connectivity_preserved`.
pub(crate) fn rebalance_public_stability(routes: &mut [GeneratedRoute], targets: StabilityTargets) {
    let mut order: Vec<usize> = (0..routes.len())
        .filter(|&i| !routes[i].route_type.is_hidden())
        .collect();
    if order.is_empty() {
        return;
    }
    order.sort_by(|&a, &b| {
        stability_level(routes[a].stability)
            .cmp(&stability_level(routes[b].stability))
            .then_with(|| routes[a].distance.cmp(&routes[b].distance))
            .then_with(|| routes[a].id.cmp(&routes[b].id))
    });
    let [c0, c1, c2] = stability_cut_indices(order.len(), targets);
    for (rank, &ri) in order.iter().enumerate() {
        routes[ri].stability = if rank < c0 {
            RouteStability::Stable
        } else if rank < c1 {
            RouteStability::Unstable
        } else if rank < c2 {
            RouteStability::Hazardous
        } else {
            RouteStability::Perilous
        };
    }

    // Connectivity guard: don't let the rebalance strand part of the graph. A
    // route freshly at Perilous that is the only navigable link between its
    // endpoints is pulled back to Hazardous. Deterministic id order.
    let mut perilous: Vec<usize> = (0..routes.len())
        .filter(|&i| {
            !routes[i].route_type.is_hidden() && routes[i].stability == RouteStability::Perilous
        })
        .collect();
    perilous.sort_by(|&a, &b| routes[a].id.cmp(&routes[b].id));
    for i in perilous {
        // `is_navigable_bridge` treats a Perilous candidate as already severed,
        // so test it as Hazardous first, then restore it if it is not a bridge.
        routes[i].stability = RouteStability::Hazardous;
        if crate::regions::is_navigable_bridge(routes, i) {
            if !routes[i]
                .tags
                .iter()
                .any(|t| t.as_ref() == "rebalance:connectivity_preserved")
            {
                routes[i]
                    .tags
                    .push("rebalance:connectivity_preserved".into());
            }
        } else {
            routes[i].stability = RouteStability::Perilous;
        }
    }
}

#[cfg(test)]
mod rebalance_tests {
    use super::*;
    use crate::ids;

    fn targets(stable: f64, unstable: f64, hazardous: f64, perilous: f64) -> StabilityTargets {
        StabilityTargets {
            stable,
            unstable,
            hazardous,
            perilous,
        }
    }

    fn route(idx: usize, dist: u32, rt: RouteType, stab: RouteStability) -> GeneratedRoute {
        // Distinct, disconnected endpoints per route so the connectivity guard
        // never fires unless a test wires shared endpoints deliberately.
        let from = ids::SystemId::new(format!("sys-{:04}", idx * 2));
        let to = ids::SystemId::new(format!("sys-{:04}", idx * 2 + 1));
        GeneratedRoute {
            id: ids::route_id(&from, &to),
            from_system_id: from,
            to_system_id: to,
            distance: dist,
            route_type: rt,
            stability: stab,
            tags: Vec::new(),
            controls: Vec::new(),
        }
    }

    #[test]
    fn cut_indices_even_split() {
        assert_eq!(
            stability_cut_indices(100, targets(1.0, 1.0, 1.0, 1.0)),
            [25, 50, 75]
        );
    }

    #[test]
    fn cut_indices_relative_weights_are_normalised() {
        // 60/30/10/0 of 100 — weights need not sum to 1.
        assert_eq!(
            stability_cut_indices(100, targets(6.0, 3.0, 1.0, 0.0)),
            [60, 90, 100]
        );
    }

    #[test]
    fn cut_indices_degenerate_weights_keep_all_safe() {
        assert_eq!(
            stability_cut_indices(50, targets(0.0, 0.0, 0.0, 0.0)),
            [50, 50, 50]
        );
        assert_eq!(
            stability_cut_indices(50, targets(-1.0, -1.0, 0.0, 0.0)),
            [50, 50, 50]
        );
    }

    #[test]
    fn rebalance_hits_target_mix() {
        // 20 public routes all currently Perilous (the storm-flood failure
        // mode). Ask for half Stable, half Unstable.
        let mut routes: Vec<GeneratedRoute> = (0..20)
            .map(|i| {
                route(
                    i,
                    1 + (i as u32 % 4),
                    RouteType::ChartedPassage,
                    RouteStability::Perilous,
                )
            })
            .collect();
        rebalance_public_stability(&mut routes, targets(1.0, 1.0, 0.0, 0.0));
        let stable = routes
            .iter()
            .filter(|r| r.stability == RouteStability::Stable)
            .count();
        let unstable = routes
            .iter()
            .filter(|r| r.stability == RouteStability::Unstable)
            .count();
        assert_eq!(stable, 10);
        assert_eq!(unstable, 10);
    }

    #[test]
    fn rebalance_preserves_short_is_safer() {
        // Same hazard class (all ChartedPassage, same pre-rebalance tier), only
        // distance differs. After rebalance, a longer route must never be safer
        // than a shorter one.
        let mut routes: Vec<GeneratedRoute> = (0..40)
            .map(|i| {
                route(
                    i,
                    1 + (i as u32 % 8),
                    RouteType::ChartedPassage,
                    RouteStability::Unstable,
                )
            })
            .collect();
        rebalance_public_stability(&mut routes, targets(0.3, 0.3, 0.2, 0.2));
        for a in &routes {
            for b in &routes {
                // The invariant is about *strictly* shorter routes: a shorter
                // `a` must be at least as safe as a longer `b`. Equal-distance
                // routes may straddle a quantile boundary (split by id) — that
                // is not an inversion.
                if a.distance < b.distance {
                    assert!(
                        stability_level(a.stability) <= stability_level(b.stability),
                        "dist {} ({:?}) less safe than longer dist {} ({:?})",
                        a.distance,
                        a.stability,
                        b.distance,
                        b.stability
                    );
                }
            }
        }
    }

    #[test]
    fn rebalance_leaves_hidden_routes_untouched() {
        let mut routes = vec![
            route(0, 9, RouteType::Webway, RouteStability::Stable),
            route(1, 1, RouteType::ChartedPassage, RouteStability::Stable),
        ];
        rebalance_public_stability(&mut routes, targets(0.0, 0.0, 0.0, 1.0));
        // Webway keeps Stable despite an all-Perilous target; the public lane is
        // pushed out of Stable (to Perilous, or Hazardous if the connectivity
        // guard saves it as the sole link — here its endpoints are isolated, so
        // it is preserved as Hazardous).
        assert_eq!(routes[0].stability, RouteStability::Stable);
        assert_ne!(routes[1].stability, RouteStability::Stable);
        assert_eq!(routes[1].stability, RouteStability::Hazardous);
    }
}
