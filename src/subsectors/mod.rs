//! Subsector grouping per `subsector_rust_spec_full.md`.
//!
//! Groups sector systems into rectangular 8×8 (configurable) tiles, classifies
//! routes as internal vs. border, resolves approximate faction control and
//! capitals, and produces deterministic per-tile summaries. Pure derivation —
//! `GeneratedSector` is not mutated.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::sector_model::{
    hex_distance, GeneratedRoute, GeneratedSector, GeneratedSystem, HexCoord,
};

/// Default target system count per subsector. Cluster count is derived as
/// `ceil(system_count / target)`.
pub const DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR: u32 = 12;
pub const DEFAULT_CLUSTER_ITERATIONS: u32 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subsector {
    pub id: String,
    pub sector_id: String,
    pub label: String,
    pub name: String,
    pub index: u32,
    pub row: u32,
    pub col: u32,
    pub bounds: SubsectorBounds,

    pub system_ids: Vec<crate::ids::SystemId>,
    /// Every (q,r) hex assigned to this subsector, including empty hexes.
    /// Drives map rendering of cluster boundaries.
    pub hex_cells: Vec<(u32, u32)>,
    pub route_ids_internal: Vec<crate::ids::RouteId>,
    pub route_ids_border: Vec<crate::ids::RouteId>,

    pub neighboring_subsector_ids: Vec<String>,
    pub connected_subsector_ids: Vec<String>,

    pub summary: SubsectorSummary,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SubsectorBounds {
    pub q_min: u32,
    pub q_max: u32,
    pub r_min: u32,
    pub r_max: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubsectorSummary {
    pub system_count: u32,
    pub world_count: u32,
    pub internal_route_count: u32,
    pub border_route_count: u32,

    pub primary_system_id: Option<crate::ids::SystemId>,
    pub subsector_capital_system_id: Option<crate::ids::SystemId>,
    pub subsector_capital_world_id: Option<crate::ids::WorldId>,
    pub controlling_faction_id: Option<crate::ids::FactionId>,

    pub dominant_factions: Vec<ScoredId>,
    pub faction_control: Vec<FactionControlSummary>,
    pub world_type_counts: BTreeMap<String, u32>,
    pub star_colour_counts: BTreeMap<String, u32>,
    pub population_counts: BTreeMap<String, u32>,
    pub tech_level_counts: BTreeMap<String, u32>,
    pub government_counts: BTreeMap<String, u32>,
    pub feature_counts: BTreeMap<String, u32>,
    pub route_type_counts: BTreeMap<String, u32>,
    pub route_stability_counts: BTreeMap<String, u32>,
    pub tag_counts: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredId {
    pub id: crate::ids::FactionId,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionControlSummary {
    pub faction_id: crate::ids::FactionId,
    pub owned_system_count: u32,
    pub owned_inhabited_system_count: u32,
    pub owned_world_count: u32,
    pub system_share_basis_points: u32,
    pub inhabited_system_share_basis_points: u32,
    pub world_share_basis_points: u32,
    pub control_score: i32,
    pub control_tier: String,
    pub contested_system_count: u32,
}

#[derive(Debug, Clone)]
pub struct SubsectorConfig {
    /// Average number of systems each cluster should contain. Cluster count
    /// `K = ceil(system_count / target_systems_per_subsector)`.
    pub target_systems_per_subsector: u32,
    /// Hard cap on Lloyd refinement iterations.
    pub max_iterations: u32,
    pub include_empty_subsectors: bool,
    pub faction_control_top_n: usize,
    pub tracked_faction_ids: Vec<String>,
    pub control_denominator: ControlDenominator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlDenominator {
    AllSystems,
    InhabitedSystems,
}

impl Default for SubsectorConfig {
    fn default() -> Self {
        Self {
            target_systems_per_subsector: DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR,
            max_iterations: DEFAULT_CLUSTER_ITERATIONS,
            include_empty_subsectors: true,
            faction_control_top_n: 5,
            tracked_faction_ids: Vec::new(),
            control_denominator: ControlDenominator::InhabitedSystems,
        }
    }
}

#[derive(Debug, Error)]
pub enum SubsectorBuildError {
    #[error("invalid sector dimensions: width={width} height={height}")]
    InvalidSectorDimensions { width: u32, height: u32 },
    #[error("invalid clustering target: target_systems_per_subsector must be >= 1")]
    InvalidClusterTarget,
    #[error("duplicate system id: {id}")]
    DuplicateSystemId { id: crate::ids::SystemId },
    #[error("system {id} coordinate ({q},{r}) is outside sector bounds")]
    CoordinateOutOfBounds {
        id: crate::ids::SystemId,
        q: i32,
        r: i32,
    },
    #[error("route {id} references unknown system {missing}")]
    RouteMissingEndpoint {
        id: crate::ids::RouteId,
        missing: crate::ids::SystemId,
    },
}

pub fn build_subsectors(
    sector: &GeneratedSector,
    config: SubsectorConfig,
) -> Result<Vec<Subsector>, SubsectorBuildError> {
    if sector.width == 0 || sector.height == 0 {
        return Err(SubsectorBuildError::InvalidSectorDimensions {
            width: sector.width,
            height: sector.height,
        });
    }
    if config.target_systems_per_subsector == 0 {
        return Err(SubsectorBuildError::InvalidClusterTarget);
    }

    // Coord + duplicate validation.
    let mut seen_system_ids: BTreeSet<crate::ids::SystemId> = BTreeSet::new();
    for sys in &sector.systems {
        if !seen_system_ids.insert(sys.id.clone()) {
            return Err(SubsectorBuildError::DuplicateSystemId { id: sys.id.clone() });
        }
        if sys.coord.q < 0
            || sys.coord.r < 0
            || (sys.coord.q as u32) >= sector.width
            || (sys.coord.r as u32) >= sector.height
        {
            return Err(SubsectorBuildError::CoordinateOutOfBounds {
                id: sys.id.clone(),
                q: sys.coord.q,
                r: sys.coord.r,
            });
        }
    }

    // ── Per-system precomputation used by clustering + capital scoring. ─────────
    let sys_by_id: BTreeMap<&str, &GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();
    let route_by_id: BTreeMap<&str, &GeneratedRoute> =
        sector.routes.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut route_degree: BTreeMap<&str, u32> = BTreeMap::new();
    let mut stable_route_degree: BTreeMap<&str, u32> = BTreeMap::new();
    for r in &sector.routes {
        *route_degree.entry(r.from_system_id.as_str()).or_default() += 1;
        *route_degree.entry(r.to_system_id.as_str()).or_default() += 1;
        if matches!(r.stability, crate::sector_model::RouteStability::Stable) {
            *stable_route_degree
                .entry(r.from_system_id.as_str())
                .or_default() += 1;
            *stable_route_degree
                .entry(r.to_system_id.as_str())
                .or_default() += 1;
        }
    }

    let owners = resolve_system_owners(&sector.systems);

    // ── Cluster systems. ────────────────────────────────────────────────────────
    let (assignment, seed_indices) = cluster_systems(sector, &route_degree, &config);

    // Build initial Subsector skeletons in seed order, then we'll relabel them
    // row-major over the capital coords once capitals are picked.
    let k = seed_indices.len();
    let mut cells: Vec<Subsector> = (0..k)
        .map(|i| {
            let seed_sys = &sector.systems[seed_indices[i]];
            Subsector {
                id: format!("subsector-tmp-{i}"),
                sector_id: sector.id.clone(),
                label: String::new(),
                name: format!("Subsector {}", seed_sys.name),
                index: i as u32,
                row: seed_sys.coord.r as u32,
                col: seed_sys.coord.q as u32,
                bounds: SubsectorBounds {
                    q_min: 0,
                    q_max: 0,
                    r_min: 0,
                    r_max: 0,
                },
                system_ids: Vec::new(),
                hex_cells: Vec::new(),
                route_ids_internal: Vec::new(),
                route_ids_border: Vec::new(),
                neighboring_subsector_ids: Vec::new(),
                connected_subsector_ids: Vec::new(),
                summary: SubsectorSummary::default(),
                tags: Vec::new(),
                notes: Vec::new(),
            }
        })
        .collect();

    // Populate system_ids per cluster.
    let mut system_to_cluster: BTreeMap<crate::ids::SystemId, usize> = BTreeMap::new();
    for (sys_idx, &cluster_idx) in assignment.iter().enumerate() {
        let sys = &sector.systems[sys_idx];
        cells[cluster_idx].system_ids.push(sys.id.clone());
        system_to_cluster.insert(sys.id.clone(), cluster_idx);
    }
    // Stable system order: by sector-level system.index, then id.
    let sys_index_by_id: BTreeMap<&str, usize> = sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.index))
        .collect();
    for cell in &mut cells {
        cell.system_ids.sort_by(|a, b| {
            let ai = sys_index_by_id
                .get(a.as_str())
                .copied()
                .unwrap_or(usize::MAX);
            let bi = sys_index_by_id
                .get(b.as_str())
                .copied()
                .unwrap_or(usize::MAX);
            ai.cmp(&bi).then_with(|| a.cmp(b))
        });
    }

    // Assign every hex (including empty ones) to nearest seed for visual borders.
    let hex_cluster = assign_hex_grid(sector, &seed_indices);
    for ((q, r), cluster_idx) in &hex_cluster {
        cells[*cluster_idx].hex_cells.push((*q, *r));
    }
    for cell in &mut cells {
        cell.hex_cells.sort();
        update_bounds(cell);
    }

    // Pick capital per cluster using existing scoring helper.
    for cell in &mut cells {
        let (cap_sys_id, cap_world_id) = pick_capital(
            cell,
            &sys_by_id,
            &route_degree,
            &stable_route_degree,
            &owners,
            None,
        );
        cell.summary.subsector_capital_system_id = cap_sys_id.clone();
        cell.summary.subsector_capital_world_id = cap_world_id;
        if let Some(id) = &cap_sys_id {
            if let Some(&sys) = sys_by_id.get(id.as_str()) {
                cell.name = format!("Subsector {}", sys.name);
            }
        }
    }

    // Relabel cells row-major by capital coord. Falls back to seed coord when
    // a cell has no capital.
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by(|&a, &b| {
        let ca = capital_or_seed_coord(&cells[a], &sys_by_id, &sector.systems, &seed_indices);
        let cb = capital_or_seed_coord(&cells[b], &sys_by_id, &sector.systems, &seed_indices);
        ca.1.cmp(&cb.1)
            .then_with(|| ca.0.cmp(&cb.0))
            .then_with(|| cells[a].name.cmp(&cells[b].name))
    });
    // Map old index → new label position.
    let mut relabeled: Vec<Subsector> = Vec::with_capacity(cells.len());
    for (new_idx, &old_idx) in order.iter().enumerate() {
        let mut cell = cells[old_idx].clone();
        let label = subsector_label(new_idx as u32);
        cell.label = label.clone();
        cell.index = new_idx as u32;
        // Stable, human-readable id derived from capital name when available.
        let id_seed = cell
            .summary
            .subsector_capital_system_id
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| label.to_ascii_lowercase());
        cell.id = format!("subsector-{}", slugify(&id_seed));
        relabeled.push(cell);
    }
    cells = relabeled;

    // Re-map system → cluster pointers from old indices to new positions.
    let new_index_by_old: BTreeMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(new_i, &old_i)| (old_i, new_i))
        .collect();
    for (_, ci) in system_to_cluster.iter_mut() {
        *ci = new_index_by_old[ci];
    }
    let hex_cluster_new: BTreeMap<(u32, u32), usize> = hex_cluster
        .into_iter()
        .map(|(k, v)| (k, new_index_by_old[&v]))
        .collect();

    // Route classification.
    for route in &sector.routes {
        let Some(&from_cell) = system_to_cluster.get(&route.from_system_id) else {
            return Err(SubsectorBuildError::RouteMissingEndpoint {
                id: route.id.clone(),
                missing: route.from_system_id.clone(),
            });
        };
        let Some(&to_cell) = system_to_cluster.get(&route.to_system_id) else {
            return Err(SubsectorBuildError::RouteMissingEndpoint {
                id: route.id.clone(),
                missing: route.to_system_id.clone(),
            });
        };
        if from_cell == to_cell {
            cells[from_cell].route_ids_internal.push(route.id.clone());
        } else {
            let to_id = cells[to_cell].id.clone();
            let from_id = cells[from_cell].id.clone();
            cells[from_cell].route_ids_border.push(route.id.clone());
            cells[to_cell].route_ids_border.push(route.id.clone());
            push_unique(&mut cells[from_cell].connected_subsector_ids, to_id);
            push_unique(&mut cells[to_cell].connected_subsector_ids, from_id);
        }
    }

    // Neighbor adjacency: any two clusters whose hexes share an edge.
    let neighbors = compute_neighbor_adjacency(&hex_cluster_new, sector, &cells);
    for (ci, neigh_ids) in neighbors.into_iter().enumerate() {
        cells[ci].neighboring_subsector_ids = neigh_ids;
        cells[ci].route_ids_internal.sort();
        cells[ci].route_ids_border.sort();
        cells[ci].connected_subsector_ids.sort();
    }

    // Summaries.
    for cell in &mut cells {
        populate_summary(
            cell,
            &sys_by_id,
            &route_by_id,
            &route_degree,
            &stable_route_degree,
            &owners,
            &config,
        );
    }

    if !config.include_empty_subsectors {
        cells.retain(|c| !c.system_ids.is_empty());
    }

    Ok(cells)
}

fn capital_or_seed_coord(
    cell: &Subsector,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    systems: &[GeneratedSystem],
    seed_indices: &[usize],
) -> (i32, i32) {
    if let Some(id) = &cell.summary.subsector_capital_system_id {
        if let Some(&sys) = sys_by_id.get(id.as_str()) {
            return (sys.coord.q, sys.coord.r);
        }
    }
    let seed = &systems[seed_indices[cell.index as usize]];
    (seed.coord.q, seed.coord.r)
}

fn update_bounds(cell: &mut Subsector) {
    if cell.hex_cells.is_empty() {
        cell.bounds = SubsectorBounds {
            q_min: 0,
            q_max: 0,
            r_min: 0,
            r_max: 0,
        };
        return;
    }
    let mut q_min = u32::MAX;
    let mut q_max = 0;
    let mut r_min = u32::MAX;
    let mut r_max = 0;
    for &(q, r) in &cell.hex_cells {
        q_min = q_min.min(q);
        q_max = q_max.max(q);
        r_min = r_min.min(r);
        r_max = r_max.max(r);
    }
    cell.bounds = SubsectorBounds {
        q_min,
        q_max,
        r_min,
        r_max,
    };
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

// ── Clustering ─────────────────────────────────────────────────────────────────

/// Greedy farthest-first seeding + Lloyd refinement over hex distance. Returns
/// `(assignment[sys_idx] = cluster_idx, seed_indices[cluster_idx] = sys_idx)`.
#[allow(clippy::needless_range_loop)]
fn cluster_systems(
    sector: &GeneratedSector,
    route_degree: &BTreeMap<&str, u32>,
    config: &SubsectorConfig,
) -> (Vec<usize>, Vec<usize>) {
    let n = sector.systems.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    let k = ((n as u32)
        .div_ceil(config.target_systems_per_subsector)
        .max(1) as usize)
        .min(n);

    // Pre-score every system as a capital seed candidate.
    let scores: Vec<i32> = sector
        .systems
        .iter()
        .map(|s| seed_score(s, route_degree))
        .collect();

    // First seed: highest score; tie-break by ascending system.index then id.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        scores[b]
            .cmp(&scores[a])
            .then_with(|| sector.systems[a].index.cmp(&sector.systems[b].index))
            .then_with(|| sector.systems[a].id.cmp(&sector.systems[b].id))
    });
    let mut seeds: Vec<usize> = vec![order[0]];
    while seeds.len() < k {
        // Maximize min hex distance to existing seeds, ties favor higher score.
        let mut best: Option<(i64, i32, usize, &str)> = None;
        for i in 0..n {
            if seeds.contains(&i) {
                continue;
            }
            let min_d = seeds
                .iter()
                .map(|&s| hex_distance(sector.systems[s].coord, sector.systems[i].coord) as i64)
                .min()
                .unwrap_or(0);
            let cand = (
                min_d,
                scores[i],
                sector.systems[i].index,
                sector.systems[i].id.as_str(),
            );
            let take = match &best {
                None => true,
                Some(b) => {
                    cand.0 > b.0
                        || (cand.0 == b.0 && cand.1 > b.1)
                        || (cand.0 == b.0 && cand.1 == b.1 && cand.2 < b.2)
                        || (cand.0 == b.0 && cand.1 == b.1 && cand.2 == b.2 && cand.3 < b.3)
                }
            };
            if take {
                best = Some(cand);
            }
        }
        match best {
            Some((_, _, idx, _)) => {
                let sys_idx = sector
                    .systems
                    .iter()
                    .position(|s| s.index == idx)
                    .unwrap_or(0);
                if seeds.contains(&sys_idx) {
                    break;
                }
                seeds.push(sys_idx);
            }
            None => break,
        }
    }

    // Lloyd refinement.
    let mut assignment = vec![0usize; n];
    for _iter in 0..config.max_iterations {
        // Assign each system to its nearest seed (hex distance).
        for i in 0..n {
            let mut best = (u32::MAX, usize::MAX);
            for (ci, &s) in seeds.iter().enumerate() {
                let d = hex_distance(sector.systems[s].coord, sector.systems[i].coord);
                if (d, ci) < (best.0, best.1) {
                    best = (d, ci);
                }
            }
            assignment[i] = best.1;
        }
        // Update seeds: highest-scoring member, tie by lowest sys.index.
        let mut new_seeds: Vec<usize> = Vec::with_capacity(k);
        for ci in 0..k {
            let members: Vec<usize> = (0..n).filter(|&i| assignment[i] == ci).collect();
            if members.is_empty() {
                new_seeds.push(seeds[ci]);
                continue;
            }
            let best = members
                .into_iter()
                .max_by(|&a, &b| {
                    scores[a]
                        .cmp(&scores[b])
                        .then_with(|| sector.systems[b].index.cmp(&sector.systems[a].index))
                        .then_with(|| sector.systems[b].id.cmp(&sector.systems[a].id))
                })
                .expect("invariant: members non-empty checked above");
            new_seeds.push(best);
        }
        if new_seeds == seeds {
            break;
        }
        seeds = new_seeds;
    }
    // Final assignment with the converged seeds.
    for i in 0..n {
        let mut best = (u32::MAX, usize::MAX);
        for (ci, &s) in seeds.iter().enumerate() {
            let d = hex_distance(sector.systems[s].coord, sector.systems[i].coord);
            if (d, ci) < (best.0, best.1) {
                best = (d, ci);
            }
        }
        assignment[i] = best.1;
    }
    (assignment, seeds)
}

