# FINDINGS.MD — Aggregated Rust Code Review

> Multi-agent review of the sectorforge workspace per the protocol in
> `RUST_REVIEW.md`. 93,958 LOC reviewed in 22 primary units + 8 cross-cutting
> sweeps. ~530 findings total across all reports under `reviews/`.
>
> Companion file: `RUST_FIXES.md` (prioritised, actionable backlog of every fix).

---

## 1. Executive summary

### Overall health: **3.1 / 5** (workable, with focused debt)

The workspace is in genuinely good shape on the things that are hardest to retrofit:

- **Zero `unsafe` code.** Whole workspace is safe Rust. Verified by grep (X01) and confirmed by per-unit reviewers. Apply `#![forbid(unsafe_code)]` at each crate root to lock the property in.
- **Determinism architecture is correct.** `src/model/rng.rs` is the single, well-designed stage-keyed RNG factory. `BTreeMap`/`BTreeSet` are used consistently across analysis and export. Golden tests gate `src/export/{bitmap, svg_export, html_export, render}`. The Fx aliases in `src/lib.rs` are correctly `pub(crate)`.
- **No async runtime.** The only concurrency is one well-bounded `rayon` parallel iterator in `src/analysis/search.rs` (order-preserving) and one well-contained file-watcher thread in `builder/`. No locks-across-await, no atomics with wrong ordering, no shared mutable state.
- **Error model uses `thiserror` uniformly** in `src/` and is largely free of `anyhow`/`eyre`/`Box<dyn Error>`. Library context attachment via `with_context`-style is exemplary on the IO seam.
- **The bitmap render path** (`src/export/bitmap/primitives.rs`) and `src/validate/diff.rs` are reference-quality and should not be touched without strong reason.

What lets the score down is concentrated in five themes:

1. **Builder command-bus discipline is broken across most editing panels.** This is the single biggest correctness debt — undo/redo silently drops user work.
2. **Pervasive per-frame allocation in egui panels.** ~340 `format!`s and 439 `.to_string()`s inside `show()` callbacks.
3. **Public API has zero `#[non_exhaustive]`** across ~1,128 `pub` items — every enum addition is a SemVer break.
4. **Two concrete determinism violations** that are currently masked but a refactor away from breaking golden tests.
5. **Viewer crate violates `CLAUDE.md` in two ways**: editor mutates without command bus + uses `rand::random()` outside `model/rng.rs`.

### Top 10 priority findings (the ones to fix first)

| Rank | ID | Severity | Crate | One-line |
|---|---|---|---|---|
| 1 | F-017-001..005 + F-016-001..006 + F-015-001..002 | HIGH | builder | **Command-bus rule (§R4) violated wholesale** — every `BuilderCommand`-less mutation breaks undo/redo. Multiple panels write directly to `state.sector.*`. |
| 2 | F-021-002 | HIGH | viewer | `rand::random::<f64>()` at `viewer/src/editor/generation_panel.rs:30,259` bypasses `src/model/rng.rs`. **Direct CLAUDE.md violation.** |
| 3 | F-017-006 | HIGH | builder | `history.rs:1409` uses `DefaultHasher` (SipHash, per-process random seed) to mint manual chronicle event IDs that land in `sector.json`. **Determinism violation.** |
| 4 | F-011-001 | HIGH | src | `importance::compute_display_buckets` (`src/analysis/importance.rs:139-179`) iterates `std::collections::HashMap` for output. Feeds the golden-tested legend writers. **Determinism violation.** |
| 5 | F-021-001 | HIGH | viewer | `viewer/src/editor/` is a fully-featured mutator — yet `CLAUDE.md` documents viewer as read-only. Either remove or rewire through the command bus. |
| 6 | F-X05-001 | HIGH | builder + viewer + gui-core | Per-frame `format!`/`to_string` density in egui panels (~340 instances). Cache uppercased labels and IDs on `SectorMapCache`. |
| 7 | F-X05-002 | HIGH | src | `run_search` processes the full 256-candidate budget after a winner is found. Replace `(0..budget).into_par_iter().map().collect()` with `find_map_first`. |
| 8 | F-X02-001 | HIGH | all | Zero `#[non_exhaustive]` on 1,128 public items. Every public enum (`SectorError`, `BuilderError`, `BuilderCommand`, `HeatmapMode`, `Constraint`, ...) is one variant away from SemVer-major. |
| 9 | F-009-001 | HIGH | src | `src/analysis/search.rs` deep-clones the entire `ProjectInput` per rayon worker per candidate (256 × ~MB). Wrap immutable arms in `Arc`. |
| 10 | F-X03-002, F-X03-003 | HIGH | builder + viewer | `reload_catalog` (15 sites) and `build_subsectors` errors (8 sites) silently swallowed. Users see stale or empty UI with no diagnostic. |

### Dominant themes (cluster summary)

