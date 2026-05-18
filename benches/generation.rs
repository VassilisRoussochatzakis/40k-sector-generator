//! Criterion benches over the deterministic generation pipeline.
//!
//! Run with: `cargo bench --bench generation`

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use sectorforge::{generate_sector, load_project, validate_project, validate_sector};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

fn load() -> sectorforge::ProjectInput {
    load_project(project_dir()).expect("load m42_project")
}

fn bench_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate_sector");
    for (w, h, count) in [(8u32, 10u32, 24usize), (16, 20, 96), (24, 30, 200)] {
        let label = format!("{w}x{h}_{count}");
        group.bench_with_input(
            BenchmarkId::from_parameter(&label),
            &(w, h, count),
            |b, &(w, h, count)| {
                b.iter_batched(
                    || {
                        let mut input = load();
                        input.config.generation.sector_width = w;
                        input.config.generation.sector_height = h;
                        input.config.generation.system_count = count;
                        input
                    },
                    |input| generate_sector(input).expect("generate"),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_validate_pre(c: &mut Criterion) {
    let input = load();
    c.bench_function("validate_project", |b| {
        b.iter(|| validate_project(&input).unwrap());
    });
}

fn bench_validate_post(c: &mut Criterion) {
    let sector = generate_sector(load()).expect("generate");
    c.bench_function("validate_sector_invariants", |b| {
        b.iter(|| validate_sector(&sector));
    });
}

criterion_group!(
    benches,
    bench_generate,
    bench_validate_pre,
    bench_validate_post
);
criterion_main!(benches);
