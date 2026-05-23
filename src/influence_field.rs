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

/// Build a continuous influence field for the given sector. Uses each
/// system's per-faction control sums as anchors; influence at distance
/// `d` from the anchor decays as `1 / (1 + d²)`.
#[must_use]
pub fn build(sector: &GeneratedSector) -> InfluenceField {
    let w = sector.width;
    let h = sector.height;
    let total = (w * h) as usize;
    let mut cells: Vec<CellAssignment> = (0..total)
        .map(|i| {
            let q = (i as u32 % w) as i32;
            let r = (i as u32 / w) as i32;
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
    let mut anchors: Vec<(HexCoord, BTreeMap<crate::ids::FactionId, f32>)> = Vec::new();
    for sys in &sector.systems {
        let mut m: BTreeMap<crate::ids::FactionId, f32> = BTreeMap::new();
        for wld in &sys.worlds {
            for p in &wld.factions {
                *m.entry(p.faction_id.clone()).or_insert(0.0) += p.dimensions.local_control_score();
            }
        }
        anchors.push((sys.coord, m));
    }

    // Cell-major projection.
    for (i, cell) in cells.iter_mut().enumerate() {
        let coord = HexCoord {
            q: (i as u32 % w) as i32,
            r: (i as u32 / w) as i32,
        };
        let mut scores: BTreeMap<crate::ids::FactionId, f32> = BTreeMap::new();
        for (anchor_coord, faction_scores) in &anchors {
            let d = hex_distance(coord, *anchor_coord);
            if d > MAX_INFLUENCE_RADIUS {
                continue;
            }
            let falloff = 1.0 / (1 + d * d) as f32;
            for (id, s) in faction_scores {
                let v = *s * falloff;
                *scores.entry(id.clone()).or_insert(0.0) += v;
            }
        }
        if scores.is_empty() {
            continue;
        }
        let mut top: Vec<(crate::ids::FactionId, f32)> = scores.into_iter().collect();
        top.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let max = top[0].1;
        // Normalise the top score against an arbitrary ceiling of 100
        // (single Dominant-level presence + distance 0). Higher values
        // clamp so the renderer doesn't blow out.
        cell.dominant = Some(top[0].0.clone());
        cell.score = max.min(100.0).round() as u8;
        cell.top = top
            .into_iter()
            .take(3)
            .map(|(id, s)| (id, s.min(100.0).round() as u8))
            .collect();
    }

    // Build per-faction band lists.
    let mut bands_by_id: BTreeMap<crate::ids::FactionId, Vec<usize>> = BTreeMap::new();
    for (i, c) in cells.iter().enumerate() {
        if let Some(id) = &c.dominant {
            bands_by_id.entry(id.clone()).or_default().push(i);
        }
    }
    let bands: Vec<TerritoryBand> = bands_by_id
        .into_iter()
        .map(|(faction_id, cells)| TerritoryBand { faction_id, cells })
        .collect();

    InfluenceField {
        width: w,
        height: h,
        cells,
        bands,
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