| Theme | Severity | Occurrences | Hotspots |
|---|---|---|---|
| **Builder command-bus violations** | HIGH | ~50 panel sites (panels-econ has 5/5 panels affected; panels-map has ~69 direct field writes) | `builder/src/builder/panels/{history,control,relations,factions,economy,world,system}.rs` |
| **Per-frame egui allocation** | HIGH/MEDIUM | ~340 `format!`, 439 `.to_string()` inside `show()` callbacks | `gui-core/src/info_panel.rs` (worst: 64 format! + 88 `to_uppercase` per frame), every panel in `builder/` and `viewer/` |
| **Missing `#[non_exhaustive]`** | HIGH | 1,128 pub items, 0 attributes | All four crates |
| **Pervasive `.clone()`** | MEDIUM | 1,667 sites (builder 877, src 583, viewer 183, gui-core 24) | Especially briefing/relations/economy clone whole sector; per-frame `Arc<str>` clones in panels |
| **Float ordering safety** | LOW/HIGH | 35 `partial_cmp`; 33 already defended, 2 still `.unwrap()` | `builder/src/builder/panels/control.rs:1231,1240` |
| **Test gaps at the loader boundary** | HIGH | 0 property tests on `loading/`, 0 fuzz | `src/loading/config.rs`, `loading/presets.rs`, `worlds_toml.rs` |
| **CLI integration coverage** | HIGH | 17 of 18 subcommands untested at binary boundary | `tests/it/cli_gui_parity.rs` covers `generate` only |
| **Duplicate `MutationError` enum (dead)** | HIGH | 1 dead enum in `src/model/errors.rs` | every consumer uses the `sector_model::mutation::MutationError` instead |
| **Cargo: no `[workspace.dependencies]` hoist** | MEDIUM | 9 deps duplicated across 3-4 manifests | `eframe`/`egui`/`image`/`camino`/`clap`/`serde`/`serde_json`/`toml`/`thiserror`/`tempfile` |
| **Builder/viewer overshare public surface** | MEDIUM | ~350 internal `pub` items in bin lib targets | Most panel `show(...)` fns |

---

## 2. Severity histogram per crate

(Approximate; exact counts in each `reviews/**/*.review.md` YAML frontmatter.)

| Crate | Critical | High | Medium | Low | Nit | Total |
|---|---:|---:|---:|---:|---:|---:|
| `sectorforge` (src/) | 0 | ~30 | ~80 | ~60 | ~35 | ~205 |
| `sectorforge-builder` | 0 | ~24 | ~50 | ~28 | ~21 | ~123 |
| `sectorforge-viewer` | 0 | ~12 | ~28 | ~22 | ~13 | ~75 |
| `sectorforge-gui-core` | 0 | ~6 | ~20 | ~16 | ~10 | ~52 |
| `tests/` + benches + cross-cuts | 0 | ~16 | ~31 | ~19 | ~9 | ~75 |
| **Total** | **0** | **~88** | **~209** | **~145** | **~88** | **~530** |

Zero CRITICAL is the headline — no memory safety, no UB, no data corruption, no crash on common input. The 88 HIGHs cluster heavily in the five themes above; **once those themes are addressed, the high-severity tail collapses below 20**.

---

## 3. Theme clusters (in detail)

### Theme T-01 — Builder command-bus discipline (HIGH, single biggest correctness debt)

`CLAUDE.md` §R4: *"Mutations in the builder always go through the command bus. Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel — that breaks undo/redo."*

Per-unit findings:
- **U015 (builder/core), F-015-001/002/003/005**: every `BuilderState` field is `pub`; `state/generation_ops.rs` and `state/regions_ops.rs` mutate `self.sector` without minting a `BuilderCommand`; `recompute_economy/relations/chronicle` in `derivations.rs` overwrite `Arc`-fields with no undo entry; `trigger_auto_save` silently swallows IO errors so a session-long auto-save failure is invisible.
- **U016 (panels-map), F-016-001..006**: `system.rs`/`world.rs` inspectors mutate `state.sector.systems[idx].<field>` directly (~69 direct write sites). `AddWorld`/`DuplicateWorld`/`AddWorldAtOrbit` dispatch a command then **patch via `iter_mut()` afterward** (acknowledged in inline comments). Every `ReplaceRoutes` carries `before: Vec::new()` (lines 401/503/902/910/962) — **route undo is broken across the entire ROUTES tab**.
- **U017 (panels-econ), F-017-001..005**: **all five panels** (`history`, `control`, `relations`, `factions`, `economy`) write directly to `state.sector.*` / `state.data_catalogs.*` / side-tables. Chronicle, presence, claim, faction-roster, relations-override, and economy-override edits all have no undo entry.
- **U018 (panels-misc), F-018-010**: `conflict.rs:167-180` issues `SetSystemConflict` **every frame** in aggregate mode, polluting the undo log.

**Fix sketch** (RUST_FIXES.md item T-01-FIX-A): introduce the missing `BuilderCommand` variants, narrow `BuilderState` public surface to `pub(crate)`, and wire each panel to dispatch via `state.run(...)`. This is the largest single piece of structural work in the backlog. **Effort L, Risk Medium** (well-tested apply/revert path on the bus itself means the cost is in plumbing, not logic).

### Theme T-02 — Per-frame allocation in egui hot paths (HIGH/MEDIUM)

Per-unit findings: F-002-001, F-001 hot-path findings, F-016-007/008/009, F-017-007/008/009/010/011, F-018-001/002, F-019-001/002/004/005/007/010/017, F-020 per-frame items, F-X05-001 (cross-cutting roll-up).

