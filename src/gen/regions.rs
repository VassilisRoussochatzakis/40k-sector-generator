//! Regional warp phenomena & large-scale map overlay (§5 NEW.md).
//!
//! A deterministic `regions` stage runs *before* route generation so route
//! weighting / classification can react to the regional overlay. Region
//! footprints are seeded blob growths from a small number of region centres
//! over the existing hex grid; each region carries a [`RegionCondition`] from
//! the catalogue that:
//!
//! * adjusts the stability of routes whose endpoints fall under the region,
//! * reweights world-generation candidate pools nearby (advisory,
//!   surfaced as tags on affected worlds),
//! * tints map output with a translucent colour overlay.
//!
//! The catalogue ships as built-in defaults; users may extend it via
//! `regions.toml` referenced by `inputs.regions` in `sectorforge.toml`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{
    hex_distance, offset_r_neighbors, GeneratedRoute, GeneratedSystem, HexCoord, RouteStability,
};

// ── Catalogue & conditions ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RegionConditionKind {
    /// Routes crossing become Perilous unless that would isolate the navigable
    /// route graph; bridge lanes are capped at Hazardous.
    WarpStorm,
    /// Routes crossing degrade by one hazard tier.
    Turbulence,
    /// Routes crossing upgrade by one tier; ignores distance falloff one tier.
    CalmCorridor,
    /// No hidden / covert route generation inside.
    Blackout,
    /// World-generation candidate weights biased toward ancient-ruins /
    /// warp-phenomena candidates nearby.
    Anomaly,
    /// Dead worlds, ancient mausoleums. Route stability normal but eerie.
    NecropolisDrift,
    /// Ancient navigation beacons. Routes upgrade by one tier (like CalmCorridor).
    BeaconChain,
    /// Veil between realspace and warp is thin. Routes degrade by one tier (like Turbulence).
    EmpyricBleed,
}

impl RegionConditionKind {
    /// All variants in stable order. Used by builder pickers (§REG1, §REG5).
    pub const ALL: &'static [RegionConditionKind] = &[
        Self::WarpStorm,
        Self::Turbulence,
        Self::CalmCorridor,
        Self::Blackout,
        Self::Anomaly,
        Self::NecropolisDrift,
        Self::BeaconChain,
        Self::EmpyricBleed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::WarpStorm => "Warp Storm",
            Self::Turbulence => "Turbulence",
            Self::CalmCorridor => "Calm Corridor",
            Self::Blackout => "Blackout",
            Self::Anomaly => "Anomaly",
            Self::NecropolisDrift => "Necropolis Drift",
            Self::BeaconChain => "Beacon Chain",
            Self::EmpyricBleed => "Empyric Bleed",
        }
    }

    /// §REG7: single-char glyph used by the Markdown sector map overlay and
    /// the builder REGIONS-tab ASCII preview.
    pub fn glyph(self) -> char {
        match self {
            Self::WarpStorm => '~',
            Self::Turbulence => '^',
            Self::CalmCorridor => '=',
            Self::Blackout => '#',
            Self::Anomaly => '*',
            Self::NecropolisDrift => '%',
            Self::BeaconChain => '+',
            Self::EmpyricBleed => '?',
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::WarpStorm => "A raging empyric front that bleeds into realspace. Routes crossing or skirting this region are perilous. Navigation is harsh, failure carries dramatic consequences.",
            Self::Turbulence => "An unstable region where currents buck and twist. Routes degrade by one tier (longer times, higher costs, more mishaps).",
            Self::CalmCorridor => "A precious stretch of stable warp-space. Routes upgrade by one tier (smoother translation, predictable arrivals).",
            Self::Blackout => "An area where augury and deep-range surveys fail. No hidden routes exist inside; its internal structure resists discovery.",
            Self::Anomaly => "Ancient ruins, impossible physics, or ghost signals. World generation leans toward abandoned megastructures or reality-warping landmarks.",
            Self::NecropolisDrift => "Scattered with dead worlds and void mausoleums. Travel is unnerving. World generation leans toward graveyard planets or reliquary sites.",
            Self::BeaconChain => "Anchored by ancient navigation beacons. Routes are usually reliable (upgrades stability), but beacons attract pilgrims, raiders, and Imperial authorities.",
            Self::EmpyricBleed => "The veil between realspace and the Warp is thin. Impossible lights and psychic echoes leak into normal space. Routes may degrade by one tier.",
        }
    }

    /// Route-effect lattice precedence (§5 NEW.md):
    /// `WarpStorm` (force Perilous) overrides `Turbulence` (one tier worse)
    /// which overrides `CalmCorridor` (one tier better). Higher = stronger.
    pub fn route_precedence(self) -> i32 {
        match self {
            Self::WarpStorm => 3,
            Self::Turbulence | Self::EmpyricBleed => 2,
            Self::CalmCorridor | Self::BeaconChain => 1,
            _ => 0,
        }
    }

    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::WarpStorm => "warp_storm",
            Self::Turbulence => "turbulence",
            Self::CalmCorridor => "calm_corridor",
            Self::Blackout => "blackout",
            Self::Anomaly => "anomaly",
            Self::NecropolisDrift => "necropolis_drift",
            Self::BeaconChain => "beacon_chain",
            Self::EmpyricBleed => "empyric_bleed",
        }
    }
}

