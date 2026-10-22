//! Criterion bench for the constraint-directed seed search
//! (`search::run_search`, RUST_FIXES.md FU-9 / finding F-009-001 / TF-P-2).
//!
//! Each candidate runs a full `generate_sector` + `analytics::analyze` +
//! constraint evaluation, parallelised over rayon. The point of this bench is
//! the *relative* cost of a fixed candidate budget — it directly exercises the
//! TF-P-2 data-parallel enumeration with the shared "lowest winning n" guard.
//!
//! The budget is deliberately small (generation is two orders of magnitude
//! more expensive than the per-candidate evaluation, per the source comment),
//! so a run stays in the seconds range rather than minutes.
//!
//! Determinism: the base seed is fixed; candidate seeds derive from it through
//! `blake3` exactly as the per-stage RNG scheme does. No `thread_rng`.
//!
//! Run with: `cargo bench --bench seed_search`

mod common;

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sectorforge::run_seed_search;
use sectorforge::search::Constraint::{self, ContestedWorldMin, RouteGraphConnected};
use sectorforge::search::{SearchConfig, WishesFile};

// docs/IMPROVEMENT.txt §42 PERF7 is specified on a 200-system project. A 30×30
// grid (square — the geometry invariant) holds 200 systems. The search clones
// this template per candidate via `clone_project_with_seed`, preserving
// `system_count`, so every one of the 128 candidates is a 200-system sector.
const PERF7_SYSTEMS: usize = 200;
const PERF7_DIM: u32 = 30;
const PERF7_CANDIDATES: u32 = 128;

/// A wishes file with a fixed base seed, a `budget` of candidates and one
/// constraint.
fn wishes(budget: u32, constraint: Constraint) -> WishesFile {
    WishesFile {
        search: SearchConfig {
            base_seed: Some("bench-fixed-seed".into()),
            budget,
            report_top: 5,
        },
        constraints: vec![constraint],
    }
}

/// The loaded project is the per-candidate template — the search clones it
/// (catalogs shared via `Arc`) and overrides only the seed.
///
/// `seed_search/{4,16}`: the stock project against `RouteGraphConnected`, a
/// real graph constraint evaluated against the analytics report; the tiny
/// budget keeps the run short while still walking the full
/// generate→analyze→evaluate path.
///
/// `search/128_cand_200sys` — docs/IMPROVEMENT.txt §42 PERF7: evaluating 128
/// candidates over a 200-system project must complete under 30 s. Its
/// `ContestedWorldMin` floor is impossible: unlike `RouteGraphConnected` it
/// never passes, so the "lowest winning n" guard never trips and every
/// candidate fully generates + analyses + evaluates — the worst case PERF7
/// bounds. It references no faction id, so it clears preflight
/// unconditionally (no early-out).
fn bench_search(c: &mut Criterion) {
    let stock = common::load();
    let perf7 = common::project(PERF7_DIM, PERF7_SYSTEMS);
    let never = ContestedWorldMin {
        min: u32::MAX,
        n_way: None,
    };
    for (group, id, template, budget, constraint) in [
        ("seed_search", "4", &stock, 4, RouteGraphConnected),
        ("seed_search", "16", &stock, 16, RouteGraphConnected),
        ("search", "128_cand_200sys", &perf7, PERF7_CANDIDATES, never),
    ] {
        let wishes = wishes(budget, constraint);
        let mut group = c.benchmark_group(group);
        // Keep sample size low: each iteration generates `budget` whole sectors.
        group.sample_size(10);
        group.throughput(Throughput::Elements(u64::from(budget)));
        group.bench_with_input(
            BenchmarkId::from_parameter(id),
            &(template, &wishes),
            |b, (template, wishes)| {
                b.iter(|| {
                    black_box(
                        run_seed_search(black_box(template), black_box(wishes)).expect("search ok"),
                    )
                });
            },
        );
        group.finish();
    }
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