Concrete worst cases:
- `gui-core/src/info_panel.rs`: 64 `format!` + 88 `.to_uppercase()` per frame.
- `gui-core/src/sector_view.rs:719..`: per-frame `Vec<Pos2>` per hex in `draw_hex`, per-subsector HashSet rebuilds, per-frame `HashMap<&str, Pos2>` for system centers.
- `builder/src/builder/panels/world.rs`: per-frame call to `feature_weights_for_world()` rebuilds project input + world pool every frame.
- `builder/src/builder/panels/economy.rs`: full `recompute_economy()` fires on every slider tick.
- `builder/src/builder/panels/factions.rs`/`relations.rs`: full `sector.factions`/`sector.relations.pairs` clone per frame to dodge `&mut state` borrow.
- `viewer/src/segmentum_view.rs:234-411`: `super_map()` builds a `BTreeMap<String,Rect>` and `HashMap<(String,SystemId),Pos2>` per frame, with `clone()` on inner-loop strings — dominant per-frame allocator pressure in the viewer.
- `viewer/src/dashboard.rs:43`: deep-clone of the full `SectorAnalysis` (10+ collections) every frame.

**Fix sketch** (RUST_FIXES.md T-02): hoist scratch buffers to `SectorMapCache`/builder state derivations, replace `format!`s with `&'static str` lookup tables (via per-enum `as_slug() -> &'static str`), gate expensive derivations behind dirty-flags. **Effort M-L per panel, Risk Low** (golden-stable transformations).

### Theme T-03 — Public API: zero `#[non_exhaustive]` (HIGH)

Findings: F-X02-001 (cross-cutting), F-001 HIGH (gui-core API), F-002-002, F-003-001 (sectorforge), F-004-005, F-009-003 (analysis), and per-unit equivalents in U008, U011, U013, U014.

Zero `#[non_exhaustive]` attributes across the 1,128 public items. Most painful:
- `SectorError`, `BuilderError`, `MutationError` (×2 dead-copy), `BuilderCommand` (32 variants).
- All 8 `taxonomy` enums in `src/worlds.rs` (Display falls through to `Debug` — exposes rustc identifiers to user-visible text — F-003-002).
- `Constraint` in `src/analysis/search.rs` (extended at least 5 times per inline `§` comments — exactly the case `#[non_exhaustive]` was designed for).
- `HeatmapMode` (16 growing variants).
- `SectorView` (24 pub fields with struct-literal construction across 4+ call sites).

**Fix sketch** (RUST_FIXES.md T-03): one-line attribute on every public enum and growable struct, no behavior change. **Effort S, Risk Low.** Combine with constructor-only construction (move from struct literals to `Builder::new(...).a(x).b(y).build()` for `SectorView`) for the structs.

### Theme T-04 — Determinism violations (HIGH; two concrete + several latent)

- **F-011-001 (CONFIRMED)**: `src/analysis/importance.rs:139-179` `compute_display_buckets` uses `std::collections::HashMap<KindGroup, AggregateAcc>` and iterates it. Consumed by `src/export/bitmap/legend.rs:70` and `src/export/svg_export/legend.rs:61` — both golden-tested. **Fix:** swap to `BTreeMap`, derive `Ord` on `KindGroup`, add tertiary tie-break to final sort.
- **F-017-006 (CONFIRMED)**: `builder/src/builder/panels/history.rs:1409` uses `DefaultHasher` (SipHash with per-process random key) to derive manual chronicle event IDs that land in `sector.json`. **Fix:** route through `blake3` via `src/model/rng.rs`.
- **F-X06-RNG (CONFIRMED)**: `viewer/src/editor/generation_panel.rs:30,259` uses `rand::random::<f64>()` for the "randomize seed" button. Violates "All RNG draws go through `src/model/rng.rs`". (Same as F-021-002.) **Fix:** `blake3` of system time, or remove the `rand = "0.8"` direct dep from viewer altogether.
- **Latent (multiple)**: tie-break direction reversed between `derive_world_control` and `derive_system_control` (F-011-002); tie-break direction inverted in `seeding vs Lloyd refinement` in `subsectors/mod.rs` (F-012-003); HashMap iteration in 4 builder panels where the result is read-only today but a small refactor could leak (F-017-017, F-016-various).

### Theme T-05 — Error swallowing at the load boundary (HIGH)

- **F-X03-002**: `builder/src/builder/project_io.rs:832-924` `reload_catalog` has **15 `if let Ok(cfg) = toml::from_str(...)` blocks** that silently leave the in-memory catalog stale on parse error. The sibling `open_project` path uses typed errors with line/col reporting — coherence break.
- **F-X03-003**: `build_subsectors` errors silently dropped at 8 sites across viewer, builder, and analytics (`.unwrap_or_default()` / `let Ok ... else`). `SubsectorBuildError` carries actionable detail (duplicate IDs, out-of-bounds coords).
- **F-015-005**: `trigger_auto_save` (`undo.rs:68-78`) swallows IO errors — auto-save can be failing for a whole session with no UI indication.
- **F-X03-004**: `src/main.rs:9-18` returns `ExitCode::from(2)` for **every** `SectorError` variant. No distinction between validation failure / IO / config / cancellation / internal. Plus three coexisting exit-code policies inside CLI runners.
- **F-005-001**: `presets::rewrite_seed` interpolates the user-supplied `--seed` into a TOML basic string without escaping — a `"` or backslash in the seed produces malformed/injected TOML.
- **F-005-003**: `read_relative` never rejects `..`/absolute paths and uses the raw user-supplied `rel` as the manifest digest key — breaks both the determinism contract and any sandbox assumption.

