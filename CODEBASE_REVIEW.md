# Codebase Review — sectorforge (40k-sector-generator)

Prepared per [`REVIEW.md`](REVIEW.md). Evidence-based, LLM-optimized: every finding cites `path:line`. No invented files. Severity follows the rubric (`REVIEW.md` §11).

> **Scope.** Single-repo Rust workspace: library crate `sectorforge` + binaries (`sectorforge`, `sectorforge-builder`, `sectorforge-viewer`) + shared crate `sectorforge-gui-core`. Tested on commit `4dbd5b7` (branch `main`, `git log -1 --format=%s` → "system view"). The `old/` directory was excluded by explicit project instruction (`CLAUDE.md:5`).

> **Comparison to previous review.** The previous CODEBASE_REVIEW.md (commit `d8b7554`) is partially obsolete:
> - **Resolved:** `src/` parent-module layout landed (`src/lib.rs:48-66`, `docs/MAP.md:1-22`) — addresses prior §5.1 in spirit. `builder/src/builder/panels/map.rs` is now split into `panels/map/{mod,cache,context_menu,dialogs,interactions}.rs` (prior §4.1).
> - **Regressed:** `cargo fmt --all -- --check` is now red (it was clean). `cargo test --workspace` now has a failing golden (it was green).
> - **Still open:** clippy `-D warnings` red (different lints now), `MutationError` duplication, hand-rolled base64, one-line README, no CI, `chronicle` still un-`Arc`'d, `eprintln!` in library, `BuilderState` God Object.

> **Toolchain status snapshot.** Captured this session on commit `4dbd5b7`.

| Command | Real exit | Notes |
|---|---:|---|
| `cargo fmt --all -- --check` | **1** | 2 files need reformatting: `gui-core/src/system_view.rs` (multiple hunks) — regression. |
| `cargo check --workspace --all-targets` | 0 | Clean. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **101** | 17 lint errors across 5 files. Categories: `clone_on_copy` ×5, `unnecessary_map_or` ×4, `field_reassign_with_default` ×3, `format_collect` / `iter_any_eq` ×2, `module_inception` ×1 (same site as before — `src/export/svg_export/tests.rs:2`), `single_char_add_str` ×1, `too_many_arguments` ×1. |
| `cargo test --workspace` | **non-zero** | **1 test fails**: `gui-core/tests/map_snapshots.rs:354` — `map_snapshots_match_goldens` hash mismatch on `system_glyphs.png`. All other suites green: 165 lib unit, 249 builder unit, 21 gui-core unit, 3 viewer unit, 84 integration (5 ignored), 6 doctests = **528 pass, 5 ignored, 1 failed**. |

---

## 1. Executive Summary

**Overall assessment.** Large deterministic generator + GUI workspace (≈ **93,958 LOC** across **275 `.rs` files**, excluding `target/` and `old/`). Library crate `sectorforge` was reorganised since the previous review into seven parent modules (`model`, `loading`, `gen`, `analysis`, `export`, `validate`, `cli`) with `src/lib.rs:48-173` re-exporting the original flat paths so downstream crates were not perturbed — a measurable architectural improvement. Determinism discipline still holds: stage-keyed RNG via `blake3` (`src/model/rng.rs`), `BTreeMap`/`BTreeSet` for output, `FxMap`/`FxSet` aliased and restricted to internal lookup (`src/lib.rs:54-55`). The mutation API in `src/model/sector_model/mutation.rs` is still the only structural-write seam. Tests are extensive and 99.8% green.

**Main risks.**
1. **CI gates would fail today.** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all return non-zero — three of the four standard gates. (Findings 2.1, 2.2, 2.3)
2. **One golden test fails.** `gui-core/tests/map_snapshots.rs::map_snapshots_match_goldens` — `system_glyphs.png` hash diverged from goldens (`b35f9bd…` vs expected `b75f25e…`). The last commit is titled "system view" — likely an un-blessed snapshot. (Finding 9.1)
3. **No CI.** No `.github/workflows/`, no `deny.toml`, no `clippy.toml`, no `rust-toolchain.toml`. (Finding 2.4)
4. **`BuilderState` is still a God Object.** `builder/src/builder/state/mod.rs:76-731` declares **137 `pub` fields** in one struct, growing with each phase. (Finding 4.4)
5. **Two `MutationError` enums still coexist.** `src/model/errors.rs:43` vs `src/model/sector_model/mutation.rs:22`. The first one has zero observed external consumers but is still `pub`. (Finding 5.2)
6. **README is still one line.** (Finding 13.1)

**Best qualities.**
- Determinism + manifest hashing (`src/model/rng.rs`, `GenerationManifest`).
- Typed IDs end-to-end (`SystemId`/`WorldId`/`RouteId`/`FactionId`).
- Library has no async runtime, no `thread_rng`, no leaks of `eframe`/`egui` into `src/` (verified: `rg 'tokio|async fn|\.await' src/` → 0; `rg 'thread_rng' src/` → 0; `rg 'eframe' src/` → 0).
- Per-stage determinism tests, round-trip JSON tests on every overlay, golden PNG tests for bitmap export.
- Parent-module split (`src/lib.rs:48-173`) preserves all original paths via `pub use`.
- Aggressive sub-module decomposition is the established pattern (`src/export/bitmap/`, `src/export/svg_export/`, `src/gen/`, `src/analysis/history/`, `builder/src/builder/state/`, `builder/src/builder/panels/map/`).

**Highest-priority recommendation.** Fix the broken gates in this order — bless or revert the golden (Finding 9.1) → `cargo fmt --all` (Finding 2.1) → fix the 17 clippy lints (Finding 2.2) → add a minimum-viable CI workflow (Finding 2.4). After that, address the `BuilderState` God Object (Finding 4.4) and the lingering `MutationError` duplication (Finding 5.2).

---

## 2. Build/CI/Release Audit

### Finding 2.1 — `cargo fmt --all -- --check` fails (regression)

- **Severity:** High
- **Category:** Build/CI
- **Evidence:** `cargo fmt --all -- --check` exits 1. The diff is concentrated in `gui-core/src/system_view.rs` — at least two hunks: an `allocate_exact_size` call missing a destructure, and a `pick_world` call argument wrap. 57 lines of `Diff in …` output total.

**What I found.** A recent commit (`4dbd5b7` "system view") introduced rendering changes in `gui-core/src/system_view.rs` without running `cargo fmt`.

**Why it matters.** The previous review captured this gate as clean. The project is moving toward strict gates (the two GUI crates pin `disallowed_types = "deny"` and `disallowed_methods = "deny"` in `builder/Cargo.toml:22-24` and `viewer/Cargo.toml:24-26`). Any CI gating on `cargo fmt` will reject the current `main`.

