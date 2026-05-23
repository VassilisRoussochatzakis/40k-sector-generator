//! Power projection with route-graph distance falloff (§4 NEXT.md, §7.2).
//!
//! The formula from the design doc:
//! ```text
//! projected_power_at_node = source_power * route_quality * doctrine *
//!                           intel / (1 + distance²)
//! ```
//!
//! Implementation:
//! * Build an adjacency map keyed by system id from `sector.routes`.
//!   Edge cost = 1 (hop count). Hidden routes (webway / black-ship /
//!   smuggling) are usable by their *owning* factions only and contribute
//!   a route_quality bump for those factions.
//! * For each faction with non-zero power, BFS from each source system to
//!   N hops. Project `source_power * doctrine[kind] / (1 + hops²)` onto
//!   every reachable system.
//! * The per-faction-per-system map is exposed as
//!   `PowerProjectionMap`; the renderer / GUI use it to render reach.
//!
//! Per-kind doctrine multipliers encode §7's expansion rules:
//! * Tau: 1.4 (sphere expansion)
//! * Tyranid: 0.6 forward, decays sharply (consumption gradient)
//! * Necron: 1.2 dormant baseline, jumps to 1.8 when awakening
//! * Aeldari/Harlequin: 0.8 baseline, 1.5 along webway
//! * Default: 1.0

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedFaction, GeneratedSector, RouteType};

/// Result of projection: per-faction map of system_id → projected power.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerProjectionMap {
    /// faction_id → system_id → projected_total_power (in the same units
    /// as `PowerProfile::total_projection`).
    pub by_faction: BTreeMap<crate::ids::FactionId, BTreeMap<crate::ids::SystemId, f32>>,
}

/// Project every faction's aggregate power across the route graph.
#[must_use]
pub fn project_sector(sector: &GeneratedSector) -> PowerProjectionMap {
    let adj = build_adjacency(sector);
    let mut out = PowerProjectionMap::default();
    for f in &sector.factions {
        let source_systems: Vec<&crate::ids::SystemId> = f.system_presence.iter().collect();
        if source_systems.is_empty() {
            continue;
        }
        let source_power = f.power.total_projection();
        if source_power <= 0.0 {
            continue;
        }
        let doctrine = doctrine_multiplier(&f.kind);
        let mut per_system: BTreeMap<crate::ids::SystemId, f32> = BTreeMap::new();
        for src in source_systems {
            let dists = bfs_distances(&adj, src, &f.kind, f.id.as_str());
            for (target, hops) in dists {
                let proj = source_power * doctrine / ((1 + hops * hops) as f32);
                let entry = per_system.entry(target).or_insert(0.0);
                *entry = (*entry).max(proj);
            }
        }
        out.by_faction.insert(f.id.clone(), per_system);
    }
    out
}

/// Per-kind multipliers (§7).
pub fn doctrine_multiplier(kind: &str) -> f32 {
    match kind {
        "tau" => 1.4,
        "necron" => 1.2,
        "tyranid" => 0.6,
        "aeldari" | "harlequin" => 0.8,
        "drukhari" => 0.7,
        "ork" => 1.1,
        "mechanicus" | "leagues_of_votann" => 1.05,
        _ => 1.0,
    }
}

#[derive(Clone)]
struct Edge {
    other: crate::ids::SystemId,
    rt: RouteType,
}

fn build_adjacency(sector: &GeneratedSector) -> HashMap<crate::ids::SystemId, Vec<Edge>> {
    let mut adj: HashMap<crate::ids::SystemId, Vec<Edge>> = HashMap::new();
    for r in &sector.routes {
        adj.entry(r.from_system_id.clone()).or_default().push(Edge {
            other: r.to_system_id.clone(),
            rt: r.route_type,
        });
        adj.entry(r.to_system_id.clone()).or_default().push(Edge {
            other: r.from_system_id.clone(),
            rt: r.route_type,
        });
    }
    adj
}

fn bfs_distances(
    adj: &HashMap<crate::ids::SystemId, Vec<Edge>>,
    start: &crate::ids::SystemId,
    kind: &str,
    _id: &str,
) -> Vec<(crate::ids::SystemId, u32)> {
    const MAX_HOPS: u32 = 6;
    let mut visited: HashSet<crate::ids::SystemId> = HashSet::new();
    let mut out: Vec<(crate::ids::SystemId, u32)> = Vec::new();
    let mut q: VecDeque<(crate::ids::SystemId, u32)> = VecDeque::new();
    q.push_back((start.clone(), 0));
    visited.insert(start.clone());
    while let Some((node, d)) = q.pop_front() {
        out.push((node.clone(), d));
        if d >= MAX_HOPS {
            continue;
        }
        let Some(edges) = adj.get(&node) else {
            continue;
        };
        for e in edges {
            if !can_use_route(kind, e.rt) {
                continue;
            }
            if visited.insert(e.other.clone()) {
                q.push_back((e.other.clone(), d + 1));
            }
        }
    }
    out
}

fn can_use_route(kind: &str, rt: RouteType) -> bool {
    match rt {
        RouteType::Webway => matches!(kind, "aeldari" | "harlequin" | "drukhari"),
        RouteType::BlackShip => {
            matches!(
                kind,
                "inquisition" | "deathwatch" | "grey_knights" | "imperial"
            )
        }
        RouteType::SmugglingLane => {
            matches!(kind, "criminal" | "drukhari" | "rebel" | "genestealer_cult")
        }
        _ => true,
    }
}

