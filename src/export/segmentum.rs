//! §14 NEW.md: multi-sector / segmentum composition.
//!
//! A segmentum is a deterministic, reproducible composition of several
//! independently-generated child sectors stitched together with inter-sector
//! warp links. The child generation pipeline is reused untouched; this module
//! only adds:
//!
//! 1. A `segmentum.toml` schema describing children + super-grid layout +
//!    stitch policy + faction-roster mode.
//! 2. A `compose` entry point that loads + generates every child, runs a
//!    deterministic stitch stage seeded from
//!    `blake3("sectorforge:{stitch_seed}:stitch:{a_id}:{b_id}")`, and returns
//!    a [`Segmentum`] DTO carrying the children and the inter-sector links.
//! 3. A super-manifest digesting the segmentum config + each child's
//!    `manifest.json` so the audit chain extends cleanly to the larger scale.
//! 4. Markdown / JSON writers.
//!
//! Same `segmentum.toml` + same children + same `sectorforge` version ⇒ same
//! bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufWriter, Write as _};

use camino::{Utf8Path, Utf8PathBuf};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng;
use crate::sector_model::{GeneratedSector, RouteStability, RouteType};

// ── Config ───────────────────────────────────────────────────────────────────

/// On-disk `segmentum.toml` schema.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SegmentumFile {
    pub segmentum: SegmentumConfig,
    #[serde(default)]
    pub stitch: StitchConfig,
    #[serde(default)]
    pub children: Vec<ChildEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SegmentumConfig {
    pub id: String,
    pub title: String,
    /// Stitch-stage root seed. Combined with each child-pair into the
    /// `blake3("sectorforge:{stitch_seed}:stitch:{a}:{b}")` discriminator so
    /// that the same segmentum file always produces the same inter-sector
    /// links.
    pub stitch_seed: String,
    /// Super-grid width — how many child sectors sit horizontally.
    pub columns: u32,
    /// Super-grid height.
    pub rows: u32,
    /// How a child's faction roster relates to its neighbours' rosters:
    /// `Shared` treats matching faction ids across children as the same
    /// entity (rosters aggregate); `Independent` namespaces them per child
    /// so `"imperium"` in child A is distinct from `"imperium"` in child B.
    #[serde(default)]
    pub faction_mode: FactionMode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FactionMode {
    #[default]
    Shared,
    Independent,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StitchConfig {
    /// Maximum inter-sector warp links emitted for each adjacent child pair.
    #[serde(default = "default_max_links")]
    pub max_links_per_pair: u32,
    /// How many hex rows/cols inwards from the shared border are eligible
    /// donors for a stitch endpoint. Larger values pull more systems into
    /// the candidate set.
    #[serde(default = "default_border_depth")]
    pub border_depth: u32,
    /// Route type stamped on every inter-sector link.
    #[serde(default = "default_link_route_type")]
    pub default_route_type: RouteType,
    /// Route stability stamped on every inter-sector link.
    #[serde(default = "default_link_stability")]
    pub default_stability: RouteStability,
}

impl Default for StitchConfig {
    fn default() -> Self {
        Self {
            max_links_per_pair: default_max_links(),
            border_depth: default_border_depth(),
            default_route_type: default_link_route_type(),
            default_stability: default_link_stability(),
        }
    }
}

fn default_max_links() -> u32 {
    2
}

fn default_border_depth() -> u32 {
    2
}

fn default_link_route_type() -> RouteType {
    RouteType::ChartedPassage
}

fn default_link_stability() -> RouteStability {
    RouteStability::Unstable
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChildEntry {
    /// Identifier used in the super-manifest, output dir name, and stitch
    /// discriminator. Must be unique across children. `[a-zA-Z0-9_-]+`.
    pub id: String,
    /// Project directory, relative to the directory holding `segmentum.toml`
    /// (or absolute). Loaded with [`crate::load_project`].
    pub project: Utf8PathBuf,
    /// Super-grid column (0-indexed, < `columns`).
    pub column: u32,
    /// Super-grid row (0-indexed, < `rows`).
    pub row: u32,
    /// Optional seed override. When absent the child uses whatever
    /// `[generation].seed` it carries.
    #[serde(default)]
    pub seed: Option<String>,
    /// Optional display title override. Falls back to the loaded sector
    /// title.
    #[serde(default)]
    pub title: Option<String>,
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

/// Top-level composed segmentum. Serialised verbatim to `segmentum.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segmentum {
    pub id: String,
    pub title: String,
    pub stitch_seed: String,
    pub columns: u32,
    pub rows: u32,
    pub faction_mode: FactionMode,
    pub generator_name: String,
    pub generator_version: String,
    pub children: Vec<SegmentumChild>,
    pub inter_sector_links: Vec<InterSectorLink>,
    pub manifest: SegmentumManifest,
}

/// One child entry's resolved state after composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentumChild {
    pub id: String,
    pub project: String,
    pub column: u32,
    pub row: u32,
    pub title: String,
    pub sector_id: String,
    pub seed: String,
    pub sector_digest: String,
    pub width: u32,
    pub height: u32,
    pub system_count: usize,
    pub world_count: usize,
    pub route_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterSectorLink {
    pub id: String,
    pub from_child_id: String,
    pub to_child_id: String,
    pub from_system_id: crate::ids::SystemId,
    pub to_system_id: crate::ids::SystemId,
    /// Edge orientation between the two child sectors on the super-grid.
    pub orientation: BorderOrientation,
    /// In-sector depth-from-border + cross-border step combined. Always
    /// `>= 1`; not directly comparable to in-sector `hex_distance`.
    pub distance_units: u32,
    pub route_type: RouteType,
    pub stability: RouteStability,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BorderOrientation {
    /// Children share a vertical edge — the left child's east border meets
    /// the right child's west border (columns differ by 1, rows equal).
    EastWest,
    /// Children share a horizontal edge — the upper child's south border
    /// meets the lower child's north border (rows differ by 1, columns
    /// equal).
    NorthSouth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentumManifest {
    pub segmentum_id: String,
    pub generator_name: String,
    pub generator_version: String,
    pub stitch_seed: String,
    pub stitch_seed_hash: String,
    /// blake3 of the canonicalised segmentum config (children listed in
    /// declared order; settings serialised through serde_json with sorted
    /// keys).
    pub settings_digest: String,
    pub faction_mode: FactionMode,
    /// `child_id -> sector_digest` derived from the child's `manifest.json`
    /// equivalent (blake3 of the canonical sector JSON).
    pub child_digests: BTreeMap<String, String>,
    pub system_count: usize,
    pub world_count: usize,
    pub route_count: usize,
    pub inter_sector_link_count: usize,
}

/// Progress events emitted by segmentum composition when a caller opts in
/// through [`compose_with_progress`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentumProgress {
    Started {
        children: usize,
        output_dir: String,
    },
    ChildLoading {
        index: usize,
        total: usize,
        id: String,
        project: String,
    },
    ChildValidated {
        index: usize,
        total: usize,
        id: String,
        warnings: usize,
    },
    ChildGenerating {
        index: usize,
        total: usize,
        id: String,
    },
    ChildSectorProgress {
        index: usize,
        total: usize,
        id: String,
        event: crate::generation::SectorProgress,
    },
    ChildInvariantsChecked {
        index: usize,
        total: usize,
        id: String,
    },
    ChildExporting {
        index: usize,
        total: usize,
        id: String,
        output_dir: String,
    },
    ChildComplete {
        index: usize,
        total: usize,
        id: String,
        systems: usize,
        worlds: usize,
        routes: usize,
    },
    Stitching {
        children: usize,
    },
    Complete {
        children: usize,
        links: usize,
        systems: usize,
        worlds: usize,
        routes: usize,
    },
}

// ── Loaders ──────────────────────────────────────────────────────────────────

/// Read and parse a `segmentum.toml` file.
///
/// # Errors
///
/// [`SectorError::Io`] on read failure, [`SectorError::ConfigParse`] on
/// TOML parse failure.
pub fn load_segmentum_file(path: &Utf8Path) -> Result<SegmentumFile, SectorError> {
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: SegmentumFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed)
}

// ── Compose ──────────────────────────────────────────────────────────────────

/// Compose a segmentum from a parsed config + the directory holding
/// `segmentum.toml` (used to resolve child project paths). Loads + generates
/// every child sequentially, then runs the deterministic stitch stage.
///
/// Children are written under `<output_dir>/children/<child_id>/...` via
/// [`crate::export_sector`] using each child's own `[outputs]` config; the
/// super-manifest and stitch artifacts live at the top of `<output_dir>`.
///
/// # Errors
///
/// Propagates any [`SectorError`] from child load / validate / generate.
/// Returns [`SectorError::InvalidConfig`] if the segmentum config is
/// inconsistent (duplicate child id, out-of-bounds column/row, no children).
pub fn compose(
    file: &SegmentumFile,
    base_dir: &Utf8Path,
    output_dir: &Utf8Path,
) -> Result<Segmentum, SectorError> {
    compose_with_progress(file, base_dir, output_dir, |_| {})
}

/// Compose a segmentum with progress callbacks.
///
/// The callback is synchronous and receives child-sector load/validate/export
/// milestones plus nested [`crate::generation::SectorProgress`] events from
/// each child generation run. The default [`compose`] wrapper uses a no-op
/// callback.
///
/// # Errors
///
/// Same as [`compose`].
pub fn compose_with_progress<F>(
    file: &SegmentumFile,
    base_dir: &Utf8Path,
    output_dir: &Utf8Path,
    mut progress: F,
) -> Result<Segmentum, SectorError>
where
    F: FnMut(SegmentumProgress),
{
    validate_config(file)?;
    progress(SegmentumProgress::Started {
        children: file.children.len(),
        output_dir: output_dir.to_string(),
    });

    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;

    let mut child_records: Vec<SegmentumChild> = Vec::with_capacity(file.children.len());
    let mut sectors: BTreeMap<String, GeneratedSector> = BTreeMap::new();
    let mut child_digests: BTreeMap<String, String> = BTreeMap::new();
    let mut total_systems = 0usize;
    let mut total_worlds = 0usize;
    let mut total_routes = 0usize;

    let children_root = output_dir.join("children");
    fs::create_dir_all(&children_root).map_err(|e| SectorError::io(children_root.as_str(), e))?;

    let total_children = file.children.len();
    for (idx, child) in file.children.iter().enumerate() {
        let index = idx + 1;
        let project_path = if child.project.is_absolute() {
            child.project.clone()
        } else {
            base_dir.join(&child.project)
        };
        progress(SegmentumProgress::ChildLoading {
            index,
            total: total_children,
            id: child.id.clone(),
            project: project_path.to_string(),
        });
        let mut input = crate::load_project(&project_path)?;
        if let Some(seed) = &child.seed {
            input.config.generation.seed = seed.clone();
        }

        let report = crate::validate_project(&input)?;
        if !report.ok {
            return Err(SectorError::InvalidConfig(format!(
                "child '{}' failed pre-generation validation ({} error(s))",
                child.id,
                report.errors.len()
            )));
        }
        progress(SegmentumProgress::ChildValidated {
            index,
            total: total_children,
            id: child.id.clone(),
            warnings: report.warnings.len(),
        });

        let output_cfg = input.config.outputs.clone();
        progress(SegmentumProgress::ChildGenerating {
            index,
            total: total_children,
            id: child.id.clone(),
        });
        let child_id = child.id.clone();
        let sector = crate::generation::generate_with_progress(input, |event| {
            progress(SegmentumProgress::ChildSectorProgress {
                index,
                total: total_children,
                id: child_id.clone(),
                event,
            });
        })?;
        let inv = crate::validate_sector(&sector);
        if !inv.ok {
            return Err(SectorError::InvalidConfig(format!(
                "child '{}' failed post-generation invariants ({} violation(s))",
                child.id,
                inv.violations.len()
            )));
        }
        progress(SegmentumProgress::ChildInvariantsChecked {
            index,
            total: total_children,
            id: child.id.clone(),
        });

        let child_out = children_root.join(&child.id);
        progress(SegmentumProgress::ChildExporting {
            index,
            total: total_children,
            id: child.id.clone(),
            output_dir: child_out.to_string(),
        });
        crate::export_sector(&sector, &output_cfg, &child_out)?;

        let digest = digest_sector(&sector)?;
        let sector_systems = sector.manifest.system_count;
        let sector_worlds = sector.manifest.world_count;
        let sector_routes = sector.manifest.route_count;
        total_systems += sector_systems;
        total_worlds += sector_worlds;
        total_routes += sector_routes;

        child_records.push(SegmentumChild {
            id: child.id.clone(),
            project: project_path.to_string(),
            column: child.column,
            row: child.row,
            title: child
                .title
                .clone()
                .unwrap_or_else(|| sector.title.to_string()),
            sector_id: sector.id.to_string(),
            seed: sector.seed.to_string(),
            sector_digest: digest.clone(),
            width: sector.width,
            height: sector.height,
            system_count: sector_systems,
            world_count: sector_worlds,
            route_count: sector_routes,
        });
        child_digests.insert(child.id.clone(), digest);
        sectors.insert(child.id.clone(), sector);
        progress(SegmentumProgress::ChildComplete {
            index,
            total: total_children,
            id: child.id.clone(),
            systems: sector_systems,
            worlds: sector_worlds,
            routes: sector_routes,
        });
    }

    progress(SegmentumProgress::Stitching {
        children: child_records.len(),
    });
    let links = stitch_children(file, &child_records, &sectors);
    let link_count = links.len();

    let stitch_seed_hash = rng::hex(&rng::hash_root_seed(&file.segmentum.stitch_seed));
    let settings_digest = digest_settings(file)?;

    let manifest = SegmentumManifest {
        segmentum_id: file.segmentum.id.clone(),
        generator_name: crate::GENERATOR_NAME.to_string(),
        generator_version: crate::GENERATOR_VERSION.to_string(),
        stitch_seed: file.segmentum.stitch_seed.clone(),
        stitch_seed_hash,
        settings_digest,
        faction_mode: file.segmentum.faction_mode,
        child_digests,
        system_count: total_systems,
        world_count: total_worlds,
        route_count: total_routes,
        inter_sector_link_count: link_count,
    };

    progress(SegmentumProgress::Complete {
        children: child_records.len(),
        links: link_count,
        systems: total_systems,
        worlds: total_worlds,
        routes: total_routes,
    });

    Ok(Segmentum {
        id: file.segmentum.id.clone(),
        title: file.segmentum.title.clone(),
        stitch_seed: file.segmentum.stitch_seed.clone(),
        columns: file.segmentum.columns,
        rows: file.segmentum.rows,
        faction_mode: file.segmentum.faction_mode,
        generator_name: crate::GENERATOR_NAME.to_string(),
        generator_version: crate::GENERATOR_VERSION.to_string(),
        children: child_records,
        inter_sector_links: links,
        manifest,
    })
}

fn validate_config(file: &SegmentumFile) -> Result<(), SectorError> {
    let cfg = &file.segmentum;
    if file.children.is_empty() {
        return Err(SectorError::InvalidConfig(
            "segmentum config has no [[children]]".into(),
        ));
    }
    if cfg.columns == 0 || cfg.rows == 0 {
        return Err(SectorError::InvalidConfig(
            "segmentum.columns and segmentum.rows must be >= 1".into(),
        ));
    }
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    let mut slots: BTreeSet<(u32, u32)> = BTreeSet::new();
    for child in &file.children {
        if !ids.insert(child.id.as_str()) {
            return Err(SectorError::InvalidConfig(format!(
                "duplicate child id '{}' in segmentum config",
                child.id
            )));
        }
        if child.column >= cfg.columns || child.row >= cfg.rows {
            return Err(SectorError::InvalidConfig(format!(
                "child '{}' at ({}, {}) is outside the {}x{} super-grid",
                child.id, child.column, child.row, cfg.columns, cfg.rows
            )));
        }
        if !slots.insert((child.column, child.row)) {
            return Err(SectorError::InvalidConfig(format!(
                "two children occupy super-grid slot ({}, {})",
                child.column, child.row
            )));
        }
    }
    Ok(())
}

fn digest_sector(sector: &GeneratedSector) -> Result<String, SectorError> {
    let bytes =
        serde_json::to_vec(sector).map_err(|e| SectorError::export("sector", e.to_string()))?;
    Ok(format!(
        "blake3:{}",
        rng::hex(blake3::hash(&bytes).as_bytes())
    ))
}

fn digest_settings(file: &SegmentumFile) -> Result<String, SectorError> {
    // Serialise through serde_json (BTreeMap-style stable ordering of fields
    // because the struct field order is fixed) so the digest is stable across
    // platforms / TOML formatting changes.
    let bytes =
        serde_json::to_vec(file).map_err(|e| SectorError::export("segmentum", e.to_string()))?;
    Ok(format!(
        "blake3:{}",
        rng::hex(blake3::hash(&bytes).as_bytes())
    ))
}

// ── Stitch ───────────────────────────────────────────────────────────────────

fn stitch_children(
    file: &SegmentumFile,
    children: &[SegmentumChild],
    sectors: &BTreeMap<String, GeneratedSector>,
) -> Vec<InterSectorLink> {
    let mut by_slot: BTreeMap<(u32, u32), &SegmentumChild> = BTreeMap::new();
    for c in children {
        by_slot.insert((c.column, c.row), c);
    }

    let cap = children
        .len()
        .saturating_mul(2)
        .saturating_mul(file.stitch.max_links_per_pair as usize);
    let mut links: Vec<InterSectorLink> = Vec::with_capacity(cap);
    let mut link_idx: u32 = 1;

    // Walk slots in (col, row) order. Emit only east + south neighbours so
    // each adjacency is considered exactly once.
    for ((col, row), a) in &by_slot {
        if let Some(b) = by_slot.get(&(col + 1, *row)) {
            let pair_links = stitch_pair(
                &file.stitch,
                &file.segmentum.stitch_seed,
                a,
                b,
                BorderOrientation::EastWest,
                sectors,
                &mut link_idx,
            );
            links.extend(pair_links);
        }
        if let Some(b) = by_slot.get(&(*col, row + 1)) {
            let pair_links = stitch_pair(
                &file.stitch,
                &file.segmentum.stitch_seed,
                a,
                b,
                BorderOrientation::NorthSouth,
                sectors,
                &mut link_idx,
            );
            links.extend(pair_links);
        }
    }
    links
}

fn stitch_pair(
    stitch: &StitchConfig,
    stitch_seed: &str,
    a: &SegmentumChild,
    b: &SegmentumChild,
    orientation: BorderOrientation,
    sectors: &BTreeMap<String, GeneratedSector>,
    link_idx: &mut u32,
) -> Vec<InterSectorLink> {
    let sec_a = match sectors.get(&a.id) {
        Some(s) => s,
        None => return vec![],
    };
    let sec_b = match sectors.get(&b.id) {
        Some(s) => s,
        None => return vec![],
    };

    let depth = stitch.border_depth.max(1);

    // Border donors: systems closest to the shared edge, in id order.
    let donors_a: Vec<(&str, u32)> = sec_a
        .systems
        .iter()
        .filter_map(|s| {
            let coord = match orientation {
                BorderOrientation::EastWest => {
                    // a is left, take rightmost columns. q in [width-depth, width-1].
                    let w = sec_a.width as i32;
                    if s.coord.q >= w - depth as i32 && s.coord.q < w {
                        Some((w - 1 - s.coord.q) as u32)
                    } else {
                        None
                    }
                }
                BorderOrientation::NorthSouth => {
                    // a is upper, take bottom-most rows. r in [height-depth, height-1].
                    let h = sec_a.height as i32;
                    if s.coord.r >= h - depth as i32 && s.coord.r < h {
                        Some((h - 1 - s.coord.r) as u32)
                    } else {
                        None
                    }
                }
            };
            coord.map(|d| (s.id.as_str(), d))
        })
        .collect();
    let donors_b: Vec<(&str, u32)> = sec_b
        .systems
        .iter()
        .filter_map(|s| {
            let coord = match orientation {
                BorderOrientation::EastWest => {
                    if s.coord.q >= 0 && s.coord.q < depth as i32 {
                        Some(s.coord.q as u32)
                    } else {
                        None
                    }
                }
                BorderOrientation::NorthSouth => {
                    if s.coord.r >= 0 && s.coord.r < depth as i32 {
                        Some(s.coord.r as u32)
                    } else {
                        None
                    }
                }
            };
            coord.map(|d| (s.id.as_str(), d))
        })
        .collect();

    if donors_a.is_empty() || donors_b.is_empty() {
        return vec![];
    }

    let mut pairs: Vec<(usize, usize, u32)> = Vec::with_capacity(donors_a.len() * donors_b.len());
    for (ia, (_, da)) in donors_a.iter().enumerate() {
        for (ib, (_, db)) in donors_b.iter().enumerate() {
            let units = da + db + 1; // cross-border step
            pairs.push((ia, ib, units));
        }
    }
    // Deterministic ordering: ascending distance then donor-index, then
    // shuffle-by-RNG only within equal-distance buckets via the stage seed.
    pairs.sort_by(|x, y| x.2.cmp(&y.2).then(x.0.cmp(&y.0)).then(x.1.cmp(&y.1)));

    let mut rng = rng::stage_rng(stitch_seed, "stitch", &format!("{}:{}", a.id, b.id));

    let cap = stitch.max_links_per_pair as usize;
    let mut chosen: Vec<InterSectorLink> = Vec::with_capacity(cap);
    let mut used_a: BTreeSet<usize> = BTreeSet::new();
    let mut used_b: BTreeSet<usize> = BTreeSet::new();
    if cap == 0 {
        return vec![];
    }

    // Bucket equal-distance pairs and randomise within each bucket so two
    // donors at the same distance don't always lose to the lower-index one,
    // but the result is still deterministic given `stitch_seed`.
    let mut i = 0;
    while i < pairs.len() && chosen.len() < cap {
        let bucket_start = i;
        let bucket_dist = pairs[i].2;
        while i < pairs.len() && pairs[i].2 == bucket_dist {
            i += 1;
        }
        let mut bucket: Vec<(usize, usize, u32)> = pairs[bucket_start..i].to_vec();
        // Fisher-Yates shuffle on the bucket using the seeded RNG.
        for k in (1..bucket.len()).rev() {
            let j = rng.gen_range(0..=k);
            bucket.swap(k, j);
        }
        for (ia, ib, units) in bucket {
            if chosen.len() >= cap {
                break;
            }
            if used_a.contains(&ia) || used_b.contains(&ib) {
                continue;
            }
            used_a.insert(ia);
            used_b.insert(ib);
            let from_sys = crate::ids::SystemId::new(donors_a[ia].0);
            let to_sys = crate::ids::SystemId::new(donors_b[ib].0);
            *link_idx += 1;
            chosen.push(InterSectorLink {
                id: format!("sl-{:04}", *link_idx - 1),
                from_child_id: a.id.clone(),
                to_child_id: b.id.clone(),
                from_system_id: from_sys,
                to_system_id: to_sys,
                orientation,
                distance_units: units,
                route_type: stitch.default_route_type,
                stability: stitch.default_stability,
            });
        }
    }

    chosen
}

// ── Render ───────────────────────────────────────────────────────────────────

/// Deterministic Markdown super-map for a composed segmentum.
#[must_use]
pub fn render_markdown(seg: &Segmentum) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} — {}\n\n", seg.id, seg.title));
    s.push_str(&format!("Stitch seed: `{}`\n\n", seg.stitch_seed));
    s.push_str(&format!(
        "Generator: {} v{}\n\n",
        seg.generator_name, seg.generator_version
    ));
    s.push_str(&format!("- **Super-grid:** {}×{}\n", seg.columns, seg.rows));
    s.push_str(&format!("- **Children:** {}\n", seg.children.len()));
    s.push_str(&format!(
        "- **Inter-sector links:** {}\n",
        seg.inter_sector_links.len()
    ));
    s.push_str(&format!(
        "- **Faction roster mode:** {}\n",
        match seg.faction_mode {
            FactionMode::Shared => "shared",
            FactionMode::Independent => "independent",
        }
    ));
    s.push_str(&format!(
        "- **Aggregate systems / worlds / routes:** {} / {} / {}\n\n",
        seg.manifest.system_count, seg.manifest.world_count, seg.manifest.route_count
    ));

    s.push_str("## Super-grid\n\n");
    s.push_str(&format_super_grid(seg));
    s.push('\n');

    s.push_str("## Children\n\n");
    s.push_str("| ID | Slot | Sector | Seed | Systems | Worlds | Routes |\n");
    s.push_str("|---|---|---|---|---:|---:|---:|\n");
    for c in &seg.children {
        s.push_str(&format!(
            "| {} | ({}, {}) | {} | `{}` | {} | {} | {} |\n",
            c.id,
            c.column,
            c.row,
            c.sector_id,
            c.seed,
            c.system_count,
            c.world_count,
            c.route_count
        ));
    }
    s.push('\n');

    s.push_str("## Inter-sector links\n\n");
    if seg.inter_sector_links.is_empty() {
        s.push_str("_No inter-sector links._\n\n");
    } else {
        s.push_str("| ID | From | To | Orientation | Units | Type | Stability |\n");
        s.push_str("|---|---|---|---|---:|---|---|\n");
        for l in &seg.inter_sector_links {
            s.push_str(&format!(
                "| {} | {}/{} | {}/{} | {} | {} | {:?} | {:?} |\n",
                l.id,
                l.from_child_id,
                l.from_system_id,
                l.to_child_id,
                l.to_system_id,
                match l.orientation {
                    BorderOrientation::EastWest => "E-W",
                    BorderOrientation::NorthSouth => "N-S",
                },
                l.distance_units,
                l.route_type,
                l.stability,
            ));
        }
        s.push('\n');
    }

    s.push_str("## Super-manifest\n\n");
    s.push_str(&format!(
        "- Stitch seed hash: `{}`\n",
        seg.manifest.stitch_seed_hash
    ));
    s.push_str(&format!(
        "- Settings digest: `{}`\n",
        seg.manifest.settings_digest
    ));
    s.push_str("- Child digests:\n");
    for (id, dig) in &seg.manifest.child_digests {
        s.push_str(&format!("  - `{}` → `{}`\n", id, dig));
    }
    s.push('\n');
    s
}

fn format_super_grid(seg: &Segmentum) -> String {
    let mut grid: BTreeMap<(u32, u32), &SegmentumChild> = BTreeMap::new();
    for c in &seg.children {
        grid.insert((c.column, c.row), c);
    }
    let mut s = String::new();
    s.push_str("```\n");
    for r in 0..seg.rows {
        for c in 0..seg.columns {
            let cell = grid
                .get(&(c, r))
                .map(|child| child.id.as_str())
                .unwrap_or(".");
            s.push_str(&format!("[{:<12}]", cell));
        }
        s.push('\n');
    }
    s.push_str("```\n");
    s
}

// ── Writers ──────────────────────────────────────────────────────────────────

/// Write `segmentum.md`, `segmentum.json`, and `super_manifest.json` into
/// `output_dir`. The per-child outputs live under `<output_dir>/children/<id>`
/// already (written by [`compose`]).
///
/// # Errors
///
/// [`SectorError::Io`] on write failure and [`SectorError::ExportFailed`] on
/// serialisation failure.
pub fn write_report(output_dir: &Utf8Path, seg: &Segmentum) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md_path = output_dir.join("segmentum.md");
    fs::write(&md_path, render_markdown(seg)).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join("segmentum.json");
    stream_pretty_json(&json_path, seg)?;
    let manifest_path = output_dir.join("super_manifest.json");
    stream_pretty_json(&manifest_path, &seg.manifest)?;
    Ok(())
}

