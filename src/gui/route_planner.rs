//! Route Planner: pathfind between two systems over the existing route graph.
//!
//! Two metrics:
//!   * `Safest`   — Dijkstra with hazard weights (avoid Unstable/Hazardous/Dangerous).
//!   * `Shortest` — BFS over hops, all passable routes equal weight.
//!
//! `Perilous` routes are always treated as impassable.

use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use crate::sector_model::{GeneratedRoute, GeneratedSector, RouteStability, RouteType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Safest,
    Shortest,
}

impl Default for Metric {
    fn default() -> Self {
        Self::Safest
    }
}

#[derive(Default, Debug, Clone)]
pub struct RoutePlannerState {
    pub from: Option<String>,
    pub to: Option<String>,
    pub metric: Metric,
    pub plan: Option<Plan>,
    pub status: String,
    pub picker: PickTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickTarget {
    #[default]
    None,
    From,
    To,
}

#[derive(Debug, Clone)]
pub struct Plan {
    /// Ordered system ids from origin to destination, inclusive.
    pub hops: Vec<String>,
    /// Route ids used, in traversal order.
    pub route_ids: Vec<String>,
    /// Edge-by-edge hazards along the path.
    pub hazards: Vec<HazardNote>,
    /// Sum of hop-weights (Safest) or hop count (Shortest).
    pub total_cost: f64,
    pub metric: Metric,
}

#[derive(Debug, Clone)]
pub struct HazardNote {
    pub route_id: String,
    pub from: String,
    pub to: String,
    pub stability: RouteStability,
    pub route_type: RouteType,
    pub severity: Severity,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Caution,
    Danger,
}

impl RoutePlannerState {
    /// Map click. If a picker is armed, fill that slot then advance.
    /// Otherwise auto-cycle: from → to → reset.
    pub fn click_system(&mut self, system_id: &str) {
        match self.picker {
            PickTarget::From => {
                self.from = Some(system_id.to_string());
                self.picker = if self.to.is_none() {
                    PickTarget::To
                } else {
                    PickTarget::None
                };
            }
            PickTarget::To => {
                if self.from.as_deref() == Some(system_id) {
                    return;
                }
                self.to = Some(system_id.to_string());
                self.picker = PickTarget::None;
            }
            PickTarget::None => {
                if self.from.is_none() {
                    self.from = Some(system_id.to_string());
                } else if self.to.is_none() && self.from.as_deref() != Some(system_id) {
                    self.to = Some(system_id.to_string());
                } else {
                    self.from = Some(system_id.to_string());
                    self.to = None;
                    self.plan = None;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.from = None;
        self.to = None;
        self.plan = None;
        self.status.clear();
        self.picker = PickTarget::None;
    }

    pub fn highlighted_route_ids(&self) -> HashSet<String> {
        self.plan
            .as_ref()
            .map(|p| p.route_ids.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn waypoint_set(&self) -> HashSet<String> {
        self.plan
            .as_ref()
            .map(|p| p.hops.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Compute a route between two systems, or `None` if no path exists.
pub fn plan_route(sector: &GeneratedSector, from: &str, to: &str, metric: Metric) -> Option<Plan> {
    if from == to {
        return None;
    }
    let adj = build_adjacency(sector);
    match metric {
        Metric::Safest => dijkstra(&adj, from, to),
        Metric::Shortest => bfs(&adj, from, to),
    }
    .map(|(hops, route_ids, cost)| {
        let hazards = collect_hazards(sector, &route_ids);
        Plan {
            hops,
            route_ids,
            hazards,
            total_cost: cost,
            metric,
        }
    })
}

struct Edge<'a> {
    other: &'a str,
    route: &'a GeneratedRoute,
    weight: f64,
}

fn build_adjacency(sector: &GeneratedSector) -> HashMap<&str, Vec<Edge<'_>>> {
    let mut adj: HashMap<&str, Vec<Edge<'_>>> = HashMap::new();
    for r in &sector.routes {
        if matches!(r.stability, RouteStability::Perilous) {
            continue;
        }
        let w = edge_weight(r);
        adj.entry(&r.from_system_id).or_default().push(Edge {
            other: &r.to_system_id,
            route: r,
            weight: w,
        });
        adj.entry(&r.to_system_id).or_default().push(Edge {
            other: &r.from_system_id,
            route: r,
            weight: w,
        });
    }
    adj
}

fn edge_weight(r: &GeneratedRoute) -> f64 {
    let mut w = match r.stability {
        RouteStability::Stable => 1.0,
        RouteStability::Unstable => 3.0,
        RouteStability::Hazardous => 8.0,
        RouteStability::Perilous => f64::INFINITY,
    };
    if matches!(r.route_type, RouteType::DangerousPassage) {
        w += 2.0;
    }
    if matches!(r.route_type, RouteType::SecretPassage) {
        w += 1.0;
    }
    w
}

#[derive(PartialEq)]
struct HeapNode<'a> {
    cost: f64,
    node: &'a str,
}

impl<'a> Eq for HeapNode<'a> {}

impl<'a> PartialOrd for HeapNode<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for HeapNode<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: invert cost ordering. NaN-safe via total_cmp on the bits.
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

fn dijkstra<'a>(
    adj: &'a HashMap<&'a str, Vec<Edge<'a>>>,
    from: &str,
    to: &str,
) -> Option<(Vec<String>, Vec<String>, f64)> {
    if !adj.contains_key(from) || !adj.contains_key(to) {
        return None;
    }
    let mut dist: HashMap<&str, f64> = HashMap::new();
    let mut prev: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut heap: BinaryHeap<HeapNode<'a>> = BinaryHeap::new();
    let start: &str = adj.get_key_value(from).unwrap().0;
    dist.insert(start, 0.0);
    heap.push(HeapNode {
        cost: 0.0,
        node: start,
    });
    while let Some(HeapNode { cost, node }) = heap.pop() {
        if node == to {
            return Some(reconstruct(node, &prev, cost));
        }
        if cost > *dist.get(node).unwrap_or(&f64::INFINITY) {
            continue;
        }
        let Some(edges) = adj.get(node) else { continue };
        for e in edges {
            let nd = cost + e.weight;
            if nd < *dist.get(e.other).unwrap_or(&f64::INFINITY) {
                dist.insert(e.other, nd);
                prev.insert(e.other, (node, &e.route.id));
                heap.push(HeapNode {
                    cost: nd,
                    node: e.other,
                });
            }
        }
    }
    None
}

fn bfs<'a>(
    adj: &'a HashMap<&'a str, Vec<Edge<'a>>>,
    from: &str,
    to: &str,
) -> Option<(Vec<String>, Vec<String>, f64)> {
    if !adj.contains_key(from) || !adj.contains_key(to) {
        return None;
    }
    let start: &str = adj.get_key_value(from).unwrap().0;
    let mut prev: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(start);
    let mut q: VecDeque<&str> = VecDeque::new();
    q.push_back(start);
    while let Some(node) = q.pop_front() {
        if node == to {
            let (hops, routes, _) = reconstruct(node, &prev, 0.0);
            let cost = (hops.len().saturating_sub(1)) as f64;
            return Some((hops, routes, cost));
        }
        let Some(edges) = adj.get(node) else { continue };
        for e in edges {
            if visited.insert(e.other) {
                prev.insert(e.other, (node, &e.route.id));
                q.push_back(e.other);
            }
        }
    }
    None
}

fn reconstruct(
    end: &str,
    prev: &HashMap<&str, (&str, &str)>,
    cost: f64,
) -> (Vec<String>, Vec<String>, f64) {
    let mut hops_rev: Vec<String> = vec![end.to_string()];
    let mut routes_rev: Vec<String> = Vec::new();
    let mut cur = end;
    while let Some((p, route_id)) = prev.get(cur) {
        hops_rev.push((*p).to_string());
        routes_rev.push((*route_id).to_string());
        cur = p;
    }
    hops_rev.reverse();
    routes_rev.reverse();
    (hops_rev, routes_rev, cost)
}

fn collect_hazards(sector: &GeneratedSector, route_ids: &[String]) -> Vec<HazardNote> {
    let by_id: HashMap<&str, &GeneratedRoute> =
        sector.routes.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut out = Vec::new();
    for rid in route_ids {
        let Some(r) = by_id.get(rid.as_str()) else {
            continue;
        };
        let (severity, note) = classify(r);
        if severity == Severity::Info {
            continue;
        }
        out.push(HazardNote {
            route_id: r.id.clone(),
            from: r.from_system_id.clone(),
            to: r.to_system_id.clone(),
            stability: r.stability,
            route_type: r.route_type,
            severity,
            note,
        });
    }
    out
}

fn classify(r: &GeneratedRoute) -> (Severity, String) {
    let stab_label = match r.stability {
        RouteStability::Stable => "stable",
        RouteStability::Unstable => "unstable warp currents",
        RouteStability::Hazardous => "hazardous — high attrition",
        RouteStability::Perilous => "perilous",
    };
    let type_label = match r.route_type {
        RouteType::StableWarpLane => None,
        RouteType::ChartedPassage => None,
        RouteType::DangerousPassage => Some("dangerous passage"),
        RouteType::SecretPassage => Some("secret passage"),
    };
    let sev = match (r.stability, r.route_type) {
        (RouteStability::Hazardous, _) | (_, RouteType::DangerousPassage) => Severity::Danger,
        (RouteStability::Unstable, _) | (_, RouteType::SecretPassage) => Severity::Caution,
        _ => Severity::Info,
    };
    let mut parts = vec![stab_label.to_string()];
    if let Some(t) = type_label {
        parts.push(t.to_string());
    }
    if !r.tags.is_empty() {
        parts.push(format!("tags: {}", r.tags.join(", ")));
    }
    (sev, parts.join("; "))
}
