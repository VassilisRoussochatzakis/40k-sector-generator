//! Sector analytics & balance dashboard (§8 NEW.md).
//!
//! Read-only derivation over a finished `GeneratedSector`. No RNG, no I/O.
//! Produces a `SectorAnalysis` of faction balance, contested-world ratio,
//! route-graph connectivity, world-type distribution, and per-subsector
//! political variety, plus a configurable set of health flags.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::ids::{FactionId, SystemId};
use crate::sector_model::{GeneratedSector, GeneratedSystem};
use crate::subsectors::{build_subsectors, Subsector, SubsectorConfig};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzeConfig {
    /// Flag any faction whose share of total projection power exceeds this.
    /// Default 0.50.
    #[serde(default = "default_share")]
    pub warn_faction_share: f32,
    /// Flag if the route graph is disconnected.
    #[serde(default = "default_true")]
    pub warn_if_disconnected: bool,
    /// Flag every articulation point in the route graph.
    #[serde(default = "default_true")]
    pub warn_if_articulation: bool,
    /// Sectors with fewer systems than this mark connectivity metrics as
    /// low-confidence rather than flagging them.
    #[serde(default = "default_tiny")]
    pub tiny_sector_threshold: usize,
    /// Flag if the contested-world ratio exceeds this.
    #[serde(default = "default_contested")]
    pub warn_contested_ratio: f32,
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            warn_faction_share: default_share(),
            warn_if_disconnected: true,
            warn_if_articulation: true,
            tiny_sector_threshold: default_tiny(),
            warn_contested_ratio: default_contested(),
        }
    }
}

