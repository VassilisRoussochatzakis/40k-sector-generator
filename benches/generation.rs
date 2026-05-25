//! Criterion benches over the deterministic generation pipeline.
//!
//! Run with: `cargo bench --bench generation`
//!
//! docs/OPTIMIZE.txt §5B matrix: tiny / normal / large scales for generation,
//! plus split groups for rasterisation vs PNG encode so encoding overhead
//! can be tracked independently of the draw pass.

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use sectorforge::{
    bitmap::{encode_png_bytes, render_sector_image, RenderOptions},
    generate_sector, load_project, validate_project, validate_sector, GeneratedSector,
};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

fn load() -> sectorforge::ProjectInput {
    load_project(project_dir()).expect("load m42_project")
}

const SCALES: &[(u32, u32, usize)] = &[
    (8, 10, 24),   // tiny
    (16, 20, 96),  // normal
    (24, 30, 200), // large
];

fn bench_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate_sector");
    for &(w, h, count) in SCALES {
        let label = format!("{w}x{h}_{count}");
        group.throughput(Throughput::Elements(count as u64));
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
                    |input| black_box(generate_sector(input).expect("generate")),
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
        b.iter(|| validate_project(black_box(&input)).unwrap());
    });
}

fn bench_validate_post(c: &mut Criterion) {
    let sector = generate_sector(load()).expect("generate");
    c.bench_function("validate_sector_invariants", |b| {
        b.iter(|| black_box(validate_sector(black_box(&sector))));
    });
}

/// Helper: produce one fixed sector per scale for raster/encode benches so
/// generation cost stays out of those measurements.
fn fixture_for(w: u32, h: u32, count: usize) -> GeneratedSector {
    let mut input = load();
    input.config.generation.sector_width = w;
    input.config.generation.sector_height = h;
    input.config.generation.system_count = count;
    generate_sector(input).expect("generate fixture")
}

/// docs/OPTIMIZE.txt G1: rasterisation alone (no PNG encode, no I/O).
fn bench_render_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_sector_image");
    for &(w, h, count) in SCALES {
        let sector = fixture_for(w, h, count);
        let label = format!("{w}x{h}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| {
                black_box(render_sector_image(
                    sector,
                    2,
                    None,
                    RenderOptions::default(),
                ))
            });
        });
    }
    group.finish();
}

/// docs/OPTIMIZE.txt G1: PNG encode alone (rasterised in setup, not measured).
fn bench_encode_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_png_bytes");
    for &(w, h, count) in SCALES {
        let sector = fixture_for(w, h, count);
        let img = render_sector_image(&sector, 2, None, RenderOptions::default());
        let label = format!("{w}x{h}_{count}");
        group.throughput(Throughput::Bytes(img.as_raw().len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &img, |b, img| {
            b.iter(|| black_box(encode_png_bytes(black_box(img)).expect("encode")));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_generate,
    bench_validate_pre,
    bench_validate_post,
    bench_render_png,
    bench_encode_png,
);
criterion_main!(benches);
