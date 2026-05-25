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

use serde::{Deserialize, Serialize};

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

pub const DEFAULT_HIDDEN_K_NEAREST: usize = HIDDEN_K_NEAREST;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiddenRoutesConfig {
    pub kind: RouteType,
    #[serde(default)]
    pub endpoints: Vec<crate::ids::SystemId>,
    #[serde(default = "default_hidden_k_nearest")]
    pub k_nearest: usize,
    #[serde(default = "default_true")]
    pub exclude_blackout_regions: bool,
}

impl Default for HiddenRoutesConfig {
    fn default() -> Self {
        Self {
            kind: RouteType::Webway,
            endpoints: Vec::new(),
            k_nearest: DEFAULT_HIDDEN_K_NEAREST,
            exclude_blackout_regions: true,
        }
    }
}

fn default_hidden_k_nearest() -> usize {
    DEFAULT_HIDDEN_K_NEAREST
}

fn default_true() -> bool {
    true
}

/// Build one explicit hidden-route layer from user-selected endpoints.
/// Existing undirected edges are skipped to preserve route uniqueness.
#[must_use]
pub fn configured_hidden_routes(
    systems: &[GeneratedSystem],
    factions: &[GeneratedFaction],
    regions: &[WarpRegion],
    existing_routes: &[GeneratedRoute],
    config: &HiddenRoutesConfig,
) -> Vec<GeneratedRoute> {
    if !config.kind.is_hidden() || config.k_nearest == 0 {
        return Vec::new();
    }
    let endpoint_ids: BTreeSet<&str> = config.endpoints.iter().map(|id| id.as_str()).collect();
    if endpoint_ids.len() < 2 {
        return Vec::new();
    }
    let blackout: BTreeSet<(i32, i32)> = if config.exclude_blackout_regions {
        regions
            .iter()
            .filter(|r| matches!(r.kind, RegionConditionKind::Blackout))
            .flat_map(|r| r.hexes.iter().map(|h| (h.q, h.r)))
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut endpoints: Vec<&GeneratedSystem> = systems
        .iter()
        .filter(|s| endpoint_ids.contains(s.id.as_str()))
        .filter(|s| !blackout.contains(&(s.coord.q, s.coord.r)))
        .collect();
    if endpoints.len() < 2 {
        return Vec::new();
    }
    endpoints.sort_by(|a, b| a.id.cmp(&b.id));

    let mut existing: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    let mut existing_route_ids: BTreeSet<crate::ids::RouteId> = BTreeSet::new();
    for r in existing_routes {
        let (a, b) = if r.from_system_id <= r.to_system_id {
            (r.from_system_id.clone(), r.to_system_id.clone())
        } else {
            (r.to_system_id.clone(), r.from_system_id.clone())
        };
        existing.insert((a, b));
        existing_route_ids.insert(r.id.clone());
    }

    let mut pairs: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    for (i, a) in endpoints.iter().enumerate() {
        let mut peers: Vec<(u32, &str, usize)> = endpoints
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, b)| (hex_distance(a.coord, b.coord), b.id.as_str(), j))
            .collect();
        peers.sort_by(|x, y| x.0.cmp(&y.0).then_with(|| x.1.cmp(y.1)));
        for (_, _, j) in peers.into_iter().take(config.k_nearest) {
            let b = endpoints[j];
            let (lo, hi) = if a.id <= b.id {
                (a.id.clone(), b.id.clone())
            } else {
                (b.id.clone(), a.id.clone())
            };
            pairs.insert((lo, hi));
        }
    }

    let systems_by_id: BTreeMap<&str, &GeneratedSystem> =
        systems.iter().map(|s| (s.id.as_str(), s)).collect();
    let suffix = hidden_suffix(config.kind);
    let mut out = Vec::new();
    for (from, to) in pairs {
        if existing.contains(&(from.clone(), to.clone())) {
            continue;
        }
        let id = crate::ids::RouteId::new(format!("{}-{suffix}", ids::route_id(&from, &to)));
        if !existing_route_ids.insert(id.clone()) {
            continue;
        }
        let Some(a) = systems_by_id.get(from.as_str()).copied() else {
            continue;
        };
        let Some(b) = systems_by_id.get(to.as_str()).copied() else {
            continue;
        };
        let mut route = GeneratedRoute {
            id,
            from_system_id: from,
            to_system_id: to,
            distance: hex_distance(a.coord, b.coord),
            route_type: config.kind,
            stability: RouteStability::Stable,
            tags: vec![format!("hidden:{suffix}").into()],
            controls: Vec::new(),
        };
        route.controls =
            crate::route_control::derive_route_controls(&route, &systems_by_id, factions);
        out.push(route);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn hidden_suffix(kind: RouteType) -> &'static str {
    match kind {
        RouteType::Webway => "webway",
        RouteType::BlackShip => "blackship",
        RouteType::SmugglingLane => "smuggling",
        _ => "hidden",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenRoutesProgress {
    LayerStarted {
        layer: &'static str,
        endpoints: usize,
    },
    LayerProgress {
        layer: &'static str,
        current: usize,
        total: usize,
        pairs: usize,
    },
    LayerEmitProgress {
        layer: &'static str,
        current: usize,
        total: usize,
        added: usize,
    },
    LayerCompleted {
        layer: &'static str,
        added: usize,
        routes: usize,
    },
}

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
    append_hidden_routes_with_regions_and_progress(systems, factions, regions, routes, |_| {})
}

pub fn append_hidden_routes_with_regions_and_progress(
    systems: &[GeneratedSystem],
    factions: &[GeneratedFaction],
    regions: &[WarpRegion],
    routes: &mut Vec<GeneratedRoute>,
    mut progress: impl FnMut(HiddenRoutesProgress),
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
        .map(|f| (f.id.as_str(), f.kind.as_ref()))
        .collect();

    let mut added = 0usize;
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        "webway",
        &["aeldari", "harlequin"],
        RouteType::Webway,
        "webway",
        |d| d.covert.mul_add(0.7, d.military * 0.3),
        &mut progress,
    );
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        "blackship",
        &["inquisition", "deathwatch", "grey_knights"],
        RouteType::BlackShip,
        "blackship",
        |d| d.covert.mul_add(0.5, d.admin * 0.5),
        &mut progress,
    );
    added += emit_layer(
        &filtered,
        &kinds,
        routes,
        "smuggling",
        &["criminal", "drukhari", "rebel", "genestealer_cult"],
        RouteType::SmugglingLane,
        "smuggling",
        |d| d.covert.mul_add(0.6, d.economic * 0.4),
        &mut progress,
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

#[allow(clippy::too_many_arguments)]
fn emit_layer(
    systems: &[&GeneratedSystem],
    kinds: &BTreeMap<&str, &str>,
    routes: &mut Vec<GeneratedRoute>,
    layer: &'static str,
    needles: &[&str],
    rtype: RouteType,
    suffix: &str,
    score_fn: impl Fn(&crate::sector_model::PresenceDimensions) -> f32 + Copy,
    progress: &mut impl FnMut(HiddenRoutesProgress),
) -> usize {
    // Find every system whose accumulated needle-kind presence clears the
    // endpoint threshold.
    let mut endpoints: Vec<&GeneratedSystem> = systems
        .iter()
        .copied()
        .filter(|s| endpoint_score(s, kinds, needles, score_fn) >= ENDPOINT_THRESHOLD)
        .collect();
    progress(HiddenRoutesProgress::LayerStarted {
        layer,
        endpoints: endpoints.len(),
    });
    if endpoints.len() < 2 {
        progress(HiddenRoutesProgress::LayerCompleted {
            layer,
            added: 0,
            routes: routes.len(),
        });
        return 0;
    }
    endpoints.sort_by(|a, b| a.id.cmp(&b.id));

    // Existing undirected edges (any route type) to skip — we don't double
    // up hidden routes on top of an existing public lane.
    let mut existing: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    let mut existing_route_ids: BTreeSet<crate::ids::RouteId> = BTreeSet::new();
    for r in routes.iter() {
        let (a, b) = order_pair(&r.from_system_id, &r.to_system_id);
        existing.insert((crate::ids::SystemId::new(a), crate::ids::SystemId::new(b)));
        existing_route_ids.insert(r.id.clone());
    }
    let endpoint_by_id: BTreeMap<&str, &GeneratedSystem> =
        endpoints.iter().map(|s| (s.id.as_str(), *s)).collect();

    // K-nearest-neighbor selection: for each endpoint, pick the
    // `HIDDEN_K_NEAREST` closest peers (hex distance; system-id breaks
    // ties). Collect into a sorted set keyed by ordered (lo, hi) ids so a
    // pair selected from both sides only emits one edge. This replaces the
    // earlier full-clique enumeration that scaled O(N²) and produced
    // thousands of edges on dense sectors.
    let mut pairs: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> = BTreeSet::new();
    let total = endpoints.len();
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
        let current = i + 1;
        if should_report_layer_progress(current, total) {
            progress(HiddenRoutesProgress::LayerProgress {
                layer,
                current,
                total,
                pairs: pairs.len(),
            });
        }
    }

    let mut added = 0;
    let emit_total = pairs.len();
    for (idx, (from, to)) in pairs.into_iter().enumerate() {
        if existing.contains(&(from.clone(), to.clone())) {
            let current = idx + 1;
            if should_report_layer_progress(current, emit_total) {
                progress(HiddenRoutesProgress::LayerEmitProgress {
                    layer,
                    current,
                    total: emit_total,
                    added,
                });
            }
            continue;
        }
        let a = endpoint_by_id.get(from.as_str()).copied().unwrap();
        let b = endpoint_by_id.get(to.as_str()).copied().unwrap();
        let dist = hex_distance(a.coord, b.coord);
        let base_id = ids::route_id(&from, &to);
        let id = crate::ids::RouteId::new(format!("{base_id}-{suffix}"));
        // Avoid duplicate inserts if a hidden lane of the same kind
        // already exists for this pair (e.g. from a re-run on a save).
        if !existing_route_ids.insert(id.clone()) {
            let current = idx + 1;
            if should_report_layer_progress(current, emit_total) {
                progress(HiddenRoutesProgress::LayerEmitProgress {
                    layer,
                    current,
                    total: emit_total,
                    added,
                });
            }
            continue;
        }
        routes.push(GeneratedRoute {
            id,
            from_system_id: from,
            to_system_id: to,
            distance: dist,
            route_type: rtype,
            stability: RouteStability::Stable,
            tags: vec![format!("hidden:{suffix}").into()],
            controls: Vec::new(),
        });
        added += 1;
        let current = idx + 1;
        if should_report_layer_progress(current, emit_total) {
            progress(HiddenRoutesProgress::LayerEmitProgress {
                layer,
                current,
                total: emit_total,
                added,
            });
        }
    }
    progress(HiddenRoutesProgress::LayerCompleted {
        layer,
        added,
        routes: routes.len(),
    });
    added
}

