//! Deterministic system placement on the sector hex grid.

use rand::Rng;

use crate::config::AppConfig;
use crate::errors::SectorError;
use crate::rng;
use crate::sector_model::{hex_distance, HexCoord};

/// Deterministic system placement on the sector hex grid.
///
/// `reroll_suffix` is folded into the RNG discriminator: an empty suffix
/// reproduces the legacy `("placement","sector")` key exactly (byte-identical);
/// a `":r{n}"` suffix yields a deterministic distinct layout. The one-shot path
/// passes `""` via [`super::generate_with_progress_and_cancel`].
pub(super) fn place_systems_reroll(
    config: &AppConfig,
    reroll_suffix: &str,
) -> Result<Vec<HexCoord>, SectorError> {
    let g = &config.generation;
    let width = g.sector_width as i32;
    let height = g.sector_height as i32;
    // Cast each operand before multiplying: `(width * height) as usize` would
    // multiply in `i32` first and overflow on absurd dims. (Validation caps
    // dims well below the overflow threshold; this is defense in depth.)
    let total_cells = (width as usize) * (height as usize);

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

    let mut rng = rng::stage_rng(&g.seed, "placement", &format!("sector{reroll_suffix}"));
    // Fisher-Yates with rng.gen_range — deterministic given seed.
    for i in (1..all.len()).rev() {
        let j = rng.gen_range(0..=i);
        all.swap(i, j);
    }

    let min_dist = g.placement.minimum_system_distance;
    // cluster_bias > 0 opts into clumpy placement; 0 keeps the byte-identical
    // scattered path below (legacy goldens depend on it).
    let bias = g.placement.cluster_bias.clamp(0.0, 1.0);

    let mut placed: Vec<HexCoord> = Vec::with_capacity(target);
    if bias > 0.0 {
        // Clustered placement: each new system either GROWS a cluster (prob =
        // cluster_bias — pulled to the free cell nearest a random already-placed
        // system) or SCATTERS to a fresh min-distance cell. bias→1 ⇒ one tight
        // blob; smaller bias ⇒ several looser clusters approaching the scatter
        // layout. ponytail: O(target·cells) per-step scan — fine for the
        // validation-capped sector dims; swap for a spatial index only if dims
        // ever grow huge.
        let mut pool = all;
        while placed.len() < target && !pool.is_empty() {
            let idx = if !placed.is_empty() && rng.gen_bool(bias) {
                let anchor = placed[rng.gen_range(0..placed.len())];
                (0..pool.len())
                    .min_by_key(|&i| hex_distance(pool[i], anchor))
                    .unwrap()
            } else {
                // First free cell respecting min_dist; fall back to the front of
                // the shuffled pool if none qualifies (mirrors the cascade below).
                (0..pool.len())
                    .find(|&i| {
                        min_dist <= 1
                            || placed.iter().all(|p| hex_distance(*p, pool[i]) >= min_dist)
                    })
                    .unwrap_or(0)
            };
            placed.push(pool.remove(idx));
        }
    } else {
        let mut leftover: Vec<HexCoord> = Vec::with_capacity(all.len().saturating_sub(target));
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
    }

    // Sort so output ordering is deterministic regardless of shuffle order.
    placed.sort();
    Ok(placed)
}

