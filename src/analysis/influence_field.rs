//! Continuous influence layers (§9 NEXT.md, design §9.3 / §9.5).
//!
//! Builds a deterministic Voronoi-style assignment of every grid cell in
//! the sector to a faction, weighted by per-system aggregate presence and
//! decaying with squared hex distance. The result is:
//!
//! * `cells`: a `Vec<CellAssignment>` (row-major: `r * width + q`) giving
//!   the dominant faction and its influence score for every cell. Cells
//!   with no influence (no faction within reach) get
//!   `dominant = None`.
//! * `bands`: a list of `TerritoryBand`s — one per faction with at least
//!   one cell — listing the cells it owns. The GUI renders bands as soft
//!   polygons.
//!
//! Power is supplied externally via per-faction system anchors. Hidden
//! factions still contribute to their cells but with a low cap so they
//! don't smear the public-facing map.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::FactionId;
use crate::sector_model::{hex_distance, GeneratedSector, HexCoord};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct InfluenceField {
    pub width: u32,
    pub height: u32,
    /// Row-major: `cells[r * width + q]`.
    pub cells: Vec<CellAssignment>,
    pub bands: Vec<TerritoryBand>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CellAssignment {
    pub q: i32,
    pub r: i32,
    pub dominant: Option<crate::ids::FactionId>,
    /// 0..=100 — normalised contribution of the dominant faction at this cell.
    pub score: u8,
    /// Top-3 (faction_id, score) by descending score.
    #[serde(default)]
    pub top: Vec<(crate::ids::FactionId, u8)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TerritoryBand {
    pub faction_id: crate::ids::FactionId,
    /// Row-major indices into `cells`.
    pub cells: Vec<usize>,
}

const MAX_INFLUENCE_RADIUS: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InfluenceFieldProgress {
    Started {
        systems: usize,
        anchors: usize,
        cells: usize,
        radius: u32,
    },
    AnchorsProjected {
        current: usize,
        total: usize,
        touched_cells: usize,
    },
    CellsResolved {
        current: usize,
        total: usize,
        claimed_cells: usize,
    },
    BandsBuilt {
        bands: usize,
        claimed_cells: usize,
    },
    Complete {
        cells: usize,
        bands: usize,
    },
}

/// Build a continuous influence field for the given sector. Uses each
/// system's per-faction control sums as anchors; influence at distance
/// `d` from the anchor decays as `1 / (1 + d²)`.
#[must_use]
pub fn build(sector: &GeneratedSector) -> InfluenceField {
    build_with_progress(sector, |_| {})
}

pub fn build_with_progress<F>(sector: &GeneratedSector, mut progress: F) -> InfluenceField
where
    F: FnMut(InfluenceFieldProgress),
{
    let w = sector.width;
    let h = sector.height;
    let width = w as usize;
    let total = width.saturating_mul(h as usize);
    let mut cells: Vec<CellAssignment> = (0..total)
        .map(|i| {
            let q = if width == 0 { 0 } else { (i % width) as i32 };
            let r = i.checked_div(width).unwrap_or(0) as i32;
            CellAssignment {
                q,
                r,
                dominant: None,
                score: 0,
                top: Vec::new(),
            }
        })
        .collect();

    // Per-system per-faction aggregate presence (anchor strengths).
    let mut raw_anchors: Vec<(HexCoord, BTreeMap<FactionId, f32>)> = Vec::new();
    for sys in &sector.systems {
        let mut m: BTreeMap<FactionId, f32> = BTreeMap::new();
        for wld in &sys.worlds {
            for p in &wld.factions {
                *m.entry(p.faction_id.clone()).or_insert(0.0) += p.dimensions.local_control_score();
            }
        }
        if !m.is_empty() {
            raw_anchors.push((sys.coord, m));
        }
    }
    let mut faction_ids: Vec<FactionId> = Vec::new();
    let mut faction_index: BTreeMap<FactionId, usize> = BTreeMap::new();
    let anchors: Vec<(HexCoord, Vec<(usize, f32)>)> = raw_anchors
        .into_iter()
        .map(|(coord, scores)| {
            let indexed = scores
                .into_iter()
                .map(|(id, score)| {
                    let index = if let Some(index) = faction_index.get(&id) {
                        *index
                    } else {
                        let index = faction_ids.len();
                        faction_ids.push(id.clone());
                        faction_index.insert(id, index);
                        index
                    };
                    (index, score)
                })
                .collect();
            (coord, indexed)
        })
        .collect();
    progress(InfluenceFieldProgress::Started {
        systems: sector.systems.len(),
        anchors: anchors.len(),
        cells: total,
        radius: MAX_INFLUENCE_RADIUS,
    });

    let faction_count = faction_ids.len();
    // Dense per-cell × per-faction accumulator. Deliberately not sparse
    // (TF-S-5): `build` runs once per generation, not per frame, and the dense
    // array is measured cheap — `cargo bench --bench influence_field` reports
    // 19 µs (10×10) → 187 µs (30×30, 200 systems), flat ~4.8 Melem/s (linear in
    // cells, no blow-up). Memory stays KB-scale (m42: 13 factions → 47 KB at
    // 30×30). A sparse map would slow the hot indexed write below to save
    // trivially little. Re-measure with the bench before switching.
    let mut cell_scores: Vec<f32> = vec![0.0; total.saturating_mul(faction_count)];
    let mut cell_touched: Vec<bool> = vec![false; total];
    let mut touched_cells = 0usize;
    if w > 0 && h > 0 {
        let radius = MAX_INFLUENCE_RADIUS as i32;
        for (anchor_index, (anchor_coord, faction_scores)) in anchors.iter().enumerate() {
            let min_q = (anchor_coord.q - radius).max(0);
            let max_q = (anchor_coord.q + radius).min(w as i32 - 1);
            let min_r = (anchor_coord.r - radius).max(0);
            let max_r = (anchor_coord.r + radius).min(h as i32 - 1);
            if min_q <= max_q && min_r <= max_r {
                for q in min_q..=max_q {
                    for r in min_r..=max_r {
                        let coord = HexCoord { q, r };
                        let d = hex_distance(coord, *anchor_coord);
                        if d > MAX_INFLUENCE_RADIUS {
                            continue;
                        }
                        let index = (r as usize * width) + q as usize;
                        if !cell_touched[index] {
                            cell_touched[index] = true;
                            touched_cells += 1;
                        }
                        let falloff = 1.0 / (1 + d * d) as f32;
                        let score_base = index * faction_count;
                        for (id, s) in faction_scores {
                            let v = *s * falloff;
                            cell_scores[score_base + *id] += v;
                        }
                    }
                }
            }
            let current = anchor_index + 1;
            if should_report_influence_progress(current, anchors.len()) {
                progress(InfluenceFieldProgress::AnchorsProjected {
                    current,
                    total: anchors.len(),
                    touched_cells,
                });
            }
        }
    }

    let mut claimed_cells = 0usize;
    for (i, cell) in cells.iter_mut().enumerate() {
        if !cell_touched[i] {
            let current = i + 1;
            if should_report_influence_progress(current, total) {
                progress(InfluenceFieldProgress::CellsResolved {
                    current,
                    total,
                    claimed_cells,
                });
            }
            continue;
        }
        let score_base = i * faction_count;
        let mut top: Vec<(usize, f32)> = cell_scores[score_base..score_base + faction_count]
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, score)| *score > 0.0)
            .collect();
        if top.is_empty() {
            let current = i + 1;
            if should_report_influence_progress(current, total) {
                progress(InfluenceFieldProgress::CellsResolved {
                    current,
                    total,
                    claimed_cells,
                });
            }
            continue;
        }
        top.sort_by(|a, b| {
            crate::analysis::cmp_f32_desc(a.1, b.1).then(a.0.cmp(&b.0))
        });
        let max = top[0].1;
        // Normalise the top score against an arbitrary ceiling of 100
        // (single Dominant-level presence + distance 0). Higher values
        // clamp so the renderer doesn't blow out.
        claimed_cells += 1;
        cell.dominant = Some(faction_ids[top[0].0].clone());
        cell.score = max.min(100.0).round() as u8;
        cell.top = top
            .into_iter()
            .take(3)
            .map(|(id, s)| (faction_ids[id].clone(), s.min(100.0).round() as u8))
            .collect();
        let current = i + 1;
        if should_report_influence_progress(current, total) {
            progress(InfluenceFieldProgress::CellsResolved {
                current,
                total,
                claimed_cells,
            });
        }
    }

    // Build per-faction band lists.
    let mut bands_by_id: BTreeMap<FactionId, Vec<usize>> = BTreeMap::new();
    for (i, c) in cells.iter().enumerate() {
        if let Some(id) = &c.dominant {
            bands_by_id.entry(id.clone()).or_default().push(i);
        }
    }
    let bands: Vec<TerritoryBand> = bands_by_id
        .into_iter()
        .map(|(faction_id, cells)| TerritoryBand { faction_id, cells })
        .collect();
    progress(InfluenceFieldProgress::BandsBuilt {
        bands: bands.len(),
        claimed_cells,
    });
    progress(InfluenceFieldProgress::Complete {
        cells: cells.len(),
        bands: bands.len(),
    });

    InfluenceField {
        width: w,
        height: h,
        cells,
        bands,
    }
}

fn should_report_influence_progress(current: usize, total: usize) -> bool {
    current == 1 || current == total || current.is_multiple_of(influence_progress_stride(total))
}

fn influence_progress_stride(total: usize) -> usize {
    match total {
        0..=25 => 1,
        26..=100 => 10,
        101..=500 => 25,
        501..=10_000 => 100,
        10_001..=100_000 => 1_000,
        _ => 10_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sector_yields_empty_field() {
        let s = crate::sector_model::GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "t".into(),
            generator_name: "t".into(),
            generator_version: "t".into(),
            width: 0,
            height: 0,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: crate::sector_model::GenerationManifest {
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
        };
        let f = build(&s);
        assert!(f.cells.is_empty());
    }
}
