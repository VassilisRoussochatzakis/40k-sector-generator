//! Sector overview render + the `SectorOverviewCache` bucket cache. Split
//! verbatim from the former `info_panel.rs` god-file (AREA_F F8, by section).

use std::sync::Arc;

use egui::{Color32, Ui};

use sectorforge::sector_model::{
    GeneratedSector, RoutePattern,
};

use crate::palette::{
    self, faction_style_by_id, stability_color, PATH_HIGHLIGHT,
};
use crate::sector_view::SectorMapCache;
use sectorforge::importance::{
    compute_display_buckets, DisplayBucket, DEFAULT_DISPLAY_CAP, DEFAULT_MINOR_FRACTION,
};

use super::{dim, legend_control_row, legend_row, legend_route_row, section, short, title};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectorOverviewCacheKey {
    sector_id: String,
    seed: String,
    width: u32,
    height: u32,
    system_count: usize,
    world_count: usize,
    route_count: usize,
    faction_count: usize,
}

impl SectorOverviewCacheKey {
    fn from_sector(sector: &GeneratedSector) -> Self {
        Self {
            sector_id: sector.id.to_string(),
            seed: sector.seed.to_string(),
            width: sector.width,
            height: sector.height,
            system_count: sector.systems.len(),
            world_count: sector.systems.iter().map(|s| s.worlds.len()).sum(),
            route_count: sector.routes.len(),
            faction_count: sector.factions.len(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SectorOverviewCache {
    key: Option<SectorOverviewCacheKey>,
    buckets: Option<Arc<Vec<DisplayBucket>>>,
}

impl SectorOverviewCache {
    pub fn buckets_for(&mut self, sector: &GeneratedSector) -> Arc<Vec<DisplayBucket>> {
        let key = SectorOverviewCacheKey::from_sector(sector);
        if self.key.as_ref() != Some(&key) || self.buckets.is_none() {
            self.key = Some(key);
            self.buckets = Some(Arc::new(compute_display_buckets(
                sector,
                DEFAULT_MINOR_FRACTION,
                DEFAULT_DISPLAY_CAP,
            )));
        }
        self.buckets.clone().unwrap_or_default()
    }

    pub fn invalidate(&mut self) {
        self.key = None;
        self.buckets = None;
    }
}

pub fn sector_overview(
    ui: &mut Ui,
    sector: &GeneratedSector,
    mode: sectorforge::sector_model::RouteViewMode,
) {
    let buckets = compute_display_buckets(sector, DEFAULT_MINOR_FRACTION, DEFAULT_DISPLAY_CAP);
    sector_overview_with_buckets(ui, sector, &buckets, mode, None);
}

pub fn sector_overview_with_buckets(
    ui: &mut Ui,
    sector: &GeneratedSector,
    buckets: &[DisplayBucket],
    mode: sectorforge::sector_model::RouteViewMode,
    cache: Option<&SectorMapCache>,
) {
    title(ui, &format!("SECTOR: {}", sector.id.to_uppercase()));
    dim(ui, &format!("SEED: {}", short(&sector.seed, 20)));
    dim(
        ui,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.manifest.world_count,
        ),
    );
    ui.add_space(8.0);

    section(ui, "ROUTE TYPE");
    match mode {
        sectorforge::sector_model::RouteViewMode::Detailed => {
            for rtype in sectorforge::sector_model::RouteType::ALL {
                legend_route_row(
                    ui,
                    palette::chrome_text(),
                    rtype.pattern(mode),
                    rtype.label(),
                );
            }
        }
        sectorforge::sector_model::RouteViewMode::TopLevel => {
            for kind in sectorforge::sector_model::RouteKind::ALL {
                legend_route_row(ui, palette::chrome_text(), kind.patterns()[0], kind.label());
            }
        }
        _ => {}
    }
    legend_route_row(ui, PATH_HIGHLIGHT, RoutePattern::Solid, "PLANNED PATH");
    ui.add_space(8.0);

    section(ui, "ROUTE STABILITY");
    for (stab, name) in [
        (sectorforge::sector_model::RouteStability::Stable, "STABLE"),
        (
            sectorforge::sector_model::RouteStability::Unstable,
            "UNSTABLE",
        ),
        (
            sectorforge::sector_model::RouteStability::Hazardous,
            "HAZARDOUS",
        ),
        (
            sectorforge::sector_model::RouteStability::Perilous,
            "PERILOUS",
        ),
    ] {
        legend_row(ui, stability_color(stab), name);
    }
    ui.add_space(8.0);

    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        section(ui, "ROUTE CONTROL");
        legend_control_row(ui, "PATROL", "PATROL");
        legend_control_row(ui, "TOLL", "TOLL");
        legend_control_row(ui, "INTERDICTION", "INTERDICTION");
        legend_control_row(ui, "PIRACY", "PIRACY");
        ui.add_space(8.0);
    }

    if !sector.factions.is_empty() {
        section(ui, "FACTIONS");
        for b in buckets {
            match b {
                DisplayBucket::Faction {
                    id,
                    name,
                    system_count,
                    world_count,
                    ..
                } => {
                    let style = cache
                        .and_then(|c| c.faction_style(id).copied())
                        .unwrap_or_else(|| faction_style_by_id(&sector.factions, id));
                    legend_row(
                        ui,
                        style.fill,
                        &format!(
                            "{}  {}S {}W",
                            short(&name.to_uppercase(), 18),
                            system_count,
                            world_count,
                        ),
                    );
                }
                DisplayBucket::Aggregated {
                    label,
                    system_count,
                    world_count,
                    ..
                } => {
                    legend_row(
                        ui,
                        Color32::from_rgb(140, 140, 150),
                        &format!(
                            "{}  {}S {}W",
                            short(&label.to_uppercase(), 18),
                            system_count,
                            world_count,
                        ),
                    );
                }
                _ => {}
            }
        }
        ui.add_space(8.0);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn overview_cache_reuses_buckets_until_sector_key_changes() {
        let mut sector = GeneratedSector::empty("cache", "Cache", "seed", 4, 4);
        let mut cache = SectorOverviewCache::default();

        let first = cache.buckets_for(&sector);
        let second = cache.buckets_for(&sector);
        assert!(Arc::ptr_eq(&first, &second));

        sector.height += 1;
        let third = cache.buckets_for(&sector);
        assert!(!Arc::ptr_eq(&second, &third));
    }

    #[test]
    fn overview_cache_invalidate_drops_buckets() {
        let sector = GeneratedSector::empty("cache", "Cache", "seed", 4, 4);
        let mut cache = SectorOverviewCache::default();

        let first = cache.buckets_for(&sector);
        cache.invalidate();
        let second = cache.buckets_for(&sector);
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
