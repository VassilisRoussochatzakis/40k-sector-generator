//! Criterion benches over the deterministic generation pipeline.
//!
//! Run with: `cargo bench --bench generation`
//!
//! docs/OPTIMIZE.txt §5B matrix: tiny / normal / large scales for generation,
//! plus split groups for rasterisation vs PNG encode so encoding overhead
//! can be tracked independently of the draw pass.

mod common;

use std::hint::black_box;

use common::{fixture, load, project, SCALES};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use image::RgbaImage;
use sectorforge::{
    bitmap::{encode_png_bytes, render_sector_image, RenderOptions},
    generate_sector, render_sector_markdown,
    svg_export::render_sector_svg,
    validate_project, validate_sector, GeneratedSector,
};

// docs/IMPROVEMENT.txt §42 PERF3..PERF6 are all specified at the 500-system
// scale. A 40×40 grid (square — the geometry invariant) comfortably holds 500
// systems. The seed is fixed (m42_project's configured seed), so every run of
// these benches builds the same sector.
const PERF_SYSTEMS: usize = 500;
const PERF_DIM: u32 = 40;

fn bench_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate_sector");
    for &(side, count) in SCALES {
        let label = format!("{side}x{side}_{count}");
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(&label),
            &(side, count),
            |b, &(side, count)| {
                b.iter_batched(
                    || project(side, count),
                    |input| black_box(generate_sector(input).expect("generate")),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

/// Pre-gen `validate_project` and post-gen `validate_sector` (the
/// `invariants::check_sector` pass): on the stock project, then at 500
/// systems, where docs/IMPROVEMENT.txt §42 PERF3 / PERF4 cap each at 50 ms.
/// Generation runs in setup, outside every measurement.
fn bench_validate(c: &mut Criterion) {
    let input = load();
    let sector = generate_sector(input.clone()).expect("generate");
    c.bench_function("validate_project", |b| {
        b.iter(|| validate_project(black_box(&input)).unwrap());
    });
    c.bench_function("validate_sector_invariants", |b| {
        b.iter(|| black_box(validate_sector(black_box(&sector))));
    });
    let input = project(PERF_DIM, PERF_SYSTEMS);
    let sector = generate_sector(input.clone()).expect("generate 500-system fixture");
    c.benchmark_group("validate").bench_function("500", |b| {
        b.iter(|| validate_project(black_box(&input)).unwrap());
    });
    c.benchmark_group("invariants").bench_function("500", |b| {
        b.iter(|| black_box(validate_sector(black_box(&sector))));
    });
}

/// docs/IMPROVEMENT.txt §42 PERF5: a full *cold* derivation pass over all
/// analysis overlays for a 500-system sector must complete under 2 s. This
/// recomputes every overlay from scratch on each iteration — the worst case a
/// cache never helps with. The overlay set + per-overlay configs mirror the
/// generation enrichment path in `src/gen/random_sector.rs` plus the standalone
/// `derive_*` CLI commands (economy / relations / history / personae / sites /
/// hooks / missions / prose / interestingness, and the `analyze` dashboard).
/// PERF6's *warm* (cache-hit) path is `derive/economy_warm` in
/// `builder/benches/builder_mutations.rs`.
fn bench_derive_cold_500(c: &mut Criterion) {
    let input = project(PERF_DIM, PERF_SYSTEMS);
    let sector = generate_sector(input.clone()).expect("generate 500-system fixture");
    let cats = &input.catalogs;
    let mut group = c.benchmark_group("derive_cold");
    // Each iteration recomputes ~10 overlays over a 500-system sector; keep the
    // sample count modest so the wall-clock stays reasonable.
    group.sample_size(10);
    group.bench_function("500", |b| {
        b.iter(|| {
            black_box(sectorforge::analyze_sector(black_box(&sector)));
            black_box(sectorforge::derive_economy_with(&sector, &cats.economy));
            black_box(sectorforge::derive_relations_with(&sector, &cats.relations));
            black_box(sectorforge::derive_history_with(&sector, &cats.history));
            black_box(sectorforge::derive_personae_with(&sector, &cats.personae));
            black_box(sectorforge::derive_sites_with(&sector, &cats.sites));
            black_box(sectorforge::derive_hooks_with(&sector, &cats.hooks));
            black_box(sectorforge::derive_missions_with(&sector, &cats.missions));
            black_box(sectorforge::derive_prose_with(&sector, &cats.prose));
            black_box(sectorforge::interestingness::derive(&sector));
        });
    });
    group.finish();
}

/// docs/OPTIMIZE.txt G1: rasterisation alone (no PNG encode, no I/O).
fn raster(sector: &GeneratedSector) -> RgbaImage {
    render_sector_image(sector, 2, None, RenderOptions::default())
}

/// RUST_FIXES.md FU-9: the whole PNG path, raster + encode — what a
/// `write_bitmap` caller pays, minus the disk write.
fn png(sector: &GeneratedSector) -> Vec<u8> {
    encode_png_bytes(&raster(sector)).expect("encode")
}

/// `sector.json` serialisation (~MB on the large scale, so worth tracking alone).
fn json(sector: &GeneratedSector) -> Vec<u8> {
    serde_json::to_vec(sector).expect("serialize")
}

/// One group named `name` timing `f` on the input built for each `SCALES`
/// entry; `bytes`, when set, reports that many bytes per input as throughput.
fn per_scale<I, O>(
    c: &mut Criterion,
    name: &str,
    inputs: &[I],
    bytes: Option<fn(&I) -> usize>,
    f: impl Fn(&I) -> O,
) {
    let mut group = c.benchmark_group(name);
    for (&(side, count), input) in SCALES.iter().zip(inputs) {
        if let Some(bytes) = bytes {
            group.throughput(Throughput::Bytes(bytes(input) as u64));
        }
        let label = format!("{side}x{side}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(label), input, |b, input| {
            b.iter(|| f(black_box(input)));
        });
    }
    group.finish();
}

/// docs/OPTIMIZE.txt G1: each emit path alone, on one fixed sector per scale
/// generated once up front, so generation stays out of every number. Raster
/// and PNG encode are timed apart (encode on a pre-rasterised image) and
/// together (`render_sector_png`), so whole ≈ raster + encode.
fn bench_emit(c: &mut Criterion) {
    let sectors: Vec<_> = SCALES
        .iter()
        .map(|&(side, count)| fixture(side, count))
        .collect();
    let images: Vec<_> = sectors.iter().map(raster).collect();
    // Same `RenderOptions::default()` (heatmap: Off) as the PNG path.
    let opts = RenderOptions::default();
    per_scale(c, "render_sector_image", &sectors, None, raster);
    per_scale(
        c,
        "encode_png_bytes",
        &images,
        Some(|i| i.as_raw().len()),
        |i| encode_png_bytes(i).expect("encode"),
    );
    per_scale(
        c,
        "render_sector_png",
        &sectors,
        Some(|s| png(s).len()),
        png,
    );
    per_scale(c, "render_sector_svg", &sectors, None, |s| {
        render_sector_svg(s, None, &opts)
    });
    per_scale(c, "serialize_json", &sectors, Some(|s| json(s).len()), json);
    per_scale(
        c,
        "render_sector_markdown",
        &sectors,
        None,
        render_sector_markdown,
    );
}

criterion_group!(
    benches,
    bench_generate,
    bench_validate,
    bench_derive_cold_500,
    bench_emit,
);
criterion_main!(benches);