fn stream_pretty_json<T: Serialize>(p: &Utf8Path, value: &T) -> Result<(), SectorError> {
    let file = fs::File::create(p).map_err(|e| SectorError::io(p.as_str(), e))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)
        .map_err(|e| SectorError::export(p.as_str(), e.to_string()))?;
    writer.flush().map_err(|e| SectorError::io(p.as_str(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedSector, GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord,
    };
    use std::collections::BTreeMap as Map;

    fn mk_system(id: &str, q: i32, r: i32) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            coord: HexCoord { q, r },
            kind: crate::sector_model::SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "Y".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
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

    fn mk_sector(id: &str, width: u32, height: u32, coords: &[(i32, i32)]) -> GeneratedSector {
        let systems: Vec<GeneratedSystem> = coords
            .iter()
            .enumerate()
            .map(|(i, (q, r))| mk_system(&format!("sys-{:04}", i + 1), *q, *r))
            .collect();
        GeneratedSector {
            id: id.into(),
            title: id.into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width,
            height,
            systems,
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: id.into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                base_seed: None,
                candidate_index: None,
                constraints_digest: None,
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: coords.len(),
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: vec![].into(),
            economy: Default::default(),
            chronicle: Default::default(),
            ..Default::default()
        }
    }

    fn mk_child(id: &str, col: u32, row: u32) -> SegmentumChild {
        SegmentumChild {
            id: id.into(),
            project: "/tmp/none".into(),
            column: col,
            row,
            title: id.into(),
            sector_id: id.into(),
            seed: "seed".into(),
            sector_digest: "blake3:0".into(),
            width: 4,
            height: 4,
            system_count: 0,
            world_count: 0,
            route_count: 0,
        }
    }

    #[test]
    fn stitch_emits_links_along_east_west_border() {
        let mut sectors: BTreeMap<String, GeneratedSector> = BTreeMap::new();
        // A: 4x4 grid with one system on the eastern border.
        sectors.insert("alpha".into(), mk_sector("a", 4, 4, &[(3, 1), (0, 0)]));
        // B: 4x4 grid with one system on the western border.
        sectors.insert("beta".into(), mk_sector("b", 4, 4, &[(0, 1), (3, 0)]));

        let file = SegmentumFile {
            segmentum: SegmentumConfig {
                id: "seg".into(),
                title: "Seg".into(),
                stitch_seed: "stitch-1".into(),
                columns: 2,
                rows: 1,
                faction_mode: FactionMode::Shared,
            },
            stitch: StitchConfig {
                max_links_per_pair: 1,
                border_depth: 1,
                ..Default::default()
            },
            children: vec![],
        };
        let children = [mk_child("alpha", 0, 0), mk_child("beta", 1, 0)];
        let mut idx = 1;
        let links = stitch_pair(
            &file.stitch,
            &file.segmentum.stitch_seed,
            &children[0],
            &children[1],
            BorderOrientation::EastWest,
            &sectors,
            &mut idx,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].from_child_id, "alpha");
        assert_eq!(links[0].to_child_id, "beta");
        assert_eq!(links[0].from_system_id, "sys-0001"); // (q=3, r=1)
        assert_eq!(links[0].to_system_id, "sys-0001"); // (q=0, r=1)
    }

    #[test]
    fn stitch_is_deterministic_across_runs() {
        let mut sectors: BTreeMap<String, GeneratedSector> = BTreeMap::new();
        sectors.insert(
            "alpha".into(),
            mk_sector("a", 4, 4, &[(3, 0), (3, 1), (3, 2)]),
        );
        sectors.insert(
            "beta".into(),
            mk_sector("b", 4, 4, &[(0, 0), (0, 1), (0, 2)]),
        );

        let file = SegmentumFile {
            segmentum: SegmentumConfig {
                id: "seg".into(),
                title: "Seg".into(),
                stitch_seed: "deterministic".into(),
                columns: 2,
                rows: 1,
                faction_mode: FactionMode::Shared,
            },
            stitch: StitchConfig {
                max_links_per_pair: 2,
                border_depth: 1,
                ..Default::default()
            },
            children: vec![],
        };
        let children = [mk_child("alpha", 0, 0), mk_child("beta", 1, 0)];
        let mut idx_a = 1;
        let mut idx_b = 1;
        let links_a = stitch_pair(
            &file.stitch,
            &file.segmentum.stitch_seed,
            &children[0],
            &children[1],
            BorderOrientation::EastWest,
            &sectors,
            &mut idx_a,
        );
        let links_b = stitch_pair(
            &file.stitch,
            &file.segmentum.stitch_seed,
            &children[0],
            &children[1],
            BorderOrientation::EastWest,
            &sectors,
            &mut idx_b,
        );
        assert_eq!(
            serde_json::to_string(&links_a).unwrap(),
            serde_json::to_string(&links_b).unwrap()
        );
        // Different stitch seed → different pairing (assuming bucket size > 1).
        let mut alt = file.clone();
        alt.segmentum.stitch_seed = "alternate".into();
        let mut idx_c = 1;
        let links_c = stitch_pair(
            &alt.stitch,
            &alt.segmentum.stitch_seed,
            &children[0],
            &children[1],
            BorderOrientation::EastWest,
            &sectors,
            &mut idx_c,
        );
        // The set of chosen systems is allowed to coincide if buckets collapse;
        // just assert determinism of the alternate stream too.
        let mut idx_d = 1;
        let links_d = stitch_pair(
            &alt.stitch,
            &alt.segmentum.stitch_seed,
            &children[0],
            &children[1],
            BorderOrientation::EastWest,
            &sectors,
            &mut idx_d,
        );
        assert_eq!(
            serde_json::to_string(&links_c).unwrap(),
            serde_json::to_string(&links_d).unwrap()
        );
    }

    #[test]
    fn validate_config_rejects_duplicate_slot() {
        let file = SegmentumFile {
            segmentum: SegmentumConfig {
                id: "seg".into(),
                title: "Seg".into(),
                stitch_seed: "s".into(),
                columns: 2,
                rows: 1,
                faction_mode: FactionMode::Shared,
            },
            stitch: StitchConfig::default(),
            children: vec![
                ChildEntry {
                    id: "a".into(),
                    project: "/x".into(),
                    column: 0,
                    row: 0,
                    seed: None,
                    title: None,
                },
                ChildEntry {
                    id: "b".into(),
                    project: "/y".into(),
                    column: 0,
                    row: 0,
                    seed: None,
                    title: None,
                },
            ],
        };
        let err = validate_config(&file).unwrap_err();
        assert!(matches!(err, SectorError::InvalidConfig(_)));
    }
}
