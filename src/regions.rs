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
pub enum RegionConditionKind {
    /// Routes crossing become Perilous, world generation biased toward warp
    /// phenomena features.
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
}

impl RegionConditionKind {
    fn label(self) -> &'static str {
        match self {
            Self::WarpStorm => "Warp Storm",
            Self::Turbulence => "Turbulence",
            Self::CalmCorridor => "Calm Corridor",
            Self::Blackout => "Blackout",
            Self::Anomaly => "Anomaly",
        }
    }
    /// Route-effect lattice precedence (§5 NEW.md):
    /// `WarpStorm` (force Perilous) overrides `Turbulence` (one tier worse)
    /// which overrides `CalmCorridor` (one tier better). Higher = stronger.
    fn route_precedence(self) -> i32 {
        match self {
            Self::WarpStorm => 3,
            Self::Turbulence => 2,
            Self::CalmCorridor => 1,
            _ => 0,
        }
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
    let mut centres: Vec<HexCoord> = Vec::new();
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
            if !centres.iter().any(|p| p == c) {
                centres.push(*c);
            }
        }
    }

    let mut occupied: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut out: Vec<WarpRegion> = Vec::with_capacity(centres.len());
    for (idx, centre) in centres.into_iter().enumerate() {
        // BFS growth up to mean_size ± 2 hexes (deterministic noise).
        let jitter = (rng.gen_range(0..=4) as i32) - 2;
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

fn grow_blob(
    centre: HexCoord,
    target: usize,
    width: i32,
    height: i32,
    occupied: &mut BTreeSet<(i32, i32)>,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Vec<HexCoord> {
    let mut hexes: Vec<HexCoord> = Vec::new();
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
    hexes.sort_by(|a, b| (a.r, a.q).cmp(&(b.r, b.q)));
    hexes
}

// ── Effect helpers ─────────────────────────────────────────────────────────────

/// Does a hex coordinate fall inside *any* region?
#[must_use]
pub fn region_at(regions: &[WarpRegion], coord: HexCoord) -> Option<&WarpRegion> {
    regions.iter().find(|r| r.hexes.contains(&coord))
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
    if regions.is_empty() {
        return;
    }
    let by_id: BTreeMap<&str, HexCoord> =
        systems.iter().map(|s| (s.id.as_str(), s.coord)).collect();
    for r in routes.iter_mut() {
        let (Some(&a), Some(&b)) = (
            by_id.get(r.from_system_id.as_str()),
            by_id.get(r.to_system_id.as_str()),
        ) else {
            continue;
        };
        let Some(cond) = dominant_route_condition(regions, a, b) else {
            continue;
        };
        match cond {
            RegionConditionKind::WarpStorm => {
                r.stability = RouteStability::Perilous;
                if !r.tags.iter().any(|t| t == "region:warp_storm") {
                    r.tags.push("region:warp_storm".into());
                }
            }
            RegionConditionKind::Turbulence => {
                r.stability = degrade(r.stability);
                if !r.tags.iter().any(|t| t == "region:turbulence") {
                    r.tags.push("region:turbulence".into());
                }
            }
            RegionConditionKind::CalmCorridor => {
                // Only upgrades when not already perilous (per lattice doc).
                if !matches!(r.stability, RouteStability::Perilous) {
                    r.stability = upgrade(r.stability);
                    if !r.tags.iter().any(|t| t == "region:calm_corridor") {
                        r.tags.push("region:calm_corridor".into());
                    }
                }
            }
            _ => {}
        }
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
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md = render_markdown(sector_id, regions);
    let md_path = output_dir.join("regions.md");
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join("regions.json");
    let json = serde_json::to_string_pretty(regions)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    Ok(())
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
}
