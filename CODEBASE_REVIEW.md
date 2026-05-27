# Codebase Review — sectorforge (40k-sector-generator)

Prepared per `REVIEW.md`. Evidence-based, LLM-optimized: every finding cites `path:line`. No invented files. Severity follows the rubric (§11 of `REVIEW.md`).

> **Scope.** Single-repo Rust workspace: library crate `sectorforge` + binaries (`sectorforge`, `sectorforge-builder`, `sectorforge-viewer`) + shared crate `sectorforge-gui-core`. Tested on commit `d8b7554` (branch `main`). The `old/` directory was excluded by explicit project instruction.

> **Toolchain status snapshot.** Captured during review.

| Command | Exit | Notes |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Clean. |
| `cargo check --workspace --all-targets` | 0 | Clean. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **101** | 3 lints: `neg_cmp_op_on_partial_ord` × 2, `module_inception` × 1. **Blocks any clippy-gated CI.** |
| `cargo test --workspace` | 0 | Aggregate: **519 unit/integration tests pass, 5 ignored, 0 failed**, plus 6 doctests. Wall time ≈ 39s (one suite dominates at ≈ 36.7s). |

---

## 1. Executive Summary

**Overall assessment.** A large, ambitious, deterministic generator with a *very* broad public surface (402 `pub fn`, 292 `pub struct`/`pub enum`, 120 `.rs` files in `src/`, ≈93k LOC across the workspace excluding `old/`). The core domain model in `src/sector_model/` is clean: serializable DTOs, `Arc<str>`-backed strings, a dedicated mutation API behind `GeneratedSector::*`, typed IDs (`SystemId`/`WorldId`/`RouteId`/`FactionId`). Determinism is taken seriously — stage-keyed RNG via `blake3` (`src/rng.rs`), `BTreeMap`/`BTreeSet` everywhere for byte-stable output, `FxMap`/`FxSet` aliased and explicitly *forbidden* in output paths (`src/lib.rs:27-32`). Tests are extensive in count and run green.

**Main risks.**
1. **Clippy is broken under `-D warnings`** — 3 lints; the workspace pins `disallowed_types`/`disallowed_methods` but the upstream Cargo workspace does not enforce a clippy gate. (Finding 2.1)
2. **No CI** — no `.github/workflows/`, no `cargo-deny`, no `cargo audit`. (Finding 2.2)
3. **Severe file/module bloat in the builder UI layer.** `builder/src/builder/panels/map.rs` is **3,341 lines**; `command.rs` is **1,486**; the panel directory holds many 700-1,500-line files. The shared `BuilderState` struct (`builder/src/builder/state/mod.rs`) has dozens of pub fields — a textbook God Object. (Findings 4.1–4.4)
4. **Domain library has very wide `pub mod` surface** — almost the entire crate is `pub mod`, with two file-level error types (`SectorError`, `MutationError`) duplicated across `src/errors.rs` and `src/sector_model/mutation.rs`. (Finding 5.1, 6.1)
5. **Empty README + no environment/setup docs** at repo root. (Finding 13.1)

**Best qualities.**
- Determinism discipline + manifest hashing (`src/rng.rs`, `src/sector_model::GenerationManifest`).
- Typed IDs end-to-end; the `FxMap` aliases come with an explicit *output-determinism* comment in `src/lib.rs:27-32`.
- `thiserror`-based domain errors with structured fields (`src/errors.rs`, `src/sector_model/mutation.rs:21-43`).
- Aggressive sub-module decomposition in *some* areas (`src/history/`, `src/bitmap/`, `src/svg_export/`, `src/generation/`), used as a model for what the over-large files should look like.
- Doctest coverage of the top-level API (`src/lib.rs`).
- No runtime `unwrap`/`expect` panics in production hot paths — every match found via `rg` was test code, `From`-impls for stable enums, or single-pass deterministic helpers.

**Highest-priority recommendation.** Fix the three clippy errors (Finding 2.1), then add a minimum-viable CI workflow (fmt + clippy + test) (Finding 2.2). After that, tackle `builder/src/builder/panels/map.rs` (Finding 4.1) before it gets any larger.

---

## 2. Build/CI/Release Audit

### Finding 2.1 — `cargo clippy -D warnings` fails (3 errors)

- **Severity:** High
- **Category:** Build/CI
- **Evidence:**
  - `src/bitmap/routes.rs:417` — `if !(spacing > 0.0)` flagged as `clippy::neg_cmp_op_on_partial_ord`.
  - `src/svg_export/routes.rs:183` — same lint.
  - `src/svg_export/tests.rs:2` — `mod tests { ... }` inside file already named `tests.rs` flagged as `clippy::module_inception`.

**What I found.** All three are mechanical fixes, but they make any clippy-gated CI red.

**Why it matters.** The two `panels/Cargo.toml` files (`builder/Cargo.toml:22-24`, `viewer/Cargo.toml:24-26`) already enforce `disallowed_types = "deny"` / `disallowed_methods = "deny"` — the project is *moving toward* strict clippy. Leaving 3 lints live now means adding the gate is blocked until they are fixed.

**Recommended fix.**

```rust
// src/bitmap/routes.rs:417
if !(spacing > 0.0)
// →
if !spacing.is_finite() || spacing <= 0.0
```

```rust
// src/svg_export/routes.rs:183 — same rewrite
```

```rust
// src/svg_export/tests.rs — drop the `mod tests` wrapper; the file is already a tests module under `mod tests;` declared in src/svg_export/mod.rs.
#[cfg(test)]
use super::*;
// ... tests as free functions
```

**Validation.** `cargo clippy --workspace --all-targets -- -D warnings` should exit 0.

**Risk of change.** Low — `spacing` is already validated by the `n.is_finite() && n > 0.0` check on the next line of `float_loop_steps`.

---

### Finding 2.2 — No CI configuration

- **Severity:** High
- **Category:** Build/CI
- **Evidence:** `ls -la .github` → not present. `ls .github/workflows` → not present. No `deny.toml`, no `clippy.toml`, no `rust-toolchain.toml`.

**Why it matters.** All four invariants the README-style cluster of docs talks about (determinism, clippy hygiene, fmt, tests) depend on local discipline. A single bad commit can silently break clippy (Finding 2.1 is exactly this scenario).

