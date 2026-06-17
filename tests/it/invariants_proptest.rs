//! Property-based fuzz tests for spec §11.11 post-generation invariants.
//!
//! Loads the bundled m42_project fixture once, then mutates generation
//! parameters across random seeds + sector sizes + system counts and asserts
//! every resulting sector passes invariants.

use proptest::prelude::*;
use sectorforge::{generate_sector, load_project, validate_sector};

use crate::shared::fixture_dir;

fn run_one(
    seed: &str,
    width: u32,
    height: u32,
    system_count: usize,
    min_worlds: usize,
    max_worlds: usize,
    route_density: f64,
) -> Result<(), String> {
    let mut input = load_project(fixture_dir()).map_err(|e| e.to_string())?;
    input.config.generation.seed = seed.to_string();
    input.config.generation.sector_width = width;
    input.config.generation.sector_height = height;
    input.config.generation.system_count = system_count;
    input.config.generation.min_worlds_per_system = min_worlds;
    input.config.generation.max_worlds_per_system = max_worlds;
    input.config.generation.routes.route_density = route_density;

    let sector = generate_sector(input).map_err(|e| e.to_string())?;
    let report = validate_sector(&sector);
    if !report.ok {
        return Err(format!(
            "seed={seed} w={width} h={height} n={system_count}: violations={:?}",
            report.violations
        ));
    }
    Ok(())
}

// G7: `invariants_hold_across_random_inputs` derives system_count from
// width*height, so extreme densities are never reached. This strategy varies
// system_count independently while keeping dims SQUARE (one `dim` feeds both
// width and height — the sector geometry invariant), bounded by total cells.
prop_compose! {
    fn square_dim_and_count()(dim in 4u32..=20)
        (system_count in 2usize..=((dim as usize) * (dim as usize)), dim in Just(dim))
        -> (u32, usize) {
        (dim, system_count)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        max_shrink_iters: 32,
        .. ProptestConfig::default()
    })]

    #[test]
    fn invariants_hold_across_random_inputs(
        seed in "[a-z0-9]{4,12}",
        width in 4u32..=20,
        height in 4u32..=20,
        density in 0.05f64..=0.6,
        min_worlds in 1usize..=3,
        extra_worlds in 0usize..=5,
    ) {
        let max_worlds = min_worlds + extra_worlds;
        let cells = (width as usize) * (height as usize);
        let system_count = (cells.saturating_mul(2) / 5).clamp(2, cells);
        run_one(&seed, width, height, system_count, min_worlds, max_worlds, density)
            .map_err(TestCaseError::fail)?;
    }

    #[test]
    fn invariants_hold_across_system_counts(
        seed in "[a-z0-9]{4,12}",
        (dim, system_count) in square_dim_and_count(),
    ) {
        // Square sector: width == height == dim (geometry invariant).
        run_one(&seed, dim, dim, system_count, 1, 3, 0.2)
            .map_err(TestCaseError::fail)?;
    }

    #[test]
    fn determinism_holds_across_random_seeds(seed in "[a-z0-9]{4,12}") {
        let mut a = load_project(fixture_dir()).map_err(|e| TestCaseError::fail(e.to_string()))?;
        a.config.generation.seed = seed.clone();
        let mut b = load_project(fixture_dir()).map_err(|e| TestCaseError::fail(e.to_string()))?;
        b.config.generation.seed = seed.clone();
        let sa = generate_sector(a).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let sb = generate_sector(b).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let ja = serde_json::to_string(&sa).unwrap();
        let jb = serde_json::to_string(&sb).unwrap();
        prop_assert_eq!(ja, jb, "non-deterministic output for seed={}", seed);
    }
}