### Theme T-06 — Viewer overreach into write-side state (HIGH)

The viewer is documented as read-only, but **F-021-001** finds a fully-featured sector mutator under `viewer/src/editor/`: direct `state.sector.as_mut()` mutations everywhere, no undo/redo, no derivation cache, divergent `empty_*` constructors with hardcoded `generator_version: "0.1.0"` strings, plus the RNG violation in T-04. **F-019-003** finds `factions_overview.rs:343-1013` exports 400+ lines of `&mut GeneratedSector` mutation helpers that have no callers but expand the public mutation surface of the viewer.

Resolve by either:
- **(A)** removing the editor module entirely (clean read-only contract), or
- **(B)** documenting the viewer's editor as a legitimate write surface and routing it through the builder's command bus (would require the viewer to depend on the builder crate or share the bus through `sectorforge-gui-core`).

### Theme T-07 — Pervasive `.clone()` density (MEDIUM)

1,667 `.clone()` sites total: builder 877 (53%), src 583, viewer 183, gui-core 24. Many are forced by egui's owned-string APIs and are acceptable; the egregious ones are:
- Whole-sector clones in `briefing::apply` (F-010-001) and `dashboard.rs` (F-019-001).
- `Arc::make_mut` on `RelationsMatrix` while still holding a borrow → always clones (F-010-001).
- Per-frame `BuilderState`-field deep-clones in 9 panels to dodge the `&mut state` borrow (F-018-001).
- 12 `_id` clones in BFS hot loops (F-011-013, F-007-001).
- `manual.extend(... .clone())` patterns across `personae`/`hooks`/`missions` (F-010-005).

### Theme T-08 — Test-suite gaps (HIGH)

(See §6 "Test-suite health" for the full breakdown.)

- 5 of 15 `tests/it/*.rs` files re-load the project + re-run `generate_sector` per `#[test]` instead of memoising (F-022-001) — ~25 redundant generations per run.
- `tests/it/segmentum_tests.rs` (5 tests, all `#[ignore]`) — `src/export/segmentum.rs` (1,168 LOC) byte-stability not gated by CI (F-022-003).
- Only `sectorforge generate` has CLI integration coverage. 17 other subcommands untested (F-022-004).
- `golden_png.rs` does NOT pin a hash — only asserts "two in-process renders agree" (F-022-006).
- Zero property tests on `loading/config.rs`, `loading/input.rs`, `worlds_toml.rs`, `loading/presets.rs` (F-X08-004).
- Zero `cargo-fuzz` setup workspace-wide.
- `viewer/` crate effectively untested (3 inline tests across 13k LOC).
- `src/cli/` has zero inline tests.
- `gui-core/src/jobs.rs:117` asserts `elapsed() < 500ms` — wall-clock flake (F-X08-002).
- `gui-core/src/visual_tokens.rs:156-170` test asserts nothing (`let _ = ...`) (F-X08-003).
- `builder/src/builder/file_watcher.rs:134-170` uses `thread::sleep(1.2s)` + up to 6s polling — flake-prone (F-X08-001 / F-022-002).

### Theme T-09 — Dependency hygiene (MEDIUM)

- **No `[workspace.dependencies]` block**: 10 deps duplicated across 3-4 manifests, easy drift.
- **3 unused direct deps** (grep-verified): `viewer/image`, `viewer/tempfile` (dev), `gui-core/eframe` (re-exports type aliases only).
- **0 CVEs** from `cargo audit`; 1 unmaintained warning (`paste` via wgpu-hal→eframe, RUSTSEC-2024-0436) — unfixable from workspace.
- **MSRV not declared** in any manifest.
- **`[lints.clippy]` config inconsistency**: builder + viewer set `disallowed_types/methods = "deny"`; root + gui-core silent. Plus `builder/clippy.toml` and `viewer/clippy.toml` are byte-identical — collapse into root.
- **`[profile.bench]` is `thin` LTO** while `[profile.release]` is `fat` LTO — bench numbers don't reflect prod (F-X05-015).

### Theme T-10 — Input-handling safety on hex/path/color parsing (MEDIUM/HIGH)

- **F-013-001** `parse_color` in `src/export/map_theme.rs:514-536` **panics on non-ASCII 6/8-byte hex input** — uses `hex.len()` (bytes) then `hex[idx..idx+2]` without char-boundary check. Reachable from user TOML themes.
- **F-014-001** `Geom::new` (`src/export/bitmap/geom.rs:20-37`) casts `u32 scale` directly to `i32` with no clamp; `as u32` of `total_w/h` can overflow and trigger an opaque alloc panic in `RgbaImage::from_pixel`.
- **F-014-002** SVG `escape_xml_into` covers `< > & " '` but not XML 1.0's forbidden control characters — user-derived names can produce non-well-formed SVG.
- **F-012-001** `segmentum.rs:646/656/665/681/688` casts `u32` to `i32` without bounds check on `stitch.border_depth` — wraps negative.