impl core::fmt::Display for RegionConditionKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RegionsFile {
    #[serde(default)]
    pub regions: RegionsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegionsConfig {
    /// Whether the stage runs. Defaults to `false` so existing projects keep
    /// byte-identical output until opt-in.
    #[serde(default)]
    pub enabled: bool,
    /// Approximate count of regions to grow. Clamped to fit the grid.
    #[serde(default = "default_count")]
    pub count: u32,
    /// Mean footprint in hexes per region. Clamped to >=1.
    #[serde(default = "default_size")]
    pub mean_size: u32,
    /// Whether region effects are *advisory* (annotate routes/worlds) or
    /// *hard* (rewrite route stability). Defaults to hard so the effect is
    /// visible on first inspection.
    #[serde(default = "default_true")]
    pub apply_to_routes: bool,
    /// User-defined condition pool; entries are sampled by `weight`.
    /// Built-in defaults apply when this list is empty.
    #[serde(default)]
    pub conditions: Vec<ConditionEntry>,
}

impl Default for RegionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            count: default_count(),
            mean_size: default_size(),
            apply_to_routes: true,
            conditions: Vec::new(),
        }
    }
}

fn default_count() -> u32 {
    2
}
fn default_size() -> u32 {
    6
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConditionEntry {
    pub kind: RegionConditionKind,
    #[serde(default = "default_weight")]
    pub weight: f64,
    /// Optional explicit label override.
    #[serde(default)]
    pub label: Option<String>,
}

fn default_weight() -> f64 {
    1.0
}

fn default_condition_pool() -> Vec<ConditionEntry> {
    vec![
        ConditionEntry {
            kind: RegionConditionKind::WarpStorm,
            weight: 2.0,
            label: None,
        },
        ConditionEntry {
            kind: RegionConditionKind::Turbulence,
            weight: 3.0,
            label: None,
        },
        ConditionEntry {
            kind: RegionConditionKind::CalmCorridor,
            weight: 1.5,
            label: None,
        },
        ConditionEntry {
            kind: RegionConditionKind::Blackout,
            weight: 1.0,
            label: None,
        },
        ConditionEntry {
            kind: RegionConditionKind::Anomaly,
            weight: 1.0,
            label: None,
        },
    ]
}

// ── Output DTO ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarpRegion {
    pub id: String,
    pub name: String,
    pub kind: RegionConditionKind,
    /// Footprint in odd-r offset hex coordinates.
    pub hexes: Vec<HexCoord>,
    /// Centre hex of the seed blob (informational).
    pub centre: HexCoord,
}

// ── Loader ─────────────────────────────────────────────────────────────────────

