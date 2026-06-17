//! End-to-end route-stability monotonicity (TEST_GAPS.md §2c gap 1).
//!
//! Drives the full public generation pipeline (`generate_sector`), which runs
//! `generate_routes` → `apply_route_effects` → `append_hidden_routes` → optional
//! `rebalance_public_stability`, across random seeds and densities. For every
//! resulting sector it asserts the two route invariants that must survive the
//! whole composition:
//!
//! 1. Within a `(route_type, region effects, endpoint feature hazards)` bucket,
//!    a strictly shorter route is never *less* safe than a longer one (weak `<=`
//!    on the severity rank — the rebalance quantiles make equal-distance ties
//!    legal, so the comparison is strict only in distance). The bucket includes
//!    the endpoint systems' `feature:*` hazards because those drive baseline
//!    danger in `classify_route` yet never appear on `route.tags`; see
//!    `hazard_signature` below.
//! 2. Every `Webway` lane is `Stable`, unconditionally.

use std::collections::BTreeMap;

use proptest::prelude::*;
use sectorforge::sector_model::{GeneratedRoute, GeneratedSystem, RouteStability, RouteType};
use sectorforge::{generate_sector, load_project};

use crate::shared::fixture_dir;

/// Severity rank mirrored from `crate::generation::stability_level` (which is
/// `pub(crate)` and therefore invisible to this external integration crate).
/// `RouteStability` is `#[non_exhaustive]` and derives no `Ord`, so the `_` arm
/// is required and the ranks are pinned by hand.
fn level(s: RouteStability) -> u8 {
    match s {
        RouteStability::Stable => 0,
        RouteStability::Unstable => 1,
        RouteStability::Hazardous => 2,
        RouteStability::Perilous => 3,
        _ => 3,
    }
}

/// Endpoint-hazard map: system id → the sorted set of `feature:*` tags carried
/// by the worlds inside that system.
///
/// This is the load-bearing half of the bucket key. `classify_route`
/// (src/gen/generation/routes.rs:289) derives a route's *baseline* danger from
/// the `feature:*` tags on the worlds of **both endpoint systems** — a
/// `feature:war_zone` endpoint adds +2 levels, a `feature:warp_phenomena` /
/// `feature:daemonic_corruption` endpoint adds +1 — yet those feature tags are
/// **never copied onto `route.tags`** (only `region:*` / `bridge` /
/// `rebalance:*` tags ever land there, regions.rs:743-748 / routes.rs:171,443).
/// Bucketing on `route.tags` alone therefore lumps a short war-zone lane in with
/// a short clean lane, and the source guarantees short-is-safer only *for the
/// same hazards* — so the endpoint feature tags must be folded into the key, or
/// the comparison is between two genuinely different hazard classes.
type EndpointHazards = BTreeMap<String, Vec<String>>;

fn endpoint_hazards(systems: &[GeneratedSystem]) -> EndpointHazards {
    systems
        .iter()
        .map(|s| {
            let mut feats: Vec<String> = s
                .worlds
                .iter()
                .flat_map(|w| w.tags.iter())
                .map(|t| t.as_ref().to_owned())
                .filter(|t| t.starts_with("feature:"))
                .collect();
            feats.sort();
            feats.dedup();
            (s.id.as_str().to_owned(), feats)
        })
        .collect()
}

/// The hazard signature that defines a comparison bucket: the route type, the
/// sorted set of `region:*` effect tags on the route, **and** the sorted set of
/// `feature:*` hazard tags carried by the two endpoint systems' worlds. Two
/// routes share a bucket only when they share the same route type, the same
/// region overlays, and the same endpoint feature hazards — exactly the inputs
/// `classify_route` (plus the region/rebalance layers) consume to set stability.
/// Routes in different buckets carry different hazards and are not comparable.
/// Everything is sorted into `Vec`s so the key is deterministic (no reliance on
/// tag/system insertion order).
fn hazard_signature(
    route: &GeneratedRoute,
    hazards: &EndpointHazards,
) -> (RouteType, Vec<String>, Vec<String>) {
    let mut region_tags: Vec<String> = route
        .tags
        .iter()
        .map(|t| t.as_ref().to_owned())
        .filter(|t| t.starts_with("region:"))
        .collect();
    region_tags.sort();
    region_tags.dedup();

    let mut endpoint_feats: Vec<String> = [&route.from_system_id, &route.to_system_id]
        .into_iter()
        .filter_map(|id| hazards.get(id.as_str()))
        .flat_map(|feats| feats.iter().cloned())
        .collect();
    endpoint_feats.sort();
    endpoint_feats.dedup();

    (route.route_type, region_tags, endpoint_feats)
}