---

## 4. Critical & High findings (grouped by crate)

> Full per-finding detail (Problem / Evidence / Suggested fix / Effort / Risk) lives in
> the individual `reviews/**/*.review.md` files. This section is the index.

**Critical:** None.

**High (88 total, grouped):**

### `sectorforge` (root lib, 30 highs)

| ID | File:Line | One-line | Category |
|---|---|---|---|
| F-003-001 | `src/lib.rs` | Re-exported enums (esp. `SectorError`) lack `#[non_exhaustive]` | API |
| F-003-002 | `src/worlds.rs` | All 8 taxonomy `Display` impls fall through to `Debug` | API |
| F-003-003 | `src/worlds_toml.rs` | TOML wire types locked into public API | API |
| F-003-004 | `src/worlds.rs` | 8 taxonomy enums are unit-only but not `Copy` | Idiom |
| F-004-001 | `src/model/errors.rs:41-52` | Duplicate dead `MutationError` enum | API |
| F-004-002 | `src/model/sector_model/mod.rs:849` | `hex_distance` `i32→u32 as` cast can wrap | Panic |
| F-004-003 | `src/model/sector_model/mutation.rs` | Every mutation is O(n) linear scan over systems | Perf |
| F-004-004 | `src/model/sector_model/mutation.rs:449-501` | `add_region` deep-clones `Arc<Vec<WarpRegion>>` per call | Perf |
| F-005-001 | `src/loading/presets.rs` | `rewrite_seed` TOML-injects user seed | Sec |
| F-005-002 | `src/loading/input.rs` | `load_project` collapses `WorldError` into `String`, breaks source chain | Errors |
| F-005-003 | `src/loading/input.rs` | `read_relative` doesn't reject `..`/abs paths; non-canonical key | Sec |
| F-005-004 | `src/cli/generate.rs` | `generate` mixes `Err(SectorError)` and bespoke `eprintln!`+exit 1 | Errors |
| F-006-001 | `src/validate/diff.rs:454,549,705` | `unreachable!()` reached on malformed model | Panic |
| F-006-002 | `src/validate/invariants.rs` | `check_systems` walks each world's tag vec 9 times | Perf |
| F-006-003 | `src/validate/validation.rs` | ~80 `let _ = writeln!(s, ...)` swallow `fmt::Error` | Errors |
| F-006-004 | `src/validate/validation.rs` | Stringly-typed `code: String` everywhere | API |
| F-007-001 | `src/gen/hidden_routes.rs:416-417` | `unwrap()` on lookup with parallel-structure invariant | Panic |
| F-007-002 | `src/gen/generation/factions.rs:131-132` | Reallocs parallel Vec per influence tier | Perf |
| F-007-003 | `src/gen/regions.rs` `is_navigable_bridge` | O(R²·log R) adjacency rebuild | Perf |
| F-008-001 | `src/gen/world_ecs.rs:168-185` | Orphan route entity with missing endpoint join | Correctness |
| F-008-002 | `src/gen/world_pool.rs:213-219` | Silently drops unrecognised notable feature names | Correctness |
| F-009-001 | `src/analysis/search.rs:1098-1123` | Per-candidate deep clone of `ProjectInput` under rayon | Perf |
| F-009-002 | `src/analysis/search.rs:843-855` | `u32` underflow on `component_count == 0` | Panic |
| F-009-003 | `src/analysis/economy.rs`+`relations.rs`+`search.rs` | No `#[non_exhaustive]` anywhere | API |
| F-009-004 | `src/model/sector_model/mod.rs:285-287` | `get_worlds_for_system` allocs Vec per call | Perf |
| F-010-001 | `src/analysis/briefing.rs:217,266` | Per-profile whole-sector + relations clone | Perf |
| F-010-002 | `src/analysis/hooks.rs:255-326` | O(R²) supply-line counter | Perf |
| F-010-003 | `src/analysis/{personae,hooks,missions,briefing}.rs` | Persona/Hook/Mission IDs bare `String` (newtype miss) | API |
| F-011-001 | `src/analysis/importance.rs:139-179` | **HashMap iteration feeds golden writers (determinism)** | Det |
| F-011-002 | `src/analysis/control.rs:368-372 vs 473-481` | World vs system tie-break direction reversed | Correctness |
| F-011-003 | `src/analysis/route_control.rs:142` | `pirate` matcher contains stale `"chaos"` kind | Correctness |
| F-012-001 | `src/export/segmentum.rs:646..688` | `u32→i32` cast on `border_depth` can wrap | Panic |
| F-012-002 | `src/export/subsectors/mod.rs:530-631` | Lloyd refinement O(n²·k·iter) due to linear scan | Perf |
| F-012-003 | `src/export/subsectors/mod.rs:518-525 vs 594-606` | Inverted tie-break across passes | Det |
| F-013-001 | `src/export/map_theme.rs:514-536` | `parse_color` panics on non-ASCII hex | Panic |
| F-014-001 | `src/export/bitmap/geom.rs:20-37` | Scale cast overflow in `Geom::new` | Panic |
| F-014-002 | `src/export/svg_export/primitives.rs:142-153` | SVG escape misses XML control chars | Sec |