fn should_report_layer_progress(current: usize, total: usize) -> bool {
    if total == 0 {
        return false;
    }
    current == 1 || current == total || current.is_multiple_of((total / 20).max(1))
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
        assert!(routes[0].tags.iter().any(|t| t.as_ref() == "hidden:webway"));
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

    #[test]
    fn configured_hidden_routes_use_explicit_endpoints_and_k() {
        let systems = vec![
            sys("sys-0001", (0, 0), ("eld", 0.0, 0.0)),
            sys("sys-0002", (1, 0), ("eld", 0.0, 0.0)),
            sys("sys-0003", (7, 7), ("eld", 0.0, 0.0)),
        ];
        let factions = vec![fac("eld", "aeldari")];
        let cfg = HiddenRoutesConfig {
            kind: RouteType::Webway,
            endpoints: systems.iter().map(|s| s.id.clone()).collect(),
            k_nearest: 1,
            exclude_blackout_regions: true,
        };
        let routes = configured_hidden_routes(&systems, &factions, &[], &[], &cfg);
        assert_eq!(routes.len(), 2);
        assert!(routes.iter().all(|r| r.route_type == RouteType::Webway));
    }

    #[test]
    fn configured_hidden_routes_exclude_blackout_endpoints() {
        let systems = vec![
            sys("sys-0001", (0, 0), ("eld", 0.0, 0.0)),
            sys("sys-0002", (1, 0), ("eld", 0.0, 0.0)),
        ];
        let factions = vec![fac("eld", "aeldari")];
        let regions = vec![crate::regions::WarpRegion {
            id: "blackout".into(),
            name: "Blackout".into(),
            kind: crate::regions::RegionConditionKind::Blackout,
            hexes: vec![HexCoord { q: 1, r: 0 }],
            centre: HexCoord { q: 1, r: 0 },
        }];
        let cfg = HiddenRoutesConfig {
            kind: RouteType::Webway,
            endpoints: systems.iter().map(|s| s.id.clone()).collect(),
            k_nearest: 1,
            exclude_blackout_regions: true,
        };
        let routes = configured_hidden_routes(&systems, &factions, &regions, &[], &cfg);
        assert!(routes.is_empty());
    }
}