/// Lightweight seed-quality score (route hub + populated worlds + world count).
/// Avoids the full prosperity computation since seeds get refined by Lloyd.
fn seed_score(sys: &GeneratedSystem, route_degree: &BTreeMap<&str, u32>) -> i32 {
    let deg = route_degree.get(sys.id.as_str()).copied().unwrap_or(0) as i32;
    let max_pop = sys
        .worlds
        .iter()
        .map(|w| population_rank(&w.world.population))
        .max()
        .unwrap_or(0);
    let max_tech = sys
        .worlds
        .iter()
        .map(|w| tech_rank(&w.world.tech_level))
        .max()
        .unwrap_or(0);
    deg * 4 + max_pop * 5 + max_tech * 2 + sys.worlds.len() as i32
}

/// Assign every hex in the sector to its nearest seed system, with stable
/// tie-breaking on cluster index. Returns a per-hex `(q,r) → cluster_idx` map.
fn assign_hex_grid(
    sector: &GeneratedSector,
    seed_indices: &[usize],
) -> BTreeMap<(u32, u32), usize> {
    let mut out = BTreeMap::new();
    let seed_coords: Vec<HexCoord> = seed_indices
        .iter()
        .map(|&i| sector.systems[i].coord)
        .collect();
    for r in 0..sector.height {
        for q in 0..sector.width {
            let mut best = (u32::MAX, usize::MAX);
            let here = HexCoord {
                q: q as i32,
                r: r as i32,
            };
            for (ci, sc) in seed_coords.iter().enumerate() {
                let d = hex_distance(*sc, here);
                if (d, ci) < (best.0, best.1) {
                    best = (d, ci);
                }
            }
            out.insert((q, r), best.1);
        }
    }
    out
}