### `sectorforge-builder` (24 highs)

| ID | File:Line | One-line | Category |
|---|---|---|---|
| F-015-001 | `builder/src/builder/state/generation_ops.rs` + `regions_ops.rs` | Mutations bypass command bus | Inv |
| F-015-002 | `builder/src/builder/state/mod.rs:76-518` | Every `BuilderState` field is `pub` | API |
| F-015-003 | `builder/src/builder/state/derivations.rs` | `recompute_*` overwrites Arc-fields with no undo entry | Inv |
| F-015-005 | `builder/src/builder/state/undo.rs:68-78` | `trigger_auto_save` swallows IO errors | Errors |
| F-015-006 | `builder/src/builder/project_io.rs:832-924` | `reload_catalog` swallows 15 parse errors | Errors |
| F-015-007 | `builder/src/builder/session.rs:95-242` | `into_state` is 130-line hand-copy of `new_blank` | Maint |
| F-016-001..006 | `builder/src/builder/panels/{system,world,routes}.rs` | ~69 direct field writes; `ReplaceRoutes` with empty `before` | Inv |
| F-016-007..010 | Same files | Per-frame full clones of `sector.routes`/world/factions/claims; full `feature_weights_for_world()` rebuild every frame | Perf |
| F-017-001..005 | `builder/src/builder/panels/{history,control,relations,factions,economy}.rs` | All 5 panels bypass command bus | Inv |
| F-017-006 | `builder/src/builder/panels/history.rs:1409` | `DefaultHasher` for manual event IDs (determinism break) | Det |
| F-017-007..011 | same | Per-frame deep clones of large collections per panel | Perf |
| F-017-012 | `builder/src/builder/panels/control.rs:1231,1240` | `partial_cmp().unwrap()` on f32 sums (NaN reachable) | Panic |
| F-018-001 | 9 panels in `panels-misc` group | Recurring "clone derived overlay per frame" pattern | Perf |
| F-018-002 | `builder/src/builder/panels/interestingness.rs` | Full `derive_interestingness_with` re-run per Add click | Perf |

### `sectorforge-viewer` (12 highs)

| ID | File:Line | One-line | Category |
|---|---|---|---|
| F-019-001 | `viewer/src/dashboard.rs:43` | Per-frame deep-clone of `SectorAnalysis` | Perf |
| F-019-002 | `viewer/src/segmentum_view.rs:234-411` | Per-frame super_map rebuild with id clones | Perf |
| F-019-003 | `viewer/src/factions_overview.rs:343-1013` | Exports 400+ lines of `&mut GeneratedSector` mutators (read-only crate) | API/Inv |
| F-020-001 | `viewer/src/app/mod.rs:211` | `serde_json::to_string_pretty().unwrap()` + ignored write error in auto-save | Panic/Errors |
| F-020-002 | `viewer/src/app/{lifecycle,mod,sector_view}.rs` | `load_project` reparses TOML on every dirty edit (3 duplicated blocks) | Perf |
| F-020-003 | `viewer/src/app/export_ui.rs` | Export errors collapsed to one-line `format!`; first failure only | Errors |
| F-020-004 | `viewer/src/app/system_view.rs:31` | `expect("sector loaded")` works only by accidental short-circuit | Panic |
| F-021-001 | `viewer/src/editor/` | Editor mutates without command bus, divergent `empty_*` ctors | Inv |
| F-021-002 | `viewer/src/editor/generation_panel.rs:30,259` | `rand::random::<f64>()` bypasses `model/rng.rs` | Inv (Det) |
| F-021-003 | `viewer/src/editor/map_panel.rs:50,92` | `expect("sector presence checked above")` per-frame in draw path | Panic |
| F-021-005 | `viewer/src/editor/wishes_panel.rs:30` | `state.wishes.as_mut().unwrap()` defended only by UI control flow | Panic |
| F-021-006 | `viewer/src/editor/{wishes,generation}_panel.rs` | APPLY WINNING SEED / PREVIEW silently overwrites dirty sector | Data-loss |

### `sectorforge-gui-core` (6 highs)

| ID | File:Line | One-line | Category |
|---|---|---|---|
| F-001-API1 | `gui-core/src/sector_view.rs` | `SectorView`, `MapTheme`, `SystemView`, 7 token enums lack `#[non_exhaustive]` | API |
| F-001-API2 | same | `SectorView` has 19 positional public fields used as struct literals | API |
| F-001-API3 | same | `pub type HeatmapCells = HashMap<...>` bakes SipHash into cross-crate surface | Det/API |
| F-002-001 | `gui-core/src/info_panel.rs` | 64 `format!` + 88 `to_uppercase` per render | Perf |
| F-002-002 | `gui-core/src/palette.rs` | `RouteControlKind` duplicates `render_core::routes::ControlKind` | API |
| F-002-003 | `gui-core/src/palette.rs` | `top_route_control` tie-break is order-sensitive | Det |

