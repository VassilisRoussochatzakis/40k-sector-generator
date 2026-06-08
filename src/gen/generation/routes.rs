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
use crate::worlds::NotableFeature;

/// The `feature:{snake}` tag a `NotableFeature` produces, built the same way as
/// `world_placement::compute_tags` (`format!("feature:{}", snake(f.as_ref()))`)
/// so a renamed variant updates the producer and these route-weight comparisons
/// together — no silent literal drift (A3).
fn feature_tag(f: &NotableFeature) -> String {
    format!("feature:{}", taxonomy::to_snake_case(f.as_ref()))
}

/// Convex exponent applied to the 0..=1 `route_density` slider before it scales
/// route count. `> 1` biases the low/mid slider toward *fewer* routes (ease-in):
/// at `2.0`, density `0.3` spends ~9% of the per-node edge budget and `0.5`
/// spends 25%. Raise for an even sparser mid-range; `1.0` is a linear response.
const ROUTE_DENSITY_CURVE: f64 = 2.0;

/// Extra warp edges *per system* added at full density (`1.0`), on top of the
/// `(n - 1)` connectivity backbone. Route count is tied to the system count
/// rather than the `O(n²)` candidate-pair pool, so the slider stays usable on
/// large sectors and at high `max_route_distance`. At full density the graph
/// holds ~`(1 + this) * n` edges (≈ average degree `2 * (1 + this)`).
const ROUTE_DENSITY_EXTRA_PER_NODE: f64 = 2.0;

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

    // Enum-keyed route-weight feature tags, built once before the O(n²) pair
    // loop (A3). Boost = strategic/economic hubs (×2.0); penalty = hazardous /
    // interdicted space (×0.25).
    let boost_tags: BTreeSet<String> = [
        NotableFeature::TradeHub,
        NotableFeature::Freeport,
        NotableFeature::MajorSpaceyard,
        NotableFeature::AdministrativeHub,
        NotableFeature::SubsectorHegemon,
    ]
    .into_iter()
    .map(|f| feature_tag(&f))
    .collect();
    let penalty_tags: BTreeSet<String> = [
        NotableFeature::WarpPhenomena,
        NotableFeature::Quarantined,
        NotableFeature::WarZone,
        NotableFeature::DaemonicCorruption,
    ]
    .into_iter()
    .map(|f| feature_tag(&f))
    .collect();

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

            if combined_tags
                .iter()
                .any(|t| boost_tags.contains(t.as_ref()))
            {
                w *= 2.0;
            }
            if combined_tags
                .iter()
                .any(|t| penalty_tags.contains(t.as_ref()))
            {
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

    // Route count skews deliberately sparse. Rather than a flat fraction of the
    // O(n²) candidate pool — which made mid-slider densities explode and large
    // sectors / high `max_distance` unusable — `density` adds EXTRA edges on top
    // of the (n - 1) spanning backbone, scaled linearly in the system count and
    // bent convexly so the low/mid slider stays sparse: density 0 → backbone
    // only; density 1 → ~ROUTE_DENSITY_EXTRA_PER_NODE * n extra edges.
    let backbone = systems.len().saturating_sub(1);
    let extra = (density.powf(ROUTE_DENSITY_CURVE)
        * ROUTE_DENSITY_EXTRA_PER_NODE
        * systems.len() as f64)
        .round() as usize;
    // `take` below saturates at `candidates.len()`, so over-targeting a small
    // candidate pool harmlessly yields every available edge.
    let target_count = backbone + extra;

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
        cap_perilous_routes(&mut routes);
    }

    routes.sort_by(|a, b| a.id.cmp(&b.id));
    routes
}