fn default_share() -> f32 {
    0.50
}
fn default_true() -> bool {
    true
}
fn default_tiny() -> usize {
    5
}
fn default_contested() -> f32 {
    0.66
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorAnalysis {
    pub sector_id: String,
    pub seed: String,
    pub system_count: usize,
    pub world_count: usize,
    pub route_count: usize,
    pub faction_count: usize,
    pub faction_balance: FactionBalance,
    pub contested_world_ratio: f32,
    pub avg_claims_per_world: f32,
    pub claim_kind_counts: BTreeMap<String, u32>,
    pub system_state_counts: BTreeMap<String, u32>,
    pub dominance_counts: BTreeMap<String, u32>,
    pub world_type_distribution: BTreeMap<String, u32>,
    pub star_colour_distribution: BTreeMap<String, u32>,
    pub population_distribution: BTreeMap<String, u32>,
    pub route_type_distribution: BTreeMap<String, u32>,
    pub route_stability_distribution: BTreeMap<String, u32>,
    pub connectivity: Connectivity,
    pub subsector_variety: Vec<SubsectorVariety>,
    pub health_flags: Vec<HealthFlag>,
    pub low_confidence: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionBalance {
    /// Gini coefficient across faction total-projection scores. 0 = perfect
    /// equality, 1 = total concentration. Empty/single-faction sectors give 0.
    pub gini: f32,
    /// Top factions by share of total projection, descending.
    pub top_factions: Vec<FactionShare>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactionShare {
    pub faction_id: FactionId,
    pub name: String,
    pub kind: String,
    pub total_projection: f32,
    pub share: f32,
    pub world_presence_count: u32,
    pub system_presence_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connectivity {
    pub component_count: u32,
    pub largest_component_size: u32,
    /// Diameter over the full route graph, in hops. None when disconnected.
    pub diameter_hops: Option<u32>,
    /// System ids whose removal would fragment the route graph.
    pub articulation_point_ids: Vec<SystemId>,
    /// Systems with zero incident routes.
    pub isolated_system_ids: Vec<SystemId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubsectorVariety {
    pub subsector_id: String,
    pub label: String,
    pub name: String,
    pub unique_dominants: u32,
    pub dominant_factions: Vec<FactionId>,
    pub contested_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFlag {
    pub severity: FlagSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlagSeverity {
    Info,
    Warning,
    Error,
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Pure derivation. Never fails.
#[must_use]
pub fn analyze(sector: &GeneratedSector) -> SectorAnalysis {
    analyze_with(sector, &AnalyzeConfig::default())
}

#[must_use]
pub fn analyze_with(sector: &GeneratedSector, cfg: &AnalyzeConfig) -> SectorAnalysis {
    let mut a = SectorAnalysis {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        system_count: sector.systems.len(),
        world_count: sector.systems.iter().map(|s| s.worlds.len()).sum(),
        route_count: sector.routes.len(),
        faction_count: sector.factions.len(),
        ..Default::default()
    };

    a.faction_balance = compute_faction_balance(sector);
    let (contested, total_worlds_with_factions, total_claims, claim_counts, dominance_counts) =
        compute_world_stats(sector);
    a.contested_world_ratio = if total_worlds_with_factions == 0 {
        0.0
    } else {
        contested as f32 / total_worlds_with_factions as f32
    };
    a.avg_claims_per_world = if a.world_count == 0 {
        0.0
    } else {
        total_claims as f32 / a.world_count as f32
    };
    a.claim_kind_counts = claim_counts;
    a.dominance_counts = dominance_counts;
    a.system_state_counts = compute_system_state_counts(sector);
    a.world_type_distribution = compute_distribution(sector, |w| w.world.world_type.to_string());
    a.star_colour_distribution = sector
        .systems
        .iter()
        .map(|s| s.star.colour_name.to_string())
        .fold(BTreeMap::new(), |mut m, k| {
            *m.entry(k).or_insert(0) += 1;
            m
        });
    a.population_distribution = compute_distribution(sector, |w| w.world.population.to_string());
    a.route_type_distribution = sector
        .routes
        .iter()
        .map(|r| format!("{:?}", r.route_type))
        .fold(BTreeMap::new(), |mut m, k| {
            *m.entry(k).or_insert(0) += 1;
            m
        });
    a.route_stability_distribution = sector
        .routes
        .iter()
        .map(|r| format!("{:?}", r.stability))
        .fold(BTreeMap::new(), |mut m, k| {
            *m.entry(k).or_insert(0) += 1;
            m
        });
    a.connectivity = compute_connectivity(sector);
    a.subsector_variety = compute_subsector_variety(sector);
    a.low_confidence = sector.systems.len() < cfg.tiny_sector_threshold;
    a.health_flags = evaluate_flags(&a, cfg);
    a
}

// ── Faction balance ─────────────────────────────────────────────────────────────

fn compute_faction_balance(sector: &GeneratedSector) -> FactionBalance {
    let totals: Vec<(usize, f32)> = sector
        .factions
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.power.total_projection()))
        .collect();
    let total: f32 = totals.iter().map(|(_, x)| *x).sum();
    let gini = gini_coefficient(&totals.iter().map(|(_, x)| *x).collect::<Vec<_>>());
    let mut shares: Vec<FactionShare> = totals
        .iter()
        .map(|(i, x)| {
            let f = &sector.factions[*i];
            FactionShare {
                faction_id: f.id.clone(),
                name: f.name.clone(),
                kind: f.kind.clone(),
                total_projection: *x,
                share: if total > 0.0 { *x / total } else { 0.0 },
                world_presence_count: f.world_presence.len() as u32,
                system_presence_count: f.system_presence.len() as u32,
            }
        })
        .collect();
    shares.sort_by(|a, b| {
        b.share
            .partial_cmp(&a.share)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.faction_id.cmp(&b.faction_id))
    });
    FactionBalance {
        gini,
        top_factions: shares,
    }
}

fn gini_coefficient(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.iter().copied().filter(|x| *x >= 0.0).collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() as f32;
    let total: f32 = sorted.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    // G = (2 * Σ i*x_i - (n + 1) * Σ x_i) / (n * Σ x_i), i in 1..=n
    let mut accum = 0.0;
    for (i, x) in sorted.iter().enumerate() {
        accum += (i as f32 + 1.0) * *x;
    }
    let g = (2.0 * accum - (n + 1.0) * total) / (n * total);
    g.clamp(0.0, 1.0)
}

// ── Per-world counters ─────────────────────────────────────────────────────────

fn compute_world_stats(
    sector: &GeneratedSector,
) -> (u32, u32, u32, BTreeMap<String, u32>, BTreeMap<String, u32>) {
    let mut contested = 0u32;
    let mut total_with_factions = 0u32;
    let mut total_claims = 0u32;
    let mut claim_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut dominance_counts: BTreeMap<String, u32> = BTreeMap::new();
    for sys in &sector.systems {
        for w in &sys.worlds {
            if !w.factions.is_empty() {
                total_with_factions += 1;
            }
            if w.control.contested {
                contested += 1;
            }
            total_claims += w.claims.len() as u32;
            for c in &w.claims {
                let key = format!("{:?}", c.claim_type);
                *claim_counts.entry(key).or_insert(0) += 1;
            }
            for p in &w.factions {
                let key = format!("{:?}", p.dominance);
                *dominance_counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    (
        contested,
        total_with_factions,
        total_claims,
        claim_counts,
        dominance_counts,
    )
}

fn compute_system_state_counts(sector: &GeneratedSector) -> BTreeMap<String, u32> {
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    for sys in &sector.systems {
        if let Some(state) = sys.control.state {
            let key = format!("{:?}", state);
            *out.entry(key).or_insert(0) += 1;
        }
    }
    out
}

fn compute_distribution<F>(sector: &GeneratedSector, key: F) -> BTreeMap<String, u32>
where
    F: Fn(&crate::sector_model::GeneratedWorld) -> String,
{
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    for sys in &sector.systems {
        for w in &sys.worlds {
            *out.entry(key(w)).or_insert(0) += 1;
        }
    }
    out
}

// ── Route graph ────────────────────────────────────────────────────────────────

fn build_adjacency(
    sector: &GeneratedSector,
) -> (Vec<SystemId>, BTreeMap<SystemId, usize>, Vec<Vec<usize>>) {
    let ids: Vec<SystemId> = sector.systems.iter().map(|s| s.id.clone()).collect();
    let index: BTreeMap<SystemId, usize> = ids.iter().cloned().zip(0..).collect();
    let n = ids.len();
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for r in &sector.routes {
        let (Some(&a), Some(&b)) = (index.get(&r.from_system_id), index.get(&r.to_system_id))
        else {
            continue;
        };
        if a == b {
            continue;
        }
        adj[a].insert(b);
        adj[b].insert(a);
    }
    let adj_vec: Vec<Vec<usize>> = adj.into_iter().map(|s| s.into_iter().collect()).collect();
    (ids, index, adj_vec)
}

fn compute_connectivity(sector: &GeneratedSector) -> Connectivity {
    let (ids, _, adj) = build_adjacency(sector);
    let n = ids.len();
    if n == 0 {
        return Connectivity::default();
    }
    // Components + isolated systems.
    let mut visited = vec![false; n];
    let mut components = 0u32;
    let mut largest = 0u32;
    let mut isolated = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        components += 1;
        let mut size = 0u32;
        let mut q: VecDeque<usize> = VecDeque::from([start]);
        visited[start] = true;
        while let Some(v) = q.pop_front() {
            size += 1;
            for &u in &adj[v] {
                if !visited[u] {
                    visited[u] = true;
                    q.push_back(u);
                }
            }
        }
        if size > largest {
            largest = size;
        }
        if size == 1 && adj[start].is_empty() {
            isolated.push(ids[start].clone());
        }
    }
    isolated.sort();

    let diameter = if components == 1 {
        Some(graph_diameter(&adj))
    } else {
        None
    };
    let mut articulation = articulation_points(&adj)
        .into_iter()
        .map(|i| ids[i].clone())
        .collect::<Vec<_>>();
    articulation.sort();
    Connectivity {
        component_count: components,
        largest_component_size: largest,
        diameter_hops: diameter,
        articulation_point_ids: articulation,
        isolated_system_ids: isolated,
    }
}

fn graph_diameter(adj: &[Vec<usize>]) -> u32 {
    let n = adj.len();
    let mut best = 0u32;
    for start in 0..n {
        let mut dist = vec![u32::MAX; n];
        dist[start] = 0;
        let mut q: VecDeque<usize> = VecDeque::from([start]);
        while let Some(v) = q.pop_front() {
            for &u in &adj[v] {
                if dist[u] == u32::MAX {
                    dist[u] = dist[v] + 1;
                    q.push_back(u);
                }
            }
        }
        for &d in &dist {
            if d != u32::MAX && d > best {
                best = d;
            }
        }
    }
    best
}

/// Tarjan's articulation-point DFS. Iterative to keep stack small for big sectors.
fn articulation_points(adj: &[Vec<usize>]) -> Vec<usize> {
    let n = adj.len();
    let mut disc = vec![-1i32; n];
    let mut low = vec![0i32; n];
    let mut parent = vec![-1i32; n];
    let mut is_ap = vec![false; n];
    let mut time = 0i32;
    // Iterative DFS: stack of (node, iter index into adj[node]).
    for start in 0..n {
        if disc[start] != -1 {
            continue;
        }
        disc[start] = time;
        low[start] = time;
        time += 1;
        let mut children_root = 0u32;
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(v, idx)) = stack.last() {
            if idx < adj[v].len() {
                let u = adj[v][idx];
                stack
                    .last_mut()
                    .expect("invariant: stack non-empty in loop guard")
                    .1 = idx + 1;
                if disc[u] == -1 {
                    parent[u] = v as i32;
                    disc[u] = time;
                    low[u] = time;
                    time += 1;
                    if v == start {
                        children_root += 1;
                    }
                    stack.push((u, 0));
                } else if u as i32 != parent[v] {
                    low[v] = low[v].min(disc[u]);
                }
            } else {
                // pop — propagate low to parent.
                stack.pop();
                if let Some(&(p, _)) = stack.last() {
                    low[p] = low[p].min(low[v]);
                    if parent[p] != -1 && low[v] >= disc[p] {
                        is_ap[p] = true;
                    }
                }
            }
        }
        if children_root > 1 {
            is_ap[start] = true;
        }
    }
    (0..n).filter(|i| is_ap[*i]).collect()
}

// ── Subsector political variety ────────────────────────────────────────────────

fn compute_subsector_variety(sector: &GeneratedSector) -> Vec<SubsectorVariety> {
    if sector.systems.is_empty() {
        return Vec::new();
    }
    let subs = match build_subsectors(sector, SubsectorConfig::default()) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let sys_by_id: BTreeMap<&str, &GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();
    subs.iter()
        .map(|sub: &Subsector| {
            let mut dominants: BTreeSet<FactionId> = BTreeSet::new();
            let mut contested_count = 0u32;
            for sid in &sub.system_ids {
                let Some(sys) = sys_by_id.get(sid.as_str()) else {
                    continue;
                };
                for w in &sys.worlds {
                    if let Some(d) = &w.control.dominant {
                        dominants.insert(d.clone());
                    }
                    if w.control.contested {
                        contested_count += 1;
                    }
                }
            }
            SubsectorVariety {
                subsector_id: sub.id.clone(),
                label: sub.label.clone(),
                name: sub.name.clone(),
                unique_dominants: dominants.len() as u32,
                dominant_factions: dominants.into_iter().collect(),
                contested_count,
            }
        })
        .collect()
}

// ── Health flags ───────────────────────────────────────────────────────────────

fn evaluate_flags(a: &SectorAnalysis, cfg: &AnalyzeConfig) -> Vec<HealthFlag> {
    let mut out = Vec::new();
    if let Some(top) = a.faction_balance.top_factions.first() {
        if top.share > cfg.warn_faction_share {
            out.push(HealthFlag {
                severity: FlagSeverity::Warning,
                code: "FACTION_DOMINANCE".to_string(),
                message: format!(
                    "{} ({}) holds {:.0}% of total faction projection (threshold {:.0}%)",
                    top.name,
                    top.faction_id,
                    top.share * 100.0,
                    cfg.warn_faction_share * 100.0
                ),
            });
        }
    }
    if cfg.warn_if_disconnected && a.connectivity.component_count > 1 {
        out.push(HealthFlag {
            severity: FlagSeverity::Warning,
            code: "ROUTE_GRAPH_DISCONNECTED".to_string(),
            message: format!(
                "route graph has {} components (largest = {})",
                a.connectivity.component_count, a.connectivity.largest_component_size
            ),
        });
    }
    if cfg.warn_if_articulation {
        for id in &a.connectivity.articulation_point_ids {
            out.push(HealthFlag {
                severity: FlagSeverity::Warning,
                code: "ARTICULATION_POINT".to_string(),
                message: format!("removing system {id} fragments the route graph"),
            });
        }
    }
    if a.contested_world_ratio > cfg.warn_contested_ratio {
        out.push(HealthFlag {
            severity: FlagSeverity::Info,
            code: "HIGH_CONTESTED_RATIO".to_string(),
            message: format!(
                "{:.0}% of inhabited worlds are contested (threshold {:.0}%)",
                a.contested_world_ratio * 100.0,
                cfg.warn_contested_ratio * 100.0
            ),
        });
    }
    if !a.connectivity.isolated_system_ids.is_empty() {
        out.push(HealthFlag {
            severity: FlagSeverity::Info,
            code: "ISOLATED_SYSTEMS".to_string(),
            message: format!(
                "{} system(s) have no incident routes: {}",
                a.connectivity.isolated_system_ids.len(),
                join_ids(&a.connectivity.isolated_system_ids)
            ),
        });
    }
    if a.low_confidence {
        out.push(HealthFlag {
            severity: FlagSeverity::Info,
            code: "TINY_SECTOR".to_string(),
            message: format!(
                "sector has only {} systems; connectivity metrics are low-confidence",
                a.system_count
            ),
        });
    }
    out
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(a: &SectorAnalysis) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "# Sector Analysis — {}", a.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", a.seed);
    let _ = writeln!(
        s,
        "\n- Systems: **{}**  ·  Worlds: **{}**  ·  Routes: **{}**  ·  Factions: **{}**",
        a.system_count, a.world_count, a.route_count, a.faction_count
    );
    if a.low_confidence {
        let _ = writeln!(
            s,
            "\n> ⚠ Low-confidence sector (small system count); structural metrics may be degenerate."
        );
    }

    // Faction balance.
    let _ = writeln!(s, "\n## Faction Balance");
    let _ = writeln!(s, "\nGini coefficient: **{:.3}**", a.faction_balance.gini);
    if !a.faction_balance.top_factions.is_empty() {
        const TOP_N: usize = 15;
        let total = a.faction_balance.top_factions.len();
        let _ = writeln!(
            s,
            "\nTop {} of {total} factions by share:",
            TOP_N.min(total)
        );
        let _ = writeln!(
            s,
            "\n| Faction | Kind | Share | Projection | Worlds | Systems |"
        );
        let _ = writeln!(s, "|---|---|---:|---:|---:|---:|");
        for f in a.faction_balance.top_factions.iter().take(TOP_N) {
            let _ = writeln!(
                s,
                "| {} (`{}`) | {} | {:.1}% | {:.1} | {} | {} |",
                f.name,
                f.faction_id,
                f.kind,
                f.share * 100.0,
                f.total_projection,
                f.world_presence_count,
                f.system_presence_count
            );
        }
        if total > TOP_N {
            let _ = writeln!(
                s,
                "\n*({} more factions omitted; full list in analysis.json)*",
                total - TOP_N
            );
        }
    }

    // World stats.
    let _ = writeln!(s, "\n## World & Claim Stats");
    let _ = writeln!(
        s,
        "\n- Contested world ratio: **{:.1}%**",
        a.contested_world_ratio * 100.0
    );
    let _ = writeln!(
        s,
        "- Avg claims per world: **{:.2}**",
        a.avg_claims_per_world
    );
    if !a.claim_kind_counts.is_empty() {
        let _ = writeln!(s, "\nClaim kinds:");
        for (k, v) in &a.claim_kind_counts {
            let _ = writeln!(s, "- {k}: {v}");
        }
    }
    if !a.dominance_counts.is_empty() {
        let _ = writeln!(s, "\nDominance buckets (per faction presence):");
        for (k, v) in &a.dominance_counts {
            let _ = writeln!(s, "- {k}: {v}");
        }
    }
    if !a.system_state_counts.is_empty() {
        let _ = writeln!(s, "\nSystem political states:");
        for (k, v) in &a.system_state_counts {
            let _ = writeln!(s, "- {k}: {v}");
        }
    }

    // Distributions.
    let _ = writeln!(s, "\n## Distributions");
    write_dist(&mut s, "World types", &a.world_type_distribution);
    write_dist(&mut s, "Star colours", &a.star_colour_distribution);
    write_dist(&mut s, "Populations", &a.population_distribution);
    write_dist(&mut s, "Route types", &a.route_type_distribution);
    write_dist(&mut s, "Route stability", &a.route_stability_distribution);

    // Connectivity.
    let c = &a.connectivity;
    let _ = writeln!(s, "\n## Route-Graph Connectivity");
    let _ = writeln!(
        s,
        "\n- Components: **{}**  ·  Largest: **{}**  ·  Diameter: {}",
        c.component_count,
        c.largest_component_size,
        match c.diameter_hops {
            Some(d) => format!("**{d}**"),
            None => "—".into(),
        }
    );
    if !c.articulation_point_ids.is_empty() {
        let _ = writeln!(
            s,
            "- Articulation points ({}): {}",
            c.articulation_point_ids.len(),
            join_ids(&c.articulation_point_ids)
        );
    }
    if !c.isolated_system_ids.is_empty() {
        let _ = writeln!(
            s,
            "- Isolated systems ({}): {}",
            c.isolated_system_ids.len(),
            join_ids(&c.isolated_system_ids)
        );
    }

    // Subsector variety.
    if !a.subsector_variety.is_empty() {
        let _ = writeln!(s, "\n## Subsector Political Variety");
        let _ = writeln!(s, "\n| Subsector | Unique Dominants | Contested Worlds |");
        let _ = writeln!(s, "|---|---:|---:|");
        for v in &a.subsector_variety {
            let _ = writeln!(
                s,
                "| {} — {} | {} | {} |",
                v.label, v.name, v.unique_dominants, v.contested_count
            );
        }
    }

    // Flags.
    let _ = writeln!(s, "\n## Health Flags");
    if a.health_flags.is_empty() {
        let _ = writeln!(s, "\n(none)");
    } else {
        for f in &a.health_flags {
            let tag = match f.severity {
                FlagSeverity::Info => "INFO",
                FlagSeverity::Warning => "WARN",
                FlagSeverity::Error => "ERROR",
            };
            let _ = writeln!(s, "- **[{tag}] {}** — {}", f.code, f.message);
        }
    }

    s
}

fn join_ids<T: AsRef<str>>(ids: &[T]) -> String {
    ids.iter()
        .map(|id| id.as_ref())
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_dist(s: &mut String, title: &str, map: &BTreeMap<String, u32>) {
    use std::fmt::Write as _;
    if map.is_empty() {
        return;
    }
    let total: u32 = map.values().sum();
    let _ = writeln!(s, "\n**{title}** (total {total}):");
    for (k, v) in map {
        let pct = if total == 0 {
            0.0
        } else {
            (*v as f32) * 100.0 / (total as f32)
        };
        let _ = writeln!(s, "- {k}: {v} ({pct:.1}%)");
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedFaction, GeneratedRoute, GeneratedStar, GeneratedSystem, GenerationManifest,
        HexCoord, PowerProfile, RouteStability, RouteType,
    };
    use std::collections::BTreeMap as Map;

    fn mini(width: u32, height: u32) -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width,
            height,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: Map::new(),
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
            chronicle: Default::default(),
        }
    }

    fn sys(id: &str, q: i32, r: i32) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
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
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    fn route(id: &str, a: &str, b: &str) -> GeneratedRoute {
        GeneratedRoute {
            id: id.into(),
            from_system_id: a.into(),
            to_system_id: b.into(),
            distance: 1,
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        }
    }

    #[test]
    fn gini_uniform_is_zero() {
        let v = vec![10.0_f32, 10.0, 10.0, 10.0];
        let g = gini_coefficient(&v);
        assert!(g.abs() < 1e-4, "gini was {g}");
    }

    #[test]
    fn gini_winner_take_all_near_one() {
        let v = vec![0.0_f32, 0.0, 0.0, 100.0];
        let g = gini_coefficient(&v);
        assert!(g > 0.7, "expected near-1, got {g}");
    }

    #[test]
    fn empty_sector_returns_defaults() {
        let s = mini(8, 8);
        let a = analyze(&s);
        assert_eq!(a.system_count, 0);
        assert_eq!(a.connectivity.component_count, 0);
        assert!(a.connectivity.diameter_hops.is_none());
    }

    #[test]
    fn articulation_point_detected_on_path_graph() {
        // sys-A — sys-B — sys-C : B is articulation.
        let mut s = mini(8, 1);
        s.systems = vec![sys("sys-A", 0, 0), sys("sys-B", 1, 0), sys("sys-C", 2, 0)];
        s.routes = vec![route("r1", "sys-A", "sys-B"), route("r2", "sys-B", "sys-C")];
        let a = analyze(&s);
        assert_eq!(a.connectivity.component_count, 1);
        assert_eq!(a.connectivity.diameter_hops, Some(2));
        assert_eq!(a.connectivity.articulation_point_ids, vec!["sys-B"]);
    }

    #[test]
    fn disconnected_components_yield_two() {
        let mut s = mini(8, 1);
        s.systems = vec![sys("sys-A", 0, 0), sys("sys-B", 1, 0), sys("sys-C", 5, 0)];
        s.routes = vec![route("r1", "sys-A", "sys-B")];
        let a = analyze(&s);
        assert_eq!(a.connectivity.component_count, 2);
        assert_eq!(a.connectivity.isolated_system_ids, vec!["sys-C"]);
        assert!(a.connectivity.diameter_hops.is_none());
    }

    #[test]
    fn faction_dominance_flag_fires() {
        let mut s = mini(4, 4);
        s.systems = vec![sys("sys-A", 0, 0)];
        let f1 = GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "Imperial".into(),
            disposition: "Order".into(),
            subfactions: Vec::new(),
            system_presence: vec!["sys-A".into()],
            world_presence: vec![],
            power: PowerProfile {
                administrative: 100.0,
                military: 100.0,
                naval: 100.0,
                ..Default::default()
            },
        };
        let f2 = GeneratedFaction {
            id: "rt".into(),
            name: "Rogue Trader".into(),
            kind: "RogueTrader".into(),
            disposition: "Opportunist".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile {
                economic: 5.0,
                ..Default::default()
            },
        };
        s.factions = vec![f1, f2];
        let a = analyze(&s);
        assert!(a.health_flags.iter().any(|f| f.code == "FACTION_DOMINANCE"));
    }
}
