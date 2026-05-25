//! Deterministic system placement on the sector hex grid.

use rand::Rng;

use crate::config::AppConfig;
use crate::errors::SectorError;
use crate::rng;
use crate::sector_model::{hex_distance, HexCoord};

pub(super) fn place_systems(config: &AppConfig) -> Result<Vec<HexCoord>, SectorError> {
    let g = &config.generation;
    let width = g.sector_width as i32;
    let height = g.sector_height as i32;
    let total_cells = (width * height) as usize;

    let target = g.system_count.min(total_cells);
    if target == 0 {
        return Ok(Vec::new());
    }
    if target > total_cells {
        return Err(SectorError::InvalidConfig(format!(
            "system_count {} > grid cells {}",
            g.system_count, total_cells
        )));
    }

    let mut all: Vec<HexCoord> = Vec::with_capacity(total_cells);
    for r in 0..height {
        for q in 0..width {
            all.push(HexCoord { q, r });
        }
    }

    let mut rng = rng::stage_rng(&g.seed, "placement", "sector");
    // Fisher-Yates with rng.gen_range — deterministic given seed.
    for i in (1..all.len()).rev() {
        let j = rng.gen_range(0..=i);
        all.swap(i, j);
    }

    let mut placed: Vec<HexCoord> = Vec::with_capacity(target);
    let mut leftover: Vec<HexCoord> = Vec::with_capacity(all.len().saturating_sub(target));
    let min_dist = g.placement.minimum_system_distance;
    for c in all {
        if placed.len() >= target {
            break;
        }
        if min_dist <= 1 || placed.iter().all(|p| hex_distance(*p, c) >= min_dist) {
            placed.push(c);
        } else {
            leftover.push(c);
        }
    }

    if placed.len() < target {
        // Couldn't satisfy minimum distance — relax constraint by progressively
        // shrinking it, still consuming the shuffled leftover pool so fill stays
        // spatially scattered rather than packed in grid order.
        let mut relaxed = min_dist;
        while placed.len() < target && relaxed > 1 {
            relaxed -= 1;
            let mut still_blocked: Vec<HexCoord> = Vec::with_capacity(leftover.len());
            for c in std::mem::take(&mut leftover) {
                if placed.len() >= target {
                    still_blocked.push(c);
                    continue;
                }
                if relaxed <= 1 || placed.iter().all(|p| hex_distance(*p, c) >= relaxed) {
                    placed.push(c);
                } else {
                    still_blocked.push(c);
                }
            }
            leftover = still_blocked;
        }
        // Final fallback: any remaining shuffled cells.
        for c in leftover {
            if placed.len() >= target {
                break;
            }
            placed.push(c);
        }
    }

    // Sort so output ordering is deterministic regardless of shuffle order.
    placed.sort();
    Ok(placed)
}