**Recommended fix.** `cargo fmt --all`. One commit, no behavioural change.

**Validation.** `cargo fmt --all -- --check` returns 0.

**Risk of change.** Zero.

---

### Finding 2.2 — `cargo clippy -- -D warnings` fails with 17 errors across 5 files

- **Severity:** High
- **Category:** Build/CI
- **Evidence:** `cargo clippy --workspace --all-targets -- -D warnings` exits 101. Distinct lint categories (counts):

| Count | Lint | Sample location |
|---:|---|---|
| 5 | `clippy::clone_on_copy` (`StabilityState`) | `builder/src/builder/command.rs:534`, `:535`, `:836`; `panels/map/context_menu.rs:254`, `:553` |
| 4 | `clippy::unnecessary_map_or` | `builder/src/builder/panels/hooks.rs:158`, `panels/missions.rs:200`, `panels/sites.rs:203`, `panels/conflict.rs` |
| 3 | `clippy::field_reassign_with_default` | `builder/src/builder/command.rs:1052,1077`, `panels/history.rs:1182,1491` |
| 2 | `clippy::iter_any_eq` (`contains()` over `iter().any()`) | `builder/src/builder/panels/conflict.rs:88,91` |
| 1 | `clippy::module_inception` | `src/export/svg_export/tests.rs:2` (same site as prior review §2.1) |
| 1 | `clippy::single_char_add_str` | `builder/src/builder/project_io.rs:278` (`push_str` of one char) |
| 1 | `clippy::too_many_arguments` (8/7) | `builder/src/builder/panels/system.rs:84` |

