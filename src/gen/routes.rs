//! Route rules loaded from `route_rules.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::ids::{self, SystemId};
use crate::sector_model::{
    hex_distance, GeneratedFaction, GeneratedRoute, GeneratedSystem, RouteStability, RouteType,
};
use crate::worlds::{Government, NotableFeature, WorldType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRulesFile {
    pub routes: RouteRules,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRules {
    #[serde(default = "default_weight")]
    pub default_weight: f64,
    #[serde(default = "default_max_distance")]
    pub max_distance: u32,
    #[serde(default)]
    pub prefer_populated_worlds: bool,
    #[serde(default)]
    pub prefer_trade_hubs: bool,
    #[serde(default)]
    pub avoid_warp_phenomena: bool,
    #[serde(default)]
    pub modifiers: Vec<RouteModifier>,
}

impl Default for RouteRules {
    fn default() -> Self {
        Self {
            default_weight: default_weight(),
            max_distance: default_max_distance(),
            prefer_populated_worlds: true,
            prefer_trade_hubs: true,
            avoid_warp_phenomena: true,
            modifiers: Vec::new(),
        }
    }
}

fn default_weight() -> f64 {
    1.0
}

fn default_max_distance() -> u32 {
    4
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteModifier {
    pub when: RouteCondition,
    pub multiplier: f64,
}

/// A route-modifier predicate. Each field is an optional *typed* enum: a value
/// present in `route_rules.toml` is deserialized into the corresponding domain
/// enum, so a misspelled condition (e.g. `notable_feature = "TradeHubb"`) is a
/// hard load-time error rather than a silently-never-matching string (P10).
///
/// `notable_feature`/`world_type`/`government` parse from their PascalCase
/// variant names (no `rename_all`); `route_type` parses from its snake_case key
/// (with the `dangerous_passage`/`DangerousPassage` aliases on `ChartedPassage`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RouteCondition {
    #[serde(default)]
    pub notable_feature: Option<NotableFeature>,
    #[serde(default)]
    pub world_type: Option<WorldType>,
    #[serde(default)]
    pub government: Option<Government>,
    #[serde(default)]
    pub route_type: Option<RouteType>,
}

// ── Route connectivity (union-find + Kruskal-style MST) ──────────────────────
//
// Relocated from the builder routes panel (REVIEW P15) so the connectivity
// algorithm lives in the lib domain layer, decoupled from `BuilderState`, and
// is unit-testable. Pure data in / pure data out; **no RNG** — the bridge edges
// are chosen deterministically (shortest hex distance, tie-broken by the
// `(SystemId, SystemId)` endpoint pair), so the output is byte-stable.

/// Greedily augment `routes` with the fewest bridge edges needed to make the
/// system graph fully connected (Kruskal-style MST over the unconnected
/// components). Candidate non-edges are sorted by hex distance, tie-broken by
/// endpoint id pair, and added shortest-first while they join two distinct
/// components. Returns the augmented, id-sorted route list plus the count of
/// bridges added.
///
/// `max_route_distance` decides each bridge's stability: a bridge no longer
/// than it is [`RouteStability::Stable`], otherwise [`RouteStability::Unstable`].
/// `factions` is threaded through so freshly-minted bridge routes get the same
/// derived [`controls`](GeneratedRoute::controls) the caller would compute.
#[must_use]
pub fn ensure_connected_routes(
    systems: &[GeneratedSystem],
    factions: &[GeneratedFaction],
    mut routes: Vec<GeneratedRoute>,
    max_route_distance: u32,
) -> (Vec<GeneratedRoute>, usize) {
    let n = systems.len();
    if n < 2 {
        return (routes, 0);
    }

    let index: BTreeMap<SystemId, usize> = systems
        .iter()
        .enumerate()
        .map(|(i, sys)| (sys.id.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut existing: BTreeSet<(SystemId, SystemId)> = BTreeSet::new();
    for route in &routes {
        if let (Some(&a), Some(&b)) = (
            index.get(&route.from_system_id),
            index.get(&route.to_system_id),
        ) {
            union(&mut parent, a, b);
        }
        let (lo, hi) = ordered_pair(&route.from_system_id, &route.to_system_id);
        existing.insert((lo, hi));
    }

    let mut candidates = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &systems[i];
            let b = &systems[j];
            let dist = hex_distance(a.coord, b.coord);
            candidates.push((dist, a.id.clone(), b.id.clone(), i, j));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    // Systems indexed by id for control derivation, matching what the builder
    // call site passed (`sector.systems` → id map) byte-for-byte.
    let systems_by_id: BTreeMap<&str, &GeneratedSystem> =
        systems.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut added = 0usize;
    for (dist, a_id, b_id, a_idx, b_idx) in candidates {
        if find_root(&mut parent, a_idx) == find_root(&mut parent, b_idx) {
            continue;
        }
        let (from, to) = ordered_pair(&a_id, &b_id);
        if existing.contains(&(from.clone(), to.clone())) {
            continue;
        }
        let stability = if dist <= max_route_distance {
            RouteStability::Stable
        } else {
            RouteStability::Unstable
        };
        let mut route = GeneratedRoute {
            id: ids::route_id(&from, &to),
            from_system_id: from.clone(),
            to_system_id: to.clone(),
            distance: dist,
            route_type: RouteType::ChartedPassage,
            stability,
            tags: vec![Arc::from("bridge")],
            controls: Vec::new(),
        };
        route.controls =
            crate::route_control::derive_route_controls(&route, &systems_by_id, factions);
        routes.push(route);
        existing.insert((from, to));
        union(&mut parent, a_idx, b_idx);
        added += 1;
    }
    routes.sort_by(|a, b| a.id.cmp(&b.id));
    (routes, added)
}

/// Number of connected components in the route graph over `systems` (union-find
/// over the routes whose endpoints both resolve to a known system).
#[must_use]
pub fn route_component_count(systems: &[GeneratedSystem], routes: &[GeneratedRoute]) -> usize {
    let n = systems.len();
    if n == 0 {
        return 0;
    }
    let index: BTreeMap<SystemId, usize> = systems
        .iter()
        .enumerate()
        .map(|(i, sys)| (sys.id.clone(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    for route in routes {
        if let (Some(&a), Some(&b)) = (
            index.get(&route.from_system_id),
            index.get(&route.to_system_id),
        ) {
            union(&mut parent, a, b);
        }
    }
    let mut roots = BTreeSet::new();
    for i in 0..n {
        roots.insert(find_root(&mut parent, i));
    }
    roots.len()
}

/// Canonical `(min, max)` ordering of an endpoint pair, so a bridge between two
/// systems has a single deterministic identity regardless of arg order.
fn ordered_pair(a: &SystemId, b: &SystemId) -> (SystemId, SystemId) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find_root(parent, a);
    let rb = find_root(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_condition_accepts_route_type_key() {
        let text = r#"
            [routes]
            default_weight = 1.0
            max_distance = 4

            [[routes.modifiers]]
            when = { route_type = "charted_passage", government = "MilitaryGovernor" }
            multiplier = 0.5
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        let modifier = &file.routes.modifiers[0];
        assert_eq!(modifier.when.route_type, Some(RouteType::ChartedPassage));
        assert_eq!(modifier.when.government, Some(Government::MilitaryGovernor));
    }

    #[test]
    fn route_condition_route_type_dangerous_passage_alias() {
        // The legacy `dangerous_passage` key still deserializes (alias on
        // ChartedPassage), so older configs keep loading.
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { route_type = "dangerous_passage" }
            multiplier = 0.5
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        assert_eq!(
            file.routes.modifiers[0].when.route_type,
            Some(RouteType::ChartedPassage)
        );
    }

    #[test]
    fn route_condition_pascalcase_world_fields_parse() {
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { notable_feature = "TradeHub", world_type = "ForgeWorld" }
            multiplier = 2.0
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        let cond = &file.routes.modifiers[0].when;
        assert_eq!(cond.notable_feature, Some(NotableFeature::TradeHub));
        assert_eq!(cond.world_type, Some(WorldType::ForgeWorld));
    }

    #[test]
    fn route_condition_misspelled_feature_is_a_load_error() {
        // P10: a typo in a condition value is now a hard deserialize error
        // instead of a silently-never-matching string.
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { notable_feature = "NotARealFeature" }
            multiplier = 2.0
        "#;
        let err = toml::from_str::<RouteRulesFile>(text)
            .expect_err("misspelled notable_feature must fail to deserialize");
        let msg = err.to_string();
        assert!(
            msg.contains("NotARealFeature") || msg.contains("unknown variant"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn route_condition_misspelled_route_type_is_a_load_error() {
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { route_type = "charted_passagee" }
            multiplier = 2.0
        "#;
        toml::from_str::<RouteRulesFile>(text)
            .expect_err("misspelled route_type must fail to deserialize");
    }

    #[test]
    fn route_condition_round_trips_through_toml() {
        // A builder-written rules file (PascalCase world fields, snake_case
        // route_type) must re-parse identically.
        let original = RouteRulesFile {
            routes: RouteRules {
                modifiers: vec![RouteModifier {
                    when: RouteCondition {
                        notable_feature: Some(NotableFeature::TradeHub),
                        world_type: Some(WorldType::ForgeWorld),
                        government: Some(Government::MilitaryGovernor),
                        route_type: Some(RouteType::ChartedPassage),
                    },
                    multiplier: 0.5,
                }],
                ..Default::default()
            },
        };
        let text = toml::to_string(&original).unwrap();
        assert!(text.contains("notable_feature = \"TradeHub\""));
        assert!(text.contains("route_type = \"charted_passage\""));
        let reparsed: RouteRulesFile = toml::from_str(&text).unwrap();
        let cond = &reparsed.routes.modifiers[0].when;
        assert_eq!(cond.notable_feature, Some(NotableFeature::TradeHub));
        assert_eq!(cond.world_type, Some(WorldType::ForgeWorld));
        assert_eq!(cond.government, Some(Government::MilitaryGovernor));
        assert_eq!(cond.route_type, Some(RouteType::ChartedPassage));
    }

    // ── Route connectivity (relocated from the builder, REVIEW P15) ──────────

    use crate::sector_model::{GeneratedSector, HexCoord};

    /// Two systems linked, a third isolated: one bridge joins all three into a
    /// single component (component count 2 → 1, `added > 0`), and the new edge
    /// is tagged `bridge` and touches the previously-isolated system.
    #[test]
    fn ensure_connected_adds_bridge_between_components() {
        let mut sector = GeneratedSector::empty("t", "T", "seed", 8, 8);
        let a = sector.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = sector.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let c = sector.add_system(HexCoord { q: 7, r: 7 }, "C").unwrap();
        sector
            .add_route(&a, &b, RouteType::ChartedPassage, RouteStability::Stable)
            .unwrap();

        assert_eq!(route_component_count(&sector.systems, &sector.routes), 2);
        let (routes, added) =
            ensure_connected_routes(&sector.systems, &sector.factions, sector.routes.clone(), 4);
        assert!(added > 0, "an isolated system must get at least one bridge");
        assert_eq!(route_component_count(&sector.systems, &routes), 1);
        assert!(
            routes
                .iter()
                .any(|r| (r.from_system_id == c || r.to_system_id == c)
                    && r.tags.iter().any(|t| t.as_ref() == "bridge")),
            "the isolated system C must be reached by a `bridge`-tagged route"
        );
    }

    /// An already-connected route set is returned unchanged (`added == 0`).
    #[test]
    fn ensure_connected_noop_when_already_connected() {
        let mut sector = GeneratedSector::empty("t", "T", "seed", 8, 8);
        let a = sector.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = sector.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let c = sector.add_system(HexCoord { q: 2, r: 0 }, "C").unwrap();
        sector
            .add_route(&a, &b, RouteType::ChartedPassage, RouteStability::Stable)
            .unwrap();
        sector
            .add_route(&b, &c, RouteType::ChartedPassage, RouteStability::Stable)
            .unwrap();

        assert_eq!(route_component_count(&sector.systems, &sector.routes), 1);
        let before = sector.routes.clone();
        let (routes, added) =
            ensure_connected_routes(&sector.systems, &sector.factions, sector.routes.clone(), 4);
        assert_eq!(added, 0, "a connected graph needs no bridges");
        let mut expected = before;
        expected.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(
            routes, expected,
            "connected input must be returned unchanged"
        );
    }

    /// The bridge selection is deterministic: identical inputs yield byte-stable
    /// bridge routes across two independent runs (no RNG, stable tie-break).
    #[test]
    fn ensure_connected_is_deterministic() {
        let mut sector = GeneratedSector::empty("t", "T", "seed", 8, 8);
        // Four mutually-isolated systems force several bridge choices, exercising
        // the distance + endpoint-pair tie-break ordering.
        let _ = sector.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let _ = sector.add_system(HexCoord { q: 4, r: 0 }, "B").unwrap();
        let _ = sector.add_system(HexCoord { q: 0, r: 4 }, "C").unwrap();
        let _ = sector.add_system(HexCoord { q: 4, r: 4 }, "D").unwrap();

        let (run1, added1) =
            ensure_connected_routes(&sector.systems, &sector.factions, sector.routes.clone(), 4);
        let (run2, added2) =
            ensure_connected_routes(&sector.systems, &sector.factions, sector.routes.clone(), 4);
        assert_eq!(added1, added2);
        assert_eq!(
            added1, 3,
            "four isolated systems need exactly three bridges"
        );
        assert_eq!(run1, run2, "the added bridge set must be byte-stable");
        assert_eq!(route_component_count(&sector.systems, &run1), 1);
    }
}
