//! Criterion bench for `briefing::apply` (RUST_FIXES.md FU-9 / finding
//! F-010-001 / TF-P-7).
//!
//! `briefing::apply` deep-clones the whole `GeneratedSector` and then walks
//! every system/world/route applying redactions. The clone dominates, so the
//! claim under test is the per-call whole-sector clone cost. We bench two
//! presets: `GmFullTruth` (clone + minimal work — isolates the clone) and
//! `PublicAtlas` (clone + the heaviest redaction path: route stripping,
//! faction redaction, relations projection).
//!
//! Run with: `cargo bench --bench briefing`

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sectorforge::briefing::{self, AudiencePreset};
use sectorforge::{generate_sector, load_project, GeneratedSector};

fn project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

/// One fixed, deterministic sector for every briefing measurement so the
/// generation cost stays out of the numbers. Fixed seed comes from the
/// checked-in `examples/m42_project` config.
fn fixture() -> GeneratedSector {
    generate_sector(load_project(project_dir()).expect("load m42_project"))
        .expect("generate fixture")
}

fn bench_apply(c: &mut Criterion) {
    let sector = fixture();
    let mut group = c.benchmark_group("briefing_apply");
    // GmFullTruth: no preset overrides, so this measures the whole-sector
    // clone plus the unavoidable per-system intel clears — the floor cost
    // F-010-001 is about. PublicAtlas exercises the full redaction surface.
    for preset in [AudiencePreset::GmFullTruth, AudiencePreset::PublicAtlas] {
        let profile = briefing::preset(preset);
        group.bench_with_input(
            BenchmarkId::from_parameter(preset.as_slug()),
            &profile,
            |b, profile| {
                b.iter(|| black_box(briefing::apply(black_box(&sector), black_box(profile))));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_apply);
criterion_main!(benches);
