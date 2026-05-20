//! Hidden route layers (§3 NEXT.md).
//!
//! Adds Aeldari `Webway`, Inquisition `BlackShip`, and criminal/Drukhari
//! `SmugglingLane` edges to the route graph after the main passable-warp
//! generator runs. Hidden routes do not honour the warp-distance cap —
//! they connect *any two systems* in the sector where both endpoints have
//! meaningful faction presence of the relevant kind. Distance is the raw
//! hex distance; stability is always `Stable` (these layers do not use
//! the warp).
//!
//! All output is deterministic given the input sector; this module does
//! not consult an RNG.

use std::collections::{BTreeMap, BTreeSet};

use crate::ids;
use crate::regions::{RegionConditionKind, WarpRegion};
use crate::sector_model::{
    hex_distance, GeneratedFaction, GeneratedRoute, GeneratedSystem, HexCoord, RouteStability,
    RouteType,
};

/// Threshold (sum of relevant presence dimensions across the system) above
/// which a system is treated as a viable endpoint for the named hidden
/// network. Tuned so a single Hidden-level presence doesn't qualify; a
/// Significant or higher presence does.
const ENDPOINT_THRESHOLD: f32 = 25.0;

/// Maximum hidden-route fan-out per endpoint. Each qualifying endpoint
/// connects to its `HIDDEN_K_NEAREST` closest peers (by hex distance, with
/// system-id tie-break); edges are deduplicated so the actual edge count is
/// at most `endpoints.len() * HIDDEN_K_NEAREST / 2`. This caps an otherwise
/// O(N²) full-clique blow-up that produced thousands of smuggling / webway
/// edges in larger sectors.
const HIDDEN_K_NEAREST: usize = 3;

/// Spec §3 NEXT: build the three hidden route layers and append them to
/// the existing route vector. Returns the count added.
///
/// Existing route ids are preserved; new ids get a `-webway` / `-blackship`
/// / `-smuggling` suffix on the canonical `route-AAAA-BBBB` form so the
/// invariants on uniqueness hold.
pub fn append_hidden_routes(
    systems: &[GeneratedSystem],
    factions: &[GeneratedFaction],
    routes: &mut Vec<GeneratedRoute>,
) -> usize {
    append_hidden_routes_with_regions(systems, factions, &[], routes)
}

/// §5 NEW.md `Blackout`: every system whose hex falls inside a Blackout
/// region is removed from the set of viable endpoints, so no hidden routes
/// terminate inside the blackout. Pure derivation, deterministic.
pub fn append_hidden_routes_with_regions(
    systems: &[GeneratedSystem],
    factions: &[GeneratedFaction],
    regions: &[WarpRegion],
    routes: &mut Vec<GeneratedRoute>,
) -> usize {
    if systems.len() < 2 || factions.is_empty() {
        return 0;
    }
    let blackout: BTreeSet<(i32, i32)> = regions
        .iter()
        .filter(|r| matches!(r.kind, RegionConditionKind::Blackout))
        .flat_map(|r| r.hexes.iter().map(|h| (h.q, h.r)))
        .collect();
    let in_blackout = |c: HexCoord| blackout.contains(&(c.q, c.r));
    let filtered: Vec<&GeneratedSystem> =
        systems.iter().filter(|s| !in_blackout(s.coord)).collect();
    if filtered.len() < 2 {
        return 0;
    }

    let kinds: BTreeMap<&str, &str> = factions
        .iter()
        .map(|f| (f.id.as_str(), f.kind.as_str()))
        .collect();

    let mut added = 0usize;
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        &["aeldari", "harlequin"],
        RouteType::Webway,
        "webway",
        |d| d.covert * 0.7 + d.military * 0.3,
    );
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        &["inquisition", "deathwatch", "grey_knights"],
        RouteType::BlackShip,
        "blackship",
        |d| d.covert * 0.5 + d.admin * 0.5,
    );
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        &["criminal", "drukhari", "rebel", "genestealer_cult"],
        RouteType::SmugglingLane,
        "smuggling",
        |d| d.covert * 0.6 + d.economic * 0.4,
    );
    added
}

fn endpoint_score(
    sys: &GeneratedSystem,
    kinds: &BTreeMap<&str, &str>,
    needles: &[&str],
    score_fn: impl Fn(&crate::sector_model::PresenceDimensions) -> f32,
) -> f32 {
    let mut s = 0.0;
    for w in &sys.worlds {
        for p in &w.factions {
            let k = p
                .subfaction_id
                .as_deref()
                .unwrap_or_else(|| kinds.get(p.faction_id.as_str()).copied().unwrap_or(""));
            if needles.contains(&k) {
                s += score_fn(&p.dimensions);
            }
        }
    }
    s
}