/// Load `regions.toml` from disk. Missing file → defaults (disabled).
///
/// # Errors
///
/// Returns [`SectorError::ConfigParse`] on a malformed file and
/// [`SectorError::Io`] on read failure.
pub fn load_regions_file(path: &Utf8Path) -> Result<RegionsConfig, SectorError> {
    if !path.exists() {
        return Ok(RegionsConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: RegionsFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.regions)
}

// ── Stage entry point ─────────────────────────────────────────────────────────

/// Build the warp-region overlay for a sector grid. Deterministic given the
/// root seed and the grid dimensions.
#[must_use]
pub fn build_regions(seed: &str, width: u32, height: u32, cfg: &RegionsConfig) -> Vec<WarpRegion> {
    if !cfg.enabled || cfg.count == 0 || width == 0 || height == 0 {
        return Vec::new();
    }

    let total_cells = (width as usize) * (height as usize);
    if total_cells == 0 {
        return Vec::new();
    }
    let target = (cfg.count as usize).min(total_cells / 2);
    let mean_size = cfg.mean_size.max(1) as usize;

    let mut rng = stage_rng(seed, "regions", "sector");
    let mut all: Vec<HexCoord> = Vec::with_capacity(total_cells);
    for r in 0..height as i32 {
        for q in 0..width as i32 {
            all.push(HexCoord { q, r });
        }
    }
    all.shuffle(&mut rng);

    let conditions = if cfg.conditions.is_empty() {
        default_condition_pool()
    } else {
        cfg.conditions.clone()
    };
    let total_weight: f64 = conditions
        .iter()
        .filter(|c| c.weight.is_finite() && c.weight > 0.0)
        .map(|c| c.weight)
        .sum();

    // Pick non-overlapping centres separated by a small minimum hex distance
    // so two regions don't grow on top of each other immediately.
    let min_centre_dist = mean_size.saturating_sub(1).max(1) as u32;
    let mut centres: Vec<HexCoord> = Vec::with_capacity(target);
    for c in &all {
        if centres.len() >= target {
            break;
        }
        if centres
            .iter()
            .all(|p| hex_distance(*p, *c) >= min_centre_dist)
        {
            centres.push(*c);
        }
    }
    // Fall back to any remaining cells if min-distance pruning starved us.
    if centres.len() < target {
        for c in &all {
            if centres.len() >= target {
                break;
            }
            if !centres.contains(c) {
                centres.push(*c);
            }
        }
    }

    let mut occupied: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut out: Vec<WarpRegion> = Vec::with_capacity(centres.len());
    for (idx, centre) in centres.into_iter().enumerate() {
        // BFS growth up to mean_size ± 2 hexes (deterministic noise).
        let jitter = rng.gen_range(0..=4) - 2;
        let target_size = ((mean_size as i32) + jitter).max(1) as usize;
        let hexes = grow_blob(
            centre,
            target_size,
            width as i32,
            height as i32,
            &mut occupied,
            &mut rng,
        );
        if hexes.is_empty() {
            continue;
        }
        let kind = if total_weight > 0.0 {
            sample_condition(&conditions, total_weight, &mut rng)
        } else {
            RegionConditionKind::Turbulence
        };
        out.push(WarpRegion {
            id: format!("reg-{:04}", idx + 1),
            name: format!("{} {:02}", kind.label(), idx + 1),
            kind,
            hexes,
            centre,
        });
    }
    out
}

fn sample_condition(
    conds: &[ConditionEntry],
    total: f64,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> RegionConditionKind {
    let mut roll = rng.gen::<f64>() * total;
    for c in conds {
        if !(c.weight.is_finite() && c.weight > 0.0) {
            continue;
        }
        roll -= c.weight;
        if roll <= 0.0 {
            return c.kind;
        }
    }
    conds
        .iter()
        .rev()
        .find(|c| c.weight.is_finite() && c.weight > 0.0)
        .map(|c| c.kind)
        .unwrap_or(RegionConditionKind::Turbulence)
}

/// §REG3: deterministically grow a blob of `target` hexes outward from
/// `centre`, BFS over hex neighbours, skipping any coord already in
/// `occupied`. Returns the grown footprint sorted by `(r, q)`.
///
/// Exposed so the builder can grow a single user-seeded region without going
/// through the full `build_regions` stage.
pub fn seed_region(
    seed: &str,
    discriminator: &str,
    centre: HexCoord,
    target_size: usize,
    width: u32,
    height: u32,
    existing: &[WarpRegion],
) -> Vec<HexCoord> {
    if width == 0 || height == 0 || target_size == 0 {
        return Vec::new();
    }
    let mut occupied: BTreeSet<(i32, i32)> = existing
        .iter()
        .flat_map(|r| r.hexes.iter().map(|h| (h.q, h.r)))
        .collect();
    let mut rng = stage_rng(seed, "regions", discriminator);
    grow_blob(
        centre,
        target_size.max(1),
        width as i32,
        height as i32,
        &mut occupied,
        &mut rng,
    )
}

fn grow_blob(
    centre: HexCoord,
    target: usize,
    width: i32,
    height: i32,
    occupied: &mut BTreeSet<(i32, i32)>,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Vec<HexCoord> {
    let mut hexes: Vec<HexCoord> = Vec::with_capacity(target);
    let mut queue: VecDeque<HexCoord> = VecDeque::new();
    let in_bounds = |c: HexCoord| c.q >= 0 && c.r >= 0 && c.q < width && c.r < height;

    if !in_bounds(centre) || occupied.contains(&(centre.q, centre.r)) {
        // Snap to nearest free in-bounds neighbour.
        let mut found = None;
        'outer: for radius in 1..=4 {
            for dq in -radius..=radius {
                for dr in -radius..=radius {
                    let c = HexCoord {
                        q: centre.q + dq,
                        r: centre.r + dr,
                    };
                    if in_bounds(c) && !occupied.contains(&(c.q, c.r)) {
                        found = Some(c);
                        break 'outer;
                    }
                }
            }
        }
        let Some(c) = found else {
            return Vec::new();
        };
        queue.push_back(c);
    } else {
        queue.push_back(centre);
    }

    while let Some(c) = queue.pop_front() {
        if hexes.len() >= target {
            break;
        }
        if occupied.contains(&(c.q, c.r)) {
            continue;
        }
        occupied.insert((c.q, c.r));
        hexes.push(c);
        let mut neighbours: Vec<HexCoord> = offset_r_neighbors(c.r)
            .into_iter()
            .map(|(dq, dr)| HexCoord {
                q: c.q + dq,
                r: c.r + dr,
            })
            .filter(|n| in_bounds(*n) && !occupied.contains(&(n.q, n.r)))
            .collect();
        neighbours.shuffle(rng);
        for n in neighbours {
            queue.push_back(n);
        }
    }
    hexes.sort_by_key(|a| (a.r, a.q));
    hexes
}

// ── Effect helpers ─────────────────────────────────────────────────────────────

/// Does a hex coordinate fall inside *any* region?
#[must_use]
pub fn region_at(regions: &[WarpRegion], coord: HexCoord) -> Option<&WarpRegion> {
    regions.iter().find(|r| r.hexes.contains(&coord))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegionRouteEffectsSummary {
    pub routes: usize,
    pub affected_routes: usize,
    pub changed_routes: usize,
    pub bridge_checks: usize,
    pub bridges_preserved: usize,
    pub stable: usize,
    pub unstable: usize,
    pub hazardous: usize,
    pub perilous: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegionRouteEffectsProgress {
    Started {
        regions: usize,
        systems: usize,
        routes: usize,
    },
    RouteScanned {
        current: usize,
        total: usize,
        affected_routes: usize,
        changed_routes: usize,
        bridge_checks: usize,
        bridges_preserved: usize,
    },
    BridgeCheckStarted {
        check: usize,
        route_index: usize,
        total_routes: usize,
        route_id: String,
    },
    Completed {
        summary: RegionRouteEffectsSummary,
    },
}

/// §5: pick the strongest route-affecting region condition along the
/// straight-line endpoints of a route. Returns `None` if no region condition
/// applies.
#[must_use]
pub fn dominant_route_condition(
    regions: &[WarpRegion],
    from: HexCoord,
    to: HexCoord,
) -> Option<RegionConditionKind> {
    let mut best: Option<RegionConditionKind> = None;
    for c in [from, to] {
        if let Some(r) = region_at(regions, c) {
            best = match best {
                None => Some(r.kind),
                Some(cur) if r.kind.route_precedence() > cur.route_precedence() => Some(r.kind),
                Some(cur) => Some(cur),
            };
        }
    }
    best.filter(|k| k.route_precedence() > 0)
}

/// Apply route-affecting region conditions to a list of routes. Idempotent
/// when called twice with the same input.
pub fn apply_route_effects(
    regions: &[WarpRegion],
    systems: &[GeneratedSystem],
    routes: &mut [GeneratedRoute],
) {
    let _ = apply_route_effects_with_progress(regions, systems, routes, |_| {});
}

pub fn apply_route_effects_with_progress(
    regions: &[WarpRegion],
    systems: &[GeneratedSystem],
    routes: &mut [GeneratedRoute],
    mut progress: impl FnMut(RegionRouteEffectsProgress),
) -> RegionRouteEffectsSummary {
    let mut summary = RegionRouteEffectsSummary {
        routes: routes.len(),
        ..Default::default()
    };
    progress(RegionRouteEffectsProgress::Started {
        regions: regions.len(),
        systems: systems.len(),
        routes: routes.len(),
    });
    if regions.is_empty() {
        count_route_stabilities(routes, &mut summary);
        progress(RegionRouteEffectsProgress::Completed { summary });
        return summary;
    }
    let by_id: BTreeMap<&str, HexCoord> =
        systems.iter().map(|s| (s.id.as_str(), s.coord)).collect();
    let total = routes.len();
    for idx in 0..total {
        if let (Some(&a), Some(&b)) = (
            by_id.get(routes[idx].from_system_id.as_str()),
            by_id.get(routes[idx].to_system_id.as_str()),
        ) {
            if let Some(cond) = dominant_route_condition(regions, a, b) {
                summary.affected_routes += 1;
                let outcome = match cond {
                    RegionConditionKind::WarpStorm => apply_route_stability_with_bridge_progress(
                        routes,
                        idx,
                        RouteStability::Perilous,
                        "region:warp_storm",
                        &mut summary,
                        &mut progress,
                    ),
                    RegionConditionKind::Turbulence => {
                        let target = degrade(routes[idx].stability);
                        apply_route_stability_with_bridge_progress(
                            routes,
                            idx,
                            target,
                            "region:turbulence",
                            &mut summary,
                            &mut progress,
                        )
                    }
                    RegionConditionKind::CalmCorridor => {
                        if matches!(routes[idx].stability, RouteStability::Perilous) {
                            RouteEffectOutcome::default()
                        } else {
                            let target = upgrade(routes[idx].stability);
                            apply_route_stability_with_bridge_progress(
                                routes,
                                idx,
                                target,
                                "region:calm_corridor",
                                &mut summary,
                                &mut progress,
                            )
                        }
                    }
                    _ => RouteEffectOutcome::default(),
                };
                if outcome.changed {
                    summary.changed_routes += 1;
                }
                if outcome.bridge_preserved {
                    summary.bridges_preserved += 1;
                }
            }
        }
        let current = idx + 1;
        if should_report_region_route_progress(current, total) {
            progress(RegionRouteEffectsProgress::RouteScanned {
                current,
                total,
                affected_routes: summary.affected_routes,
                changed_routes: summary.changed_routes,
                bridge_checks: summary.bridge_checks,
                bridges_preserved: summary.bridges_preserved,
            });
        }
    }
    count_route_stabilities(routes, &mut summary);
    progress(RegionRouteEffectsProgress::Completed { summary });
    summary
}

fn apply_route_stability_with_bridge_progress(
    routes: &mut [GeneratedRoute],
    idx: usize,
    target: RouteStability,
    tag: &str,
    summary: &mut RegionRouteEffectsSummary,
    progress: &mut impl FnMut(RegionRouteEffectsProgress),
) -> RouteEffectOutcome {
    let needs_bridge_check =
        target == RouteStability::Perilous && routes[idx].stability != RouteStability::Perilous;
    if needs_bridge_check {
        summary.bridge_checks += 1;
        if should_report_region_route_progress(summary.bridge_checks, summary.routes) {
            progress(RegionRouteEffectsProgress::BridgeCheckStarted {
                check: summary.bridge_checks,
                route_index: idx + 1,
                total_routes: summary.routes,
                route_id: routes[idx].id.to_string(),
            });
        }
    }
    apply_route_stability(routes, idx, target, tag)
}

fn count_route_stabilities(routes: &[GeneratedRoute], summary: &mut RegionRouteEffectsSummary) {
    summary.stable = 0;
    summary.unstable = 0;
    summary.hazardous = 0;
    summary.perilous = 0;
    for route in routes {
        match route.stability {
            RouteStability::Stable => summary.stable += 1,
            RouteStability::Unstable => summary.unstable += 1,
            RouteStability::Hazardous => summary.hazardous += 1,
            RouteStability::Perilous => summary.perilous += 1,
        }
    }
}

fn should_report_region_route_progress(current: usize, total: usize) -> bool {
    if total == 0 {
        return false;
    }
    current == 1 || current == total || current.is_multiple_of((total / 100).max(1))
}

#[derive(Debug, Clone, Copy, Default)]
struct RouteEffectOutcome {
    changed: bool,
    bridge_preserved: bool,
}

fn apply_route_stability(
    routes: &mut [GeneratedRoute],
    idx: usize,
    target: RouteStability,
    tag: &str,
) -> RouteEffectOutcome {
    let before = routes[idx].stability;
    let was_navigable = routes[idx].stability != RouteStability::Perilous;
    let preserve_bridge =
        target == RouteStability::Perilous && was_navigable && is_navigable_bridge(routes, idx);
    let marks_perilous = target == RouteStability::Perilous && was_navigable && !preserve_bridge;
    routes[idx].stability = if preserve_bridge {
        RouteStability::Hazardous
    } else {
        target
    };
    push_route_tag(&mut routes[idx], tag);
    if marks_perilous {
        push_route_tag(&mut routes[idx], "region:perilous_applied");
    }
    if preserve_bridge {
        push_route_tag(&mut routes[idx], "region:connectivity_preserved");
    }
    RouteEffectOutcome {
        changed: routes[idx].stability != before,
        bridge_preserved: preserve_bridge,
    }
}

fn is_navigable_bridge(routes: &[GeneratedRoute], candidate_idx: usize) -> bool {
    let Some(candidate) = routes.get(candidate_idx) else {
        return false;
    };
    if candidate.stability == RouteStability::Perilous {
        return false;
    }
    let from = candidate.from_system_id.as_str();
    let to = candidate.to_system_id.as_str();
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (idx, route) in routes.iter().enumerate() {
        if idx == candidate_idx || route.stability == RouteStability::Perilous {
            continue;
        }
        let a = route.from_system_id.as_str();
        let b = route.to_system_id.as_str();
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    seen.insert(from);
    queue.push_back(from);
    while let Some(cur) = queue.pop_front() {
        if let Some(neighbours) = adjacency.get(cur) {
            for next in neighbours {
                if seen.insert(next) {
                    queue.push_back(next);
                    if *next == to {
                        return false;
                    }
                }
            }
        }
    }
    !seen.contains(to)
}

fn push_route_tag(route: &mut GeneratedRoute, tag: &str) {
    if !route.tags.iter().any(|t| t.as_ref() == tag) {
        route.tags.push(tag.to_string().into());
    }
}

fn degrade(s: RouteStability) -> RouteStability {
    match s {
        RouteStability::Stable => RouteStability::Unstable,
        RouteStability::Unstable => RouteStability::Hazardous,
        RouteStability::Hazardous | RouteStability::Perilous => RouteStability::Perilous,
    }
}

fn upgrade(s: RouteStability) -> RouteStability {
    match s {
        RouteStability::Perilous => RouteStability::Hazardous,
        RouteStability::Hazardous => RouteStability::Unstable,
        RouteStability::Unstable | RouteStability::Stable => RouteStability::Stable,
    }
}

// ── Markdown render ────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(sector_id: &str, regions: &[WarpRegion]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Warp Regions — {sector_id}");
    let _ = writeln!(s, "\nTotal regions: **{}**", regions.len());
    if regions.is_empty() {
        let _ = writeln!(
            s,
            "\n_No regions configured. Enable in `regions.toml` or `sectorforge.toml`._"
        );
        return s;
    }
    let _ = writeln!(s, "\n| ID | Name | Kind | Hexes | Centre |");
    let _ = writeln!(s, "|----|------|------|-------|--------|");
    for r in regions {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | ({},{}) |",
            r.id,
            r.name,
            r.kind.label(),
            r.hexes.len(),
            r.centre.q,
            r.centre.r
        );
    }
    s
}

/// Write `regions.md` + `regions.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    sector_id: &str,
    regions: &[WarpRegion],
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        "regions",
        &render_markdown(sector_id, regions),
        &regions,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(count: u32, size: u32) -> RegionsConfig {
        RegionsConfig {
            enabled: true,
            count,
            mean_size: size,
            apply_to_routes: true,
            conditions: Vec::new(),
        }
    }

    #[test]
    fn disabled_yields_empty() {
        let regions = build_regions("seed", 6, 6, &RegionsConfig::default());
        assert!(regions.is_empty());
    }

    #[test]
    fn deterministic_blobs() {
        let a = build_regions("seed", 8, 8, &cfg(3, 5));
        let b = build_regions("seed", 8, 8, &cfg(3, 5));
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn blob_size_in_bounds() {
        let regions = build_regions("seed", 8, 8, &cfg(2, 5));
        assert!(!regions.is_empty());
        for r in &regions {
            assert!(!r.hexes.is_empty());
            for h in &r.hexes {
                assert!(h.q >= 0 && h.q < 8);
                assert!(h.r >= 0 && h.r < 8);
            }
        }
    }

    #[test]
    fn warp_storm_forces_perilous() {
        // Place a single hex region covering both endpoints, then verify the
        // route is rewritten to perilous.
        let region = WarpRegion {
            id: "reg-0001".into(),
            name: "storm".into(),
            kind: RegionConditionKind::WarpStorm,
            hexes: vec![HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 }],
            centre: HexCoord { q: 0, r: 0 },
        };
        let cond =
            dominant_route_condition(&[region], HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        assert_eq!(cond, Some(RegionConditionKind::WarpStorm));
    }

    #[test]
    fn perilous_effect_caps_navigable_bridge() {
        let mut routes = vec![
            route("r1", "a", "b", RouteStability::Hazardous),
            route("r2", "b", "c", RouteStability::Stable),
        ];
        apply_route_stability(
            &mut routes,
            0,
            RouteStability::Perilous,
            "region:warp_storm",
        );
        assert_eq!(routes[0].stability, RouteStability::Hazardous);
        assert!(routes[0]
            .tags
            .iter()
            .any(|t| t.as_ref() == "region:warp_storm"));
        assert!(routes[0]
            .tags
            .iter()
            .any(|t| t.as_ref() == "region:connectivity_preserved"));
    }

    #[test]
    fn perilous_effect_removes_non_bridge() {
        let mut routes = vec![
            route("r1", "a", "b", RouteStability::Hazardous),
            route("r2", "b", "c", RouteStability::Stable),
            route("r3", "a", "c", RouteStability::Stable),
        ];
        apply_route_stability(
            &mut routes,
            0,
            RouteStability::Perilous,
            "region:warp_storm",
        );
        assert_eq!(routes[0].stability, RouteStability::Perilous);
        assert!(!routes[0]
            .tags
            .iter()
            .any(|t| t.as_ref() == "region:connectivity_preserved"));
    }

    fn route(id: &str, from: &str, to: &str, stability: RouteStability) -> GeneratedRoute {
        GeneratedRoute {
            id: crate::ids::RouteId::new(id),
            from_system_id: crate::ids::SystemId::new(from),
            to_system_id: crate::ids::SystemId::new(to),
            distance: 1,
            route_type: crate::sector_model::RouteType::StableWarpLane,
            stability,
            tags: Vec::new(),
            controls: Vec::new(),
        }
    }
}
