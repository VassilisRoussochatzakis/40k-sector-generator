//! Criterion bench for the full sector → PNG render path (RUST_FIXES.md FU-9,
//! `src/export/bitmap`).
//!
//! `generation.rs` already times rasterisation and PNG encode *separately*
//! (groups `render_sector_image` / `encode_png_bytes`). FU-9 asks for the
//! combined end-to-end PNG path — the cost a caller of `write_bitmap` actually
//! pays minus the disk write — so this bench measures rasterise + encode as one
//! unit. That makes the two split groups and this whole-path number directly
//! comparable (whole ≈ raster + encode) and gives the figure the perf claim is
//! stated against.
//!
//! Run with: `cargo bench --bench render_png`

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sectorforge::bitmap::{encode_png_bytes, render_sector_image, RenderOptions};
use sectorforge::{generate_sector, load_project, GeneratedSector};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

// Square sectors (width == height): (side, system_count). Matches the scales
// in generation.rs so the whole-path numbers line up with the split groups.
const SCALES: &[(u32, usize)] = &[(10, 24), (20, 96), (30, 200)];

// Same scale factor generation.rs uses for its raster/encode benches.
const SCALE: u32 = 2;

fn fixture(side: u32, count: usize) -> GeneratedSector {
    let mut input = load_project(project_dir()).expect("load m42_project");
    input.config.generation.sector_width = side;
    input.config.generation.sector_height = side;
    input.config.generation.system_count = count;
    generate_sector(input).expect("generate fixture")
}

/// Full path: rasterise the sector to an RGBA image, then PNG-encode it. The
/// disk write is intentionally excluded so the bench is I/O-free and stable.
fn bench_render_to_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_sector_png");
    for &(side, count) in SCALES {
        let sector = fixture(side, count);
        // Throughput in output PNG bytes so larger sectors report a rate.
        let bytes = {
            let img = render_sector_image(&sector, SCALE, None, RenderOptions::default());
            encode_png_bytes(&img).expect("encode").len() as u64
        };
        group.throughput(Throughput::Bytes(bytes));
        let label = format!("{side}x{side}_{count}");
        group.bench_with_input(BenchmarkId::from_parameter(&label), &sector, |b, sector| {
            b.iter(|| {
                let img = render_sector_image(
                    black_box(sector),
                    black_box(SCALE),
                    None,
                    RenderOptions::default(),
                );
                black_box(encode_png_bytes(black_box(&img)).expect("encode"))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_render_to_png);
criterion_main!(benches);
