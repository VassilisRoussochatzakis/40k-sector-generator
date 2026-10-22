//! Large-sector builder mutation benchmarks (T7 / §40, exercising PERF1 / §42).
//!
//! Run with: `cargo bench -p sectorforge-builder --bench builder_mutations`
//!
//! These measure the builder's edit loop on a deterministic large sector
//! (>=200 systems, >=800 worlds — `examples/big_test`, fixed seed). Three
//! groups mirror what the builder does on every edit:
//!
//!   1. `command_apply` — `BuilderState::run(cmd)` for a cheap command
//!      (identity move / rename) and a heavier one (sector-wide archetype
//!      auto-assign). `run` re-checks invariants and invalidates caches, so
//!      this is the full edit cost. (T7 / PERF1 target: <1ms.)
//!   2. `invariants_check` — `invariants::check_sector`, the exact invariant
//!      pass `run` performs after a mutation. (T7 target: <50ms.)
//!   3. `derive` — cold (`recompute_economy`, unconditional full recompute) vs
//!      warm (`ensure_fresh`, the cache-gated no-recompute frame path).
//!      (T7 targets: <500ms cold / <50ms warm.)
//!
//! The bench only needs to compile and run; thresholds are not asserted here
//! (criterion just measures wall time).

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use sectorforge::config::AppConfig;
use sectorforge::invariants::check_sector;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::{generate_sector, load_project};

use sectorforge_builder::{
    state_from_generated_sector, ArchetypeApplyFlags, BuilderCommand, BuilderState, DerivationKind,
};

/// `examples/big_test` — `system_count = 200`, `2..=6` worlds per system
/// (>=400 worlds, typically ~800+), fixed seed `big-test-default-seed`. The
/// path is anchored on `CARGO_MANIFEST_DIR` (the builder package) so the bench
/// never depends on the process's working directory.
fn big_test_project_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/big_test")
}

/// Generate the large fixture sector once, returning it alongside the config
/// that produced it (needed to rebuild a `BuilderState`). Deterministic: the
/// seed is fixed in the project's `sectorforge.toml`.
fn big_fixture() -> (GeneratedSector, AppConfig) {
    let input = load_project(big_test_project_dir()).expect("load examples/big_test");
    let config = input.config.clone();
    let sector = generate_sector(input).expect("generate big_test fixture");
    (sector, config)
}

/// Build a fresh `BuilderState` wrapping a clone of the fixture sector. Used in
/// `iter_batched` setup so each measured `run` starts from a pristine document
/// (`BuilderState` is not `Clone`, so we rebuild from the sector clone).
fn fresh_state(sector: &GeneratedSector, config: &AppConfig) -> BuilderState {
    state_from_generated_sector(sector.clone(), config.clone())
}

/// Group 1: cost of applying a command through the bus on the large sector.
fn bench_command_apply(c: &mut Criterion) {
    let (sector, config) = big_fixture();

    // Capture a real system id + its current coordinate from the fixture for
    // the cheap commands. An identity move (`to == from`) is always valid (the
    // occupancy check excludes the system itself) and still drives the full
    // `run` path: mutate, rebuild index, clear cache, re-check invariants.
    let first = sector.systems.first().expect("fixture has systems");
    let commands = [
        // Cheap: move a single system to its own coordinate.
        (
            "move_system_cheap",
            BuilderCommand::MoveSystem {
                id: first.id.clone(),
                from: first.coord,
                to: first.coord,
            },
        ),
        // Cheap: rename a single system.
        (
            "rename_system_cheap",
            BuilderCommand::RenameSystem {
                id: first.id.clone(),
                from: String::new(), // `apply` uses `to`; `from` is only for revert.
                to: format!("{}-bench", first.name),
            },
        ),
        // Heavier: sector-wide archetype auto-assign (`archetypes::apply_all` over
        // every system, then per-axis masking). `before` is filled by `apply`.
        (
            "auto_assign_archetypes_heavy",
            BuilderCommand::AutoAssignArchetypes {
                flags: ArchetypeApplyFlags::default(),
                before: Vec::new(),
            },
        ),
    ];

    let mut group = c.benchmark_group("command_apply");
    for (name, cmd) in commands {
        group.bench_function(name, |b| {
            b.iter_batched(
                // The command is cloned here in setup, outside the timed routine.
                || (fresh_state(&sector, &config), cmd.clone()),
                |(mut state, cmd)| {
                    state.run(black_box(cmd)).expect(name);
                    black_box(state)
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Group 2: full invariant re-check over the large sector — the exact
/// `invariants::check_sector` pass `BuilderState::run` performs after every
/// mutation (stored into `invariant_report`).
fn bench_invariants_check(c: &mut Criterion) {
    let (sector, _config) = big_fixture();
    let mut group = c.benchmark_group("invariants_check");
    group.bench_function("big_test", |b| {
        b.iter(|| black_box(check_sector(black_box(&sector))));
    });
    group.finish();
}

/// Group 3: cold vs warm overlay derivation on the large sector. The economy
/// overlay is representative — `recompute_economy` walks the full derivation
/// graph and installs the result onto `sector.economy`.
fn bench_derive(c: &mut Criterion) {
    let (sector, config) = big_fixture();

    let mut group = c.benchmark_group("derive");
    // Each cold iteration recomputes the whole economy graph; keep the sample
    // count modest so the wall-clock stays reasonable.
    group.sample_size(20);

    // Cold: `recompute_economy` unconditionally re-derives regardless of cache
    // state, so every iteration is a true from-scratch derive.
    group.bench_function("economy_cold", |b| {
        let mut state = fresh_state(&sector, &config);
        b.iter(|| {
            state.recompute_economy();
            black_box(&state);
        });
    });

    // Warm: derive once so the overlay is installed + the ledger fingerprint is
    // recorded, then measure the cache-gated `ensure_fresh` frame path. With
    // the overlay fresh, `ensure_fresh` early-returns without recomputing —
    // exactly what the builder does on a warm frame (`pump_derivations`).
    group.bench_function("economy_warm", |b| {
        let mut state = fresh_state(&sector, &config);
        state.recompute_economy();
        b.iter(|| {
            state.ensure_fresh(black_box(DerivationKind::Economy));
            black_box(&state);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_command_apply,
    bench_invariants_check,
    bench_derive,
);
criterion_main!(benches);