**Recommended fix.** Add `.github/workflows/ci.yml`:

```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

**Risk of change.** Low. Add `cargo audit` / `cargo deny check` as separate non-blocking jobs once a `deny.toml` exists.

---

### Finding 2.3 — Optional dev tools absent

- **Severity:** Low
- **Category:** Build/CI
- **Evidence:** No `deny.toml`, `clippy.toml`, no `cargo-machete`/`cargo-udeps` artifacts.

**Recommended fix.** After CI lands, drop in a minimal `deny.toml` and run `cargo machete` once to inventory unused deps. The workspace has 9 root crates and 3 sub-crates — a `cargo tree -d` audit is worth one pass.

---

## 3. Architecture Assessment

### Repository Map (workspace)

```
sector-generator (lib + sectorforge bin + dhat-profile bin) — src/
├── sectorforge-gui-core  — gui-core/          // shared egui widgets
├── sectorforge-viewer    — viewer/            // viewer/editor binary
└── sectorforge-builder   — builder/           // builder binary
```

Dependency direction (read top→down, no cycles observed):

```
sectorforge (lib, headless domain + IO)
   ▲                         ▲
   │                         │
sectorforge-gui-core   sectorforge-viewer ── consumes lib + gui-core
   ▲
sectorforge-builder ── consumes lib + gui-core
```

**Assessment.** Sensible split. Domain logic is in the library, not in the GUI. `gui-core` is the right placement for the read-only `SectorView` widget reused by builder and viewer (`gui-core/src/sector_view.rs:1`). The `eframe`/`egui` dependency is correctly isolated to the GUI crates and never leaks into the library (verified via `rg eframe src/` — no hits).

### Layer Assessment

| Layer | State | Evidence |
|---|---|---|
| CLI parse + dispatch | **Clear** | `src/main.rs` (18 LOC) just delegates; `src/cli/mod.rs` is a flat dispatch over per-subcommand modules. |
| Project IO / config | **Clear** | `src/input.rs`, `src/config.rs`, per-feature `load_*_file` in each domain module. |
| Domain DTOs | **Clear** | `src/sector_model/mod.rs` defines `Generated*` + IDs. |
| Mutation API | **Clear (but big)** | `src/sector_model/mutation.rs` (866 LOC) owns every structural mutation; `BuilderCommand::apply` is the only caller (`builder/src/builder/command.rs`). |
| Derivations (history, prose, hooks, missions, etc.) | **Clear** | Each lives in its own module under `src/`; library `derive_*` / `derive_*_with` pairs in `src/lib.rs:549-848`. |
| Export | **Clear, well-split** | `src/bitmap/`, `src/svg_export/`, `src/html_export.rs`, `src/render.rs`, `src/export.rs`. The bitmap + svg modules follow the pattern the rest of the codebase should aspire to. |
| Builder state | **Leaky** | `BuilderState` (`builder/src/builder/state/mod.rs:76+`) bundles undo, derivation cache, file watcher, modal state, transient dialog state, drag state, region paint scratch, bulk-edit form state — see Finding 4.4. |
| Builder panels | **Leaky** | Several 700-1500 line panel files mix tool dispatch, dialog rendering, command dispatch, and undocumented helpers — see Findings 4.1-4.3. |
| Viewer | **Mixed** | `viewer/src/app/` is decomposed; `viewer/src/factions_overview.rs` (1,349 LOC) is not — review further. |

---

## 4. File/Module Organization Audit

### Top 15 longest `.rs` files (workspace, excluding `target/`, `old/`)

| Rank | File | Lines | Cohesion | Split urgency |
|---:|---|---:|---|---|
| 1 | `builder/src/builder/panels/map.rs` | **3,341** | Mixed: render + tools + dialogs + ctx menu | **High** |
| 2 | `src/economy.rs` | 1,743 | Mixed: config + resource model + derivation + markdown | Medium |
| 3 | `src/relations.rs` | 1,628 | Mixed: enums + stance algebra + derivation + IO | Medium |
| 4 | `builder/src/builder/panels/history.rs` | 1,557 | Wizard + table + filter | Medium |
| 5 | `builder/src/builder/command.rs` | **1,486** | One enum with ~80 variants, all `apply` impls | High |
| 6 | `gui-core/src/sector_view.rs` | 1,447 | One widget; mostly cohesive | Low/Medium |
| 7 | `builder/src/builder/panels/control.rs` | 1,405 | Five overlays + control derivation | Medium |
| 8 | `src/search.rs` | 1,367 | Constraint eval + report; rayon | Medium (cohesive) |
| 9 | `src/worlds.rs` | 1,361 | Enums + tables for the taxonomy | Low (declarative) |
| 10 | `builder/src/builder/panels/system.rs` | 1,356 | Inspector + bulk ops | Medium |
| 11 | `viewer/src/factions_overview.rs` | 1,349 | Single view | Medium |
| 12 | `src/diff.rs` | 1,308 | Diff algos + markdown | Medium |
| 13 | `builder/src/builder/panels/world.rs` | 1,249 | One panel | Medium |
| 14 | `src/sector_model/mod.rs` | 1,242 | DTO definitions + small impls | Low/Medium (cohesive) |
| 15 | `builder/src/builder/panels/routes.rs` | 1,177 | Route inspector + bulk + hidden-route builder | Medium |

Workspace totals: **257 `.rs` files**, **93,727 LOC** (excluding `old/`, `target/`).

### Finding 4.1 — `builder/src/builder/panels/map.rs` is 3,341 lines

- **Severity:** High
- **Category:** Modularity
- **Evidence:** `wc -l builder/src/builder/panels/map.rs` → 3,341. Comment header at line 1 lists *eight* responsibilities: hex render, tool dispatch, drag/drop, ADD ROUTE preview, rect-select, double-click rename, pinned/multi-select overlays, collision dialog, plus *§CTX1 Phase 1-7* right-click menu, transient dialogs, bulk rename, region rename, partial-regen anchor. `rg '^pub fn|^fn ' builder/src/builder/panels/map.rs` → 23 functions; many are >100 LOC each (see e.g. `handle_click`, `apply_sector_menu_action`).

**What I found.** This file is the single most concentrated source of GUI complexity in the project and the most-cited target in `CLAUDE.md`'s instructions (`CLAUDE.md` lists the `§CTX1` Phase 1-7 surface as living here).

**Why it matters.** Every right-click menu feature lands here, growing it further; the recently-added Phase 2-7 work is documented in `CLAUDE.md` as belonging to this file; this is by design but the design is wrong.

**Recommended fix.** Split into a sibling submodule directory:

```
builder/src/builder/panels/map/
  mod.rs            -- pub fn show; small dispatcher (≤ 250 LOC)
  toolbox.rs        -- show_toolbox + MapTool plumbing
  canvas.rs         -- show_hex_map + interaction loop
  interaction.rs    -- handle_click / handle_drag / rect-select
  context_menu.rs   -- §CTX1 menus (resolve_*, render_*, apply_*)
  dialogs.rs        -- show_*_dialog (place / rename / bulk / region / collision)
  partial_regen.rs  -- apply_partial_regen_anchor_click + arming UI