/// Legacy 10% perilous-cap: if more than `round(len * 0.10)` routes are
/// `Perilous`, downgrade the excess to `Hazardous`, shortest-distance first
/// (route id ascending as a deterministic tie-break). Downgrading the shortest
/// excess keeps the short-is-safer invariant — a longer route is never left
/// safer than a shorter one. Extracted verbatim from `generate_routes` (the gate
/// on `stability_targets.is_none()` stays at the call site) so the algorithm can
/// be unit-tested directly.
pub(crate) fn cap_perilous_routes(routes: &mut [GeneratedRoute]) {
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

    // ── §2c: legacy 10% perilous-cap (cap_perilous_routes) ─────────────────

    #[test]
    fn cap_downgrades_shortest_excess() {
        // 5 routes, all Perilous, distinct distances 1..=5 (idx ascending so ids
        // also ascend). perilous_limit = round(5 * 0.10) = round(0.5) = 1, so
        // excess = 4. Shortest-first downgrade leaves only the dist-5 route
        // Perilous; the four shorter ones become Hazardous.
        let mut routes: Vec<GeneratedRoute> = (0..5)
            .map(|i| {
                route(
                    i,
                    (i as u32) + 1,
                    RouteType::ChartedPassage,
                    RouteStability::Perilous,
                )
            })
            .collect();
        cap_perilous_routes(&mut routes);
        // routes[i] has distance i+1; the longest (dist 5) is still Perilous.
        assert_eq!(routes[4].distance, 5);
        assert_eq!(routes[4].stability, RouteStability::Perilous);
        for r in &routes[..4] {
            assert_eq!(
                r.stability,
                RouteStability::Hazardous,
                "shorter dist {} should be downgraded",
                r.distance
            );
        }
    }

    #[test]
    fn cap_tie_breaks_on_id() {
        // 20 routes total → perilous_limit = round(20 * 0.10) = round(2.0) = 2.
        // Three Perilous routes: a tied pair at the shortest distance (1) and one
        // longer (dist 5). excess = 3 - 2 = 1, so exactly one route downgrades.
        // The tied pair sorts first (shortest distance), and the id tie-break
        // (ascending) picks the smaller-id member — idx 0, whose id
        // `route-sys-0000-sys-0001` is lexicographically below idx 1's
        // `route-sys-0002-sys-0003`. So idx 0 downgrades; idx 1 stays Perilous.
        let mut routes: Vec<GeneratedRoute> = Vec::new();
        routes.push(route(
            0,
            1,
            RouteType::ChartedPassage,
            RouteStability::Perilous,
        ));
        routes.push(route(
            1,
            1,
            RouteType::ChartedPassage,
            RouteStability::Perilous,
        ));
        routes.push(route(
            2,
            5,
            RouteType::ChartedPassage,
            RouteStability::Perilous,
        ));
        // Pad with non-Perilous routes up to 20 so the limit is exactly 2.
        for i in 3..20 {
            routes.push(route(
                i,
                1,
                RouteType::ChartedPassage,
                RouteStability::Stable,
            ));
        }
        // Sanity: idx 0's id sorts below idx 1's (the tie-break direction).
        assert!(routes[0].id < routes[1].id);

        cap_perilous_routes(&mut routes);

        // Smaller-id member of the tied pair is the one downgraded.
        assert_eq!(
            routes[0].stability,
            RouteStability::Hazardous,
            "smaller-id tied route should downgrade first"
        );
        assert_eq!(
            routes[1].stability,
            RouteStability::Perilous,
            "larger-id tied route should survive"
        );
        // The longer Perilous route is untouched (only one excess to downgrade).
        assert_eq!(routes[2].stability, RouteStability::Perilous);
    }

    #[test]
    fn cap_is_noop_under_limit() {
        // 10 routes, exactly 1 Perilous → perilous_limit = round(1.0) = 1, which
        // is not exceeded, so cap_perilous_routes leaves everything unchanged.
        let mut routes: Vec<GeneratedRoute> = (0..10)
            .map(|i| {
                let stab = if i == 0 {
                    RouteStability::Perilous
                } else {
                    RouteStability::Stable
                };
                route(i, 1 + (i as u32 % 4), RouteType::ChartedPassage, stab)
            })
            .collect();
        let before = routes.clone();
        cap_perilous_routes(&mut routes);
        assert_eq!(
            routes, before,
            "perilous_count (1) <= perilous_limit (1): no downgrade"
        );
    }
}