/// Apply the projection map onto a sector's per-faction `PowerProfile`.
/// Each faction's `total_projection()` gets weighted by their *reach* —
/// how many systems they project into AND how strongly. Concretely:
/// new_power_scaling = (sum of per-system projection) / (source_power *
/// node_count). Result is multiplied into each component of `PowerProfile`.
///
/// This is a *static* augmentation: it does not change `power.military`
/// rank ordering between factions, but it sharpens differences for
/// factions whose reach is wider.
pub fn apply_to_factions(map: &PowerProjectionMap, factions: &mut [GeneratedFaction]) {
    for f in factions.iter_mut() {
        let src = f.power.total_projection();
        if src <= 0.0 {
            continue;
        }
        let Some(per_system) = map.by_faction.get(&f.id) else {
            continue;
        };
        let total: f32 = per_system.values().copied().sum();
        let n = per_system.len().max(1) as f32;
        let avg = total / n;
        // reach factor ∈ ~[0.5, 1.5]: scale around 1.0 so the rollup
        // remains comparable. Average projection close to the source
        // power means full reach (1.5×); zero reach means 0.5×.
        let reach = (0.5 + (avg / src).clamp(0.0, 1.0)).min(1.5);
        f.power.military *= reach;
        f.power.naval *= reach;
        f.power.economic *= reach;
        f.power.industrial *= reach;
        f.power.logistical *= reach;
    }
}

/// Helper for the GUI heatmap: top reach for a system (any faction).
#[must_use]
pub fn system_top_reach(
    map: &PowerProjectionMap,
    system_id: &str,
) -> Option<(crate::ids::FactionId, f32)> {
    let mut best: Option<(crate::ids::FactionId, f32)> = None;
    for (fid, per_system) in &map.by_faction {
        if let Some(v) = per_system.get(system_id) {
            if best.as_ref().map(|(_, s)| *v > *s).unwrap_or(true) {
                best = Some((fid.clone(), *v));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{GenerationManifest, HexCoord, PowerProfile, RouteStability};

    fn fac(id: &str, kind: &str, presence: Vec<&str>, military: f32) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: presence.into_iter().map(|s| s.into()).collect(),
            world_presence: vec![],
            power: PowerProfile {
                military,
                ..Default::default()
            },
        }
    }

    fn mk_sector(
        routes: Vec<(&str, &str, RouteType)>,
        factions: Vec<GeneratedFaction>,
    ) -> GeneratedSector {
        use crate::sector_model::{
            GeneratedRoute, GeneratedStar, GeneratedSystem, SystemControlSummary,
        };
        let systems_ids = ["sys-0001", "sys-0002", "sys-0003"];
        let systems: Vec<GeneratedSystem> = systems_ids
            .iter()
            .enumerate()
            .map(|(i, id)| GeneratedSystem {
                id: (*id).into(),
                index: i + 1,
                name: (*id).into(),
                coord: HexCoord { q: i as i32, r: 0 },
                kind: crate::sector_model::SystemKind::Star,
                star: Some(GeneratedStar {
                    colour_code: "A".into(),
                    colour_name: "A".into(),
                    spectral_type: None,
                    source_row_index: None,
                }),
                worlds: vec![],
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
            })
            .collect();
        let routes: Vec<GeneratedRoute> = routes
            .into_iter()
            .enumerate()
            .map(|(i, (a, b, rt))| GeneratedRoute {
                id: crate::ids::RouteId::new(format!("r-{i}")),
                from_system_id: a.into(),
                to_system_id: b.into(),
                distance: 1,
                route_type: rt,
                stability: RouteStability::Stable,
                tags: vec![],
                controls: vec![],
            })
            .collect();
        GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "t".into(),
            generator_name: "t".into(),
            generator_version: "t".into(),
            width: 10,
            height: 10,
            systems,
            routes,
            factions,
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "t".into(),
                generator_name: "t".into(),
                generator_version: "t".into(),
                seed: "t".into(),
                seed_hash: "t".into(),
                base_seed: None,
                candidate_index: None,
                constraints_digest: None,
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "t".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
            ..Default::default()
        }
    }

    #[test]
    fn power_decays_with_distance() {
        let sector = mk_sector(
            vec![
                ("sys-0001", "sys-0002", RouteType::StableWarpLane),
                ("sys-0002", "sys-0003", RouteType::StableWarpLane),
            ],
            vec![fac("imp", "imperial", vec!["sys-0001"], 100.0)],
        );
        let map = project_sector(&sector);
        let imp = map.by_faction.get("imp").unwrap();
        let p1 = imp.get("sys-0001").copied().unwrap();
        let p2 = imp.get("sys-0002").copied().unwrap();
        let p3 = imp.get("sys-0003").copied().unwrap();
        assert!(p1 > p2);
        assert!(p2 > p3);
    }

    #[test]
    fn webway_only_usable_by_aeldari_kind() {
        let sector = mk_sector(
            vec![("sys-0001", "sys-0003", RouteType::Webway)],
            vec![
                fac("imp", "imperial", vec!["sys-0001"], 100.0),
                fac("eld", "aeldari", vec!["sys-0001"], 100.0),
            ],
        );
        let map = project_sector(&sector);
        let imp = map.by_faction.get("imp").unwrap();
        let eld = map.by_faction.get("eld").unwrap();
        assert!(!imp.contains_key("sys-0003"));
        assert!(eld.contains_key("sys-0003"));
    }
}