```

Each file ≤ ~500 LOC. Pattern follows `src/svg_export/` and `src/bitmap/` already-split modules — apply the same rule.

**Concrete steps.**
1. Extract `show_sector_context_menu` + `resolve_sector_context` + `apply_sector_menu_action` + the 5 `render_*_menu` helpers into a new `context_menu.rs`. They are already grouped under §CTX1 comments and share only `SectorContextMenu` / `SectorMenuTarget` from `state::types`.
2. Extract the 5 dialog functions (`show_place_dialog`, `show_rename_dialog`, `show_bulk_rename_dialog`, `show_region_rename_dialog`, `show_collision_dialog`) into `dialogs.rs`.
3. Update `CLAUDE.md` source-layout table to match.

**Risk of change.** Low — `pub(crate)` re-exports keep call sites stable. The `panel-as-free-function` contract documented in `builder/src/builder/panels/mod.rs:3-13` is preserved.

**Validation.** `cargo check --workspace`, `cargo test -p sectorforge-builder`, manual smoke of every right-click path.

---

### Finding 4.2 — `builder/src/builder/command.rs` is 1,486 lines

- **Severity:** Medium
- **Category:** Modularity
- **Evidence:** `wc -l builder/src/builder/command.rs` → 1,486. The file declares `pub enum BuilderCommand { ... }` and the corresponding `impl BuilderCommand { pub fn apply(...) }` matching every variant. Per `CLAUDE.md` the variants span: system/world/route/faction/region/intel/conflict/archetype mutations, partial regen, bulk operations, derivations.

**Why it matters.** Every new mutation lands here. The `apply` match is the *only* call site for `sector_model::mutation::*`; growing both files in lockstep is unavoidable.

**Recommended fix.** Keep the enum in one place (so the command bus remains single-file dispatch), but split the `impl` arms by resource via free helpers:

```
builder/src/builder/command/
  mod.rs                -- enum BuilderCommand; impl::apply dispatcher
  apply_systems.rs      -- fn apply_add_system / apply_move_system / ...
  apply_worlds.rs
  apply_routes.rs
  apply_factions.rs
  apply_regions.rs
  apply_overlays.rs     -- conflict / archetype / intel
  apply_bulk.rs
```

The `apply` dispatcher then becomes a flat `match` calling `apply_systems::add(...)`, `apply_worlds::rename(...)`, etc.

**Risk of change.** Low — internal-only refactor; `BuilderCommand` is `Serialize + Deserialize`, so as long as the enum lives in one file the on-disk session format is unchanged.

---

### Finding 4.3 — Several other panels exceed 1,000 lines

- **Severity:** Medium
- **Category:** Modularity
- **Evidence:** `builder/src/builder/panels/{history,control,system,world,routes}.rs` are 1,557 / 1,405 / 1,356 / 1,249 / 1,177 lines respectively. `viewer/src/factions_overview.rs` is 1,349.

**Recommended fix.** For each: identify the 2-4 conceptually-distinct sub-panels (e.g. `control.rs` has Co-Sovereign / Dominance / Primary / Heatmap / overlay-builder), extract each to a sibling file under a directory of the same name. Follow the model of `builder/src/builder/state/` which already split `mod.rs` cleanly into `selection.rs` / `undo.rs` / `derivations.rs` / `regions_ops.rs` / `generation_ops.rs` / `nav.rs` / `types.rs`.

**Priority order** (highest churn first, per `CLAUDE.md` table):
1. `panels/system.rs` — central inspector, hosts §CTX1 Phase 6 dispatch + system-map embedding.
2. `panels/world.rs` — pulled in by §6.7 / §6.8 / §6.9.
3. `panels/control.rs` — five overlays with five rebuild functions.
4. `panels/history.rs` — wizard + filter + table are independent.
5. `panels/routes.rs` — bulk + hidden-route builder are independent.

---

### Finding 4.4 — `BuilderState` is a God Object

- **Severity:** Medium → High (grows with each phase)
- **Category:** Architecture
- **Evidence:** `builder/src/builder/state/mod.rs:76-…` — `pub struct BuilderState` has **dozens** of fields spanning: project IO + dirty tracking + command log + cursor + snapshots + capacity + pinned sets + derivation cache + file watcher state + validation debouncer + selection mailboxes (5 typed ids + 1 string) + active tab + map tool + preview + partial regen rect + drag state + 5 transient dialogs (`pending_*`) + rect select + zoom + cache + per-tab UI scratch (region paint, route bulk filters, hidden-route builder, dominance locks, primary-faction locks, control overlay, history wizard, tick log, scroll target, context menu, system context menu, last menu action…).

`CLAUDE.md`'s table for `state/mod.rs` runs to *seven* paragraphs of "Phase X adds Y" appendages. The file header at `builder/src/builder/state/mod.rs:24-36` admits the `impl` blocks are *already* split by concern — but the struct itself has not been.

**Why it matters.** Every panel takes `state: &mut BuilderState` and so has free access to every other panel's UI scratch. Cross-panel coupling is not detectable by the compiler. New phases compound; the CLAUDE.md churn is the symptom, not the cause.

**Recommended fix.** Group fields into sub-structs by concern, exposing them through borrowing helpers. Keep the outer `BuilderState` flat at the top level only for the things every panel needs (`sector`, `config`, `index`, `command_log`, `dirty`, `modal`). Example target:

```rust
pub struct BuilderState {
    pub sector: GeneratedSector,
    pub config: AppConfig,
    pub project: ProjectIo,           // path / dirty / dirty_files / file_mtimes / file_watcher / auto_save
    pub history: CommandHistory,      // command_log / cursor / capacity / snapshots
    pub selection: Selection,         // selected_*_id, selected_systems, pinned_*
    pub map: MapUiState,              // tool, zoom, drag, rect_select, pending_*, ctx menus, view cache
    pub control: ControlUiState,
    pub routes: RoutesUiState,
    pub regions: RegionsUiScratch,
    pub generation: GenerationUiState,
    pub validation: ValidationState,  // report, debounce, dirty_since
    pub derivation_cache: DerivationCache,
    pub jobs: Vec<JobHandle>,
    pub modal: Option<ModalKind>,
}
```

Each `*UiState` lives in `state/<concern>.rs`. Panels then take `&mut state.map` (or similar) rather than the whole world.

**Risk of change.** Medium — touches every panel. Can be done incrementally: introduce one sub-struct per PR and keep delegating fields temporarily.

**Quick win first.** Move all `pending_*` fields and both context menus (`sector_context_menu`, `system_context_menu`) into a single `DialogState` sub-struct.

---

### Finding 4.5 — Inconsistent module style: nested `mod tests` inside `tests.rs`

- **Severity:** Nit (also fires Clippy in 2.1)
- **Category:** Modularity
- **Evidence:** `src/svg_export/tests.rs:1-2` wraps the tests in `#[cfg(test)] mod tests { ... }` while the parent module already imports them as `mod tests;`. `src/bitmap/tests.rs` follows the same pattern. `src/history/tests.rs` does *not* — it uses bare `#[test] fn ...`.

