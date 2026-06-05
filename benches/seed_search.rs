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

criterion_group!(benches, bench_run_search);
criterion_main!(benches);
