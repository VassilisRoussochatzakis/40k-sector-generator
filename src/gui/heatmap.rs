//! GUI wrapper around the pure heatmap scoring in [`crate::heatmap`].
//!
//! Just re-exports `HeatmapMode` and converts the RGB cells into `Color32`
//! cells so the egui sector view can paint them directly.

use std::{collections::HashMap, sync::Arc};

use egui::Color32;

use crate::sector_model::GeneratedSector;

pub use crate::heatmap::HeatmapMode;

/// One per-hex sample: tint colour plus a 0..=1 intensity.
#[derive(Debug, Clone, Copy)]
pub struct HeatCell {
    pub color: Color32,
    pub intensity: f32,
}

pub type HeatmapCells = HashMap<crate::ids::SystemId, HeatCell>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeatmapCacheKey {
    sector_id: String,
    seed: String,
    width: u32,
    height: u32,
    system_count: usize,
    route_count: usize,
    faction_count: usize,
    region_count: usize,
    economy_system_count: usize,
    economy_route_count: usize,
}

impl HeatmapCacheKey {
    fn from_sector(sector: &GeneratedSector) -> Self {
        Self {
            sector_id: sector.id.clone(),
            seed: sector.seed.clone(),
            width: sector.width,
            height: sector.height,
            system_count: sector.systems.len(),
            route_count: sector.routes.len(),
            faction_count: sector.factions.len(),
            region_count: sector.regions.len(),
            economy_system_count: sector.economy.systems.len(),
            economy_route_count: sector.economy.routes.len(),
        }
    }
}

#[derive(Debug, Default)]
pub struct HeatmapCache {
    key: Option<HeatmapCacheKey>,
    mode: HeatmapMode,
    cells: Option<Arc<HeatmapCells>>,
}

impl HeatmapCache {
    pub fn get_or_compute(
        &mut self,
        sector: &GeneratedSector,
        mode: HeatmapMode,
    ) -> Option<Arc<HeatmapCells>> {
        if matches!(mode, HeatmapMode::Off) {
            return None;
        }

        let key = HeatmapCacheKey::from_sector(sector);
        let needs_recompute =
            self.key.as_ref() != Some(&key) || self.mode != mode || self.cells.is_none();

        if needs_recompute {
            self.key = Some(key);
            self.mode = mode;
            self.cells = Some(Arc::new(compute(sector, mode)));
        }

        self.cells.clone()
    }

    pub fn invalidate(&mut self) {
        self.key = None;
        self.cells = None;
    }
}

/// Computed heatmap values keyed by system id.
pub fn compute(sector: &GeneratedSector, mode: HeatmapMode) -> HeatmapCells {
    crate::heatmap::compute_rgb(sector, mode)
        .into_iter()
        .map(|(k, c)| {
            (
                k,
                HeatCell {
                    color: Color32::from_rgb(c.rgb.0, c.rgb.1, c.rgb.2),
                    intensity: c.intensity,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn cache_reuses_cells_until_key_or_mode_changes() {
        let mut sector = crate::gui::editor::state::empty_sector("cache", "Cache", "seed", 4, 4);
        let mut cache = HeatmapCache::default();

        let first = cache
            .get_or_compute(&sector, HeatmapMode::Threat)
            .expect("enabled heatmap");
        let second = cache
            .get_or_compute(&sector, HeatmapMode::Threat)
            .expect("enabled heatmap");
        assert!(Arc::ptr_eq(&first, &second));

        let changed_mode = cache
            .get_or_compute(&sector, HeatmapMode::Trade)
            .expect("enabled heatmap");
        assert!(!Arc::ptr_eq(&first, &changed_mode));

        sector.width += 1;
        let changed_sector = cache
            .get_or_compute(&sector, HeatmapMode::Trade)
            .expect("enabled heatmap");
        assert!(!Arc::ptr_eq(&changed_mode, &changed_sector));
    }

    #[test]
    fn cache_returns_none_for_off_mode() {
        let sector = crate::gui::editor::state::empty_sector("cache", "Cache", "seed", 4, 4);
        let mut cache = HeatmapCache::default();

        assert!(cache.get_or_compute(&sector, HeatmapMode::Off).is_none());
    }
}
