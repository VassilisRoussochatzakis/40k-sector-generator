//! Per-frame map lookup caches: hex→{subsector,system,region} tables, region
//! centroids, and pre-resolved system labels / faction styles. Split verbatim
//! from the former `sector_view.rs` god-file (AREA_F F3).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use egui::{Pos2, Shape};

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{GeneratedSector, HexCoord};
use sectorforge::subsectors::Subsector;

use crate::palette::{faction_style_by_id, FactionStyle};

/// Memoized §BEAUTY star-dust shapes for one map rect (F10). Deterministic in the
/// rect only, so it is rebuilt just when the rect's rounded position/size changes
/// — not the per-frame hash loop the inline paint did.
pub struct StarDust {
    /// Rounded `(min.x, min.y, width, height)` the `shapes` were built for.
    pub key: (u32, u32, u32, u32),
    pub shapes: Vec<Shape>,
}

pub struct SectorMapCache {
    pub hex_subsector: HashMap<(i32, i32), String>,
    pub hex_system: HashMap<(i32, i32), sectorforge::ids::SystemId>,
    pub hex_region: HashMap<(i32, i32), (String, RegionConditionKind)>,
    pub region_centroids: HashMap<String, Pos2>,
    /// TF-P-3: pre-`to_ascii_uppercase`-d display label per system id. The hot
    /// consumer is the map label render (`render`, pass 1 obstacle measure +
    /// pass 2 paint), which uppercases every visible system's name twice per
    /// frame; this hoists that transform to one build per cache rebuild. Stored
    /// ASCII-upper to match the map's `to_ascii_uppercase` draw exactly (a
    /// `SystemId`-keyed `Arc<str>` so lookups are O(log n) and clone-cheap).
    pub system_label_cache: BTreeMap<SystemId, Arc<str>>,
    /// TF-P-4: per-faction style lookup built once per cache rebuild. Replaces
    /// the O(N) `faction_style_by_id` scan that was firing per-route + per
    /// system in info_panel / control panels.
    pub faction_style_index: BTreeMap<FactionId, FactionStyle>,
    /// F10: lazily-memoized star-dust shapes for the current map rect (live-only
    /// void flourish). Interior-mutable so the render path can fill it through the
    /// shared `&SectorMapCache` it holds; rebuilt only when the rect changes.
    pub star_dust: RefCell<Option<StarDust>>,
}

impl SectorMapCache {
    pub fn new(sector: &GeneratedSector, subsectors: &[Subsector]) -> Self {
        let mut hex_subsector = HashMap::new();
        for s in subsectors {
            for &(q, r) in &s.hex_cells {
                hex_subsector.insert((q as i32, r as i32), s.id.as_ref().to_string());
            }
        }

        let mut hex_system = HashMap::new();
        for sys in &sector.systems {
            hex_system.insert((sys.coord.q, sys.coord.r), sys.id.clone());
        }

        let mut hex_region = HashMap::new();
        let mut region_hex_counts: HashMap<String, (f32, f32, f32)> = HashMap::new();

        for reg in sector.regions.iter() {
            let mut sx = 0.0;
            let mut sy = 0.0;
            for h in &reg.hexes {
                hex_region.insert((h.q, h.r), (reg.id.to_string(), reg.kind));
                sx += h.q as f32;
                sy += h.r as f32;
            }
            if !reg.hexes.is_empty() {
                let n = reg.hexes.len() as f32;
                region_hex_counts.insert(reg.id.to_string(), (sx / n, sy / n, n));
            }
        }

        let mut region_centroids = HashMap::new();
        // Centroids are stored in axial coords (q, r) for now, will be projected in render.
        for (id, (q, r, _)) in region_hex_counts {
            region_centroids.insert(id, Pos2::new(q, r));
        }

        let mut system_label_cache: BTreeMap<SystemId, Arc<str>> = BTreeMap::new();
        for sys in &sector.systems {
            let label: Arc<str> = Arc::from(sys.name.to_ascii_uppercase());
            system_label_cache.insert(sys.id.clone(), label);
        }

        let mut faction_style_index: BTreeMap<FactionId, FactionStyle> = BTreeMap::new();
        for f in &sector.factions {
            let style = faction_style_by_id(&sector.factions, f.id.as_ref());
            faction_style_index.insert(f.id.clone(), style);
        }

        Self {
            hex_subsector,
            hex_system,
            hex_region,
            region_centroids,
            system_label_cache,
            faction_style_index,
            star_dust: RefCell::new(None),
        }
    }

    /// O(log n) lookup for the pre-`to_ascii_uppercase`-d system display label.
    #[must_use]
    pub fn system_label(&self, id: &SystemId) -> Option<&Arc<str>> {
        self.system_label_cache.get(id)
    }

    /// O(log n) lookup for a faction's pre-resolved style. Takes any
    /// `&str`-like id (`FactionId: Borrow<str>`) so callers holding a `&str`
    /// or `&FactionId` can query without allocating a key.
    #[must_use]
    pub fn faction_style(&self, id: &str) -> Option<&FactionStyle> {
        self.faction_style_index.get(id)
    }

    /// §CTX1 Phase 5 — region id containing `coord`, if any. O(1) lookup over
    /// the precomputed `hex_region` table. Used by `panels/map.rs` to surface
    /// the §6.5 region-hex context menu.
    #[must_use]
    pub fn region_for_hex(&self, coord: HexCoord) -> Option<&str> {
        self.hex_region
            .get(&(coord.q, coord.r))
            .map(|(id, _)| id.as_str())
    }
}