**Recommended fix.** Standardize on the bare form (matches `history/tests.rs`); remove the outer `mod tests { ... }` in `svg_export/tests.rs` and `bitmap/tests.rs`. Resolves the clippy lint in 2.1.

---

## 5. Library Public Surface

### Finding 5.1 — Almost every `src/*.rs` is `pub mod`

- **Severity:** Medium
- **Category:** API hygiene
- **Evidence:** `src/lib.rs:24-87` declares 50+ `pub mod`. Only `pub(crate) type FxMap` / `FxSet` are restricted (`src/lib.rs:31-32`). `rg -c 'pub fn' src/` reports **402** `pub fn` and `rg -c 'pub struct |pub enum'` reports **292** definitions.

**Why it matters.** Two consumers exist for the library: `sectorforge-builder` and `sectorforge-viewer`. Both need a narrow façade; today, anything inside the crate is reachable. That makes future internal refactors potentially breaking changes.

**Recommended fix.** Pass 1: identify modules whose only out-of-crate consumer is `sectorforge::` re-exports in `src/lib.rs`. Demote those to `pub(crate) mod`. Cross-reference call sites with `rg 'sectorforge::<modname>::' builder/ viewer/ gui-core/`.

For each module currently consumed externally, audit the `pub fn` list and drop unused public symbols to `pub(crate)`.

Per `REVIEW.md` §6.4: "Are `pub mod` declarations necessary? Could some modules be private?" — currently the answer is *no, and yes* respectively.

**Risk of change.** Low — these are internal-only crates.

---

### Finding 5.2 — Two `MutationError` types coexist

- **Severity:** Medium
- **Category:** Rust Idioms / API
- **Evidence:**
  - `src/errors.rs:41-52` — `pub enum MutationError { NotFound, Collision, InvalidCoord, InvalidState }` (`Serialize + Deserialize`, used externally).
  - `src/sector_model/mutation.rs:21-43` — `pub enum MutationError { SystemNotFound, WorldNotFound, RouteNotFound, FactionNotFound, RegionNotFound, CoordOutOfBounds, CoordOccupied, DuplicateRoute, SelfRoute, EventNotFound }`.

Both are *the* error type for mutations, both are named identically, both are `pub`. The first is referenced by `BuilderCommand` (`builder/src/builder/command.rs:14`); the second is the one the mutation API actually raises.

**Why it matters.** Confusing for new readers and dangerous if the wrong one is imported. The variants do not overlap, so combining them is non-trivial.

**Recommended fix.** Rename `src/errors.rs::MutationError` → `MutationErrorKind` (or remove if dead) and have callers import only `sector_model::mutation::MutationError`. If both are needed, give one a discriminator name that makes the difference obvious.

**Validation.** `rg 'errors::MutationError' --glob '*.rs'` — confirm the public consumers and update them in one pass.

---

### Finding 5.3 — `Result<Vec<u8>, String>` in production code

- **Severity:** Low
- **Category:** Rust Idioms
- **Evidence:** `builder/src/builder/session.rs:305` — `pub fn decode_base64(input: &str) -> Result<Vec<u8>, String>`.

**Why it matters.** Project standard everywhere else is `thiserror`. A string error obscures structure.

