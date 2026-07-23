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
    generate_sector, load_project, render_sector_markdown,
    svg_export::render_sector_svg,
    validate_project, validate_sector, GeneratedSector, ProjectInput,
};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

fn load() -> sectorforge::ProjectInput {
    load_project(project_dir()).expect("load m42_project")
}

// Sectors are square (width == height); the bench grids follow suit.
const SCALES: &[(u32, u32, usize)] = &[
    (10, 10, 24),  // tiny
    (20, 20, 96),  // normal
    (30, 30, 200), // large
];

// docs/IMPROVEMENT.txt §42 PERF3..PERF6 are all specified at the 500-system
// scale. A 40×40 grid (square — the geometry invariant) comfortably holds 500
// systems. The seed is fixed (m42_project's configured seed), so every run of
// these benches builds the same sector.
const PERF_SYSTEMS: usize = 500;
const PERF_DIM: u32 = 40;

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

/// A 500-system project template (fixed seed, square grid) for the §42
/// PERF3..PERF6 measurements. Built once per bench; the generation cost is
/// kept out of the validation/derivation measurements below.
fn perf_project() -> ProjectInput {
    let mut input = load();
    input.config.generation.sector_width = PERF_DIM;
    input.config.generation.sector_height = PERF_DIM;
    input.config.generation.system_count = PERF_SYSTEMS;
    input
}

/// The generated 500-system sector shared by the invariant/derivation benches.
fn perf_sector() -> GeneratedSector {
    generate_sector(perf_project()).expect("generate 500-system fixture")
}

/// docs/IMPROVEMENT.txt §42 PERF3: pre-generation validation must stay under
/// 50 ms for a 500-system config. `validate_project` runs the `validation`
/// rule set against the loaded project (the pre-gen path).
fn bench_validate_500(c: &mut Criterion) {
    let input = perf_project();
    let mut group = c.benchmark_group("validate");
    group.bench_function("500", |b| {
        b.iter(|| validate_project(black_box(&input)).unwrap());
    });
    group.finish();
}

/// docs/IMPROVEMENT.txt §42 PERF4: the sectorforge invariant check
/// (`invariants::check_sector`, via `validate_sector`) must stay under 50 ms
/// for a 500-system generated sector.
fn bench_invariants_500(c: &mut Criterion) {
    let sector = perf_sector();
    let mut group = c.benchmark_group("invariants");
    group.bench_function("500", |b| {
        b.iter(|| black_box(validate_sector(black_box(&sector))));
    });
    group.finish();
}

/// docs/IMPROVEMENT.txt §42 PERF5: a full *cold* derivation pass over all
/// analysis overlays for a 500-system sector must complete under 2 s. This
/// recomputes every overlay from scratch on each iteration — the worst case a
/// cache never helps with. The overlay set + per-overlay configs mirror the
/// generation enrichment path in `src/gen/random_sector.rs` plus the standalone
/// `derive_*` CLI commands (economy / relations / history / personae / sites /
/// hooks / missions / prose / interestingness, and the `analyze` dashboard).
fn bench_derive_cold_500(c: &mut Criterion) {
    let input = perf_project();
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
            black_box(sectorforge::derive_interestingness(&sector));
        });
    });
    group.finish();
}

/// docs/IMPROVEMENT.txt §42 PERF6: a *warm* derivation pass (cache hit) must
/// stay under 50 ms. The derived overlays are `Arc`-wrapped fields on
/// `GeneratedSector` (`economy`, `relations`, `chronicle`, …) — exactly how the
/// builder/viewer install a recomputed overlay (`self.sector.economy =
/// Arc::new(report)`). A cache hit re-reads those already-installed overlays
/// without recomputing them, which is the `Arc`-clone the cache-hit path
/// returns. We derive once into a sector, then measure that warm re-read.
fn bench_derive_warm_500(c: &mut Criterion) {
    let input = perf_project();
    let mut sector = generate_sector(input.clone()).expect("generate 500-system fixture");
    let cats = &input.catalogs;

    // Cold pass once: install the overlays carried as sector fields (economy /
    // relations / chronicle), mirroring the generation enrichment / builder
    // recompute install points. The warm measurement below only re-reads these
    // installed overlays — the cache-hit return path.
    sector.economy = std::sync::Arc::new(sectorforge::derive_economy_with(&sector, &cats.economy));
    sector.relations =
        std::sync::Arc::new(sectorforge::derive_relations_with(&sector, &cats.relations));
    // `SectorChronicle` is a type alias for `HistoryReport`, so the derived
    // report installs directly onto the field (the generation/builder pattern).
    sector.chronicle = sectorforge::derive_history_with(&sector, &cats.history);

    let mut group = c.benchmark_group("derive_warm");
    group.bench_function("500", |b| {
        b.iter(|| {
            // Cache hit: clone the already-derived overlay `Arc`s (refcount
            // bumps) rather than recomputing them.
            black_box(std::sync::Arc::clone(&black_box(&sector).economy));
            black_box(std::sync::Arc::clone(&black_box(&sector).relations));
            black_box(black_box(&sector).chronicle.clone());
        });
    });
    group.finish();
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

/// docs/OPTIMIZE.txt G1: SVG vector render alone (rasterised sector in setup,
/// not measured). Same `RenderOptions::default()` (heatmap: Off) as the PNG path.
fn bench_render_svg(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_sector_svg");
    let opts = RenderOptions::default();
    for &(w, h, count) in SCALES {
        let sector = fixture_for(w, h, count);
        let label = format!("{w}x{h}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| black_box(render_sector_svg(sector, None, &opts)));
        });
    }
    group.finish();
}

/// docs/OPTIMIZE.txt G1: JSON serialisation of the full `GeneratedSector`
/// (the sector.json export is ~MB on the large scale, so worth tracking alone).
fn bench_serialize_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize_json");
    for &(w, h, count) in SCALES {
        let sector = fixture_for(w, h, count);
        let bytes = serde_json::to_vec(&sector).expect("serialize sector");
        let label = format!("{w}x{h}_{count}");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| black_box(serde_json::to_vec(black_box(sector)).expect("serialize")));
        });
    }
    group.finish();
}

/// docs/OPTIMIZE.txt G1: Markdown render alone.
fn bench_render_markdown(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_sector_markdown");
    for &(w, h, count) in SCALES {
        let sector = fixture_for(w, h, count);
        let label = format!("{w}x{h}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| black_box(render_sector_markdown(sector)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_generate,
    bench_validate_pre,
    bench_validate_post,
    bench_validate_500,
    bench_invariants_500,
    bench_derive_cold_500,
    bench_derive_warm_500,
    bench_render_png,
    bench_encode_png,
    bench_render_svg,
    bench_serialize_json,
    bench_render_markdown,
);
criterion_main!(benches);