/// For each cluster, build the sorted list of neighbor subsector ids: clusters
/// whose hexes share at least one pointy-top hex edge across the cluster border.
fn compute_neighbor_adjacency(
    hex_cluster: &BTreeMap<(u32, u32), usize>,
    sector: &GeneratedSector,
    cells: &[Subsector],
) -> Vec<Vec<String>> {
    let mut adjacency: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); cells.len()];
    for r in 0..sector.height {
        let neighbor_deltas = crate::sector_model::offset_r_neighbors(r as i32);
        for q in 0..sector.width {
            let Some(&here) = hex_cluster.get(&(q, r)) else {
                continue;
            };
            for (dq, dr) in &neighbor_deltas {
                let nq = q as i32 + dq;
                let nr = r as i32 + dr;
                if nq < 0 || nr < 0 || nq as u32 >= sector.width || nr as u32 >= sector.height {
                    continue;
                }
                let Some(&there) = hex_cluster.get(&(nq as u32, nr as u32)) else {
                    continue;
                };
                if there != here {
                    adjacency[here].insert(there);
                    adjacency[there].insert(here);
                }
            }
        }
    }
    adjacency
        .into_iter()
        .map(|set| {
            let mut v: Vec<String> = set.into_iter().map(|i| cells[i].id.clone()).collect();
            v.sort();
            v
        })
        .collect()
}