**Recommended fix.** Use the `base64` crate (zero-cost, in everyone's tree already, brings its own error type) — *or* define `pub enum Base64DecodeError { ... }` with `thiserror`. The handcrafted decoder is ~50 LOC and has no test seam in `session.rs`.

---

## 6. Error Handling

### Finding 6.1 — Two error enums (`SectorError`, `MutationError`) duplicated across modules

Covered by Finding 5.2 above. No central error mapping module — most call sites build `SectorError` via the constructors in `src/errors.rs:54-75`, which is fine.

### Finding 6.2 — Auto-save errors swallowed

- **Severity:** Medium
- **Category:** Error Handling / Observability
- **Evidence:** `builder/src/builder/state/undo.rs:68-78`:

```rust
pub fn trigger_auto_save(&mut self) {
    let Some(path) = self.auto_save_path.as_ref() else { return; };
    let Ok(text) = serde_json::to_string_pretty(&self.sector) else { return; };
    if std::fs::write(Path::new(path.as_std_path()), text).is_ok() {
        self.dirty = false;
    }
}
```

The serialization failure and the write failure are both silent. `self.dirty = false` *not* being set is the only signal; nothing surfaces to the user.

**Why it matters.** A user who saw "dirty marker cleared" can wrongly believe the project is saved. The fail-silent posture risks data loss exactly at the moment the user expects autosave to have run.

**Recommended fix.** Capture the error into a `last_auto_save_error: Option<String>` on `BuilderState`, surface it in `panels/status.rs` next to the dirty pip. The status bar already tails `last_menu_action` (per `CLAUDE.md`); add a sibling field.

**Risk of change.** Low.

---

### Finding 6.3 — `eprintln!` for warnings in library code

- **Severity:** Low
- **Category:** Observability
- **Evidence:**
  - `src/html_export.rs:63-67` — emits "sectorforge: interactive HTML is N bytes (warn threshold M)" to stderr.
  - `src/main.rs:14` — error path (acceptable; this is the CLI binary).
  - `src/bin/dhat_profile.rs:43` — acceptable, profiling-only.

**Why it matters.** The library currently writes to stderr without a hook. Programmatic consumers (the GUI) cannot redirect this.

**Recommended fix.** Either return the warning in a structured way (e.g. add `warnings: Vec<String>` to a result type) or accept an `&mut impl Write` for warnings. For just this single warning, the simplest fix is to push it into `cfg.warnings_callback: Option<Box<dyn Fn(&str)>>` on `HtmlConfig` and default to `eprintln!`-equivalent in the CLI binary.

---

## 7. Async / Concurrency

- **Severity:** N/A — no async runtime in use. `rg 'tokio|async fn|\.await' --glob '*.rs'` is empty in `src/` (only matches are in `docs/`).
- The single concurrent primitive in the GUI is `Arc<Mutex<f32>>` for progress in `gui-core/src/jobs.rs:11,69` — used only for atomic float updates from background closures; cannot deadlock.
- Parallelism is via `rayon` in `src/search.rs:1091-1099`. The use is order-preserving (`.collect()` into a `Vec`) — the comment at `Cargo.toml:35-37` explicitly calls out the determinism contract. ✓

No findings.

---

## 8. Security Review

This is an offline content-generation tool with no network surface, no auth, no untrusted user input beyond local TOML/JSON files chosen by the operator. The threat model is therefore narrow.

### Finding 8.1 — User-controlled relative paths joined to project root

- **Severity:** Low (informational)
- **Category:** Security
- **Evidence:**
  - `src/input.rs:76` — `let data_dir = root_dir.join(&data_dir_rel);` where `data_dir_rel = config.inputs.world_data_dir` (operator-controlled TOML string).
  - Same pattern at `src/input.rs:233` — `let abs = root.join(rel);`.
  - `src/presets.rs:72,89,117,138` — preset-relative paths.

**Why it matters.** The operator chooses both the project root *and* the TOML file, so this is not a privilege boundary in normal usage. If `sectorforge` is ever embedded as a multi-tenant service or invoked on untrusted projects (a shared CI step running PR-submitted projects), an attacker-controlled `world_data_dir = "../../etc/passwd"` would resolve outside the project root.

**Recommended fix.** Normalize and assert containment when the use case requires it: `assert!(data_dir.starts_with(&root_dir));` after canonicalization. For now, document in `src/input.rs` that all relative paths in config are trusted to the same level as the binary that ran them.

**Risk of change.** Low.

---

### Finding 8.2 — `examples/big_test`, `examples/m42_project`, etc. ship `data/` directories — check no secrets

- **Severity:** Low (informational)
- **Category:** Security
- **Evidence:** Multiple `examples/*/data` and `examples/*/out` directories exist. `.gitignore:33` excludes `examples/*/out/` but not `examples/*/data/`. Spot-checked — these are toml data files, no secrets.

No fix needed; flagged for periodic re-check.

---

## 9. Testing Review

### Inventory

```
running 165 tests   --> src/ unit tests
running  84 tests   --> tests/it/* integration tests (5 ignored)
running 249 tests   --> builder/src/ unit tests
running  17 tests   --> sectorforge-gui-core unit tests
running   1 test    --> gui-core/tests/map_snapshots.rs (golden)
running   3 tests   --> sectorforge-viewer unit tests
running   6 tests   --> doctests
```

**Total: 525 tests pass, 5 ignored, 0 failed.**

### Strengths

- Golden tests for the bitmap export: `tests/it/golden_png.rs` (11 `expect`s for fixture loading — acceptable test pattern).
- Property tests for invariants: `tests/it/invariants_proptest.rs`.
- Round-trip JSON tests embedded in domain modules: `src/html_export.rs:445-487`, `src/diff.rs:1304-1305`, `src/ids.rs:213-215`, `src/prose.rs:469`, `src/sites.rs:812`, `src/interestingness.rs:507`. These guard the serialization contract that the GUI depends on.
- Per-stage RNG determinism tests: `src/rng.rs:79-108`.
- `tests/it/cli_gui_parity.rs` — explicit contract test between CLI and GUI derivations.

### Finding 9.1 — Slow test suite imbalance

- **Severity:** Low
- **Category:** Testing
- **Evidence:** One test runner takes ≈ 36.7s while every other runner finishes in ≤ 2.3s. Almost certainly an integration test (likely `tests/it/golden_*` or `tests/it/segmentum_tests.rs`).

**Recommended fix.** Run with `cargo test -- --report-time` once to identify the slowest tests; consider gating the heaviest fixtures behind `#[cfg_attr(not(feature = "slow-tests"), ignore)]` or splitting into a separate `large` integration target.

**Risk of change.** Low — informational.

---

### Finding 9.2 — `expect("CARGO_MANIFEST_DIR")` pattern repeated across integration tests

- **Severity:** Nit
- **Category:** Testing
- **Evidence:** `tests/it/segmentum_tests.rs:9`, `tests/it/analytics_and_presets.rs:6,11`, `tests/it/svg_export_tests.rs:6`, `tests/it/search_and_diff.rs:8`, `tests/it/golden_generation.rs:213`, `tests/it/invariants_tests.rs:306`, `tests/it/validation_tests.rs:51`.

**Recommended fix.** A single `tests/it/common/mod.rs` helper:

```rust
pub fn manifest_dir() -> camino::Utf8PathBuf {
    camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
```

Replaces 7 copies; switches `env::var` (runtime) to `env!` (compile-time) which is sufficient for `CARGO_MANIFEST_DIR`.

---

## 10. Performance Review

No live measurements taken (out of scope); audit by code reading only.

### Observations (no actionable smells found)

- `src/rng.rs:33-68` `weighted_index` is O(n) per draw — acceptable for sub-million-element pools.
- Output writers consistently sort by ID first via `BTreeMap` / `BTreeSet`, paying log(n) per insert but giving byte-stable output. Trade-off is intentional and called out in `src/lib.rs:27-32`.
- `rayon` parallelism in `src/search.rs:1091-1099` is order-preserving (`into_par_iter().collect()` into `Vec`).
- Heap profiling is wired behind `dhat-heap` feature (`Cargo.toml:38-46`, `src/bin/dhat_profile.rs`). Profiling profile is configured (`Cargo.toml:80-85`).
- Release profile uses `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"` (`Cargo.toml:67-71`).

### Finding 10.1 — `GeneratedSector::all_worlds` is O(systems × worlds)

- **Severity:** Nit
- **Category:** Performance
- **Evidence:** `src/sector_model/mod.rs:289-291`:

```rust
pub fn all_worlds(&self) -> impl Iterator<Item = &GeneratedWorld> {
    self.systems.iter().flat_map(|s| s.worlds.iter())
}
```

Correct; this is the linear traversal. The lookup variant `get_world` (`mod.rs:274-283`) is also O(N×M):

```rust
pub fn get_world(&self, id: &WorldId) -> Option<&GeneratedWorld> {
    for sys in &self.systems {
        for w in &sys.worlds {
            if w.id == *id { return Some(w); }
        }
    }
    None
}
```

For a typical 24-system sector this is fine. The builder has `BuilderIndex` (`builder/src/builder/index.rs`) which presumably maintains an `id → (system_idx, world_idx)` table; library-level callers fall back to the linear scan.

**Recommended fix.** None for now — typical sectors are small. If profiling ever points here, expose a lazy `OnceCell<HashMap<WorldId, (usize, usize)>>` on the sector. Do *not* preemptively add this — typical N is 24 systems.

---

## 11. Observability Review

- No `tracing`, no structured logging.
- `eprintln!` is used in the CLI binary (`src/main.rs:14`) and one library location (`src/html_export.rs:63`) — see Finding 6.3.
- Progress events for generation are exposed structurally as `enum SectorProgress` (`src/generation/mod.rs:31-80+`). Caller-supplied closure pattern — clean.
- Manifest hashing (`src/sector_model::GenerationManifest` — `seed_hash`, `input_digests`, `settings_digest`) gives reproducibility without runtime logging.

### Finding 11.1 — No `tracing` / structured logging

- **Severity:** Low
- **Category:** Observability
- **Evidence:** `rg 'tracing::' src/ --glob '*.rs'` — no hits. No `log = ` in `Cargo.toml`.

**Why it matters.** For a CLI-only single-shot generator, this is fine. For the builder (long-running GUI process), tracing would help debug user-reported "the regenerate panel hangs" / "history wizard skipped a step" issues that aren't reproducible from a save file.

**Recommended fix.** Defer. If/when added, prefer `tracing` + `tracing-subscriber` + `tracing-tree` for builder. Library should accept a `tracing` span from the caller, not initialize its own subscriber.

---

## 12. Configuration / Secrets

- All config is TOML, parsed via the `toml` crate. Structured DTOs in `src/config.rs:5-…`.
- No environment variables read in library code (only `CARGO_MANIFEST_DIR` in tests + `HOME` in `builder/src/builder/preferences.rs:28`).
- No secrets stored anywhere — this is a tabletop content generator, no auth.

No findings.

---

## 13. Documentation & DX

### Finding 13.1 — `README.md` is one line ("# 40k-sector-generator")

- **Severity:** Medium (very low-effort fix, very high-effort consequence)
- **Category:** Documentation
- **Evidence:** `cat README.md` → `# 40k-sector-generator`. No build instructions, no examples, no "what is this", no link to `GUIDE.md` (which exists and is 257KB) or `OVERVIEW.md` (29KB) or `BUILDER.md` (35KB).

**Why it matters.** A new contributor or external reader cannot tell what to do. The project has *substantial* documentation in `GUIDE.md` + `OVERVIEW.md` + `BUILDER.md` but the entry point is empty.

**Recommended fix.** ≤ 50-line `README.md`:

```markdown
# sectorforge — deterministic 40k sector generator

A deterministic generator + interactive builder/viewer for Warhammer 40k star sectors.
Pure Rust, no network calls, byte-stable output for any (seed, inputs) pair.

## Build & run

cargo build
cargo run --bin sectorforge -- generate --project examples/m42_project --out out
cargo run --bin sectorforge-viewer -- out/sector.json
cargo run --bin sectorforge-builder -- --project examples/m42_project

## Layout

- `src/` — library (`sectorforge` crate). Headless generator + derivations.
- `gui-core/` — shared egui widgets (read-only sector renderer, palette).
- `viewer/` — viewer/editor binary.
- `builder/` — builder binary (full editor + command bus + undo).

## Docs
- `GUIDE.md` — feature-by-feature reference.
- `OVERVIEW.md` — domain model overview.
- `BUILDER.md` — builder architecture.
- `CLAUDE.md` — source layout map.

## Test/lint
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # currently fails (see CODEBASE_REVIEW.md §2.1)
```

**Risk of change.** Zero.

---

### Finding 13.2 — `CLAUDE.md` carries an outsized share of "current state" content

- **Severity:** Low
- **Category:** Documentation
- **Evidence:** `CLAUDE.md` is 20.5KB; the source-layout table contains paragraphs of "§CTX1 Phase X adds Y" appendages on individual file entries (e.g. the entry for `state/mod.rs` runs across seven sentences describing six phases). This is doubling as architectural changelog rather than a stable map.

**Why it matters.** `CLAUDE.md` is loaded into the LLM prompt; it should describe steady-state, not history. History belongs in commit messages.

**Recommended fix.** Pass through `CLAUDE.md` and replace "Phase X adds Y" prose with declarative facts ("Holds `pending_world_rename` for the §6.7 rename dialog"). Drop references to specific phase numbers from the file table.

---

### Finding 13.3 — `INPUT.md` doubles as instructions and as token-saving rules

- **Severity:** Nit
- **Category:** Documentation
- **Evidence:** `INPUT.md:1-27` — agent-context optimization rules; referenced from `CLAUDE.md:3`.

This is fine but worth flagging as a non-standard convention. Document it in the README so future contributors know `INPUT.md` is *not* a feature spec.

---

## 14. Dependencies

| Crate | Used for | Notes |
|---|---|---|
| `clap` (4, derive) | CLI parsing | Standard. |
| `serde` (1, derive, rc) | DTO derives, `Arc<str>` serde | `rc` feature enables `Arc`/`Rc` serde — correct for `Arc<str>` usage. |
| `serde_json` | JSON IO | Standard. |
| `toml` (0.8) | Config IO | Standard. |
| `thiserror` (1) | Domain errors | Standard. |
| `rand` (0.8) / `rand_chacha` (0.3) | Deterministic RNG | Pinned versions — correct, since `ChaCha8` output is part of the deterministic contract. |
| `blake3` (1) | Stage-key derivation | `src/rng.rs:9-11`. |
| `camino` (1, serde1) | UTF-8 paths | Consistent. |
| `image` (0.25, png only) | Bitmap export | `default-features = false, features = ["png"]` — clean. |
| `rustc-hash` (2) | `FxHashMap`/`FxHashSet` for internal lookups | Correctly *not* used in output paths. |
| `rayon` (1) | Search parallelism | Order-preserving usage. |
| `dhat` (0.3, optional) | Heap profile | Behind `dhat-heap` feature flag. |
| `eframe` / `egui` (0.29) | GUI (workspace crates only) | Correctly absent from `sectorforge`. |
| `rfd` (0.17) | File dialogs (builder/viewer) | Confined to GUI crates. |
| `tempfile` (3, dev) | Test temp dirs | Standard. |
| `proptest` (1, dev) | Property tests | Standard. |
| `criterion` (0.5, dev) | Bench harness | Standard. |

### Finding 14.1 — Hand-rolled base64 instead of `base64` crate

- **Severity:** Low
- **Category:** Dependencies
- **Evidence:** `builder/src/builder/session.rs:305-…` — ~50 LOC manual base64 decoder with `Result<_, String>`.

**Recommended fix.** Add `base64 = "0.22"` to `builder/Cargo.toml` and replace.

**Risk of change.** Low. The session format itself doesn't change — base64 is base64.

---

### Finding 14.2 — No `cargo-deny` or `cargo-audit` configuration

- **Severity:** Low
- **Category:** Dependencies
- **Evidence:** No `deny.toml`; CI absent (Finding 2.2).

**Recommended fix.** Add as part of the CI workflow (Finding 2.2) once it lands.

---

## 15. API / Shared-Type Contracts

This is a single-process app — no front-end/back-end HTTP boundary in the `REVIEW.md` sense. The relevant boundary is **library DTOs ↔ on-disk JSON ↔ GUI consumers**.

### Assessment

- All DTOs in `src/sector_model/mod.rs` derive both `Serialize` and `Deserialize`.
- `Arc<str>` is used throughout (cheap clone, serde-compatible via `rc` feature).
- IDs are newtypes: `SystemId`, `WorldId`, `RouteId`, `FactionId` from `src/ids.rs`.
- Round-trip tests exist (Finding 9 above).
- `skip_serializing_if` is consistently applied to default/empty fields, so the on-disk JSON stays compact and stable.

### Finding 15.1 — `chronicle` field is *not* under `Arc` while every other overlay is

- **Severity:** Low
- **Category:** API Contract
- **Evidence:** `src/sector_model/mod.rs:25-55`:

```rust
pub influence_field:  Arc<crate::influence_field::InfluenceField>,
pub power_projection: Arc<crate::power_projection::PowerProjectionMap>,
pub relations:        Arc<crate::relations::RelationsMatrix>,
pub regions:          Arc<Vec<crate::regions::WarpRegion>>,
pub economy:          Arc<crate::economy::EconomyReport>,
pub chronicle:        crate::history::SectorChronicle,        // <- not Arc
```

**Why it matters.** Inconsistent clone cost. Every other derivation overlay is shared via `Arc`; `chronicle` clones in full whenever the sector is cloned (which the builder does for background jobs per `state/mod.rs:7-14`).

**Recommended fix.** Wrap `chronicle` in `Arc` to match. Adjust callers in `src/lib.rs::derive_history` and `src/history/`.

**Risk of change.** Medium — `SectorChronicle` is `Serialize + Deserialize`, so the on-disk JSON shape is unchanged, but every place that mutates `chronicle` in-place needs `Arc::make_mut`. There appear to be only a couple of those (search `rg 'chronicle =' src/` and `rg 'sector.chronicle' src/`).

---

## 16. Prioritized Findings Table

| # | Severity | Category | Location | Finding | Action |
|---:|---|---|---|---|---|
| 1 | High | Build/CI | `src/bitmap/routes.rs:417`, `src/svg_export/routes.rs:183`, `src/svg_export/tests.rs:2` | Clippy `-D warnings` fails (3 lints) | Fix lints + add CI gate (§2.1) |
| 2 | High | Build/CI | repo root | No CI workflow | Add `.github/workflows/ci.yml` (§2.2) |
| 3 | High | Modularity | `builder/src/builder/panels/map.rs` | 3,341-line panel mixing 8+ concerns | Split into `panels/map/` submodule (§4.1) |
| 4 | Medium → High | Architecture | `builder/src/builder/state/mod.rs` | `BuilderState` God Object, grows every phase | Group fields by concern into sub-structs (§4.4) |
| 5 | Medium | Modularity | `builder/src/builder/command.rs` | 1,486-line enum + apply | Split `impl` arms by resource (§4.2) |
| 6 | Medium | Modularity | `builder/src/builder/panels/{history,control,system,world,routes}.rs`, `viewer/src/factions_overview.rs` | 1,000–1,557-line panels | Per-panel split (§4.3) |
| 7 | Medium | API hygiene | `src/lib.rs:24-87` | 50+ `pub mod` — nothing private | Demote internal modules to `pub(crate)` (§5.1) |
| 8 | Medium | Rust Idioms | `src/errors.rs:41`, `src/sector_model/mutation.rs:21` | Two `MutationError` enums | Rename + consolidate (§5.2) |
| 9 | Medium | Error Handling | `builder/src/builder/state/undo.rs:68-78` | Auto-save errors silent | Capture + surface in status bar (§6.2) |
| 10 | Medium | Documentation | `README.md` | One-line README | Write ≤ 50-line entry-point doc (§13.1) |
| 11 | Low | Rust Idioms | `builder/src/builder/session.rs:305` | Hand-rolled base64, `Result<_, String>` | Use `base64` crate (§14.1) |
| 12 | Low | API Contract | `src/sector_model/mod.rs:48-52` | `chronicle` not wrapped in `Arc` like sibling overlays | Wrap + audit mutate sites (§15.1) |
| 13 | Low | Security | `src/input.rs:76,233`, `src/presets.rs` | Operator-controlled relative paths joined to project root | Document trust assumption (§8.1) |
| 14 | Low | Observability | `src/html_export.rs:63` | `eprintln!` in library | Move to caller-supplied warning channel (§6.3) |
| 15 | Low | Documentation | `CLAUDE.md` | Source layout doubles as phase changelog | Strip phase prose, keep facts (§13.2) |
| 16 | Low | Testing | tests | One suite ≈ 36.7s vs ≤ 2.3s for others | Profile + isolate slow tests (§9.1) |
| 17 | Low | Build/CI | repo root | No `deny.toml` / `cargo audit` | Add after CI lands (§2.3, §14.2) |
| 18 | Nit | Modularity | `src/svg_export/tests.rs`, `src/bitmap/tests.rs` | `mod tests { ... }` inside `tests.rs` | Drop wrapper (§4.5) — also resolves a clippy lint |
| 19 | Nit | Testing | `tests/it/*.rs` (7 files) | `env::var("CARGO_MANIFEST_DIR").expect(..)` repeated | Single helper (§9.2) |
| 20 | Nit | Documentation | `INPUT.md` | Doubles as agent rules + project doc | Cross-reference in README (§13.3) |

---

## 17. Refactoring Roadmap

### Stage 0 — Safety net (≤ 1 day)
1. Fix the 3 clippy lints (§2.1).
2. Add CI workflow with fmt + clippy + test gates (§2.2).
3. Write the README (§13.1).
4. Add `last_auto_save_error` and surface it (§6.2).

**Exit criteria.** Clippy green; CI green; new contributor can `cargo run` from the README.

### Stage 1 — Module hygiene (≤ 1 week)
1. Consolidate `MutationError` (§5.2).
2. Audit `pub mod` → `pub(crate) mod` in `src/lib.rs` (§5.1). One PR per module group.
3. Drop the `mod tests` wrapper in `tests.rs` files (§4.5).
4. Replace handcrafted base64 (§14.1).
5. Wrap `chronicle` in `Arc` (§15.1).

**Exit criteria.** No external behavior change; `cargo doc` shows a narrower public surface.

### Stage 2 — Builder file splits (≤ 2 weeks, parallelizable)
1. Split `builder/src/builder/panels/map.rs` (§4.1).
2. Split `builder/src/builder/command.rs` (§4.2).
3. Split `builder/src/builder/state/mod.rs` God Object incrementally (§4.4) — one sub-struct per PR, start with `DialogState`.
4. Split panels in priority order: `system.rs` → `world.rs` → `control.rs` → `history.rs` → `routes.rs` (§4.3).

**Exit criteria.** No file over ~700 LOC in `builder/src/builder/panels/`. `cargo test -p sectorforge-builder` still green.

### Stage 3 — Observability + DX (≤ 1 week)
1. Replace `eprintln!` in library with caller-supplied warning channel (§6.3).
2. Update `CLAUDE.md` to drop phase prose (§13.2).
3. Optional: add `tracing` to the builder (§11.1).
4. Optional: profile + isolate slow tests (§9.1).

---

## 18. Quick Wins (≤ 1 day each)

1. **Fix the 3 clippy lints.** §2.1 — three two-line edits.
2. **Write `README.md`.** §13.1 — template given above.
3. **Add `.github/workflows/ci.yml`.** §2.2 — template given above.
4. **Add `last_auto_save_error` to `BuilderState` and tail it in the status bar.** §6.2 — one new `Option<String>` field + 4 lines in `status.rs`.
5. **Drop `mod tests { ... }` wrapper in `src/svg_export/tests.rs` + `src/bitmap/tests.rs`.** §4.5 — also clears one clippy lint.
6. **Replace hand-rolled base64 with `base64` crate.** §14.1 — drops ~50 LOC.
7. **Add a tests helper for `CARGO_MANIFEST_DIR`.** §9.2 — removes 7 copies.
8. **Wrap `chronicle` in `Arc` to match sibling overlays.** §15.1 — small consistency fix.

---

## 19. Validation Checklist (for the maintainer applying these fixes)

```bash
# After each change set:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # confirms public surface shrinks as expected
```

For Stage 2 split PRs additionally:
- Smoke-test every context menu schema in the builder (`§CTX1` Phase 1-7).
- Confirm session round-trip via "open project → make a change → save → reopen".
- Re-generate `examples/m42_project` and diff against the pre-refactor JSON.

---

## 20. Open Questions

These are *not* blocking the review — each is a clarification that would make follow-up work more precise.

1. **Is the workspace ever expected to grow a network surface?** The current architecture assumes single-process. If yes, the audit findings in §8 become higher-severity.
2. **Are external consumers of the `sectorforge` library expected (outside this workspace)?** If yes, the `pub mod` audit (§5.1) is mandatory and the version pinning policy needs to be decided.
3. **Is there a target deletion plan for `old/`?** It is excluded by `CLAUDE.md` instruction and `.gitignore`, but it sits in working tree.
4. **Is `docs/CONTEXT_MENU.txt` the canonical spec for the `§CTX1` series?** That would let me cross-check `panels/map.rs` against the spec to see *which* responsibilities legitimately belong together.

---

## 21. Self-Check (reviewer)

- [x] Every finding cites a file path; most cite line numbers.
- [x] Facts, inferences, and recommendations are separated within each finding.
- [x] Severity assigned per the `REVIEW.md` rubric.
- [x] No invented files, symbols, or behaviors.
- [x] Both library (`src/`) and GUI crates (`builder/`, `viewer/`, `gui-core/`) reviewed.
- [x] Shared/contract boundary (DTOs + JSON) reviewed in §15.
- [x] Quick wins separated from stage roadmap.
- [x] Validation commands provided.
- [x] Reviewer noted what was *not* measured (perf, security beyond static review).