(Reproduce: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -E "^\s+--> " | sort -u`.)

**What I found.** All 17 are mechanical. The exact lints differ from the prior review — the previously listed `neg_cmp_op_on_partial_ord` lints (`src/bitmap/routes.rs:417`, `src/svg_export/routes.rs:183`) appear to have been silently resolved by the export refactor; new lints replaced them.

**Why it matters.** Same as 2.1 — gating CI is blocked.

**Recommended fix.** One PR per category, ordered by edit footprint:
1. `clone_on_copy` on `StabilityState` — verify `StabilityState` is `Copy` (`rg 'impl.*Copy.*for.*StabilityState' src/`); replace `.clone()` with implicit copy on those five sites.
2. `field_reassign_with_default` — rewrite as struct-literal with `..Default::default()` per the lint suggestion at `builder/src/builder/command.rs:1077`.
3. `unnecessary_map_or` — `x.map_or(false, |v| v == y)` → `x == Some(y)` (or `is_some_and`).
4. `iter_any_eq` — `v.iter().any(|x| x == &target)` → `v.contains(&target)`.
5. `module_inception` — drop the outer `mod tests { ... }` wrapper in `src/export/svg_export/tests.rs:1-83`. Inside `tests.rs` the file *is* the module — re-nesting `mod tests` doubles the name. (Same pattern was applied successfully to `src/export/bitmap/tests.rs` already — note `src/export/bitmap/mod.rs:205-206` declares `mod tests;` and `src/export/bitmap/tests.rs` uses bare free-functions; only `svg_export/tests.rs` is still wrapped.)
6. `single_char_add_str` — replace `s.push_str("c")` with `s.push('c')`.
7. `too_many_arguments` in `panels/system.rs:84` — group related params into a struct (or apply `#[allow]` with a justification comment).

**Validation.** `cargo clippy --workspace --all-targets -- -D warnings` exits 0.

**Risk of change.** Low — none of these lint fixes change semantics.

---

### Finding 2.3 — Test suite is red (golden snapshot)

- **Severity:** High
- **Category:** Build/CI / Testing
- **Evidence:** `cargo test --workspace` fails 1 test: `gui-core/tests/map_snapshots.rs:354` — `map_snapshots_match_goldens` panics with:
```
assertion `left == right` failed: map snapshot system_glyphs changed; inspect /Users/.../target/map_snapshots/current/system_glyphs.png then run `UPDATE_MAP_SNAPSHOTS=1 cargo test -p sectorforge-gui-core map_snapshots_match_goldens --quiet` to bless
  left: "b35f9bd40b5f6566516c52f9637283ea772e6b906798ccc6d434a4f4baab2d72"
 right: "b75f25e66f1c45699e6166b86c57478b57f790dacf4d16790ea01c09badd8482"
```
The diverging artefact is the `system_glyphs` snapshot. HEAD commit subject is "system view" — likely a missed bless.

**Why it matters.** A red test on `main` undermines any test-as-gate posture and risks normalising failure. See also Finding 9.1.

**Recommended fix.** Either:
- Roll back the rendering change if unintentional, OR
- Inspect `target/map_snapshots/current/system_glyphs.png` against `gui-core/tests/goldens/`, confirm the new visual is intended, then `UPDATE_MAP_SNAPSHOTS=1 cargo test -p sectorforge-gui-core map_snapshots_match_goldens --quiet` and commit the new golden.

**Validation.** `cargo test --workspace` exits 0.

**Risk of change.** Medium — golden goldens encode visual contracts; only bless if the change is *intended*.

---

### Finding 2.4 — No CI configuration (unchanged)

- **Severity:** High
- **Category:** Build/CI
- **Evidence:** `ls -la .github` → no such directory. No `deny.toml`, `clippy.toml`, `rust-toolchain.toml`.

**Why it matters.** Findings 2.1–2.3 are all states the prior review noted would be caught by CI. They were not. The cost of three independent breakages on a green-trending main is exactly what a 30-line workflow prevents.

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

Optionally add `cargo audit` / `cargo deny check` as separate non-blocking jobs once a `deny.toml` exists.

**Risk of change.** Low.

---

### Finding 2.5 — Optional dev tools absent (unchanged)

- **Severity:** Low
- **Category:** Build/CI
- **Evidence:** No `deny.toml`, no `clippy.toml`, no `cargo-machete`/`cargo-udeps` artefacts.

**Recommended fix.** After CI lands, drop in a minimal `deny.toml` and run `cargo machete` once to inventory unused deps.

---

## 3. Architecture Assessment

### Repository Map (workspace)

```
sector-generator (lib + sectorforge bin + dhat-profile bin) — src/
├── sectorforge-gui-core  — gui-core/          // shared egui widgets
├── sectorforge-viewer    — viewer/            // read-only viewer binary
└── sectorforge-builder   — builder/           // editor binary + command bus
```

Dependency direction (top → down, no cycles observed):

```
sectorforge (lib, headless domain + IO)
   ▲                         ▲
   │                         │
sectorforge-gui-core   sectorforge-viewer  ── consumes lib + gui-core
   ▲
sectorforge-builder ── consumes lib + gui-core
```

**Library layout.** `src/lib.rs:48-66` declares **seven** parent modules; each was a flat `pub mod foo` at the root in the prior review. Compatibility re-exports at `src/lib.rs:73-133` preserve every original `sectorforge::foo` path.

```
src/
├── model/        → DTOs, IDs, RNG, error types, taxonomy
│   └── sector_model/  (1242 + 866 LOC — mod.rs + mutation.rs)
├── loading/      → project / config / presets / sector_save
├── gen/          → generation pipeline (regions, routes, factions, hidden_routes, sites, …)
├── analysis/     → pure read-only derivations (history, prose, hooks, missions, economy, …)
├── export/       → bitmap, svg_export, html_export, render, segmentum, subsectors, system_map
├── validate/     → diff, invariants, validation
├── cli/          → binary command dispatcher
├── bin/          → dhat_profile.rs (heap profiling, gated behind `dhat-heap` feature)
├── worlds.rs, worlds_toml.rs  (foundational taxonomy at crate root)
├── lib.rs, main.rs
```

**Assessment.** Sensible split. Domain logic stays in the library; GUI dependencies (`eframe`, `egui`, `rfd`) are isolated to the three downstream crates and never leak into `src/` (verified: `rg eframe src/ → 0`, `rg egui src/ → 0`, `rg rfd src/ → 0`).

### Layer Assessment

| Layer | State | Evidence |
|---|---|---|
| CLI parse + dispatch | **Clear** | `src/main.rs` delegates; `src/cli/mod.rs` is flat dispatch over per-subcommand modules. |
| Project IO / config | **Clear** | `src/loading/{input,config,presets,sector_save}.rs`. |
| Domain DTOs | **Clear** | `src/model/sector_model/mod.rs` (1,242 LOC — declarative + small impls). |
| Mutation API | **Clear (large)** | `src/model/sector_model/mutation.rs` (866 LOC) owns every structural write. Only caller is `BuilderCommand::apply` (`builder/src/builder/command.rs:298+`). |
| Derivations | **Clear** | Each derivation lives in its own module under `src/analysis/`; library `derive_*` / `derive_*_with` pairs in `src/lib.rs`. |
| Export | **Clear, well-split** | `src/export/bitmap/`, `src/export/svg_export/`, `src/export/html_export/`, `src/export/render_core/`, etc. The export tree is the de-facto template for what oversized files should look like. |
| Builder state | **Leaky (unchanged from prior review)** | `BuilderState` (`builder/src/builder/state/mod.rs:76-731`) — 137 `pub` fields. See Finding 4.4. |
| Builder panels | **Mixed** | `panels/map/` split (good); `panels/system.rs` grew to 1,508 LOC; `panels/control.rs`, `panels/history.rs`, etc. still oversized. See Finding 4.3. |
| Viewer | **Mixed** | `viewer/src/app/` is decomposed; `viewer/src/factions_overview.rs` (1,349 LOC) is not. |

---

## 4. File/Module Organization Audit

### Top 25 longest `.rs` files (workspace, excluding `target/`, `old/`)

| Rank | File | LOC | Notes |
|---:|---|---:|---|
| 1 | `src/analysis/economy.rs` | 1,743 | Config + resource model + derivation + markdown. Cohesive but at threshold. |
| 2 | `src/analysis/relations.rs` | 1,628 | Enums + stance algebra + derivation + IO. |
| 3 | `builder/src/builder/panels/history.rs` | 1,557 | Wizard + table + filter. Mixed concerns (multiple clippy hits at 1182, 1491). |
| 4 | **`builder/src/builder/panels/system.rs`** | **1,508** | +152 LOC since prior review. Inspector + bulk ops + §CTX1 Phase 6 dispatch + clippy `too_many_arguments` at line 84. |
| 5 | `builder/src/builder/command.rs` | 1,486 | Single enum with ~80 variants + `apply` arms. Same as prior review. |
| 6 | `gui-core/src/sector_view.rs` | 1,480 | One widget; mostly cohesive. |
| 7 | `builder/src/builder/panels/control.rs` | 1,405 | Five overlays + control derivation. |
| 8 | `src/analysis/search.rs` | 1,367 | Constraint eval + report; uses `rayon`. Cohesive. |
| 9 | `src/worlds.rs` | 1,361 | Taxonomy enums + tables. Declarative; acceptable. |
| 10 | `viewer/src/factions_overview.rs` | 1,349 | Single view, not split. |
| 11 | **`builder/src/builder/panels/map/mod.rs`** | 1,338 | Was a 3,341 LOC monolith last review. Split landed, but the `mod.rs` dispatcher is still oversized. |
| 12 | `src/validate/diff.rs` | 1,308 | Diff algorithms + markdown. |
| 13 | `builder/src/builder/panels/world.rs` | 1,249 | One panel. |
| 14 | `src/model/sector_model/mod.rs` | 1,242 | DTOs + small impls. Cohesive. |
| 15 | `builder/src/builder/panels/routes.rs` | 1,177 | Route inspector + bulk + hidden-route builder. |
| 16 | `src/export/segmentum.rs` | 1,168 | Segmentum composition. |
| 17 | `gui-core/src/info_panel.rs` | 1,142 | New widget (vs prior review). |
| 18 | **`builder/src/builder/panels/map/context_menu.rs`** | 1,133 | Result of map.rs split. Still oversized. |
| 19 | `builder/src/builder/panels/system_map.rs` | 1,097 | New panel since prior review. |
| 20 | `src/analysis/personae.rs` | 1,078 | Derivation. |
| 21 | `builder/src/builder/panels/relations.rs` | 1,076 | Inspector + matrix. |
| 22 | `builder/src/builder/project_io.rs` | 1,053 | Project IO; clippy `single_char_add_str` at line 278. |
| 23 | `src/analysis/analytics.rs` | 988 | Derivation. |
| 24 | `src/export/subsectors/mod.rs` | 986 | Subsector composition. |
| 25 | `builder/src/builder/panels/map/interactions.rs` | 460 | Result of map.rs split. Within budget. |

Workspace totals: **275 `.rs` files**, **93,958 LOC** (excluding `old/`, `target/`).

Bucketed: **44 files > 700 LOC**, **64 files > 500 LOC**, **117 files > 250 LOC**.

### Finding 4.1 — `panels/map/mod.rs` (1,338) and `panels/map/context_menu.rs` (1,133) still oversized after the split

- **Severity:** Medium
- **Category:** Modularity
- **Evidence:** The prior review's §4.1 recommended splitting the 3,341 LOC `panels/map.rs`. That landed:

```
builder/src/builder/panels/map/
  cache.rs            98 LOC
  context_menu.rs   1,133 LOC
  dialogs.rs          245 LOC
  interactions.rs     460 LOC
  mod.rs            1,338 LOC
```

Five clippy errors still land in `mod.rs` / `context_menu.rs` (`builder/src/builder/panels/map/context_menu.rs:254,553` — `clone_on_copy`).

**What I found.** The split moved out dialogs (correctly small at 245 LOC) and interactions (460 LOC, acceptable), but `mod.rs` retained both the canvas rendering loop and tool dispatch, and `context_menu.rs` retained all §CTX1 Phase 1-7 logic in a single file.

**Why it matters.** The previously-flagged growth pressure didn't go away — every new right-click menu and tool still lands in two large files instead of one huge file.

**Recommended fix.** Continue the split:
- `mod.rs` → split into `mod.rs` (dispatcher ≤ 250 LOC) + `canvas.rs` (hex render + interaction loop) + `toolbox.rs` (`MapTool` plumbing).
- `context_menu.rs` → split by menu target: `context_menu/{system,system_pinned,multi_select,region,sector_hex,route,world}.rs` mirroring the seven `§CTX1` phases. Re-export from `context_menu/mod.rs`.

**Risk of change.** Low — `pub(crate)` re-exports keep callsites stable.

---

### Finding 4.2 — `command.rs` is 1,486 lines (unchanged from prior review)

- **Severity:** Medium
- **Category:** Modularity
- **Evidence:** `wc -l builder/src/builder/command.rs` → 1,486. Same as the prior review's count. The `clippy` `clone_on_copy` lints at lines 534, 535, 836 and `field_reassign_with_default` at 1052/1053/1077/1078 all land in this file.

**Recommended fix.** Same as the prior review's §4.2 — keep the `BuilderCommand` enum in one file (serde stability) but split the `impl` arms by resource into a `builder/src/builder/command/` directory.

**Risk of change.** Low — internal refactor; on-disk session format unchanged.

---

### Finding 4.3 — Other oversized panels (largely unchanged)

- **Severity:** Medium
- **Category:** Modularity
- **Evidence:**

| Panel | LOC | Δ vs prior review |
|---|---:|---|
| `panels/history.rs` | 1,557 | unchanged |
| `panels/system.rs` | 1,508 | **+152** |
| `panels/control.rs` | 1,405 | unchanged |
| `panels/world.rs` | 1,249 | unchanged |
| `panels/routes.rs` | 1,177 | unchanged |
| `panels/system_map.rs` | 1,097 | **new** |
| `panels/relations.rs` | 1,076 | new |

**Recommended fix.** Priority order (highest churn first):
1. `panels/system.rs` — `clippy::too_many_arguments` at line 84 suggests an unhealthy entry-point signature already; group the §CTX1 Phase 6 dispatch and bulk operations into siblings.
2. `panels/history.rs` — wizard + filter + table are independent; clippy hits at 1182/1491 are inside the wizard sub-area.
3. `panels/world.rs`, `panels/control.rs`, `panels/routes.rs` — follow the `builder/src/builder/state/` split model.

---

### Finding 4.4 — `BuilderState` God Object (unchanged from prior review)

- **Severity:** Medium → High (still growing)
- **Category:** Architecture
- **Evidence:** `builder/src/builder/state/mod.rs:76-731`. Counted `^    pub ` lines → **137 `pub` fields**. File is 731 LOC. The struct mixes: project IO + dirty tracking + command log + cursor + snapshots + capacity + pinned sets + derivation cache + file watcher + validation debouncer + 5 selection mailboxes + active tab + map tool + preview + partial regen rect + drag state + 5+ `pending_*` dialog states + rect select + zoom + view cache + per-tab UI scratch (region paint, route bulk filters, hidden-route builder, history wizard, tick log, scroll target, two context menus, last menu action…).

**Recommended fix.** Group fields into sub-structs by concern. Start with the smallest, safest win:

```rust
// All the pending_* fields collapse into a single concern.
pub struct DialogState {
    pub pending_place: Option<PendingPlace>,
    pub pending_rename: Option<PendingRename>,
    pub pending_world_rename: Option<PendingWorldRename>,
    pub pending_collision: Option<PendingCollision>,
    pub pending_bulk_rename: Option<PendingBulkRename>,
    pub pending_region_rename: Option<PendingRegionRename>,
    pub sector_context_menu: Option<SectorContextMenu>,
    pub system_context_menu: Option<SystemContextMenu>,
}
```

Then `MapUiState`, `RoutesUiState`, `HistoryUiState`, etc. Each lives in `state/<concern>.rs`. Panels then take `&mut state.dialogs` / `&mut state.map` rather than `&mut state` whole.

**Risk of change.** Medium — touches every panel; do one sub-struct per PR.

---

### Finding 4.5 — Inconsistent module style (regression)

- **Severity:** Nit (also fires clippy in Finding 2.2)
- **Category:** Modularity
- **Evidence:** `src/export/svg_export/tests.rs:1-2` still wraps its tests in `#[cfg(test)] mod tests { ... }`. The sibling `src/export/bitmap/tests.rs` does *not* (uses bare free `#[test]` functions). Identical to the prior review's §4.5.

**Recommended fix.** Drop the outer `mod tests { ... }` wrapper in `src/export/svg_export/tests.rs`. The file is already included as `mod tests;` from `src/export/svg_export/mod.rs`. Resolves the `module_inception` clippy error.

---

## 5. Library Public Surface

### Finding 5.1 — Public surface is wide but now structured (partial improvement)

- **Severity:** Medium
- **Category:** API hygiene
- **Evidence:** `src/lib.rs:48-173` declares 7 parent `pub mod`s and ~50 `pub use` re-exports. Only `FxMap`/`FxSet` are `pub(crate)` (`src/lib.rs:54-55`). `rg -c 'pub fn' src/` counts 77 files with `pub fn`. The original concern — "almost every `src/*.rs` is `pub mod`" — is structurally addressed by the parent-module split, but the *re-export surface* is still wide enough that downstream crates can reach any internal type via `sectorforge::<module>::<type>`.

**What I found.** The parent-module split is mostly cosmetic from a public-API perspective — the `pub use parent::foo;` block at `src/lib.rs:73-133` re-exports every parent's children. Demoting a child to `pub(crate)` requires both the parent and the re-export to agree.

**Recommended fix.** Pass 1: cross-reference call sites: `rg 'sectorforge::<modname>::' builder/ viewer/ gui-core/`. For modules whose only external consumers are `sectorforge::` re-exports inside `lib.rs`, drop both `pub use` and demote the child `pub mod` to `pub(crate) mod`.

For each module still externally consumed, audit the `pub fn` list and drop unused symbols to `pub(crate)`.

**Risk of change.** Low — internal-only crates.

---

### Finding 5.2 — Two `MutationError` types still coexist (unchanged from prior review)

- **Severity:** Medium
- **Category:** Rust Idioms / API
- **Evidence:**
  - `src/model/errors.rs:43-52` — `pub enum MutationError { NotFound, Collision, InvalidCoord, InvalidState }` (`Serialize + Deserialize`).
  - `src/model/sector_model/mutation.rs:22-43` — `pub enum MutationError { SystemNotFound, WorldNotFound, RouteNotFound, FactionNotFound, RegionNotFound, CoordOutOfBounds, CoordOccupied, DuplicateRoute, SelfRoute, EventNotFound }`.

External consumers (verified via `rg "errors::MutationError|model::errors::MutationError|use crate::errors::MutationError" -r --include='*.rs' . | grep -v target | grep -v old/`): **zero matches**. The only `MutationError` import outside `src/model/errors.rs` is the `sector_model::mutation::MutationError` one used by `builder/src/builder/command.rs:16` and `builder/src/builder/errors.rs:3`.

**What I found.** The `src/model/errors.rs::MutationError` enum appears to be **dead code**, kept `pub` and `Serialize + Deserialize` for no observed reason. It is structurally distinct from the live enum (different variant set).

**Recommended fix.**
1. Confirm with `rg '\bMutationError\b' src/model/errors.rs` and follow the references: it's only defined and not used outside `errors.rs`.
2. Remove it (or, if a future on-disk format wants it, rename to `MutationErrorKind` and document the role).

**Validation.** `cargo check --workspace --all-targets` + `cargo test --workspace`.

**Risk of change.** Low.

---

### Finding 5.3 — `Result<Vec<u8>, String>` in production code (unchanged)

- **Severity:** Low
- **Category:** Rust Idioms
- **Evidence:** `builder/src/builder/session.rs:307` — `pub fn decode_base64(input: &str) -> Result<Vec<u8>, String>`.

**Recommended fix.** Use the `base64` crate. Replaces ~50 LOC and switches `Result<_, String>` to a structured error. See Finding 14.1.

---

## 6. Error Handling

### Finding 6.1 — Two `MutationError` enums (covered by 5.2)

### Finding 6.2 — Auto-save errors swallowed (unchanged from prior review)

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

The serialisation failure and the write failure are both silent. `self.dirty = false` is *only* set on success; the failure path doesn't update any visible state.

Six call sites observed: `panels/history.rs:1263`, `state/regions_ops.rs:24,38,88`, `state/derivations.rs:159,178,207`.

**Why it matters.** A user who saw "dirty marker cleared" after autosave can wrongly conclude the project is saved. With six call sites the failure modes are user-invisible across most of the editor surface.

**Recommended fix.** Add `pub last_auto_save_error: Option<String>` (or a `thiserror` enum) to `BuilderState`. Surface it in `panels/status.rs` adjacent to the dirty pip; clear on the next successful save.

**Risk of change.** Low.

---

### Finding 6.3 — `eprintln!` in library code (unchanged from prior review)

- **Severity:** Low
- **Category:** Observability
- **Evidence:**
  - `src/export/html_export.rs:63` — "sectorforge: interactive HTML is N bytes (warn threshold M)" warning to stderr from inside the library.
  - `src/main.rs:14`, `src/cli/generate.rs:45,68,73,114,137,142`, `src/cli/search.rs:29`, `src/cli/common.rs:158`, `src/bin/dhat_profile.rs:43` — all CLI/binary contexts; acceptable.

**Recommended fix.** Move the html-export warning into a caller-supplied warning channel — e.g. add `warnings: Vec<String>` to the export return type, or accept a `&mut impl FnMut(&str)` warning sink. Programmatic GUI consumers cannot intercept `eprintln!`.

**Risk of change.** Low.

---

## 7. Async / Concurrency

- **Severity:** N/A — no async runtime in use. `rg 'tokio|async fn|\.await' src/ --glob '*.rs'` → 0 hits.
- Concurrency surface in the GUI is `Arc<Mutex<f32>>` for progress in `gui-core/src/jobs.rs` — atomic float updates only.
- Parallelism is via `rayon` in `src/analysis/search.rs` (order-preserving `into_par_iter().collect()` per the determinism contract — `Cargo.toml:35-37`). ✓

No findings.

---

## 8. Security Review

Offline content-generation tool. No network surface, no auth, no untrusted user input beyond local TOML/JSON chosen by the operator.

### Finding 8.1 — User-controlled relative paths joined to project root (unchanged)

- **Severity:** Low (informational)
- **Category:** Security
- **Evidence:**
  - `src/loading/input.rs:55` — `let config_path = root_dir.join("sectorforge.toml");`
  - `src/loading/input.rs:75-77` — `let data_dir_rel = config.inputs.world_data_dir.clone(); let data_dir = root_dir.join(&data_dir_rel);`
  - `src/loading/input.rs:233` — `let abs = root.join(rel);`

**Why it matters.** The operator chooses both the project root *and* the TOML file. Not a privilege boundary in normal usage. If `sectorforge` is ever embedded in a multi-tenant context (a shared CI that runs PR-submitted projects), `world_data_dir = "../../etc/passwd"` would resolve outside the project root.

**Recommended fix.** Document in `src/loading/input.rs` that all relative paths in config are trusted to the same level as the binary. If multi-tenant use ever arrives: canonicalise + `assert!(data_dir.starts_with(&root_dir))`.

---

### Finding 8.2 — `examples/*/data/` ship `.toml` files — no secrets observed (unchanged)

- **Severity:** Low (informational)
- **Evidence:** `examples/big_test`, `examples/big_sparse_test`, `examples/huge_sparse_test`, `examples/llm_test`, `examples/m42_project`, `examples/segmentum_example.toml`. `.gitignore` excludes `examples/*/out/` but not `examples/*/data/`. Spot-checked: deterministic TOML inputs only.

No fix needed; flagged for periodic re-check.

---

## 9. Testing Review

### Inventory

```
running 165 tests   → src/ lib unit tests
running  84 tests   → tests/it/* integration tests (5 ignored)
running 249 tests   → builder/src/ unit tests
running  21 tests   → sectorforge-gui-core unit tests
running   1 test    → gui-core/tests/map_snapshots.rs (golden)   FAILED
running   3 tests   → sectorforge-viewer unit tests
running   6 tests   → doctests
```

**Aggregate: 528 pass, 5 ignored, 1 failed.**

### Strengths (unchanged from prior review)

- Golden tests for the bitmap export: `tests/it/golden_png.rs`.
- Property tests for invariants: `tests/it/invariants_proptest.rs`.
- Round-trip JSON tests embedded in domain modules.
- Per-stage RNG determinism tests: `src/model/rng.rs`.
- CLI-vs-GUI parity test: `tests/it/cli_gui_parity.rs`.

### Finding 9.1 — `map_snapshots_match_goldens` is red on `main` (new)

- **Severity:** High
- **Category:** Testing
- **Evidence:** `gui-core/tests/map_snapshots.rs:354`. See Finding 2.3 for the exact failure. The recent commit ("system view", `4dbd5b7`) is the most likely cause — `gui-core/src/system_view.rs` is also the file where `cargo fmt` regressed.

**Recommended fix.** Inspect the divergent artefact (`target/map_snapshots/current/system_glyphs.png`), then either bless (`UPDATE_MAP_SNAPSHOTS=1 cargo test -p sectorforge-gui-core map_snapshots_match_goldens --quiet`) or roll back. Don't leave `main` red.

**Risk of change.** Medium — goldens are the visual contract; only bless if the new render is intended.

---

### Finding 9.2 — Slow test suite imbalance (unchanged)

- **Severity:** Low
- **Category:** Testing
- **Evidence:** One runner dominates wall time (≈ 36s vs ≤ 2.3s elsewhere) per the prior review. Likely `tests/it/golden_*` or `tests/it/segmentum_tests.rs`.

**Recommended fix.** Add `cargo test -- --report-time` to identify culprits; gate the heaviest behind `#[cfg_attr(not(feature = "slow-tests"), ignore)]`.

---

### Finding 9.3 — `expect("CARGO_MANIFEST_DIR")` pattern repeated across integration tests (unchanged)

- **Severity:** Nit
- **Category:** Testing
- **Evidence:** 7 copies across `tests/it/*.rs` per the prior review.

**Recommended fix.** Single `tests/it/common/mod.rs` helper returning `Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))`.

---

## 10. Performance Review

No live measurements taken; static reading only.

### Observations (unchanged)

- `src/model/rng.rs::weighted_index` O(n) per draw — acceptable for sub-million pools.
- Output writers consistently sort by ID first via `BTreeMap`/`BTreeSet` (determinism contract, `src/lib.rs:54-55`).
- `rayon` parallelism in `src/analysis/search.rs` is order-preserving (`into_par_iter().collect()` → `Vec`).
- Heap profiling wired behind `dhat-heap` feature (`Cargo.toml:38-46`, `src/bin/dhat_profile.rs`).
- Release profile: `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"` (`Cargo.toml:67-71`).
- Profiling profile defined separately (`Cargo.toml:80-85`).

### Finding 10.1 — `GeneratedSector::all_worlds` / `get_world` linear (unchanged)

- **Severity:** Nit
- **Category:** Performance
- **Evidence:** `src/model/sector_model/mod.rs` — `get_world` is O(N×M).

**Recommended fix.** Don't preemptively add a lookup table; typical N is 24 systems. Add a lazy `OnceCell<HashMap<…>>` only if profiling ever points here.

---

## 11. Observability Review

- No `tracing`, no structured logging (`rg 'tracing::' src/ --glob '*.rs'` → 0 hits; no `log = ` dependency).
- Library `eprintln!` at `src/export/html_export.rs:63` — see Finding 6.3.
- Generation progress is structured (`SectorProgress` enum + caller-supplied closure) — clean.
- Manifest hashing (`src/model/sector_model::GenerationManifest`: `seed_hash`, `input_digests`, `settings_digest`) gives reproducibility without runtime logging.

### Finding 11.1 — No `tracing` / structured logging (unchanged)

- **Severity:** Low
- **Category:** Observability

**Recommended fix.** Defer. If/when added, prefer `tracing` + `tracing-subscriber` for the builder; the library should accept a `tracing` span from the caller, never initialise a subscriber.

---

## 12. Configuration / Secrets

- All config is TOML, parsed via the `toml` crate.
- No environment variables read in library code (only `CARGO_MANIFEST_DIR` in tests + `HOME` in `builder/src/builder/preferences.rs`).
- No secrets stored anywhere.

No findings.

---

## 13. Documentation & DX

### Finding 13.1 — `README.md` is one line (unchanged from prior review)

- **Severity:** Medium
- **Category:** Documentation
- **Evidence:** `cat README.md` → `# 40k-sector-generator`. No build, no examples, no link to `GUIDE.md`/`OVERVIEW.md`/`BUILDER.md`/`docs/MAP.md` (all present).

**Recommended fix.** ≤ 50-line `README.md` template (copy from the prior review §13.1).

**Risk of change.** Zero.

---

### Finding 13.2 — `CLAUDE.md` carries "current state" content (unchanged)

- **Severity:** Low
- **Category:** Documentation
- **Evidence:** `CLAUDE.md` still describes the parent-module split with prose hooks. `docs/MAP.md` exists and carries the file-by-file map, which is the right place for it.

**Recommended fix.** Continue moving steady-state architectural facts to `docs/MAP.md`; keep `CLAUDE.md` to invariants, agent routing rules, and pointers to deeper docs.

---

### Finding 13.3 — `INPUT.md` doubles as instructions and token-saving rules (unchanged)

- **Severity:** Nit
- **Category:** Documentation
- **Evidence:** `INPUT.md` is agent-context optimisation rules; referenced from `CLAUDE.md`.

**Recommended fix.** Cross-reference from `README.md` once written so future contributors know `INPUT.md` is *not* a feature spec.

---

## 14. Dependencies

(Audited from `Cargo.toml`, `builder/Cargo.toml`, `viewer/Cargo.toml`, `gui-core/Cargo.toml`. No `Cargo.lock` inspection performed — out of scope for static review.)

| Crate | Version | Used for | Notes |
|---|---|---|---|
| `clap` | 4 (derive) | CLI parsing | Standard. |
| `serde` | 1 (derive, rc) | DTOs, `Arc<str>` serde | `rc` feature enables `Arc`/`Rc` serde — correct for the `Arc<str>` usage. |
| `serde_json` | 1 | JSON IO | Standard. |
| `toml` | 0.8 | Config IO | Standard. |
| `thiserror` | 1 | Domain errors | Standard. |
| `rand` | 0.8 | RNG primitives | Pinned. |
| `rand_chacha` | 0.3 | Deterministic ChaCha8 | Pinned — part of the determinism contract. |
| `blake3` | 1 | Stage-key derivation | `src/model/rng.rs`. |
| `camino` | 1 (serde1) | UTF-8 paths | Consistent. |
| `image` | 0.25 (png only) | Bitmap export | `default-features = false, features = ["png"]` — clean. |
| `rustc-hash` | 2 | `FxHashMap`/`FxHashSet` internal lookups | Correctly *not* used in output paths. |
| `rayon` | 1 | Search parallelism | Order-preserving usage. |
| `dhat` | 0.3 (optional) | Heap profile | Behind `dhat-heap` feature. |
| `eframe` / `egui` | 0.29 | GUI (workspace crates only) | Absent from `sectorforge` — verified `rg eframe src/` → 0. |
| `rfd` | 0.17 | File dialogs (builder/viewer) | Confined to GUI crates. |
| `tempfile` | 3 (dev) | Test temp dirs | Standard. |
| `proptest` | 1 (dev) | Property tests | Standard. |
| `criterion` | 0.5 (dev) | Bench harness | Standard. |

### Finding 14.1 — Hand-rolled base64 instead of `base64` crate (unchanged)

- **Severity:** Low
- **Category:** Dependencies
- **Evidence:** `builder/src/builder/session.rs:307` — ~50 LOC manual decoder, `Result<_, String>`.

**Recommended fix.** `base64 = "0.22"` in `builder/Cargo.toml`; replace `decode_base64` with `base64::engine::general_purpose::STANDARD.decode(...)`.

---

### Finding 14.2 — No `cargo-deny` / `cargo-audit` configuration (unchanged)

- **Severity:** Low
- **Category:** Dependencies

**Recommended fix.** Add once CI exists (Finding 2.4).

---

## 15. API / Shared-Type Contracts

Single-process app — no HTTP boundary. Relevant boundary is **library DTOs ↔ on-disk JSON ↔ GUI consumers**.

### Assessment

- All DTOs in `src/model/sector_model/mod.rs` derive `Serialize` + `Deserialize`.
- `Arc<str>` used for borrowed-cost strings (cheap clone, serde-compatible via `rc` feature).
- IDs are newtypes (`SystemId`, `WorldId`, `RouteId`, `FactionId` in `src/model/ids.rs`).
- Round-trip tests exist.
- `skip_serializing_if` applied to default/empty fields → compact stable JSON.

### Finding 15.1 — `chronicle` field is *not* under `Arc` while every other overlay is (unchanged)

- **Severity:** Low
- **Category:** API Contract
- **Evidence:** `src/model/sector_model/mod.rs:31-52`:

```rust
pub influence_field:  std::sync::Arc<crate::influence_field::InfluenceField>,
pub power_projection: std::sync::Arc<crate::power_projection::PowerProjectionMap>,
pub relations:        std::sync::Arc<crate::relations::RelationsMatrix>,
pub regions:          std::sync::Arc<Vec<crate::regions::WarpRegion>>,
pub economy:          std::sync::Arc<crate::economy::EconomyReport>,
pub chronicle:        crate::history::SectorChronicle,        // <- not Arc
```

**Why it matters.** Inconsistent clone cost. The builder clones `GeneratedSector` for background jobs (`state/mod.rs` snapshots); `chronicle` clones in full each time while every sibling is cheap-clone via `Arc`.

**Recommended fix.** Wrap `chronicle` in `Arc`. On-disk JSON shape unchanged. Adjust the few mutation sites with `Arc::make_mut` (search `rg 'sector\.chronicle' src/` and `rg 'chronicle =' src/`).

**Risk of change.** Medium — touches every place that mutates `chronicle` in-place. Roughly the same number of touchpoints as `relations`, which was successfully `Arc`-wrapped.

---

## 16. Prioritized Findings Table

| # | Severity | Category | Location | Finding | Action |
|---:|---|---|---|---|---|
| 1 | High | Build/CI | `gui-core/src/system_view.rs` | `cargo fmt` red (regression) | `cargo fmt --all` (§2.1) |
| 2 | High | Build/CI | 5 files | `cargo clippy -D warnings` red — 17 errors in 7 categories | Fix lints one category per PR (§2.2) |
| 3 | High | Testing | `gui-core/tests/map_snapshots.rs:354` | `map_snapshots_match_goldens` fails on `system_glyphs.png` | Bless or revert (§2.3, §9.1) |
| 4 | High | Build/CI | repo root | No CI workflow | Add `.github/workflows/ci.yml` (§2.4) |
| 5 | Medium → High | Architecture | `builder/src/builder/state/mod.rs:76-731` | `BuilderState` God Object — 137 `pub` fields | Group fields into sub-structs (§4.4) |
| 6 | Medium | Modularity | `builder/src/builder/panels/map/{mod,context_menu}.rs` | Two files still > 1,000 LOC after the prior split | Continue the split (§4.1) |
| 7 | Medium | Modularity | `builder/src/builder/command.rs` | 1,486 LOC; clippy hits at 8 lines | Split impl arms by resource (§4.2) |
| 8 | Medium | Modularity | `builder/src/builder/panels/{history,system,control,world,routes,system_map,relations}.rs`, `viewer/src/factions_overview.rs`, `gui-core/src/{sector_view,info_panel}.rs` | 9 files > 1,000 LOC | Per-panel split (§4.3) |
| 9 | Medium | API hygiene | `src/lib.rs:48-173` | Wide `pub mod` + `pub use` surface | Demote externally-unused children to `pub(crate)` (§5.1) |
| 10 | Medium | Rust Idioms | `src/model/errors.rs:43` | `MutationError` enum here is **dead** (zero external consumers) | Delete or rename (§5.2) |
| 11 | Medium | Error Handling | `builder/src/builder/state/undo.rs:68-78` | Auto-save errors silent across 6 call sites | Capture + surface in status bar (§6.2) |
| 12 | Medium | Documentation | `README.md` | One-line README | Write ≤ 50-line entry-point doc (§13.1) |
| 13 | Low | Rust Idioms | `builder/src/builder/session.rs:307` | Hand-rolled base64; `Result<_, String>` | Use `base64` crate (§14.1) |
| 14 | Low | API Contract | `src/model/sector_model/mod.rs:52` | `chronicle` not `Arc`-wrapped like siblings | Wrap + audit mutate sites (§15.1) |
| 15 | Low | Security | `src/loading/input.rs:55,75-77,233` | Operator-controlled relative paths joined to project root | Document trust assumption (§8.1) |
| 16 | Low | Observability | `src/export/html_export.rs:63` | `eprintln!` in library code | Move to caller-supplied warning channel (§6.3) |
| 17 | Low | Documentation | `CLAUDE.md` | Some "current state" content lingering | Move steady-state facts to `docs/MAP.md` (§13.2) |
| 18 | Low | Testing | tests | One integration suite dominates wall time | Profile + isolate slow tests (§9.2) |
| 19 | Low | Build/CI | repo root | No `deny.toml` / `cargo audit` | Add after CI lands (§2.5, §14.2) |
| 20 | Nit | Modularity | `src/export/svg_export/tests.rs` | `mod tests { ... }` wrapper triggers `module_inception` | Drop wrapper (§4.5) — resolves one clippy error |
| 21 | Nit | Testing | `tests/it/*.rs` (7 files) | `env::var("CARGO_MANIFEST_DIR").expect(..)` repeated | Single helper (§9.3) |
| 22 | Nit | Documentation | `INPUT.md` | Doubles as agent rules + project doc | Cross-reference in README (§13.3) |

---

## 17. Refactoring Roadmap

### Stage 0 — Restore green (≤ half a day)

1. **Bless or revert the golden** (§9.1). Don't leave `main` red.
2. `cargo fmt --all` (§2.1).
3. Fix the 17 clippy lints (§2.2). One PR per category recommended; the whole set is mechanical.
4. Add `.github/workflows/ci.yml` (§2.4) — locks in the gates so this doesn't recur.

**Exit criteria.** `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` all exit 0 on `main`.

### Stage 1 — Quick hygiene (≤ 1 day total, parallelisable)

1. Write the README (§13.1).
2. Delete dead `MutationError` in `src/model/errors.rs:43` (§5.2).
3. Add `last_auto_save_error` to `BuilderState`; surface in status bar (§6.2).
4. Drop the `mod tests` wrapper in `src/export/svg_export/tests.rs` (§4.5) — also resolves a clippy lint, so do it as part of Stage 0 if convenient.
5. Replace hand-rolled base64 (§14.1).
6. Wrap `chronicle` in `Arc` (§15.1).
7. Add a `tests/it/common/mod.rs` helper for `CARGO_MANIFEST_DIR` (§9.3).

**Exit criteria.** No external behavior change. `cargo doc --workspace --no-deps` shows a narrower public surface (after Findings 5.1, 5.2).

### Stage 2 — Builder file splits (≤ 2 weeks, parallelisable)

1. Continue splitting `builder/src/builder/panels/map/mod.rs` (§4.1) → `canvas.rs` + slimmer `mod.rs`.
2. Split `panels/map/context_menu.rs` by phase (§4.1).
3. Split `builder/src/builder/command.rs` impl arms by resource (§4.2).
4. Group `BuilderState` fields into sub-structs (§4.4) — start with `DialogState` as the smallest, safest win, one sub-struct per PR.
5. Split panels in priority order: `panels/system.rs` → `panels/history.rs` → `panels/control.rs` → `panels/world.rs` → `panels/routes.rs` → `panels/system_map.rs` → `panels/relations.rs` (§4.3).

**Exit criteria.** No file over ~700 LOC in `builder/src/builder/panels/`. `cargo test -p sectorforge-builder` still green.

### Stage 3 — Observability + DX (≤ 1 week)

1. Replace `eprintln!` in library with caller-supplied warning channel (§6.3).
2. Migrate "current state" lingering in `CLAUDE.md` into `docs/MAP.md` (§13.2).
3. Optional: add `tracing` to the builder (§11.1).
4. Optional: profile + isolate slow tests (§9.2).
5. Optional: `deny.toml` + `cargo audit` jobs (§2.5, §14.2).

---

## 18. Quick Wins (≤ 1 day each)

1. **`cargo fmt --all`.** §2.1 — zero-risk format.
2. **Bless or revert the failing golden.** §2.3 — required to restore green.
3. **Drop `mod tests { ... }` wrapper in `src/export/svg_export/tests.rs`.** §4.5 — resolves one clippy error and a style nit at the same time.
4. **Fix `clippy::clone_on_copy` (5 sites).** §2.2 — `rg "\.clone\(\)" builder/src/builder/{command,panels/map/context_menu}.rs` + remove on `StabilityState`.
5. **Fix `clippy::field_reassign_with_default` (3 sites).** §2.2 — clippy gives the exact suggestion at each line.
6. **Write `README.md`.** §13.1 — template in the prior review.
7. **Add `.github/workflows/ci.yml`.** §2.4 — template above.
8. **Delete dead `MutationError` in `src/model/errors.rs`.** §5.2 — zero external consumers.
9. **Add `last_auto_save_error` + tail it in `panels/status.rs`.** §6.2.
10. **Wrap `chronicle` in `Arc`.** §15.1 — small consistency fix.

---

## 19. Validation Checklist

```bash
# After each change set:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # confirms public surface shrinks when expected
```

For Stage 2 split PRs additionally:
- Smoke-test every context-menu schema in the builder (§CTX1 Phase 1-7).
- Confirm session round-trip via "open project → make a change → save → reopen".
- Re-generate `examples/m42_project` and `diff` against pre-refactor JSON.

For golden updates:
- `UPDATE_MAP_SNAPSHOTS=1 cargo test -p sectorforge-gui-core map_snapshots_match_goldens --quiet`
- Then commit both the updated golden PNG and the test file.

---

## 20. Open Questions

These do *not* block the review.

1. **Is the test failure in `system_glyphs.png` intentional?** The "system view" commit landed without blessing the golden — was this a missed step, or was the visual change unintended?
2. **Is the workspace ever expected to grow a network surface?** §8 findings sharpen if so.
3. **Are external consumers of the `sectorforge` library expected (outside this workspace)?** If yes, the `pub mod` / `pub use` audit (§5.1) becomes mandatory.
4. **Is there a target deletion plan for `old/`?** It is excluded by `CLAUDE.md` and `.gitignore` but still sits in the working tree.
5. **Is `docs/CONTEXT_MENU.txt` the canonical spec for the `§CTX1` series?** Confirms the §4.1 split boundaries.

---

## 21. Self-Check (reviewer)

- [x] Every finding cites a file path; most cite line numbers.
- [x] Facts, inferences, and recommendations are separated within each finding.
- [x] Severity assigned per the `REVIEW.md` rubric.
- [x] No invented files, symbols, or behaviours.
- [x] Both library (`src/`) and GUI crates (`builder/`, `viewer/`, `gui-core/`) reviewed.
- [x] Shared/contract boundary (DTOs + JSON) reviewed in §15.
- [x] Compared to the previous review on commit `d8b7554` — resolved/regressed/still-open status given for every prior finding.
- [x] Quick wins separated from stage roadmap.
- [x] Validation commands provided.
- [x] Real exit codes verified (the prior baseline table mis-captured a clippy failure as `EXIT=0`; this review captured `EXIT=101` correctly).
