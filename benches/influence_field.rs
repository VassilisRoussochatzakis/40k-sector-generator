//! Criterion bench for `influence_field::build` (RUST_FIXES.md FU-9 / finding
//! TF-S-5).
//!
//! `build` allocates a dense `Vec<f32>` of `cells * faction_count` for the
//! per-cell per-faction score accumulator, plus a `Vec<CellAssignment>` and a
//! `Vec<bool>` touch mask. TF-S-5 is the storage decision: is the dense array
//! cheap enough, or does it need a sparse representation? This bench is what
//! unblocks that call — it times `build` across sector sizes so the dense
//! allocation + Voronoi accumulation cost is measurable in isolation.
//!
//! Run with: `cargo bench --bench influence_field`

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sectorforge::influence_field;
use sectorforge::{generate_sector, load_project, GeneratedSector};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

// Square sectors (width == height); each entry is (side, system_count). The
// dense accumulator scales with cells (side²) × faction_count, so sweeping the
// side length is exactly what the storage decision needs.
const SCALES: &[(u32, usize)] = &[(10, 24), (20, 96), (30, 200)];

fn fixture(side: u32, count: usize) -> GeneratedSector {
    let mut input = load_project(project_dir()).expect("load m42_project");
    input.config.generation.sector_width = side;
    input.config.generation.sector_height = side;
    input.config.generation.system_count = count;
    generate_sector(input).expect("generate fixture")
}

fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("influence_field_build");
    for &(side, count) in SCALES {
        let sector = fixture(side, count);
        let cells = u64::from(side) * u64::from(side);
        group.throughput(Throughput::Elements(cells));
        let label = format!("{side}x{side}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| black_box(influence_field::build(black_box(sector))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_build);
criterion_main!(benches);
