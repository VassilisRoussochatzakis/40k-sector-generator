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

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sectorforge::search::{Constraint, SearchConfig, WishesFile};
use sectorforge::{load_project, run_seed_search, ProjectInput};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

/// The loaded project is the per-candidate template — the search clones it
/// (catalogs shared via `Arc`) and overrides only the seed.
fn template() -> ProjectInput {
    load_project(project_dir()).expect("load m42_project")
}

// docs/IMPROVEMENT.txt §42 PERF7 is specified on a 200-system project. A 30×30
// grid (square — the geometry invariant) holds 200 systems. The search clones
// this template per candidate via `clone_project_with_seed`, preserving
// `system_count`, so every one of the 128 candidates is a 200-system sector.
const PERF7_SYSTEMS: usize = 200;
const PERF7_DIM: u32 = 30;
const PERF7_CANDIDATES: u32 = 128;

/// 200-system per-candidate template for the PERF7 search measurement.
fn template_200() -> ProjectInput {
    let mut input = template();
    input.config.generation.sector_width = PERF7_DIM;
    input.config.generation.sector_height = PERF7_DIM;
    input.config.generation.system_count = PERF7_SYSTEMS;
    input
}

/// A wishes file with a fixed base seed and an *unsatisfiable* constraint
/// (`ContestedWorldMin` with an impossible floor). Unlike `RouteGraphConnected`,
/// this never passes, so the "lowest winning n" guard never trips and the full
/// candidate budget is generated + analysed + evaluated — the worst case PERF7
/// is meant to bound. `ContestedWorldMin` references no faction id, so it clears
/// preflight unconditionally (no early-out).
fn wishes_unsatisfiable(budget: u32) -> WishesFile {
    WishesFile {
        search: SearchConfig {
            base_seed: Some("bench-fixed-seed".into()),
            budget,
            report_top: 5,
        },
        constraints: vec![Constraint::ContestedWorldMin {
            min: u32::MAX,
            n_way: None,
        }],
    }
}

/// Build a wishes file with a fixed base seed and a constraint that forces the
/// search to actually generate + analyse every candidate (an unsatisfiable
/// floor on connectivity is cheap to evaluate but never short-circuits, so the
/// whole budget runs — the worst case the bench wants to time).
fn wishes(budget: u32) -> WishesFile {
    WishesFile {
        search: SearchConfig {
            base_seed: Some("bench-fixed-seed".into()),
            budget,
            report_top: 5,
        },
        // RouteGraphConnected is a real graph constraint evaluated against the
        // analytics report; pairing it with a tiny budget keeps the run short
        // while still walking the full generate→analyze→evaluate path.
        constraints: vec![Constraint::RouteGraphConnected],
    }
}

fn bench_run_search(c: &mut Criterion) {
    let template = template();
    let mut group = c.benchmark_group("seed_search");
    // Keep sample size low: each iteration generates `budget` whole sectors.
    group.sample_size(10);
    for budget in [4u32, 16u32] {
        let wishes = wishes(budget);
        group.throughput(Throughput::Elements(u64::from(budget)));
        group.bench_with_input(
            BenchmarkId::from_parameter(budget),
            &(&template, &wishes),
            |b, (template, wishes)| {
                b.iter(|| {
                    black_box(
                        run_seed_search(black_box(template), black_box(wishes))
                            .expect("search ok"),
                    )
                });
            },
        );
    }
    group.finish();
}

/// docs/IMPROVEMENT.txt §42 PERF7: evaluating 128 candidates over a 200-system
/// project must complete under 30 s. Bench id `search/128_cand_200sys`. The
/// unsatisfiable constraint forces all 128 candidates to fully generate +
/// analyse + evaluate (no short-circuit), which is the worst case the target
/// bounds.
fn bench_search_perf7(c: &mut Criterion) {
    let template = template_200();
    let wishes = wishes_unsatisfiable(PERF7_CANDIDATES);
    let mut group = c.benchmark_group("search");
    // Each iteration generates 128 whole 200-system sectors; one sample is the
    // most we can afford here. Criterion still reports a wall-clock estimate the
    // PERF7 budget can be checked against.
    group.sample_size(10);
    group.throughput(Throughput::Elements(u64::from(PERF7_CANDIDATES)));
    group.bench_with_input(
        BenchmarkId::from_parameter("128_cand_200sys"),
        &(&template, &wishes),
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

criterion_group!(benches, bench_run_search, bench_search_perf7);
criterion_main!(benches);