/// The generated route set together with the endpoint-hazard map needed to
/// bucket those routes by the hazards that actually drive their stability.
struct RouteSet {
    routes: Vec<GeneratedRoute>,
    hazards: EndpointHazards,
}

fn generate_at(seed: &str, dim: u32, system_count: usize, density: f64) -> RouteSet {
    let mut input = load_project(fixture_dir()).expect("load m42 fixture");
    input.config.generation.seed = seed.to_string();
    // Square sector: one `dim` feeds both sides (geometry invariant).
    input.config.generation.sector_width = dim;
    input.config.generation.sector_height = dim;
    input.config.generation.system_count = system_count;
    input.config.generation.min_worlds_per_system = 1;
    input.config.generation.max_worlds_per_system = 3;
    input.config.generation.routes.route_density = density;
    let sector = generate_sector(input).expect("generate sector");
    let hazards = endpoint_hazards(&sector.systems);
    RouteSet {
        routes: sector.routes,
        hazards,
    }
}

/// Assert the intra-sector bucket monotonicity invariant over one route set.
fn assert_bucket_monotonic(set: &RouteSet) -> Result<(), TestCaseError> {
    let RouteSet { routes, hazards } = set;
    for a in routes {
        for b in routes {
            // Strictly shorter `a` vs longer `b`, same hazard bucket.
            if a.distance < b.distance
                && hazard_signature(a, hazards) == hazard_signature(b, hazards)
            {
                prop_assert!(
                    level(a.stability) <= level(b.stability),
                    "shorter dist {} ({:?}) less safe than longer dist {} ({:?}) in bucket {:?}",
                    a.distance,
                    a.stability,
                    b.distance,
                    b.stability,
                    hazard_signature(a, hazards),
                );
            }
        }
    }
    Ok(())
}

/// Assert every webway lane is Stable over one route set.
fn assert_webways_stable(set: &RouteSet) -> Result<(), TestCaseError> {
    for r in &set.routes {
        if r.route_type == RouteType::Webway {
            prop_assert_eq!(
                r.stability,
                RouteStability::Stable,
                "webway {} was not Stable",
                r.id
            );
        }
    }
    Ok(())
}

// Vary system_count independently of dims while keeping the sector SQUARE — one
// `dim` feeds both width and height (geometry invariant). Mirrors
// `invariants_proptest.rs::square_dim_and_count`.
prop_compose! {
    fn square_dim_and_count()(dim in 6u32..=16)
        (system_count in 4usize..=((dim as usize) * (dim as usize)), dim in Just(dim))
        -> (u32, usize) {
        (dim, system_count)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        max_shrink_iters: 32,
        .. ProptestConfig::default()
    })]

    // Single-sector clauses: within each generated sector, buckets are
    // monotone and every webway is Stable.
    #[test]
    fn route_monotonicity_holds_within_a_sector(
        seed in "[a-z0-9]{4,12}",
        (dim, system_count) in square_dim_and_count(),
        density in 0.05f64..=0.6,
    ) {
        let set = generate_at(&seed, dim, system_count, density);
        assert_bucket_monotonic(&set)?;
        assert_webways_stable(&set)?;
    }

    // Paired-density clause: regenerate the SAME sector at two different
    // densities and re-check both invariants on each. The invariants are
    // per-sector, so they must hold at every density independently.
    #[test]
    fn route_monotonicity_holds_across_two_densities(
        seed in "[a-z0-9]{4,12}",
        (dim, system_count) in square_dim_and_count(),
        d_lo in 0.05f64..=0.30,
        d_extra in 0.05f64..=0.30,
    ) {
        let d_hi = d_lo + d_extra; // strictly greater than d_lo

        let set_lo = generate_at(&seed, dim, system_count, d_lo);
        assert_bucket_monotonic(&set_lo)?;
        assert_webways_stable(&set_lo)?;

        let set_hi = generate_at(&seed, dim, system_count, d_hi);
        assert_bucket_monotonic(&set_hi)?;
        assert_webways_stable(&set_hi)?;
    }
}