### Cross-cutting (16 highs)

| ID | Sweep | One-line |
|---|---|---|
| F-X02-001 | public-api | Zero `#[non_exhaustive]` across 1,128 pub items |
| F-X02-002 | public-api | Naming collision: two unrelated `MapTheme` / two `heatmap` modules |
| F-X02-003 | public-api | ~350 internal `pub` items in bin lib targets |
| F-X02-004 | public-api | `SectorView`/`SectorMapCache`/`JobHandle` over-expose internals (incl. raw `Arc<Mutex<...>>`) |
| F-X03-001 | error-model | Duplicate dead `MutationError` |
| F-X03-002 | error-model | `reload_catalog` 15 swallowed parse errors |
| F-X03-003 | error-model | `build_subsectors` errors dropped at 8 sites |
| F-X03-004 | error-model | CLI exit codes collapse all errors to 2 |
| F-X05-001 | perf | Per-frame `format!`/`to_string` density (~340 + 439 instances) |
| F-X05-002 | perf | `run_search` processes full budget after winner found |
| F-X05-003 | perf | `faction_style_by_id` linear scan per-route per-frame |
| F-X05-004 | perf | SVG `color_hex` allocates `String` per attribute per shape |
| F-X07-001 | panic-surface | `partial_cmp().unwrap()` in `panels/control.rs:1231,1240` |
| F-X07-002 | panic-surface | `Utf8PathBuf::from_path_buf().unwrap()` on `rfd::pick_file` result |
| F-X08-001 | testing | `file_watcher` sleep+poll test (1.2s + 6s) |
| F-X08-002 | testing | `jobs.rs:117` wall-clock `elapsed() < 500ms` assertion |
| F-X08-003 | testing | `visual_tokens.rs:156-170` test asserts nothing |
| F-X08-004 | testing | Zero property tests on `loading/` and `worlds_toml.rs` |
| F-022-001 | tests/integration | 5/15 `tests/it/*.rs` re-generate per test (no shared fixture) |
| F-022-003 | tests/integration | All 5 segmentum tests `#[ignore]`'d — 1,168 LOC ungated |
| F-022-004 | tests/integration | 17 of 18 CLI subcommands have no integration coverage |
| F-022-006 | tests/integration | `golden_png.rs` doesn't pin a hash |

---

## 5. Remediation backlog summary

See `RUST_FIXES.md` for the prioritised action list. Briefly:

- **Quick wins** (S effort, Low risk, ≥Medium severity): ~80 items. Top examples — apply `#[non_exhaustive]` everywhere (one-line attributes); guard the two `partial_cmp().unwrap()` sites; bound `Vec::with_capacity`; replace `count() as u32` with `try_into`; fix `parse_color` byte-boundary; replace `DefaultHasher` with blake3; switch `compute_display_buckets` to `BTreeMap`; expand `pirate` matcher in route_control.
- **Targeted fixes** (M effort): ~30 items. Top — wrap `ProjectInput` immutable arms in `Arc` for rayon; introduce per-enum `as_slug() -> &'static str` (kills many `format!{:?}` callers); add `find_map_first` short-circuit in `run_search`; hoist scratch buffers to `SectorMapCache`; collapse builder/viewer's duplicated `empty_*` ctors into `sectorforge`; hoist Cargo dep block to workspace.
- **Structural refactors** (L/XL): ~10 items. Top — **the command-bus rewrite** (~50 sites across builder), narrow `BuilderState` visibility to `pub(crate)`, decide viewer/editor's read/write contract and either remove or rewire, deduplicate the dual `MapTheme`/`heatmap` modules.

---

## 6. Test-suite health

(Roll-up of U022 + X08 + per-unit testing observations.)

**Inventory**
- **89 inline `#[cfg(test)]` modules, 439 `#[test]` functions** workspace-wide.
- **`tests/it/` integration**: 14 files, 2,309 LOC total. 1 file (`invariants_proptest.rs`) is property-test based.
- **`benches/`**: 1 file (`generation.rs`, 125 LOC).
- **`gui-core/tests/map_snapshots.rs`**: snapshot-test file (correct template).

**Distribution skew**
- ~250 of the 439 inline tests live in `builder/`.
- `viewer/` has 3 inline tests across ~13,100 LOC.
- `src/cli/` has 0 inline tests.

**Clean axes (no findings)**
- No `#[ignore]` left in inline test code (only in `tests/it/segmentum_tests.rs`).
- No broad `should_panic` without `expected = "..."`.
- No `HashMap`-iteration-order assertions.
- No `thread_rng` in tests.
- No trivially-true asserts.

**Test-quality findings**
- 3 wall-clock-based flake risks: `file_watcher` sleep, `jobs.rs` 500ms cap, `mtime` polling.
- 1 "test that asserts nothing" (`visual_tokens.rs:156-170`).
- All 5 `segmentum_tests.rs` cases `#[ignore]`'d — golden coverage gap on a 1,168-LOC writer.
- 4 nearly identical `determinism_holds_across_random_seeds` proptests duplicate the sector-level proptest in `invariants_proptest.rs` — 80 redundant `generate_sector` calls per run.
- Doctest coverage on public API is essentially zero (one `no_run` in `src/lib.rs`).