fn emit_layer(
    systems: &[&GeneratedSystem],
    kinds: &BTreeMap<&str, &str>,
    routes: &mut Vec<GeneratedRoute>,
    needles: &[&str],
    rtype: RouteType,
    suffix: &str,
    score_fn: impl Fn(&crate::sector_model::PresenceDimensions) -> f32 + Copy,
) -> usize {
    // Find every system whose accumulated needle-kind presence clears the
    // endpoint threshold.
    let mut endpoints: Vec<&GeneratedSystem> = systems
        .iter()
        .copied()
        .filter(|s| endpoint_score(s, kinds, needles, score_fn) >= ENDPOINT_THRESHOLD)
        .collect();
    if endpoints.len() < 2 {
        return 0;
    }
    endpoints.sort_by(|a, b| a.id.cmp(&b.id));

    // Existing undirected edges (any route type) to skip — we don't double
    // up hidden routes on top of an existing public lane.
    let mut existing: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    for r in routes.iter() {
        let (a, b) = order_pair(&r.from_system_id, &r.to_system_id);
        existing.insert((crate::ids::SystemId::new(a), crate::ids::SystemId::new(b)));
    }

    // K-nearest-neighbor selection: for each endpoint, pick the
    // `HIDDEN_K_NEAREST` closest peers (hex distance; system-id breaks
    // ties). Collect into a sorted set keyed by ordered (lo, hi) ids so a
    // pair selected from both sides only emits one edge. This replaces the
    // earlier full-clique enumeration that scaled O(N²) and produced
    // thousands of edges on dense sectors.
    let mut pairs: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    for (i, a) in endpoints.iter().enumerate() {
        let mut peers: Vec<(u32, &str, usize)> = endpoints
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, b)| (hex_distance(a.coord, b.coord), b.id.as_str(), j))
            .collect();
        peers.sort_by(|x, y| x.0.cmp(&y.0).then_with(|| x.1.cmp(y.1)));
        for (_, _, j) in peers.into_iter().take(HIDDEN_K_NEAREST) {
            let b = endpoints[j];
            let (lo, hi) = if a.id <= b.id {
                (a.id.clone(), b.id.clone())
            } else {
                (b.id.clone(), a.id.clone())
            };
            pairs.insert((lo, hi));
        }
    }

    let mut added = 0;
    for (from, to) in pairs {
        if existing.contains(&(from.clone(), to.clone())) {
            continue;
        }
        let a = endpoints.iter().find(|s| s.id == from).copied().unwrap();
        let b = endpoints.iter().find(|s| s.id == to).copied().unwrap();
        let dist = hex_distance(a.coord, b.coord);
        let base_id = ids::route_id(&from, &to);
        let id = crate::ids::RouteId::new(format!("{base_id}-{suffix}"));
        // Avoid duplicate inserts if a hidden lane of the same kind
        // already exists for this pair (e.g. from a re-run on a save).
        if routes.iter().any(|r| r.id == id) {
            continue;
        }
        routes.push(GeneratedRoute {
            id,
            from_system_id: from,
            to_system_id: to,
            distance: dist,
            route_type: rtype,
            stability: RouteStability::Stable,
            tags: vec![format!("hidden:{suffix}")],
            controls: Vec::new(),
        });
        added += 1;
    }
    added
}

fn order_pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedStar, GeneratedWorld, HexCoord, PowerProfile,
        PresenceDimensions, SystemControlSummary, WorldControlSummary, WorldDto,
        WorldFactionPresence,
    };

    fn sys(id: &str, coord: (i32, i32), faction: (&str, f32, f32)) -> GeneratedSystem {
        let (fid, covert, military) = faction;
        let world = GeneratedWorld {
            id: crate::ids::WorldId::new(format!("{id}-w1")),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: "AgriWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
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
        };
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            coord: HexCoord {
                q: coord.0,
                r: coord.1,
            },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            },
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

    fn fac(id: &str, kind: &str) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: "secretive".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile::default(),
        }
    }

    #[test]
    fn webway_links_two_aeldari_endpoints() {
        let systems = vec![
            sys("sys-0001", (0, 0), ("eld", 80.0, 30.0)),
            sys("sys-0002", (10, 10), ("eld", 70.0, 30.0)),
        ];
        let factions = vec![fac("eld", "aeldari")];
        let mut routes: Vec<GeneratedRoute> = Vec::new();
        let n = append_hidden_routes(&systems, &factions, &mut routes);
        assert_eq!(n, 1);
        assert_eq!(routes[0].route_type, RouteType::Webway);
        assert!(routes[0].tags.iter().any(|t| t == "hidden:webway"));
    }

    #[test]
    fn no_layer_when_only_one_endpoint_qualifies() {
        let systems = vec![
            sys("sys-0001", (0, 0), ("inq", 80.0, 40.0)),
            sys("sys-0002", (10, 10), ("nav", 0.0, 80.0)),
        ];
        let factions = vec![fac("inq", "inquisition"), fac("nav", "imperial_guard")];
        let mut routes: Vec<GeneratedRoute> = Vec::new();
        let n = append_hidden_routes(&systems, &factions, &mut routes);
        assert_eq!(n, 0);
    }
}
