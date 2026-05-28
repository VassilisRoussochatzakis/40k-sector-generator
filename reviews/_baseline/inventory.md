# Workspace Inventory (Phase 0)

## Crates

| Crate | Path | Edition | Type | Purpose |
|---|---|---|---|---|
| `sector-generator` (lib `sectorforge`, bin `sectorforge`, optional bin `dhat-profile`) | `./` | 2021 | lib + bin | Domain model, generation, analysis, exports, CLI |
| `sectorforge-builder` | `builder/` | 2021 | bin | Egui editor (mutating) |
| `sectorforge-viewer` | `viewer/` | 2021 | bin | Egui viewer (read-only + light edits) |
| `sectorforge-gui-core` | `gui-core/` | 2021 | lib | Shared egui widgets (sector_view, palette, info_panel, system_view) |

Dependency graph: `gui-core` ← {`builder`, `viewer`}; `sectorforge` (root lib) ← every other crate.

No MSRV declared in any manifest. No `#![forbid(unsafe_code)]` declared anywhere — but zero `unsafe` blocks present in workspace source (clean).

## Profiles

Root `Cargo.toml`:
- `dev.package.*`: dependency `debug=false` (cuts DWARF cost).
- `release`: `lto=fat`, `codegen-units=1`, `panic=abort`, `strip=symbols`.
- `bench`: `lto=thin`, `codegen-units=1`.
- `profiling` (inherits release): `debug="line-tables-only"`, `strip=none`, `codegen-units=16`, `lto=thin`.

Workspace member crates do not override profiles.

## Features

- `sector-generator`: `default = []`, `dhat-heap = ["dep:dhat"]` (gates `dhat-profile` bin).
- Builder, viewer, gui-core: no features.
- `image`: `default-features = false, features = ["png"]` — minimal, correct.
- `eframe`: `default-features = false, features = ["default_fonts", "glow", "wayland", "x11"]` — minimal.

## Lints

- Root: no `[lints]` block.
- `builder/Cargo.toml` + `viewer/Cargo.toml`: `disallowed_types = "deny"`, `disallowed_methods = "deny"`.
- `gui-core`: no lints config.
- No workspace-level `clippy.toml` (checked: not present).

## Baseline tool sweep

| Tool | Status | Notes |
|---|---|---|
| `cargo build --workspace --all-targets` | exit 0 | Clean |
| `cargo clippy --workspace --all-targets --all-features -- -W clippy::all -W clippy::pedantic -W clippy::nursery` | exit 0 | ~2,979 warnings: builder 542 lib + 27 unique test, viewer 217 + 1 bin, gui-core 187 + 6/10 unique test+snap, dhat-profile 2. With default lints only: 0. |
| `cargo test --workspace --no-run` | exit 0 | All test binaries compile |
| `cargo tree --workspace --duplicates` | 0 | Many transitive duplicates (objc2-app-kit 0.2/0.3, objc2-foundation 0.2/0.3, bitflags 1/2, core-foundation 0.9/0.10, block2 0.5/0.6, png 0.17/0.18) — all rooted in eframe/winit/rfd/arboard stack, not directly fixable from workspace manifests. |
| `cargo audit` | not run | tool likely not installed; recommend |
| `cargo +nightly udeps` | not run | nightly likely not installed; recommend |
| `cargo geiger` | not run | not installed; low priority (0 unsafe in workspace) |

## Hot signals (whole-workspace greps)

| Signal | Count | Notes |
|---|---|---|
| `unsafe` blocks/fns | **0** | Excellent — entire workspace is safe Rust. |
| Panic surface (`unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!`) | 497 | Primary cross-cutting risk concentration. |
| `.clone()` call sites | 1,667 | Distribution: src 583, builder 877, viewer 183, gui-core 24. Builder ≈ 53% of total — dominant ergonomic-clone surface. |
| `Rc<>` / `Arc<>` / `RefCell<>` / `Mutex<>` / `RwLock<>` | 152 lines | No `Mutex`/`RwLock` use seen in greps — predominantly `Arc<str>` for shared immutable string identity (good idiom). |
| `async fn` / `.await` / `tokio::` / `futures::` | 0 | No async runtime — single-threaded plus `rayon` (in `sector-generator` only). Concurrency model is simple. |
| HashMap / HashSet uses | 71 lines across ~22 files | Mix of `FxHashMap` (lookup) and `BTreeMap` (deterministic output). CLAUDE.md prohibits Fx for iteration. |

## Per-crate LOC + largest files

### `src/` (45,392 LOC)

Top files (1k+):
- `src/analysis/economy.rs` 1743
- `src/analysis/relations.rs` 1628
- `src/analysis/search.rs` 1367
- `src/worlds.rs` 1361
- `src/validate/diff.rs` 1308
- `src/model/sector_model/mod.rs` 1242
- `src/export/segmentum.rs` 1168
- `src/analysis/personae.rs` 1078

Subtrees:
- `src/analysis/` ≈ 10,000 LOC
- `src/gen/` ≈ 5,500 LOC
- `src/export/` ≈ 6,500 LOC
- `src/model/` ≈ 2,500 LOC
- `src/validate/` ≈ 2,500 LOC
- `src/loading/` ≈ 1,100 LOC
- `src/cli/` ≈ 1,400 LOC
- `src/worlds.rs` + `src/worlds_toml.rs` + `src/lib.rs` ≈ 2,500 LOC
- `src/main.rs`, `src/bin/` ≈ tiny

### `builder/src/` (30,434 LOC)

Top files (1k+):
- `builder/src/builder/panels/history.rs` 1557
- `builder/src/builder/panels/system.rs` 1508
- `builder/src/builder/command.rs` 1486
- `builder/src/builder/panels/control.rs` 1405
- `builder/src/builder/panels/map/mod.rs` 1338
- `builder/src/builder/panels/world.rs` 1249
- `builder/src/builder/panels/routes.rs` 1177
- `builder/src/builder/panels/map/context_menu.rs` 1133
- `builder/src/builder/panels/system_map.rs` 1097
- `builder/src/builder/panels/relations.rs` 1076
- `builder/src/builder/project_io.rs` 1053

### `viewer/src/` ≈ 10,000 LOC

Top files:
- `viewer/src/factions_overview.rs` 1349
- `viewer/src/segmentum_view.rs` 798
- `viewer/src/app/sector_view.rs` 689
- `viewer/src/app/export_ui.rs` 481
- `viewer/src/route_planner.rs` 471

### `gui-core/src/` ≈ 5,000 LOC

- `gui-core/src/sector_view.rs` 1480
- `gui-core/src/info_panel.rs` 1142
- `gui-core/src/palette.rs` 850
- `gui-core/src/system_view.rs` 495

### `tests/it/` ≈ 2,309 LOC, `benches/generation.rs` 125

Tests:
- `invariants_tests.rs` 308
- `search_and_diff.rs` 229
- `golden_generation.rs` 219
- `personae_tests.rs` 198
- 12 more files, all sub-200 LOC each.

## Notes

- Determinism invariants (CLAUDE.md §"Determinism invariants"):
  - Output iteration must use `BTreeMap`/`BTreeSet` or sorted keys, not `Fx*`.
  - All RNG draws through `src/model/rng.rs` (stage-keyed via `blake3`).
  - Output writers (bitmap/svg_export/html_export/render) byte-stable; golden tests gate.
  - Builder writes only via command bus (undo/redo invariant).
- `old/` directory excluded from all reviews (project rule).