**Coverage gaps** (top 10)
1. CLI: all subcommands except `generate` (17 of 18).
2. `src/loading/{config,input,presets}.rs` + `src/worlds_toml.rs`: 0 property tests.
3. `viewer/` crate generally.
4. `src/cli/common.rs` and `src/cli/mod.rs`.
5. `src/validate/{validation,invariants}.rs` inline tests.
6. `gui-core/src/{map_theme,nav}.rs` — 0 tests.
7. `builder/src/builder/state/regions_ops.rs` — has rich tests but doesn't exercise undo path.
8. Builder's `command::execute` per-variant round-trip (only `AddSystem` is tested).
9. SVG/HTML/segmentum export byte-pinned golden hashes (only PNG is golden-pinned, and even that one only asserts agreement of two in-process renders).
10. Fuzz targets for any TOML/JSON loader.

**Projected suite-time reduction** (if F-022-001/002/007/010 + F-X08-001/002 applied): **~50%** of current total suite wall-clock per U022's estimate.

---

## 7. Confidence ledger

### High confidence (the review is sure)

- **All 5 hard-rule violations** (T-04 determinism, T-01 command-bus, T-06 RNG in viewer) — verifiable with grep + golden-test flip.
- **Per-frame allocation theme (T-02)** — quantified counts from grep; the specific call sites are cited.
- **Public API surface (T-03)** — exhaustive enumeration via `grep -rn '^pub '`.
- **`unsafe = 0`** — verified twice in recon, confirmed by all per-unit agents.
- **The 12 reachable panic sites** in non-test code (X07) — explicitly enumerated and classified.
- **Cargo dep hoist opportunities (T-09)** — concrete plan in `reviews/_xcut/dependencies.review.md`.

### Medium confidence (review states the pattern; numbers need profiling)

- **Performance findings** that say "hot path" — context is correct (per-frame vs per-build), but the *magnitude* needs `cargo bench`/`criterion` measurement before claiming wins. Specifically: rayon `Arc`-hoist (F-009-001), `find_map_first` short-circuit (F-X05-002), per-frame `format!` rewrite (T-02). The agents flagged this themselves.
- **The big-O claims** in `src/analysis/` (O(R²) supply-line counter, O(n²·k·iter) Lloyd, O(reachable × routes) BFS) — algorithmically sound, but absolute timing is unknown without profile data.
- **Briefing/dashboard whole-sector clone cost** — clearly wasteful, but the absolute size of `GeneratedSector` for a 200-system sector is not measured; could be cheap in practice.

### Low confidence (review flags for human judgment)

- **Viewer/editor scope (T-06, Theme F-021-001)** — the right answer is a product decision (read-only vs read-write), not an engineering one.
- **`SubsectorBuildError` handling (F-X03-003)** — some sites might legitimately want to fall back silently; the user should triage.
- **`#[non_exhaustive]` blast radius** — adding the attribute is cheap, but downstream `match` blocks need wildcard arms; agents didn't enumerate every match site.
- **Cargo hoist (T-09)** — the proposed `[workspace.dependencies]` block is a refactor with non-trivial blast radius; recommend running `cargo build --all-targets` after each crate's manifest is converted, not bulk.

---

## 8. Coverage statement

- **LOC reviewed:** 93,958 (entire workspace excluding `old/`, `target/`, `out/`, `presets/`, `WOW/`).
- **Units completed vs manifest:** 22/22 primary + 8/8 cross-cutting sweeps = 30/30. No files skipped.
- **Files explicitly excluded:** `old/` (per CLAUDE.md rule); `Cargo.lock` (read for dep audit but not "reviewed").
- **Tooling used:**
  - `cargo build --workspace --all-targets` → clean.
  - `cargo clippy --workspace --all-targets --all-features -- -W clippy::all -W clippy::pedantic -W clippy::nursery` → 2,979 warnings (default lints: 0). Per-package: builder lib 542 + 27 unique test, viewer lib 217 + 1 bin, gui-core lib 187 + 6/10 unique test+snap, dhat-profile 2.
  - `cargo test --workspace --no-run` → clean (test binaries compile).
  - `cargo tree --workspace --duplicates` → many transitive duplicates, all rooted in eframe/winit/objc2 stack, not workspace-fixable. See `reviews/_baseline/dup-deps.txt`.
  - `cargo audit` → 0 CVEs, 1 unmaintained warning.
  - `cargo +nightly udeps` → not run (nightly toolchain not present in env). Verified unused deps via grep.
  - `cargo geiger` → not run (low priority given `unsafe = 0`).
  - `cargo nextest run` / `cargo llvm-cov` → not run (sandbox blocks; recommended as follow-up).
- **Determinism gate** (`cargo test --test it -- golden`) was not invoked during the review (read-only pass per protocol). The two confirmed determinism violations (F-011-001, F-017-006) are visible to it; golden tests **will** flip if downstream agents apply fixes that affect rendered output.

---

*End of FINDINGS.MD. For per-finding detail browse `reviews/`. For the prioritised action backlog see `RUST_FIXES.md`.*