#[cfg(test)]
mod route_classify_tests {
    use super::*;
    use crate::ids;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedStar, GeneratedWorld, HexCoord,
        PresenceDimensions, SystemControlSummary, WorldControlSummary, WorldDto,
        WorldFactionPresence,
    };
    use proptest::prelude::*;

    // Minimal valid `GeneratedSystem` — copied from the canonical builder in
    // `src/gen/hidden_routes.rs` `mod tests` (l.533). `classify_route` only
    // reads `worlds[..].tags`, so tests mutate `system.worlds[0].tags` after
    // construction to inject `feature:*` strings.
    fn sys(id: &str, coord: (i32, i32), faction: (&str, f32, f32)) -> GeneratedSystem {
        let (fid, covert, military) = faction;
        let world = GeneratedWorld {
            id: ids::WorldId::new(format!("{id}-w1")),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: crate::worlds::StarColour::White,
                world_type: crate::worlds::WorldType::AgriWorld,
                atmosphere: crate::worlds::Atmosphere::Breathable,
                temperature: crate::worlds::Temperature::Temperate,
                biosphere: crate::worlds::Biosphere::Thriving,
                population: crate::worlds::Population::DenselyPopulated,
                tech_level: crate::worlds::TechLevel::High,
                government: crate::worlds::Government::MagistrateCouncil,
                notable_features: vec![],
            },
            factions: vec![WorldFactionPresence {
                faction_id: fid.into(),
                subfaction_id: None,
                subfaction_name: None,
                force_id: None,
                force_name: None,
                influence: FactionInfluence::Significant,
                relationship_to_government: "secretive".into(),
                dimensions: PresenceDimensions {
                    covert,
                    military,
                    visibility: 30.0,
                    ..Default::default()
                },
                dominance: DominanceState::default(),
                intel_confidence: 30,
            }],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
            intel: Default::default(),
        };
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            kind: crate::sector_model::SystemKind::Star,
            coord: HexCoord {
                q: coord.0,
                r: coord.1,
            },
            star: Some(GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![world],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    /// Build a `(GeneratedSystem, GeneratedSystem)` pair whose combined world
    /// tags are exactly `tags`. `classify_route` chains `a.worlds.tags` then
    /// `b.worlds.tags`, so putting every tag on `a` is sufficient.
    fn pair_with_tags(tags: &[&str]) -> (GeneratedSystem, GeneratedSystem) {
        let mut a = sys("sys-0000", (0, 0), ("f", 0.0, 0.0));
        let b = sys("sys-0001", (1, 0), ("f", 0.0, 0.0));
        a.worlds[0].tags = tags.iter().map(|t| (*t).into()).collect();
        (a, b)
    }

    // ── GAP 2: distance_base_level monotonicity + boundaries ──────────────

    proptest! {
        #[test]
        fn distance_base_level_monotonic_in_dist(
            max in 1u32..=12,
            d1 in 0u32..=20,
            d2 in 0u32..=20,
        ) {
            prop_assume!(d1 <= d2);
            prop_assert!(distance_base_level(d1, max) <= distance_base_level(d2, max));
        }
    }

    #[test]
    fn distance_base_level_boundaries() {
        // 3 of 4 → 75% band → Unstable baseline (level 1).
        assert_eq!(distance_base_level(3, 4), 1);
        // dist >= max → worst baseline (level 3).
        assert_eq!(distance_base_level(4, 4), 3);
        // <= 50% of cap → Stable baseline.
        assert_eq!(distance_base_level(2, 4), 0);
        // Small-max anchor: dist == max → worst.
        assert_eq!(distance_base_level(3, 3), 3);
    }

    #[test]
    fn distance_base_level_max_zero_saturates_no_panic() {
        // max clamps to 1; dist >= max → 3.
        assert_eq!(distance_base_level(3, 0), 3);
        // dist <= 2 arm wins before the dist>=max arm.
        assert_eq!(distance_base_level(0, 0), 0);
        // dist=1 is <= 2 → Stable even though it also satisfies dist>=max.
        assert_eq!(distance_base_level(1, 1), 0);
    }

    // ── GAP 8: stability_from_level / stability_level round-trip ───────────

    #[test]
    fn stability_level_round_trips_level_to_stab_to_level() {
        for lvl in 0u8..=3 {
            assert_eq!(stability_level(stability_from_level(lvl)), lvl);
        }
    }

    #[test]
    fn stability_from_level_round_trips_stab_to_level_to_stab() {
        for s in [
            RouteStability::Stable,
            RouteStability::Unstable,
            RouteStability::Hazardous,
            RouteStability::Perilous,
        ] {
            assert_eq!(stability_from_level(stability_level(s)), s);
        }
    }

    #[test]
    fn stability_from_level_saturates() {
        assert_eq!(stability_from_level(4), RouteStability::Perilous);
        assert_eq!(stability_from_level(255), RouteStability::Perilous);
    }

    #[test]
    fn stability_level_is_a_total_order_0_to_3() {
        // RouteStability has no `Ord`; assert the explicit rank values so the
        // ordering Stable < Unstable < Hazardous < Perilous is pinned.
        assert_eq!(stability_level(RouteStability::Stable), 0);
        assert_eq!(stability_level(RouteStability::Unstable), 1);
        assert_eq!(stability_level(RouteStability::Hazardous), 2);
        assert_eq!(stability_level(RouteStability::Perilous), 3);
    }

    // ── GAP 3: classify_route hazard layering ─────────────────────────────

    #[test]
    fn classify_route_war_zone_adds_two_tiers() {
        // baseline 0 (dist=1,max=4) + war_zone(+2) → level 2 → Hazardous.
        // (A +1 bump would give Unstable; assert Hazardous to prove it is +2.)
        let (a, b) = pair_with_tags(&["feature:war_zone"]);
        assert_eq!(
            classify_route(&a, &b, 1, 4).1,
            RouteStability::Hazardous,
            "war_zone must add two tiers, not one"
        );
    }

    #[test]
    fn classify_route_war_zone_saturates_to_perilous() {
        // baseline 2 (dist=7,max=8) + war_zone(+2) = 4 → .min(3) → Perilous.
        let (a, b) = pair_with_tags(&["feature:war_zone"]);
        assert_eq!(classify_route(&a, &b, 7, 8).1, RouteStability::Perilous);
    }

    #[test]
    fn classify_route_warp_adds_one_tier() {
        // baseline 0 (dist=1,max=4) + warp(+1) → level 1 → Unstable.
        let (a, b) = pair_with_tags(&["feature:warp_phenomena"]);
        assert_eq!(classify_route(&a, &b, 1, 4).1, RouteStability::Unstable);
    }

    #[test]
    fn classify_route_stable_warp_lane_requires_all_four_conditions() {
        // All four hold: baseline 0 <= 1, no war_zone, no warp, trade_hub tag.
        let (a, b) = pair_with_tags(&["feature:trade_hub"]);
        assert_eq!(classify_route(&a, &b, 1, 4).0, RouteType::StableWarpLane);

        // administrative_hub equally yields a StableWarpLane.
        let (a, b) = pair_with_tags(&["feature:administrative_hub"]);
        assert_eq!(classify_route(&a, &b, 1, 4).0, RouteType::StableWarpLane);

        // (a) add war_zone → ChartedPassage.
        let (a, b) = pair_with_tags(&["feature:trade_hub", "feature:war_zone"]);
        assert_eq!(classify_route(&a, &b, 1, 4).0, RouteType::ChartedPassage);

        // (b) add warp → ChartedPassage.
        let (a, b) = pair_with_tags(&["feature:trade_hub", "feature:warp_phenomena"]);
        assert_eq!(classify_route(&a, &b, 1, 4).0, RouteType::ChartedPassage);

        // (c) bump baseline above 1 (dist=7,max=8 → baseline 2), keep trade_hub,
        //     no hazards → level>1 fails the StableWarpLane gate → ChartedPassage.
        let (a, b) = pair_with_tags(&["feature:trade_hub"]);
        assert_eq!(classify_route(&a, &b, 7, 8).0, RouteType::ChartedPassage);

        // (d) remove the hub tag entirely → ChartedPassage.
        let (a, b) = pair_with_tags(&[]);
        assert_eq!(classify_route(&a, &b, 1, 4).0, RouteType::ChartedPassage);
    }

    #[test]
    fn classify_route_monotone_within_fixed_hazard_set() {
        // No hazard tags on either system: stability is then purely the
        // distance baseline, which is non-decreasing in distance.
        let (a, b) = pair_with_tags(&[]);
        let max = 10;
        for d1 in 0u32..=12 {
            for d2 in d1..=12 {
                let s1 = stability_level(classify_route(&a, &b, d1, max).1);
                let s2 = stability_level(classify_route(&a, &b, d2, max).1);
                assert!(
                    s1 <= s2,
                    "dist {d1} (level {s1}) classified less safe than longer dist {d2} (level {s2})"
                );
            }
        }
    }
}