/// Legacy (no-reroll) placement — equivalent to `place_systems_reroll(config,
/// "")`. Retained for the byte-contract tests below; the pipeline always calls
/// the reroll-aware variant directly.
#[cfg(test)]
fn place_systems(config: &AppConfig) -> Result<Vec<HexCoord>, SectorError> {
    place_systems_reroll(config, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random_sector::{build_random_config, SectorSize};

    /// Helper: a square 4×4 config with the placement knobs the cascade tests
    /// need. `build_random_config` already sets width/height for `Custom{dim:4}`
    /// and seeds `generation.seed`; the explicit overrides make the intent
    /// self-documenting and robust to changes in the roller's defaults.
    fn config_4x4(seed: &str, system_count: usize, min_dist: u32) -> AppConfig {
        let mut cfg = build_random_config(seed, SectorSize::Custom { dim: 4 });
        cfg.generation.seed = seed.to_string();
        cfg.generation.sector_width = 4;
        cfg.generation.sector_height = 4;
        cfg.generation.system_count = system_count;
        cfg.generation.placement.minimum_system_distance = min_dist;
        cfg
    }

    // GAP 167: 4×4, count=12, min_dist=3 → the relaxation cascade reaches the
    // target (12 ≤ 16 cells), the returned Vec is sorted, and two runs are
    // byte-identical (same seed ⇒ identical Fisher-Yates shuffle).
    #[test]
    fn place_systems_relaxation_cascade_reaches_target_sorted_and_deterministic() {
        let cfg = config_4x4("place-cascade", 12, 3);
        let placed = place_systems(&cfg).expect("placement ok");

        // The cascade's final fallback loop fills any remaining shuffled cells
        // until placed.len() == target. target=12 ≤ total_cells=16, so it is
        // always reached — guaranteed by construction, not seed-dependent.
        assert_eq!(placed.len(), 12);

        // Output is sorted (place_systems calls `placed.sort()` last). Ordering
        // is derived `Ord` on HexCoord{q, r}: lexicographic by q then r.
        let mut sorted = placed.clone();
        sorted.sort();
        assert_eq!(placed, sorted, "place_systems output must be sorted");

        // Determinism: same seed ⇒ byte-identical placement.
        let again = place_systems(&cfg).unwrap();
        assert_eq!(placed, again);

        // All coords in-bounds for a 4×4 grid.
        assert!(placed
            .iter()
            .all(|c| (0..4).contains(&c.q) && (0..4).contains(&c.r)));

        // No duplicate coordinates.
        let set: std::collections::BTreeSet<_> = placed.iter().copied().collect();
        assert_eq!(set.len(), 12);
    }

    // GAP 168: count==0 returns Vec::new(); count==1 returns exactly one
    // in-bounds coordinate.
    #[test]
    fn place_systems_count_zero_returns_empty() {
        let cfg = config_4x4("place-zero", 0, 1);
        let placed = place_systems(&cfg).unwrap();
        assert!(placed.is_empty());
        assert_eq!(placed.len(), 0);
    }

    #[test]
    fn place_systems_count_one_places_single_in_bounds() {
        let cfg = config_4x4("place-one", 1, 1);
        let placed = place_systems(&cfg).unwrap();
        assert_eq!(placed.len(), 1);
        let c = placed[0];
        assert!((0..4).contains(&c.q) && (0..4).contains(&c.r));
    }

    // GAP 174: regression guard for the cast ordering at the top of
    // `place_systems` — `(width as usize) * (height as usize)` casts each
    // operand to usize BEFORE multiplying, so the product does not overflow i32
    // on absurd dims. This is a pure arithmetic mirror of placement.rs's
    // `total_cells` computation; it deliberately does NOT call `place_systems`
    // with these dims (that would attempt Vec::with_capacity(~2.1e9) → OOM, and
    // validation caps real dims at MAX_CUSTOM_DIM anyway).
    #[test]
    fn total_cells_cast_does_not_overflow_i32() {
        // 46341² = 2_147_488_281 > i32::MAX (2_147_483_647). The buggy
        // `(width * height) as usize` would multiply in i32 first and panic in
        // debug builds; the fn's per-operand cast avoids that.
        let width: u32 = 46_341;
        let height: u32 = 46_341;
        let total_cells = (width as usize) * (height as usize); // mirror placement.rs
        assert_eq!(total_cells, 2_147_488_281usize);
        // Sanity: the naive i32 path would have overflowed.
        assert!(total_cells > i32::MAX as usize);
    }

    // cluster_bias must actually clump: on a roomy grid with few systems, the
    // mean nearest-neighbour distance at bias=1.0 is strictly tighter than the
    // scattered bias=0.0 layout. If this fails, cluster_bias is dead again.
    #[test]
    fn clustered_placement_is_tighter_and_deterministic() {
        let make = |bias: f64| {
            let mut cfg = build_random_config("cluster-test", SectorSize::Custom { dim: 16 });
            cfg.generation.seed = "cluster-test".to_string();
            cfg.generation.sector_width = 16;
            cfg.generation.sector_height = 16;
            cfg.generation.system_count = 12;
            cfg.generation.placement.minimum_system_distance = 1;
            cfg.generation.placement.cluster_bias = bias;
            cfg
        };

        let scattered = place_systems_reroll(&make(0.0), "").unwrap();
        let clustered = place_systems_reroll(&make(1.0), "").unwrap();
        assert_eq!(scattered.len(), 12);
        assert_eq!(clustered.len(), 12);

        let mean_nn = |pts: &[HexCoord]| -> f64 {
            let sum: u32 = pts
                .iter()
                .map(|&a| {
                    pts.iter()
                        .filter(|&&b| b != a)
                        .map(|&b| hex_distance(a, b))
                        .min()
                        .unwrap()
                })
                .sum();
            sum as f64 / pts.len() as f64
        };
        assert!(
            mean_nn(&clustered) < mean_nn(&scattered),
            "clustered mean-NN {} should be < scattered {}",
            mean_nn(&clustered),
            mean_nn(&scattered)
        );

        // Same seed ⇒ identical clustered layout (RNG via stage_rng).
        let again = place_systems_reroll(&make(1.0), "").unwrap();
        assert_eq!(clustered, again);
    }
}