/// Row-major spreadsheet label: 0→A, 25→Z, 26→AA.
pub fn subsector_label(index: u32) -> String {
    let mut n = index as i64 + 1;
    let mut buf = Vec::new();
    while n > 0 {
        n -= 1;
        buf.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    buf.reverse();
    String::from_utf8(buf).expect("ascii")
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !v.iter().any(|x| x == &s) {
        v.push(s);
    }
}

mod summary;
use summary::{pick_capital, populate_summary, population_rank, resolve_system_owners, tech_rank};

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord, WorldDto,
    };

    fn mini_sector(width: u32, height: u32, systems: Vec<(i32, i32)>) -> GeneratedSector {
        let mut sys_vec = Vec::new();
        for (i, (q, r)) in systems.into_iter().enumerate() {
            let id = crate::ids::SystemId::new(format!("sys-{:04}", i + 1));
            let name = id.as_str().to_string();
            sys_vec.push(GeneratedSystem {
                id,
                index: i + 1,
                name,
                coord: HexCoord { q, r },
                star: GeneratedStar {
                    colour_code: "G".into(),
                    colour_name: "Yellow".into(),
                    spectral_type: None,
                    source_row_index: None,
                },
                worlds: vec![],
                primary_factions: vec![],
                tags: vec![],
                notes: vec![],
                control: Default::default(),
                stability: Default::default(),
                orbital_assets: Vec::new(),
                blockade: Default::default(),
                conflict: Default::default(),
                intel: Default::default(),
                archetype: Default::default(),
            });
        }
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width,
            height,
            systems: sys_vec,
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "test".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    fn _unused(_w: WorldDto) {}

    #[test]
    fn label_spreadsheet_extends_past_z() {
        assert_eq!(subsector_label(0), "A");
        assert_eq!(subsector_label(25), "Z");
        assert_eq!(subsector_label(26), "AA");
        assert_eq!(subsector_label(27), "AB");
        assert_eq!(subsector_label(51), "AZ");
        assert_eq!(subsector_label(52), "BA");
    }

    #[test]
    fn clustering_covers_every_system_once() {
        let sector = mini_sector(
            32,
            32,
            vec![(0, 0), (7, 7), (8, 0), (0, 8), (31, 31), (16, 16), (4, 20)],
        );
        let subs = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        let mut seen = BTreeSet::new();
        for s in &subs {
            for id in &s.system_ids {
                assert!(seen.insert(id.clone()), "duplicate system in clusters");
            }
        }
        assert_eq!(seen.len(), sector.systems.len());
    }

    #[test]
    fn cluster_count_scales_with_systems() {
        let mut coords = Vec::new();
        for r in 0..6 {
            for q in 0..6 {
                coords.push((q, r));
            }
        }
        // 36 systems, target 12 → expect 3 clusters.
        let sector = mini_sector(8, 8, coords);
        let subs = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        assert_eq!(subs.len(), 3);
    }

    #[test]
    fn empty_subsectors_dropped_when_requested_yields_one_for_single_system() {
        let sector = mini_sector(32, 32, vec![(0, 0)]);
        let cfg = SubsectorConfig {
            include_empty_subsectors: false,
            ..SubsectorConfig::default()
        };
        let subs = build_subsectors(&sector, cfg).unwrap();
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn subsector_named_after_capital_system() {
        let sector = mini_sector(8, 8, vec![(0, 0), (1, 1)]);
        let mut sector = sector;
        sector.systems[0].name = "Aurelia".to_string();
        sector.systems[1].name = "Bromios".to_string();
        let cfg = SubsectorConfig {
            target_systems_per_subsector: 1,
            ..SubsectorConfig::default()
        };
        let subs = build_subsectors(&sector, cfg).unwrap();
        // Each cluster should have one system, named after it.
        let names: BTreeSet<String> = subs.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains("Subsector Aurelia"));
        assert!(names.contains("Subsector Bromios"));
        // And the capital id matches that single system.
        for s in &subs {
            assert_eq!(s.system_ids.len(), 1);
            assert_eq!(
                s.summary.subsector_capital_system_id.as_deref(),
                Some(s.system_ids[0].as_str())
            );
        }
    }

    #[test]
    fn route_classification_internal_vs_border() {
        // Place two systems far apart so they end up in distinct clusters under
        // dynamic grouping. Target 1 system per cluster forces 1:1 partitioning.
        let mut sector = mini_sector(32, 32, vec![(0, 0), (1, 1), (20, 20)]);
        sector.routes.push(GeneratedRoute {
            id: "r-internal".into(),
            from_system_id: "sys-0001".into(),
            to_system_id: "sys-0002".into(),
            distance: 1,
            route_type: crate::sector_model::RouteType::ChartedPassage,
            stability: crate::sector_model::RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        });
        sector.routes.push(GeneratedRoute {
            id: "r-border".into(),
            from_system_id: "sys-0001".into(),
            to_system_id: "sys-0003".into(),
            distance: 20,
            route_type: crate::sector_model::RouteType::ChartedPassage,
            stability: crate::sector_model::RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        });
        // Target 2 systems per subsector so sys-0001 + sys-0002 cluster together,
        // sys-0003 forms its own cluster.
        let cfg = SubsectorConfig {
            target_systems_per_subsector: 2,
            ..SubsectorConfig::default()
        };
        let subs = build_subsectors(&sector, cfg).unwrap();
        let owner_of = |sid: &str| -> &Subsector {
            subs.iter()
                .find(|s| s.system_ids.iter().any(|x| x == sid))
                .unwrap()
        };
        let a = owner_of("sys-0001");
        let c = owner_of("sys-0003");
        assert_eq!(a.route_ids_internal, vec!["r-internal".to_string()]);
        assert_eq!(a.route_ids_border, vec!["r-border".to_string()]);
        assert_eq!(c.route_ids_border, vec!["r-border".to_string()]);
    }

    #[test]
    fn hex_cells_cover_entire_sector() {
        let sector = mini_sector(8, 8, vec![(0, 0), (7, 7), (4, 4)]);
        let subs = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        let total: usize = subs.iter().map(|s| s.hex_cells.len()).sum();
        assert_eq!(total, (sector.width * sector.height) as usize);
    }

    #[test]
    fn deterministic_output_for_same_input() {
        let sector = mini_sector(
            16,
            16,
            vec![(0, 0), (8, 0), (0, 8), (15, 15), (4, 4), (10, 10)],
        );
        let a = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        let b = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.label, y.label);
            assert_eq!(x.name, y.name);
            assert_eq!(x.system_ids, y.system_ids);
        }
    }

    #[test]
    fn invariants_count_consistency() {
        let sector = mini_sector(16, 16, vec![(0, 0), (8, 0), (0, 8), (15, 15)]);
        let subs = build_subsectors(&sector, SubsectorConfig::default()).unwrap();
        let total: u32 = subs.iter().map(|s| s.summary.system_count).sum();
        assert_eq!(total as usize, sector.systems.len());
    }
}
