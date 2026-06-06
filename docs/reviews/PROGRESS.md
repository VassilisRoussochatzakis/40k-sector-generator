# Review execution — progress tracker

Tracks the **fix** status of the findings verified in this directory. The
`AREA_*.md` files describe *what* the findings are; this file tracks *what has
been done about them*, against the [README](README.md) "Recommended execution
sequence". Update this file whenever a finding moves status.

**Legend:** ✅ DONE · 🔄 IN PROGRESS · ⏳ PENDING · ⏸️ DEFERRED (blocked / owner-decision)

> Note: the **Status** column inside each `AREA_*.md` records whether a finding
> *reproduced* (`✅ Confirmed` / `⚠️ Partial` / `🔄 Moved` / `🟢 Already fixed`).
> That is **not** the same as fix status — this file is the fix tracker.

## Area roll-up

| Area | Findings | ✅ Done | 🔄 In progress | ⏳ Pending | ⏸️ Deferred |
|---|---|---|---|---|---|
| A `src/model` + generation | 12 | 11 (A1,A2,A3,A4,A6,A7,A8,A9,A10,A11,A12) | 0 | 0 | 1 (A5) |
| B `src/analysis` | 14 | 11 (B-S2*,B-S3,B1,B3,B4,B5,B6,B7,B9,B11,B12) | 0 | 1 (B-S1) | 2 (B8,B10) |
| C export/validate/worlds/cli | 13 | 4 (C1,C-S2,C3,C6) | 0 | 9 | 0 |
| D builder command + state | 14 | 12 | 0 | 0 | 2 (D-S3/D5) |
| E builder panels | 17 | **17 (E1,E2,E3,E4,E5*,E6,E7,E8*,E9,E10,E11,E12,E13,E14,E-S1,E-S2*,E-S3*) — AREA COMPLETE** | 0 | 0 | 0 |
| F viewer + gui-core | 15 | **15 (F1–F12, F-S1/F-S2/F-S3 — AREA COMPLETE)** | 0 | 0 | 0 |
| G tests | 13 | 1 (G2) | 0 | 12 | 0 |

## Execution sequence (README order)

1. **P0 bus fixes** — triaged E2 → E1 → D1, plus D4.
   - D1 region add/remove/paint/erase commands — ✅ DONE (`7a06824`)
   - D4 move/swap route-controls — ✅ DONE, documented carve-out (`7a06824`)
   - D2/D11 chronicle undo precedence — ✅ DONE, on-bus active / off-bus passive split (`7a06824`)
   - **E2** per-frame `primary_factions` off-bus write — ✅ DONE (this session)
   - **E1** `ApplyFactionPower` command — ✅ DONE (this session)
2. **`labeled()` extraction** (E3 / E-S1, 33 files) — ✅ DONE (this session)
3. **C1** diff-drift fix — ✅ DONE · **`enum_slug!` macro** (B-S3, 61/62 sites) — ✅ DONE (this session)
4. **G2 content golden** (`sector.json` / `sector.md`) — ✅ DONE (this session) · **gate cleared — A–F god-file splits now have their safety net**
5. **Dedup waves + god-file splits** (behind G2) — 🔄 IN PROGRESS (2026-06-05)
   - **Wave 1 — AREA_B mechanical** (proportionate, compiler-checked, golden-gated):
     B7 ✅ · B12 ✅ · B9 ✅ (this session). See log below.
   - **Wave 2+ — god-file splits** (verbatim carves behind G2):
     B11 ✅ · A1 ✅ · C6 ✅ · E7 ✅ · E4 ✅ (split-only) · F3 ✅ (split-only) ·
     F8 ✅ (by-section). Remaining splits: none outstanding; the deferred
     API-shape items (F-S3, D-S3/D5, A5, E4-part-a) stay owner-gated.
   - Remaining dedup: AREA_B perf (**B1/B3/B5/B6 ✅ — all done**), **B4 ✅**;
     trait/macro dedup (B-S1, B-S2 merge-half — both owner-gated; **E-S3 ✅**
     (16/26 sites; 10 divergent left by design), C2, F-S1); **C3 ✅** (this
     session).
   - **Wave 4 — AREA_F semantic-color sweep** (viewer chrome, no snapshot
     exposure): F6 ✅ · F9 ✅ · F11 ✅ · F12 ✅ (warm-ups) · F5 ✅ (the
     ~25-site `Color32::from_rgb` → `palette::warning/danger/success` sweep).
     See log below.
   - **Wave 5 — AREA_F hot-path cache** (render-equivalent, snapshot-gated):
     F4 ✅ (planner + editor map `SectorView { cache: None }` → real
     `SectorMapCache`). See log below.
   - **Wave 6 — AREA_F cross-crate widget dedup** (form widget, no snapshot
     exposure): F2 ✅ (the duplicated `enum_combo` hoisted to
     `gui_core::widgets::enum_combo`; viewer + builder keep thin forwarders).
     See log below.

## Detailed log

### 2026-06-05 — session start

- **AREA_D** confirmed fully closed by commit `7a06824` (all 14 findings).
  `D-S3`/`D5` (154-field god-struct split) intentionally ⏸️ DEFERRED behind the
  G2 content-golden net per the review's own sequencing.

- **E2** — `builder/src/builder/panels/control.rs` §C5. Replaced the per-frame
  off-bus write `systems[i].primary_factions = derived` with a **change-gated,
  dirty-tracked passive reconcile** (`if !locked && stored != derived { write;
  dirty }`).
  **Decision (owner-visible):** chose README option *(a) derivation/read-model*
  — keep the passive reconcile **off** the undo bus — over option *(b) commit via
  `EditSystem`*. Rationale: `primary_factions` has many stored readers (export
  render/labels, info_panel, analysis, validate) so the field must stay
  populated; and dispatching a command during render would inject an undo entry
  on mere tab navigation and *fight* undo (undo → re-derive from unchanged
  presence → re-dispatch). This mirrors the **LD4 chronicle §R4 carve-out** the
  D2/D11 fix (`7a06824`) just established: passive view-refresh of denormalized
  document state stays off-bus to protect the redo tail; the **active** "↺
  Re-derive" button remains on-bus via `EditSystem`.
  _Known residual (pre-existing, out of scope):_ for an unlocked system whose
  presence changes while the CONTROL tab is not open, the stored field stays
  stale until next view — same window as the old code; any reader needing the
  live value calls `derive_system_control()` directly.

- **E1** — `control.rs` §C6 + `builder/src/builder/command.rs`. Added
  `BuilderCommand::ApplyFactionPower { before: Vec<(FactionId, PowerProfile)>,
  after: BTreeMap<FactionId, PowerProfile> }` — `dep_classes = [Factions]`
  (fans out to PowerProjection + faction overlays), `before` captured on
  `apply`, `revert` restores. The "↺ Apply to faction totals" button now routes
  through `state.run`; manual `dirty`/`mark_validation_dirty` dropped (bus rails
  handle it). Added `apply_faction_power_round_trip` test; extended
  `dep_classes_cover_all_variants` (38 → 39).

- **Verification (E1+E2)** — `cargo test -p sectorforge-builder` 317/317 pass
  (incl. `dep_classes_cover_all_variants`, `apply_faction_power_round_trip`);
  `cargo test --test it -- golden` 13/13 pass (byte-stable — `primary_factions`
  feeds render/labels, unchanged); `cargo clippy -p sectorforge-builder
  --all-targets -- -D warnings` clean.

- **G2 / G-S3** — `tests/it/golden_generation.rs` + `tests/goldens/`. Added a
  full-content golden for the exported `sector.json` (~1.8 MB) and `sector.md`
  (~286 KB) of the m42 fixture, gated by `UPDATE_GOLDEN_JSON` /
  `UPDATE_GOLDEN_MD`. Chose **full-file pin over a blake3 hash** (the golden_png
  style) because G2's purpose is a *reviewable* diff of text drift (renamed
  field / dropped markdown row) — a hash detects drift but can't show it.
  Failure output reports the first differing line, not the whole file. Blessed
  then re-verified reproducible: **15/15 golden tests pass**. This is the safety
  net the A–F god-file splits are gated on — **gate now cleared**.
  _Size trade-off:_ the two blessed files add ~2.1 MB to the repo. If a lean
  history is preferred over diff-ability, switch to a committed blake3 hash pair.

- **C1 / C-S2** — `src/validate/diff.rs:370`. Replaced the stringly-typed
  `match *k { … _ => 0.0 }` resource closure with `sector_balance.get(k)` (the
  canonical `ResourceVector::get`; `&&str`→`&str` coerces, so no explicit deref).
  A new `RESOURCE_KEYS` entry now flows into economy diffs automatically instead
  of silently reading 0.0 — kills the data-loss class. Behaviour-identical for
  the current set; `diff` + `golden` tests green.

- **B-S3** — added `macro_rules! enum_slug!` in `src/macros.rs` (wired crate-wide
  via `#[macro_use]`), with a normal arm + a `const` arm; slugs written verbatim
  so output is byte-identical. Converted **61 of 62** hand-written `as_slug`
  enums across analysis / gen / export / model / loading / validate (5 by hand —
  personae + validation, incl. the one `const` enum `ValidationCode`; 56 via a
  general-purpose subagent gated on the G2 content golden). The 1 holdout,
  `SectorSize` in `gen/random_sector.rs`, keeps its hand-written `as_slug`: it has
  a struct variant `Custom { dim }`, so it is not the fieldless enum the macro
  targets (forcing it through the macro is a compile error). Verified: golden
  15/15 byte-identical, lib 191/191, `it` 93/93, workspace clippy clean.

- **E3 / E-S1** — hoisted the byte-identical private `fn labeled` from 33 builder
  panel files to `pub fn labeled` in `gui-core/src/ui_kit.rs` (sibling to the
  existing `field`), removed all 33 copies, and folded the import into each
  file's existing `ui_kit` use. 9 files also needed an unused-import cleanup
  (`Ui` / `RichText` / `palette` used only inside the deleted fn). The mechanical
  sweep was delegated to a `panel-implementer` subagent; verified here:
  `grep 'fn labeled' builder/src` → empty, **workspace clippy clean**, builder
  **317/317**, `it` suite **93/93**.

### 2026-06-05 — step 5, wave 1 (AREA_B mechanical)

Lead with the proportionate, compiler-checked, low-risk fixes (AREA_B's own
local order, not the README's trait-first listing) — matches the standing
refactor preference. All three verified together: clippy clean, lib **191/191**,
golden **15/15 byte-identical**.

- **B7** — `analytics.rs`. Replaced `format!("{}", x).into()` with
  `Arc::from(x.as_slug())` at the 5 enum-key sites (`route_type`, `stability`,
  `claim_type`, `dominance`, system `state`). Verified each type's `Display`
  delegates to `as_slug` (`f.write_str(self.as_slug())`) → byte-identical slugs,
  one fewer heap alloc per key.
- **B12** — `economy.rs`. Named the magic thresholds as module `const f32`:
  `SELF_SUFFICIENCY_OUTPUT` (30.0, sites 1164/1244), `SUPPLY_RESILIENCE_SAFE`
  (30.0, site 1275 — kept **separate** from the output threshold; different
  quantity that merely shares the value), and the route-friction
  divisors/caps (`ROUTE_{PIRACY,INTERDICTION}_DIVISOR/MAX_MALUS`,
  `ROUTE_PATROL_DIVISOR/MAX_BONUS`). `f32` literals → bit-identical.
- **B9** — new `pub(crate) cmp_f32_desc` / `cmp_f32_asc` in `analysis/mod.rs`;
  converted **13** scattered `partial_cmp(..).unwrap_or(Equal)` sorts across 9
  files (9 desc + 4 asc), preserving every `.then_with`/`.then` tiebreaker; the
  test-only `.unwrap()` at `search.rs:1447` left as-is. Dropped a now-unused
  `Ordering` import in `history/routes.rs`.
  **Decision (owner-visible):** kept the **exact** historical NaN policy
  (`partial_cmp → Equal`) rather than the review's suggested `total_cmp` swap —
  zero behaviour change, so goldens stayed green and there's no output-order
  surprise. The policy is now single-source, so upgrading to `total_cmp` later is
  a one-line change in one place if wanted.

### 2026-06-05 — step 5, wave 2 (god-file splits — B11)

Per owner choice, jumped to the mechanical god-file splits behind the G2 golden
net. Each split is a **verbatim** carve (no logic change); the byte-stable
`sector.json`/`sector.md` goldens are the proof that nothing moved semantically.
Large carves delegated to a subagent (keeps ~1800 LOC out of main context),
then re-verified here in the main thread.

- **B11 (economy half)** — ✅ DONE. `src/analysis/economy.rs` (1789 LOC) → a
  directory module `economy/`: `config.rs` (459 — types/consts/DTOs/status
  enums), `tables.rs` (368 — built-in world-type vectors + rule fns),
  `derive.rs` (511 — loader + derivation + dependency edges + route economy +
  `apply_stability_nudge`), `risk.rs` (134 — supply-risk + tithe classifiers),
  `render.rs` (146 — `render_markdown`/`write_report`), `mod.rs` (251 —
  re-exports + moved `#[cfg(test)]` tests). Cross-submodule internals raised
  private→`pub(super)` only (the §B12 consts, `ResourceVector::scale`,
  `StrategicOutput::{add_assign,scale,clamp_scores}`, the 9 `tables` fns,
  `strategic_needs_for_world`, and the 5 risk classifiers); nothing newly `pub`.
  Public surface unchanged at `economy::` (lib.rs re-export + tests untouched).
  **Re-verified in main thread:** `economy.rs` removed, check + clippy
  `-D warnings` clean, **golden 15/15 byte-identical**, lib 191/191, economy
  integration 7/7.
- **B11 (relations half)** — ✅ DONE. `src/analysis/relations.rs` (1675 LOC) →
  `relations/`: `config.rs` (403 — `Stance` + schema + DTOs +
  `RelationAttitude`/`TreatyStatus`), `tables.rs` (281 — `*_KINDS` consts +
  stance/ideology classifiers), `tension.rs` (175 — cooccurrence + `tension_of`),
  `derive.rs` (542 — entry points + pipeline + loader, incl. the load-bearing
  `derive_with_threshold`), `render.rs` (127), `mod.rs` (218 — re-exports +
  moved tests). Cross-submodule internals raised private→`pub(super)` only
  (`Stance::shift`, `RelationAttitude::{level,from_stance,to_stance}`, the
  `tables` classifiers used by `derive`, `CooccurStats`/`build_cooccurrence`/
  `tension_of`, `canonical_pair`); nothing newly `pub`. B6 `canonical_pair` /
  B3 rule-indexing **excluded** (verbatim move only — separate perf findings).
  **Re-verified in main thread:** `relations.rs` removed, golden **15/15
  byte-identical**, lib 191/191, relations integration 6/6, **`cargo check
  --workspace --all-targets` clean** (downstream builder/viewer resolve the
  preserved `relations::`/`economy::` re-exports). A rust-analyzer "syntax error"
  flag on `derive.rs:166` (`rng.gen()`) was a false alarm — verbatim original
  code; cargo compiles it clean.

  **B11 finding now fully closed** (both god-modules split).
- **A1 (model render-vocab split)** — ✅ DONE. Extracted the route **render
  vocabulary** out of the `src/model/sector_model/mod.rs` DTO god-file (1490 →
  1272 LOC) into a new sibling `routes_view.rs` (235 LOC): `RoutePattern` +
  `strides`, `RouteViewMode` (+`enum_slug!`+`Display`), the private
  `stable_pattern_hash`, and the render impl-methods split out of the mixed
  impl blocks — `RouteType::{pattern,patterns,pattern_for_key,pattern_key}`,
  `RouteKind::patterns`, `RouteStability::pattern_key`,
  `GeneratedRoute::{pattern,pattern_with_salt}`. The DTO/identity halves
  (enums + serde + `kind`/`label`/`Display` + `RouteType::is_hidden`, judged
  legend-not-render) **stay** in `mod.rs`. Inherent impls legally live in the
  new module; `pub use routes_view::{RoutePattern, RouteViewMode}` keeps
  `crate::sector_model::*` paths intact (consumers: export render_core, gui-core
  info_panel/legend, viewer settings). No private→`pub(super)` bumps needed
  (the `GeneratedRoute` fields read were already `pub`). **Re-verified in main
  thread:** golden **15/15 byte-identical**, gui-core **map snapshots pass
  un-blessed** (`map_snapshots_match_goldens`), lib 191/191, `cargo clippy
  --workspace` clean. (A rust-analyzer E0599/E0432 flurry on export `routes.rs`/
  `legend.rs` was a stale-index false alarm — `cargo check -p sectorforge`
  compiles those exact files and passed.)

  _Not done:_ **A5** (157-pub-field visibility tightening) is the *other* AREA_A
  god-file item but is a wide cascade into builder/viewer/tests, not a clean
  split — left for an owner decision like D-S3/D5.

### 2026-06-05 — step 5, wave 3 (god-file splits — C6)

- **C6 (worlds IO split)** — ✅ DONE. Moved the worlds-data **loader** out of the
  `src/worlds.rs` taxonomy god-file (1371 → 1325 LOC) into the existing
  `src/worlds_toml.rs` IO module (346 → 399 LOC): `WorldError` (enum),
  `WorldsLoad` (struct + `into_legacy_tuple`), and `load_worlds_data`. Verbatim
  carve — bodies unchanged (kept the `crate::worlds_toml::*` self-paths and the
  exact `WorldError::Invalid` map_err strings). `worlds.rs` now re-exports them
  (`pub use crate::worlds_toml::{load_worlds_data, WorldError, WorldsLoad};`) so
  `crate::worlds::*` paths stay stable (consumers: `model/errors.rs`,
  `loading/input.rs`, `gen/world_pool.rs` via the retained `load_generation_rows`
  shim). `worlds_toml.rs` dropped `WorldError` from its `use crate::worlds::{…}`
  (now local); the now-unused `use thiserror::Error;` was removed from
  `worlds.rs`. Bidirectional module `use` between the two siblings is legal — no
  cycle. **Phase 2** (`enum_slug!`/`strum` collapse of the 9 `FromStr`/`Display`/
  `VARIANTS` triples, ~940 LOC) intentionally **not** done — output-equivalence
  risk on the hand-written slugs; left as a separate, owner-gated pass like the
  `SectorSize` macro holdout (B-S3). **Re-verified in main thread:** `cargo check
  -p sectorforge --all-targets` clean, **golden 15/15 byte-identical**, lib
  191/191, `it` 93/93, **`cargo clippy --workspace --all-targets -- -D warnings`
  clean**. (A rust-analyzer E0432 flurry on `worlds.rs:11` during the carve was a
  stale-index false alarm — cargo resolves the re-export.) MAP.md + GUIDE.md
  updated.

- **C3 (emit-triple dedup)** — ✅ DONE. Collapsed the
  `if let Some(dir) = out { write+println } else if json { print_json } else
  { render_markdown }` triple into a single `common::emit_report<R: Serialize>(
  out, json, report, write_dir, render_md)` helper. Converted **13** call sites
  (the 12 the review flagged — analyze, economy, history, hooks,
  interestingness, missions, personae, prose, regions, relations, sites,
  search — **plus** `diff.rs`, the review's "variant": it fits the same helper
  by passing `args.out.as_ref()` / `args.json`, so no special-case was needed).
  Each runner's `write_*` call **and** its verbatim `"Wrote …"` confirmation
  stay inside the per-runner `write_dir` closure (the filenames differ), so CLI
  **stdout is byte-identical by construction** — `print!("{md}")` ≡
  `print!("{}", md)`, same branch order, same `?` error path. `print_json` is
  now called only inside `emit_report`; the 13 runners swapped their
  `common::print_json` import for `common::emit_report` (the 4 that also import
  `load_or_regenerate` kept it). **Not in scope** (correctly): `compose.rs`
  (2-way `json`/write branch, different "Composed" message) and `validate.rs`
  (report printers) — neither is the out/json/md triple. **Re-verified in main
  thread:** check clean, lib 191/191, **it 93/93** (incl. the CLI stdout tests
  `economy_tests`/`hooks_tests`/`personae_tests`/`relations_tests` +
  `cli_smoke`/`cli_gui_parity`), golden 15/15 byte-identical, workspace clippy
  `-D warnings` clean. Pure dedup — no file moved, so MAP.md/GUIDE.md untouched.
  (C2 — the sibling `(project,sector)` resolve-match dedup — intentionally left
  open; C3's helper signature is independent of it.)

### 2026-06-05 — step 5, wave 3 (god-file split — E7)

- **E7 (`system.rs` split)** — ✅ DONE. Split the 2017-LOC
  `builder/src/builder/panels/system.rs` into a `system/` directory module
  (total 2095 LOC across 6 files; the +78 is per-file imports + `mod`/`use`/
  re-export/visibility lines): `mod.rs` (700 — module doc + imports, 3 consts,
  3 label helpers, `show`/roster/inspector/header, the 5 read-only deep-link
  sections, re-exports, and the verbatim `#[cfg(test)]` block), `identity.rs`
  (428 — identity/coord/star/tags), `archetype.rs` (303), `preview.rs` (230 —
  `show_system_map_section` + view-click + bitmap preview), `bulk_ops.rs` (297
  — `show_bulk_ops` + the 5 `apply_bulk_*`), `regen.rs` (137). Pure mechanical
  move: cross-boundary privates raised to `pub(super)` only; `show` +
  `apply_bulk_*` kept `pub(crate)`; the external surface stays at
  `panels::system::` via `mod.rs` re-exports (consumers: `nav.rs` `show`,
  `map/context_menu.rs` `apply_bulk_{primary_faction,control_state,reseed}`,
  `map/dialogs.rs` `apply_bulk_rename`). `panels/mod.rs` unchanged
  (`pub mod system;` resolves to the dir). Large carve delegated to a
  `panel-implementer` subagent, then **re-verified in the main thread**.
  - **Two forced deviations (both validated here, safe):** (1)
    `show_system_map_section` is `pub(crate)` not `pub(super)` — Rust rejects a
    `pub(crate) use` re-export of a `pub(super)` item (E0364); the re-export
    (needed for the `system_map.rs` doc-link path) forces the wider visibility.
    Benign — it was module-private before and only doc-referenced. (2) The
    `apply_bulk_clear_factions` re-export was dropped: a `pub(crate) use` with no
    consumer trips `unused_imports` under `-D warnings`. **Verified zero external
    callers** (`grep` shows it only used inside `bulk_ops.rs`); the fn stays
    `pub(crate)` verbatim, so no live path breaks. The alternatives (`pub use`
    over-exposes; `#[allow]` is banned) were both correctly avoided.
  - **Verbatim proof:** byte-diffed `apply_coord_move` + `pretty_slug` against
    `git HEAD:system.rs` → **identical**; a holistic diff of every
    string/`format!` literal in the file showed the *only* change is one
    `pub(super)` prefix on a const (no logic/string/format touched).
  - **Verification:** builder **317/317**, `cargo clippy --workspace
    --all-targets -- -D warnings` clean, golden **15/15 byte-identical** (E7
    touches no `sectorforge` code, so output is provably unaffected). MAP.md +
    GUIDE.md link/structure refs repointed `system.rs` → `system/` module.

- **E4 (`world.rs` split, split-only)** — ✅ DONE. Split the 1608-LOC
  `builder/src/builder/panels/world.rs` into a `world/` directory module (7
  files, 1762 LOC; the +154 is per-file imports + the `pub(super)` prefix
  pushing four `show_*_section` signatures past 100 cols so rustfmt wrapped the
  param list — no body changed): `mod.rs` (386 — doc/imports, `show` +
  roster/inspector/header, the `EnumPicker` trait + 7 impls + `combo_enum` kept
  PRIVATE since child modules may read ancestor privates, and the tests),
  `identity.rs` (265), `environment.rs` (122 — environment + society),
  `features.rs` (321 — §W5 features + weights + coupling warnings),
  `factions.rs` (204 — presence + `INFLUENCE_TIERS`/`DOMINANCE_STATES`),
  `claims.rs` (197 — §W7 + `claim_chip_colours` + `CLAIM_TYPES`), `overlays.rs`
  (267 — control/overlays/chronicle/regen). Only `show` stays `pub(crate)`
  (sole external consumer: `nav.rs`); cross-boundary section fns raised to
  `pub(super)`; nothing newly `pub`. `panels/mod.rs` unchanged. Carve delegated
  to `panel-implementer`, **re-verified in main thread**: no `cargo fmt` leak
  (git shows only `world.rs`→`world/`), all **9 `format!("{…:?}")` sites
  byte-identical** (the deferred slug swap), an independent whitespace-normalised
  logic-line diff vs `git HEAD:world.rs` shows only added `use` lines (zero logic
  change), builder **317/317**, `clippy --workspace --all-targets -- -D warnings`
  clean, golden **15/15 byte-identical**. MAP.md + GUIDE.md repointed.

### 2026-06-05 — step 5, wave 3 (god-file split — F3)

- **F3 (`sector_view.rs` split, split-only)** — ✅ DONE. Split the 1711-LOC
  `gui-core/src/sector_view.rs` into a `sector_view/` directory module (4 files,
  1769 LOC; the +58 is per-file imports + the 20 `pub(super)` prefixes + module
  docs): `mod.rs` (19 — doc + `mod` decls + the 5 `pub use` re-exports that keep
  the public surface at `sector_view::`), `cache.rs` (123 — `SectorMapCache` +
  impl), `view.rs` (895 — the `SectorView` 27-field struct + `SectorClick` +
  the monolithic 810-LOC `show()`), `render.rs` (730 — `SectorGeom` + impl,
  `point_segment_distance`, `paint_system_rings`, all hex math / paint / label
  helpers, the 4 `*_MIN_VISIBLE_PX` consts, and the moved `#[cfg(test)]`
  geometry tests). **Verbatim carve** — slices taken at item boundaries via
  `sed` so bodies are byte-identical by construction; `view.rs`/`cache.rs`
  bodies **diff-clean vs `git HEAD`**, `render.rs` differs in **exactly 20
  lines**, each only a `fn X(` → `pub(super) fn X(` prefix (the helpers `show()`
  calls cross-module). The 4 internal-only helpers (`hex_center_xy`,
  `region_label_font_px`, `region_label_anchor`, `region_label_text`) stayed
  private; `SectorGeom`/`paint_system_rings` kept `pub`,
  `point_segment_distance` kept `pub(crate)`; nothing newly fully-`pub`. Submodule
  `use` blocks rewrote the former `super::{heatmap,map_theme,palette,…}` →
  `crate::…` (one level deeper now); two now-unused imports (`Arc`, `RouteId`,
  both used only fully-qualified in the struct) trimmed from `view.rs`.
  **F-S3 NOT done** (the 27-field `Default`/builder collapse + the `show()`
  body decomposition into `render_routes`/`render_systems`/… ) — parked behind
  an owner decision per the review's own note that body splits can alter the
  map snapshots. **Re-verified in main thread:** `cargo check --workspace
  --all-targets` clean (downstream builder/viewer resolve the preserved
  `sector_view::` re-exports), `cargo clippy --workspace --all-targets -- -D
  warnings` clean, **gui-core `map_snapshots_match_goldens` passes UN-blessed**
  (no render drift), gui-core lib **30/30** (incl the 5 relocated geometry
  tests under `sector_view::render::tests`), golden **15/15 byte-identical**,
  `sectorforge` lib **191/191**. MAP.md + GUIDE.md repointed (mod.rs for the
  whole-widget rows; render.rs for the flourish / font-px / hit_route /
  `paint_system_rings` item links).

### 2026-06-05 — step 5, wave 3 (god-file split — F8)

- **F8 (`info_panel.rs` split, by-section)** — ✅ DONE. Split the 1156-LOC
  `gui-core/src/info_panel.rs` into an `info_panel/` directory module (7 files,
  1315 LOC; the +159 is per-file imports + module docs + the 1 `pub(super)`
  raise). The review's literal fix (extract a pure `format.rs` of
  `route_summary_text`-style helpers) was **not** taken — the pure-formatting
  surface is tiny (`route_endpoint_label`, `short`, the two `event_mentions_*`
  predicates) and the rest of the formatting is fused into the render fns, so
  pulling it out would be a behaviour-restructure, not a verbatim move. Per the
  owner instruction ("only if it stays a pure move; otherwise split by
  section") it was carved **by entity section** instead: `overview.rs` (237 —
  `SectorOverviewCache` + `sector_overview*` + the 2 cache tests), `system.rs`
  (325 — `system_summary` / `star_detail` + the 5 per-system blocks),
  `route.rs` (113 — `route_summary` + `route_endpoint_label`), `world.rs` (196
  — `world_detail`), `subsector.rs` (183 — `subsector_summary`), `history.rs`
  (99 — `world_history` / `system_history` + `event_mentions_*`), `mod.rs` (162
  — module doc, the shared text primitives `title`/`section`/`body`/`dim`/`kv`/
  `short`, the legend rows, `stability_block`, `mod` decls, and the `pub use`
  re-exports). **Key visibility win:** the shared primitives stay as
  **parent-private** fns in `mod.rs` — child submodules read them via
  `use super::{…}` (ancestor privates are visible to descendants), so **zero**
  of them needed a `pub(super)` raise. The only raise is `system_history`
  (`history.rs`), the one helper called across a submodule boundary
  (`system::system_summary`). Entity-local helpers (the 5 system blocks,
  `route_endpoint_label`, `event_mentions_*`) moved with their sole caller and
  stayed private. **Verbatim carve** — every slice taken at a blank-line item
  boundary via `sed`; the 23-range partition reconstructs `[22-1156]` with no
  gaps/overlaps. **Re-verified in main thread:** all 7 bodies **byte-identical
  vs `git HEAD`** except `history.rs`'s single `fn system_history(` →
  `pub(super) fn system_history(` (proved by per-file diff). Unused imports from
  the broad shared import block were trimmed by `cargo fix --lib` (use-lines
  only; bodies re-diffed identical afterward). `info_panel` is **not** exercised
  by the `map_snapshots` suite (verified — no snapshot exposure), and a verbatim
  move can't change render output regardless. **Gate:** `cargo check` +
  `cargo clippy --workspace --all-targets -- -D warnings` clean (downstream
  builder/viewer resolve the preserved `info_panel::` re-exports, incl. the
  viewer's `info_panel::SectorOverviewCache`), gui-core lib **30/30** (incl the
  2 relocated `overview::tests`), `map_snapshots_match_goldens` passes
  **un-blessed**, golden **15/15 byte-identical**, `sectorforge` lib
  **191/191**. MAP.md + GUIDE.md repointed to `info_panel/mod.rs`.

### 2026-06-05 — step 5, wave 4 (AREA_F semantic-color sweep)

No verbatim god-file splits remain; moved to the AREA_F "safe warm-ups" per the
file's own suggested local order. All viewer chrome — **no golden / map-snapshot
exposure** (`viewer/` has no snapshot suite, and `gui-core` render paths were
untouched except F12's comment). One commit per finding, each gated on `cargo
check` + `clippy --workspace --all-targets -D warnings` clean, **golden 15/15
byte-identical**, viewer **7/7**, gui-core `map_snapshots_match_goldens` passing
**un-blessed**.

- **F6 (`542d14e`)** — `editor/dialogs.rs`. SaveAs error label
  `Color32::from_rgb(235, 90, 90)` → `crate::palette::danger()`.
- **F9 (`a577d79`)** — `editor/factions_panel.rs`. Deleted the local
  `palette_dim()` (hardcoded `(150,145,165)` = the dark-theme `TEXT_DIM`) and
  routed its one call site to `crate::palette::chrome_text_dim()`; dropped the
  now-unused `Color32` import.
- **F11 (`e918de2`)** — `factions_overview.rs`. The designer rendered both
  "saved &lt;path&gt;" and "save failed: &lt;e&gt;" in the same amber. Added a
  `status_is_error` flag with `set_status_ok`/`set_status_err` helpers wired
  through all 8 status sites, and colored the message `palette::success()` vs
  `palette::danger()`.
- **F12 (`c61d9cd`)** — `gui-core/src/palette.rs`. Comment-only: documented that
  `stability_color`'s amber/red are a **data-viz** palette (domain-lore route
  tier on the map canvas), intentionally **not** `warning()`/`danger()`, so the
  numeric overlap is never "fixed" into a theme-status coupling. No render change.
- **F5 (`362af3d`)** — the ~25-site sweep across 14 viewer chrome files. Swapped
  amber unsaved/warning/stress (`235,200,90` / `240,200,90` / `235,190,90` /
  `235,180,50`) → `palette::warning()`; red error/danger/severity (`235,90,90` /
  `180,80,80` / `Color32::RED`) → `palette::danger()`; positive OK/WINNER
  (`120,220,130` / `Color32::GREEN`) → `palette::success()`. **Behavioural** (not
  verbatim) — chrome colors now track the active theme; acceptable because no
  golden/snapshot covers these panels. **Intentional data-viz / background fills
  left untouched and annotated** (AREA_F F5): the region-kind hue (`220,160,60`)
  at `regions_view`/`sector_view`, the preview-banner / APPLY-PREVIEW
  call-to-action fills (`0,80,0` / `0,100,0`), the segmentum chrome fills, and the
  starless-system fallback dot (`140,140,150`). The two sites already converted by
  F6/F11 were excluded; 4 now-unused `Color32` imports removed. **Decision
  (owner-visible):** also folded the two semantic `Color32::{RED,GREEN}` *consts*
  (system-not-found error, wishes "WINNER" success) into the same sweep even
  though they are not `from_rgb` — they are the same theme-unaware status-color
  class and pair with amber siblings in the same widget.

- **F-S2 (umbrella) — ✅ DONE.** The MED color-audit parent of F5/F6/F9/F11 is
  now fully satisfied: every **semantic** amber/red/green site was routed to
  `palette::warning/danger/success` (F5 bulk + F6/F11/F9 singletons), and the
  audit's second half — "explicitly comment any **remaining** hardcoded RGBs as
  intentional data-viz" — is now complete. A re-grep found **8** surviving
  `Color32::from_rgb` in `viewer/`, all intentional: 5 already carried the
  `(AREA_F F5)` intent comment (region-kind hue ×2, preview/APPLY-PREVIEW green
  fills ×2, starless fallback dot); the **3 segmentum-browser chrome fills**
  (`segmentum_view.rs` active-tile `40,36,52` / active-card `42,38,52` /
  empty-slot `18,16,24`) were the only ones F5's log called "annotated" but had
  **no inline marker** — added now, matching the existing comment style. Zero
  semantic status colors remain unconverted. Comment-only viewer change → no
  golden / map-snapshot exposure; `cargo check -p sectorforge-viewer` clean.

### 2026-06-05 — step 5, wave 5 (AREA_F hot-path cache — F4)

- **F4 (`9519b2a`)** — `app/planner_view.rs` + `editor/map_panel.rs` +
  `editor/state.rs`. Both maps built `SectorView { cache: None, .. }`, forcing
  the gui-core render down the O(regions·hexes) hex→region fallback scan per
  visible hex per frame (~1280 iters/frame on a 20-region 8×8, on every mouse
  move). Fixes:
  - **planner** — threaded the App's already-maintained `sector_map_cache`
    (rebuilt on load/edit alongside `app.sector`) into the SectorView. One-liner.
  - **editor** — added a transient `EditorState.map_cache: Option<SectorMapCache>`,
    built lazily in `show_map` and invalidated to `None` on every sector change
    via `set_sector` / `mark_dirty`. **Audited all editor sector-mutation paths**
    (factions/routes/system/world/settings/dialogs/map/generation/wishes panels)
    — every one routes through `set_sector` or `mark_dirty`, so the cache rides
    the **same dirty signal** the App→editor sync already depends on; no stale
    window is introduced that didn't already exist for the sync. Editor passes
    `subsectors: None`, so the cache is built with `&[]`.
  - **Render-equivalence (why goldens stay green):** the cache's
    `hex_region`/`hex_system`/centroid/label tables are built from the *same*
    `sector.regions`/`systems` the fallback scans — pure memoization, identical
    output. The gui-core `map_snapshots` golden was already blessed via the
    cache path (`render.rs` builds a `SectorMapCache`), so it passes **un-blessed**
    after the change. Transient view state, not document state — stored directly
    on `EditorState` (no command bus in the viewer; carve-out analogous to the
    builder's transient-UI-state rule).
  - **Verification:** `cargo check` + `clippy --workspace --all-targets -D
    warnings` clean, viewer **7/7**, gui-core `map_snapshots_match_goldens`
    **un-blessed pass**, golden **15/15 byte-identical**. No file moved, so
    MAP.md/GUIDE.md untouched.
  - **Not done (separate findings):** F10 (memoize `centers`/star-dust into the
    cache — gui-core render-path, owner-gated) and the `App`/editor stack
    unification F1/F-S1 the cache duplication ultimately stems from.

### 2026-06-05 — step 5, wave 6 (AREA_F cross-crate widget dedup — F2)

- **F2 (`enum_combo` dedup)** — ✅ DONE. The structurally-identical `enum_combo`
  in `viewer/src/data_editor.rs:287` (`F: Fn(&T) -> String`, no tooltips) and
  `builder/src/builder/panels/worlds_editor.rs:363` (`F: Fn(&T) -> &'static str`,
  `T: Debug`, **two** extra hovers: the `—` sentinel "Any — leave this field
  unset" + per-variant `format!("key: {v:?}")`) collapsed onto one shared
  `pub fn enum_combo` in `gui-core/src/widgets.rs`. **Cross-crate, sequential**
  (gui-core ← builder/viewer): added the widget, converted viewer, then builder,
  `cargo check` after each.
  - **Signature decision** — the shared widget takes `label_of: Fn(&T) -> W where
    W: Into<egui::WidgetText>` (covers **both** the `String` and `&'static str`
    label closures the review flagged), plus a `hover_of: Fn(&T) -> Option<String>`
    and a `none_hover: Option<&str>` so each caller supplies its **own** tooltip
    policy. The widget carries **no `Debug` bound** — the builder's `{v:?}` key
    lives in its forwarder's closure, so the bound stays at the call boundary,
    not in gui-core.
  - **Builder UX preserved (not silently dropped)** — per the review's "the
    builder's extra Debug hover is a behavioural superset … do NOT silently change
    builder UX", both builder hovers are kept verbatim via its forwarder
    (`|v| Some(format!("key: {v:?}"))` + `Some("Any — leave this field unset")`).
    The viewer forwarder passes `|_| None, None` → byte-for-byte its old
    no-tooltip behaviour.
  - **Minimal-diff shape** — each panel keeps its original `fn enum_combo`
    signature as a **3-line forwarder**, so all **18 call sites (9+9) are
    untouched**. Viewer reaches the widget via a new `widgets` entry on the
    `pub use sectorforge_gui_core::{…}` re-export (matches the existing
    `crate::ui_kit` idiom); builder via `use sectorforge_gui_core::{…, widgets}`.
  - **Scope** — `enum_combo` only, as instructed. The sibling
    `builder/.../map/theme.rs:617` `enum_combo<E: Copy>` is a **different shape**
    (non-`Option`, `Copy` enum) — left alone. The larger `edit_rows`/worlds-grid
    widget dedup the review also mentions stays a separate follow-up.
  - **No clippy-ban impact** — gui-core has no `clippy.toml`; `enum_combo` uses
    `ComboBox`/`selectable_label`/`on_hover_text`, none of the
    `Painter`/`Shape`/`Mesh` primitives the builder/viewer `clippy.toml` ban.
  - **Verification:** `cargo clippy --workspace --all-targets -- -D warnings`
    clean; gui-core **31/31** (new `enum_combo_headless` test exercising both
    hover policies, +1 over the prior 30) with `map_snapshots_match_goldens`
    passing **un-blessed** (form widget, not the map render — confirmed no
    exposure); viewer **7/7**; builder **317/317**; golden **15/15
    byte-identical**. MAP.md updated (widgets.rs row); no file moved.

### 2026-06-05 — step 5, wave 7 (AREA_F stack unification — F1/F-S1, increment 1)

F1/F-S1 (the two parallel viewer editing stacks — `App.sector`+`live_dirty`
vs `EditorState.sector`+`editor.dirty`, bridged every frame at `app/mod.rs:193`
and reverse-synced at `lifecycle.rs:224` / `app/sector_view.rs:658`) is the
dominant viewer hazard and a **large cascade with no golden/snapshot net**
(runtime write paths). Doing it as **compiler-checked increments**, each
committed, rather than one blind sweep — per the proportionate-refactor
preference.

- **Increment 1 — `empty_*` ctors → `sectorforge` (part c).** ✅ DONE. The
  review's "extract the `empty_*` constructors into `sectorforge` (pure domain
  logic, not UI)" half is the safe, self-contained foundation and is independent
  of the dual-stack coupling. Moved `empty_sector` / `empty_system` /
  `empty_world` / `empty_route` / `empty_faction` **verbatim** out of
  `viewer/src/editor/state.rs` into a new `src/model/sector_model/scaffold.rs`
  (`pub fn`, re-exported at `sector_model::`). These are blank-DTO constructors
  with no RNG and no UI dep — distinct from the sibling `mutation.rs` (mutate an
  *existing* sector under the invariant contract); `scaffold` only *constructs* a
  fresh internally-consistent blank. The 8 call sites + 3 panel `use` lines
  re-pointed from `editor::state::` / `super::state::` to
  `sectorforge::sector_model::`; `state.rs` trimmed its now-unused
  sector_model/ids imports and the test module imports `empty_sector` from the
  crate. Two `super::{editor}` module imports (`app/system_view.rs`,
  `app/sector_view.rs`) dropped (used only for the moved ctors).
  - **Golden-safe:** these ctors are viewer-only — no generation/export path
    calls them — so adding the `pub fn`s changes no existing code path. Verified:
    workspace clippy `-D warnings` clean, `sectorforge` lib **191/191**, golden
    **15/15 byte-identical**, viewer **7/7**, gui-core **31/31** +
    `map_snapshots_match_goldens` **un-blessed**.
  - **Increment 2 done below (parts a/b).** MAP.md updated (scaffold.rs row).

- **Increment 2 — unify on `EditorState` as the single source of truth (parts
  a/b).** ✅ DONE — closes **F-S1 + F1** (same hazard, two rows). Owner-chosen
  "review-literal" shape.
  - **`App.sector` demoted to a derived read snapshot.** It is now an `Arc`
    cache rebuilt *only* by the frame bridge (or seeded on explicit load) and
    never mutated in place — so the ~15 App-level read sites keep reading
    `self.sector` unchanged (cheap `Arc` clone into render), but it is no longer
    an independent write target.
  - **All edits funnel through `editor.sector`.** The 6 App live-edit ops
    (`add_system_at` / `remove_selected_system` / `add_route_between` /
    `remove_selected_route` in `app/sector_view.rs`; `add_planet_to_system` /
    `remove_planet_from_system` in `app/system_view.rs`) now mutate
    `editor.sector` directly (owned — no `Arc::make_mut`) and finalize through a
    slimmed `mark_live_sector_dirty` that reindexes IDs on `editor.sector`,
    follows the rename through the selection, and calls `editor.mark_dirty()`.
    The old per-op reverse-sync (`if !editor.dirty { editor.set_sector(...) }`)
    and the duplicated cache/subsector rebuild are **gone** — the bridge owns
    that now.
  - **One dirty flag.** `App.live_dirty` removed entirely; its 2 readers
    (`app/system_view.rs`, `app/sector_view.rs` "unsaved" indicators) read
    `editor.dirty`. The title `*` marker already used `editor.dirty`.
  - **Saves serialize the source of truth.** `write_sector_to_path` (App rfd
    save) now serializes `editor.sector` and updates `editor.{dirty,loaded_from}`
    on success (dropping its `Arc::make_mut(App.sector)` + the post-save
    reverse-sync); `save_sector_as`'s guard reads `editor.sector`. The editor
    SaveAs (`dialogs.rs` → `save_project_sector`) already serialized
    `editor.sector`, so **both save entry points now read one store and cannot
    diverge.**
    **Decision (owner-visible):** kept the *two* save functions (arbitrary-path
    vs project-named — different UX) rather than forcibly merging them into "one
    write fn" as the review literally suggested; unifying the **store** removes
    the divergence hazard, and merging the two UX paths would be a behaviour
    change. Revisit if a single entry point is wanted.
  - **Bridge is revision-gated (perf).** Added `EditorState.revision` (bumped by
    `mark_dirty` + `set_sector`) and `App.synced_revision`; the bridge
    (extracted to `App::sync_derived_sector`, called once per `update`) re-derives
    only when the revision advances — so an idle unsaved sector is no longer
    deep-cloned + cache-rebuilt every frame (the old `if editor.dirty` bridge
    did; that cost would have spread to App-live-edit-then-idle states once they
    started leaving `editor.dirty` set).
  - **Automated net.** Extracting the bridge made it unit-testable: new
    `app::tests::editor_sector_is_single_source_of_truth` (viewer 7→**8**)
    asserts load seeds the snapshot + leaves the bridge in sync, a SoT mutation
    bumps the revision without touching the snapshot, `sync_derived_sector`
    re-derives it, and an idle re-sync does **not** rebuild the `Arc`. **Still
    recommended:** an interactive smoke (`cargo run -p sectorforge-viewer` →
    add/remove a system + route in the map, Save, reload) — the button→op→render
    UI wiring has no headless coverage.
  - **Verification:** workspace clippy `-D warnings` clean; viewer **8/8**;
    golden **15/15 byte-identical**; gui-core **31/31** +
    `map_snapshots_match_goldens` **un-blessed**.
  - **Not done (separate findings):** **F7** (the two map-edit code paths —
    App `sector_view.rs` vs editor `map_panel.rs` — still duplicate the
    drag/add-route distance logic; both now write `editor.sector`, but the
    dedup is F7's job) and **F10** (render-path memoization, owner-gated).

### 2026-06-05 — step 5, wave 8 (AREA_F route-distance dedup — F7)

- **F7 (`recompute_route_distances` dedup)** — ✅ DONE. The "find both endpoint
  coords → `hex_distance` → write `route.distance`" pattern was hand-rolled in
  **four** viewer map-edit sites: editor `map_panel.rs` drag-move (per touched
  route), add-route, and route-pick; and App `app/sector_view.rs`
  `add_route_between`. Extracted a single `GeneratedSector::recompute_route_distances()`
  method in `src/model/sector_model/mutation.rs` (recomputes every route from its
  endpoints' current coords; a route with a missing endpoint keeps its distance —
  same policy as the existing `move_system` per-route refresh it generalizes) and
  routed all four sites through it.
  - **Scope decision (proportionate, golden-safe):** added the helper but did
    **not** refactor the existing `move_system` / `swap_systems` /
    `swap_route_endpoints` / `add_route` ops (which bake in their own narrower
    targeted refresh) to delegate to it — those are builder-facing + golden-tested,
    and changing them risks output drift for zero viewer benefit. Their targeted
    unit tests (`move_system_updates_route_distance`, etc.) stay meaningful. The
    internal `move_system`→`recompute_route_distances` delegation is a separate,
    optional follow-up. Also did **not** adopt the full `MutationApi`
    (`add_route`/`move_system` with `MutationError`) in the viewer — that is a
    larger per-op migration beyond F7's distance-dedup scope.
  - **Behaviour notes (owner-visible):** (1) the App `add_route_between` dropped
    its App-only "route endpoint missing" early-return guard (the editor path
    never had it; endpoints are always existing picked systems, so it was dead
    defensive code) — now aligned with the editor path. (2) The reindex divergence
    the review flagged (App live-edit calls `reindex_ids`; the editor does **not**)
    is **documented, not unified** (per the review's "document or unify") with a
    note at the editor's edit-finalize: the editor deliberately keeps IDs stable
    under the user during a session. Unifying either direction is a behaviour
    change outside F7.
  - **Verification:** new lib test
    `mutation::tests::recompute_route_distances_refreshes_all_from_coords`
    (sectorforge lib 191→**192**); workspace clippy `-D warnings` clean; viewer
    **8/8**; golden **15/15 byte-identical** (no existing mutation op changed);
    gui-core **31/31** + `map_snapshots_match_goldens` **un-blessed**. MAP.md
    updated (mutation.rs row).

### 2026-06-05 — step 5, wave 9 (AREA_F render-path memo — F10)

- **F10 (star-dust memo)** — ✅ DONE (star-dust half; centers half declined with
  analysis). `gui-core/src/sector_view/{cache,render,view}.rs`.
  - **Star-dust memoized.** `paint_star_dust` split into a pure
    `build_star_dust(rect) -> Vec<Shape>` + a memoizing painter. Added
    `StarDust { key, shapes }` + `SectorMapCache.star_dust: RefCell<Option<StarDust>>`
    (interior-mutable because the render path holds the cache by shared ref —
    `SectorView::cache: Option<&_>`, and `show(self)` can't take `&mut`). The field
    is a pure function of the rect, so it rebuilds only when the rounded rect
    `(min.x, min.y, w, h)` changes instead of re-running the per-frame hash loop;
    the painter just re-`extend`s the stored shapes.
  - **Byte-identical → NO re-bless.** `build_star_dust` pushes the exact same
    `Shape::circle_filled`s in the same order the inline `painter.circle_filled`
    loop produced, so the paint list / tessellation is unchanged.
    `map_snapshots_match_goldens` passes **un-blessed** — the pre-approved
    `UPDATE_MAP_SNAPSHOTS` re-bless was **not needed** (no golden churn).
  - **`centers` half deliberately NOT done.** The per-frame `centers`
    `HashMap<&str, Pos2>` (view.rs) is a function of the live view transform
    (`hex_size` + pan `origin`) **and** the dynamic `drag_override`, so it cannot
    be cached across frames — every pan/zoom/drag changes it. The review itself
    flags "only the static (non-drag) centers can be cached", but even those move
    with the view transform, so a cross-frame cache would invalidate constantly
    for no gain. Left as-is, documented.
  - **Honest benefit note:** the saving is the per-frame hash loop only — the
    shape tessellation/paint cost (the bulk) is unchanged in egui's immediate
    mode. Marginal but real; bounded, byte-identical, no snapshot risk.
  - **Verification:** workspace clippy `-D warnings` clean;
    `map_snapshots_match_goldens` **un-blessed pass**; gui-core lib **31/31**;
    golden **15/15 byte-identical** (star-dust is live-only — export goldens never
    reach it). MAP.md updated (cache.rs row).

### 2026-06-05 — step 5, wave 10 (AREA_F SectorView ctor — F-S3) · AREA F COMPLETE

- **F-S3 (`SectorView::new` + struct-update)** — ✅ DONE.
  `gui-core/src/sector_view/view.rs`. `SectorView` is a 27-field `pub` struct with
  no `Default`, so every call site had to spell all 27 fields and any new field
  cascaded into all of them. Added `pub fn SectorView::new(sector: &GeneratedSector)
  -> Self` filling `sector` + every other field at its neutral default
  (None/false, `hex_size 40`, `origin ZERO`, `Sense::hover()`,
  `RouteViewMode::default()`). Chose a **constructor over `impl Default`** because
  `sector: &'a GeneratedSector` is a required reference — `Default` would need a
  placeholder `&'static GeneratedSector`; `new(sector)` keeps `sector` mandatory
  and needs no global placeholder.
  - **All 5 call sites converted** to `SectorView { <overrides>, ..SectorView::new(sector) }`:
    viewer `app/sector_view.rs`, `app/planner_view.rs`, `editor/map_panel.rs`;
    builder `panels/map/interactions.rs`; the gui-core `map_snapshots` test. A new
    field added to `SectorView` now defaults through `..new(sector)` instead of
    breaking every caller.
  - **Byte-identical (safe conversion):** at each site only fields whose literal
    value **is** the new default (`None` / `false` / `Sense::hover()` / `Pos2::ZERO`
    / dropped `sector`) were removed; every non-default field stays explicit. So
    the constructed value is unchanged. Proven by `map_snapshots_match_goldens`
    passing **un-blessed** (the snapshot site is one of the five). Dropped the now
    unused `Sense` import from the snapshot test.
  - **`show()` body decomposition still parked** (the *other* half the review
    mentioned — `render_routes`/`render_systems`/… extraction): that one is
    genuinely snapshot-behaviour-sensitive and large; left for an owner decision.
  - **Verification:** workspace clippy `-D warnings` clean;
    `map_snapshots_match_goldens` **un-blessed pass**; gui-core lib **31/31**;
    viewer **8/8**; builder **317/317**; golden **15/15 byte-identical**. MAP.md
    note added.

- **AREA F is now fully closed (15/15).** The remaining whole-review backlog is
  AREA A/B/C/E/G + the owner-gated API-shape items (A5, D-S3/D5, C2, the trait/
  macro dedups, and the F-S3 `show()` decomposition).

### 2026-06-05 — step 5, wave 11 (AREA_B perf — B1)

Back to AREA_B after AREA_F closed. Resuming the file's own suggested local
order at the first remaining perf item (B1; B7/B4-adjacent/B9/B12 already done).

- **B1 (`f78119b`) — supply-risk edge pre-bucketing.** ✅ DONE.
  `src/analysis/economy/{derive,risk}.rs`. `system_supply_risk` filtered the
  full `dependency_edges` slice (`e.to_system_id == sy.system_id && e.resource
  == resource`) for **every** (system, world, resource) triple — O(S·W·R·E) over
  the whole economy derivation. Built the index **once** at the call site in
  `derive_with`: `BTreeMap<(to_system_id, resource), Vec<&DependencyEdge>>`
  (one pass over the owned `dependency_edges`, borrowed — NLL drops the index
  before the `Vec` is moved into `EconomyReport`), and swapped the inner
  `.filter().collect()` for an O(1) `incoming_by_target.get(&(sy.system_id
  .as_str(), *resource))` lookup. **Byte-identical:** the classifier's two uses
  of the bucket are `is_empty()` (presence) and `iter().map(|e| e.risk).min()`
  (order-independent), so the `SupplyRisk` tier is unchanged regardless of
  bucket insertion order. Signature change is private (`pub(super)`), no public
  surface touched. **Verification:** clippy `-D warnings` clean, lib **192/192**,
  golden **15/15 byte-identical**, economy integration **7/7**. Pure perf — no
  file moved, MAP.md/GUIDE.md untouched.

- **B6 (`445b385`) — cooccurrence map keyed on faction indices.** ✅ DONE.
  `src/analysis/relations/{derive,tension}.rs`. The tension co-occurrence
  accumulator (`BTreeMap<(String, String), CooccurStats>`) allocated two
  `String`s per pair-event in `build_cooccurrence`'s walk **and** two more per
  `.get()` in `tension_of` / `build_relation` — ~30k allocs per derivation at 60
  factions, quadratic at the 1000-faction catalogue. Built a faction-id → index
  `FxMap<&str, u32>` once in `derive_with_threshold` and re-keyed the map on
  `(u32, u32)` via a new `canonical_pair_idx` (branch+swap, zero allocs);
  `bump_cooccur` resolves indices and **skips catalogue-absent ids** (those
  entries were never read — the lookup path only queries `sector.factions` ids —
  so dropping them is observably identical). Lookups go through a shared
  `cooccur_stats` helper. **Golden-safe rationale:** the map is **lookup-only**
  (grep-verified: only `.get()`, never `.iter()/.keys()/.values()`), so the
  integer key type changes no emission order; the output `pairs` Vec is sorted by
  tension, not map key. The id-string `canonical_pair` is **kept** for its other
  two roles (pair-ordering / override matching and the public `stance_between`).
  No external/test caller of the four touched fns (all internal). **Verification:**
  clippy `-D warnings` clean, lib **192/192**, golden **15/15 byte-identical**,
  relations integration **6/6**. Pure perf — MAP.md/GUIDE.md untouched.

- **B5 (`2b8603c`) — field-wise vector ops driven off a field-list helper.**
  ✅ DONE. `src/analysis/economy/{config,derive}.rs` + `src/analysis/control.rs`.
  `StrategicOutput` (add_assign/scale/clamp_scores, 10 fields), `ResourceVector`
  (scale + the `add` free fn, 6 fields), and `PresenceDimensions`
  (scale/clamp/add_dimensions, 10 fields) each hand-unrolled the same per-field
  arithmetic across 3–4 functions. Added a `fields_mut()` + `fields()` accessor
  per struct (`pub(super)` for `ResourceVector` since `add` lives in derive.rs;
  free `dimension_fields*` helpers for the model-owned `PresenceDimensions`) and
  rewrote each op as a loop. **Bit-identical:** every op is independent per field,
  so iteration order is irrelevant — golden proves it. The one non-uniform field,
  `PresenceDimensions::scale`'s `visibility *= k.max(0.3)` floor, stays explicit
  (captured pre-loop, re-applied after the linear loop overwrites it). The
  `weighted_priority_score` mul_add chain (**B10**, golden-risk) left untouched.
  **Verification:** clippy `-D warnings` clean, lib **192/192**, golden **15/15
  byte-identical**, economy integration **7/7**. No file moved — MAP.md/GUIDE.md
  untouched.

- **B3 (`e5c4be1`) — kind/disposition rules indexed off the pair loop.** ✅ DONE.
  `src/analysis/relations/derive.rs` (+ a lib test in `relations/mod.rs`).
  `compute_pair` rescanned `cfg.kind_rules` + `cfg.disposition_rules` per pair —
  O(F²·R). Added a `RuleIndex` built once in `derive_with_threshold`: intern the
  kind/disposition strings appearing in rules and key small `BTreeMap`s on the
  canonical `(u32, u32)` pair (reusing B6's `canonical_pair_idx`), so each pair
  does two lookups instead of two scans. **Semantics preserved exactly:**
  kind = first-match-wins (`entry().or_insert` keeps the first rule per canonical
  key); disposition = sum-all + cause-concat in cfg order (a `Vec` per key in push
  order); a string absent from every rule is never interned, and a missing index
  entry is precisely the scan's "no match" (a kind/disposition matching no rule
  cannot satisfy a match) → the built-in fallbacks (`default_kind_stance` /
  `default_disposition_delta`) fire unchanged. **Net gap closed:** goldens only
  exercise the *fallback* path (no fixture sets user rules), so added
  `user_rules_index_preserves_first_match_and_sum_order` asserting
  `cause == "KFIRST; D1; D2"` — proves first-kind-rule-wins + both disposition
  causes appended in order, independent of the seed-derived stance perturbation.
  **Verification:** clippy `-D warnings` clean, lib **193/193** (+1), golden
  **15/15 byte-identical**, relations integration **6/6**. Pure perf —
  MAP.md/GUIDE.md untouched.
  - **AREA_B perf bucket (B1/B3/B5/B6) now fully closed.**

- **B4 (`33569c4`) — generic `cap_per_anchor` extracted.** ✅ DONE.
  `src/analysis/{mod,hooks,missions}.rs`. The byte-identical `cap_per_anchor`
  bodies in hooks.rs + missions.rs (differing only in element type + how the
  anchor key was computed) collapsed onto one `cap_per_anchor<T: WeightedAnchored>`
  in `analysis/mod.rs` (beside `cmp_f32_desc`), behind a 3-method
  `WeightedAnchored { weight, id, anchor_key }` trait impl'd for `Hook` (inline
  anchor match) and `MissionSeed` (delegates to the existing free `anchor_key`).
  **Byte-identical:** same sort (weight desc, id asc) + same per-anchor retain;
  the `id()`→`as_str()` tiebreak matches the old `HookId`/`MissionId` `cmp`
  because `define_id!` derives `Ord` over the inner `Arc<str>`. **Verification:**
  clippy `-D warnings` clean, lib **193/193**, golden **15/15 byte-identical**
  (sector.md carries both reports), hooks integration **6/6**.
  - **B-S2 merge-half deliberately NOT done — owner decision (see notes).**

### 2026-06-05 — step 5, wave 12 (AREA_E builder panels — warm-ups)

AREA_B done (perf bucket + B4; B-S2 closed-as-designed; B-S1/B8/B10 deferred).
Owner picked AREA_E next. Leading with the file's S-effort safe warm-ups —
builder-only, **no sectorforge emission / no golden / no map-snapshot exposure**
(none touch the gui-core render path). Gate per commit: `cargo clippy --workspace
--all-targets -D warnings` clean + builder lib **317/317**.

- **E6 (`a9c82d0`) — shared `SYSTEM_STATES` const.** ✅ DONE. The byte-identical
  `const SYSTEM_STATES: &[SystemState]` in `panels/control.rs` + `panels/history.rs`
  hoisted to one `pub(crate) const` in `panels/mod.rs`; both read it via
  `super::SYSTEM_STATES`. `SystemState` import stays live in each file (their
  label/key/parse match fns still use it). _Not folded in:_ `system_state_label`
  is **also** duplicated in both files, but it's not part of E6's finding — left
  alone (no scope creep). Builder **317/317**, clippy clean.

- **E9 (`009ac5f`) — stop cloning `chronicle.events` per frame.** ✅ DONE.
  `panels/history.rs` cloned the full `Vec<HistoryEvent>` (Arc-heavy) **twice**
  per frame. (1) Event-list grid: iterate `&state.sector.chronicle.events`
  directly — the loop only mutates the **disjoint** `selected_history_event`
  field, so NLL allows it. (2) Timeline: the clone was **load-bearing** (the
  review's "remove both" under-analyzed this) — the row body calls
  `state.focus_entity(..)` + `focus_anchor(state, i)`, both whole-`&mut state`.
  Restructured to an index loop reading each event's display fields under a
  **scoped borrow that ends before** those calls, so only the shown fields are
  cloned, not the whole Vec (`focus_anchor` already re-reads by index). Builder
  **317/317**, clippy clean.

- **E13 (`7ede957`) — `mark_catalog_dirty` helper.** ✅ DONE. Seven catalog
  panels' `on_catalog_edited` hand-rolled the `state.dirty = true; if let
  Some(rel) = config.inputs.X.clone() { insert(rel) } else { insert(DEFAULT_X
  .into()) }` fallback. Added `BuilderState::mark_catalog_dirty(Option<String>,
  &str)` in `state/derivations.rs` (beside `mark_validation_dirty`; both set
  transient save-tracking state, **off** the command bus — not document state, so
  no §R4 bus violation) and collapsed all seven to one-liners (personae,
  relations, missions, hooks, prose, history, sites). Two-phase borrow permits
  `state.mark_catalog_dirty(state.config.inputs.X.clone(), …)`. _Count note:_ the
  review's "6 panels / 12 inserts" counted raw `dirty_files.insert` calls; the
  actual Some/else-default shape is **7** panels — all converted. The other
  `dirty_files.insert` sites (factions/routes/regions/files/theme/worlds_editor)
  are a different shape and were left alone. Builder **317/317**, clippy clean.

- **E14 (`e384e83`) — shared `claim_chip_colours`.** ✅ DONE. The byte-identical
  13-arm `claim_chip_colours` match in `control.rs` + `world/claims.rs` hoisted
  into a **new** `panels/presence_widgets.rs` (`pub(crate) fn`); both call it,
  private copies deleted (`world/claims.rs` dropped its now-unused `Color32`
  import). This **seeds the `presence_widgets` module** the review earmarks for
  **E5** (the duplicated `show_add_presence_row`). Colours kept hardcoded —
  annotated as an intentional data-viz palette (lore claim tiers). Builder
  **317/317**, clippy clean. MAP.md updated (new file row).

- **E8 (`route_component_count`) — ✅ DONE as NON-ISSUE (`*` on roll-up).** The
  review's "union-find twice per frame" premise does not hold, so it is
  reclassified MED → non-issue and closed with **no code change**. Site 1
  (`show_summary`, routes.rs:75) runs every frame — **one** union-find. Site 2
  (`show_ensure_connected`, :1279) is inside `ui_kit::collapsing_section(.., false,
  ..)` whose `CollapsingHeader::show` body runs **only when expanded**
  (default-collapsed), so it is not a per-frame cost; and when it does run it is
  immediately followed by `ensure_connected_routes(state, routes.clone())` (:1280),
  a heavier clone+connect pass that **dominates** the union-find, and it must stay
  **live** (it follows a checkbox handler that can mutate routes the same frame —
  hoisting to the top of `show` would make it a frame stale). No per-frame
  redundancy exists to remove. (If profiling on a very large R≈500+ sector ever
  shows the cheap idle-frame site-1 union-find matters, memoize site 1 only, keyed
  on an existing derivation fingerprint — but that is speculative, not warranted
  now.) AREA_E file row + section updated to 🟢 Non-issue.

### 2026-06-05 — step 5, wave 11 (AREA_E E-S3 — edit_world/edit_system helpers)

- **E-S3 (`edit_world` / `edit_system`) — ✅ DONE (partial-by-design).** Added two
  helpers on `BuilderState` in `state/generation_ops.rs` (beside
  `find_world_indices`): `edit_world(WorldId, impl FnOnce(&mut GeneratedWorld))
  -> Result<(), BuilderError>` and `edit_system(SystemId, impl FnOnce(&mut
  GeneratedSystem)) -> …`. Each looks the entity up by id, clones it, runs the
  closure on the clone, and dispatches `EditWorld`/`EditSystem { before: None,
  after }` through `self.run`. A stale id maps to
  `MutationError::{WorldNotFound,SystemNotFound}` — byte-identical to what
  `run(EditX)` itself returns. This **wraps** the command bus (§R4-safe), not a
  bypass.
  - **Pre-checks (both passed, gating the design):** (1) **every** one of the 16
    `EditWorld` + 10 `EditSystem` dispatch sites already passes `before: None`
    (grep-verified) — the bus captures the prior payload on `apply`, so no site
    needed the old snapshot; standardizing on `before: None` in the helper is
    safe. (2) The modal text **differs** across sites ("Edit failed" ×6 control /
    "World edit failed" ×8 / "System edit failed" ×5 / "Intel edit failed" /
    "Control flip failed" / "Control update failed" / "Duplicate world failed"),
    so the helper does **not** bake a fixed string — it **returns** the error and
    each caller keeps its exact `ModalKind::Message`.
  - **Converted 16 of 26** sites (10 `EditWorld` + 6 `EditSystem`) to one-liners:
    control.rs CTL-2/3a/3b/5a/5b, world/features, world/identity tags+notes,
    world/factions ×2, world/claims ×2, system/identity kind+tags+notes,
    system/mod control-flip. (CTL-5a's `if i < draft.claims.len()` guard moved to
    read the live world — equivalent, since `draft` was a fresh clone.)
  - **Left 10 sites hand-written + noted (genuinely divergent, not clean
    clone→mutate→dispatch):** the WORLD-tab **classification / environment /
    society** editors + both **INTEL** editors build the draft *across* egui
    render closures (mutation interleaved with the UI, dispatch gated on a
    `changed`/`dirty` flag) — they'd need a redundant double-clone to fit;
    **system_map duplicate-world** grafts a *different* source world's full
    payload (not a mutation of the target); the **3 bulk_ops loops** carry a
    no-op-skip filter fused into `find().filter().cloned()` (lifting it would
    change the shape); **control.rs CTL-1** reads the **edited** removed
    presence's `faction_id` to update the transient `dominance_locked` side-table,
    which can't move out of the draft closure (an edit + the removal can hit the
    same index). All 10 retain their original behaviour and modal text.
  - **Verification:** workspace `clippy --all-targets -- -D warnings` clean;
    builder lib **317 → 319** (added `edit_world_round_trip` +
    `edit_system_round_trip` in `state/tests.rs`: one undoable command per call,
    undo restores the pre-edit payload, stale id → `…NotFound`). Builder-only
    change — no `sectorforge`/`gui-core` source touched, so golden + map snapshots
    are unaffected (not run). Three now-unused `BuilderCommand` imports removed
    (world/claims, world/factions, world/features — all their EditWorld sites
    converted). No file moved → MAP.md untouched; GUIDE.md §R4 detail-editor note
    extended.

### 2026-06-05 — step 5, wave 13 (AREA_E E-S2 — proportionate roster dedup) · AREA_E effectively complete

- **E-S2 (master-detail shell) — ✅ DONE (proportionate, partial-by-design).**
  Investigated all five named panels first: the review's premise **does not
  hold**. RELATIONS is a matrix/cell editor (no roster); ECONOMY's override
  editors key off the **shared** `selected_world_id`/`selected_system_id` (not a
  catalog-specific roster); the three true roster panels (MISSIONS/PERSONAE/HOOKS)
  render their lists with panel-specific `card::selectable_plate` rails sourced
  from a **derived report** (not the raw catalog); and there is **no add-row
  scratch buffer** anywhere — rows are appended blank via
  `cfg.manual.push(blank_manual_*(len))`. So the proposed `roster_detail<T>` and
  `add_row_scratch<T: Default>` helpers had **no real consumers**, and the named
  `ui.data_mut`-temp-vs-`BuilderState`-field "scratch lifetime" decision was moot
  for these panels (that scratch lives in the *control/world* presence/claim
  add-rows — different panels, partly handled by E5/E14).
  - **Owner decision (AskUserQuestion):** chose **proportionate micro-dedup** over
    the full generic shell / defer / close. Extracted the **two** genuinely
    byte-identical idioms into a new **`panels/roster.rs`** (`pub(crate)`):
    - `detail_target(edit_target, selected_id) -> Option<String>` — the
      `edit_target.clone().or_else(|| selected_id.clone())` detail-target
      resolution. Used by MISSIONS + HOOKS `show_detail_card`.
    - `id_edit_field<I: Display + From<String>>(ui, &mut I) -> bool` — the
      `let mut id_buf = x.id.to_string(); if changed { x.id = id_buf.into() }`
      inline-rename. Generic over the `define_id!` newtypes (`MissionId`/`HookId`),
      using the same `Display`+`From<String>` the hand-written sites relied on.
      Used by MISSIONS + HOOKS manual editors.
  - **Divergence left hand-written (noted):** PERSONAE's detail keys on
    `selected_persona_id` only (no edit-target `or_else`), and its id field
    interleaves a `scroll_to_me(edit_target)` focus — neither fits the shared
    helpers. RELATIONS/ECONOMY out of shape entirely. No generic roster shell built
    (no consumers).
  - **Location deviation (documented):** placed at `panels/roster.rs`, **not** the
    review's suggested `builder/src/builder/ui/` — matches the E14
    `panels/presence_widgets.rs` precedent for shared panel helpers, avoids a
    near-empty new top-level module dir, and dodges an `ui` module-name vs the
    ubiquitous `ui` variable collision.
  - **Behaviour-identical:** `detail_target` is the same `clone().or_else`;
    `id_edit_field` does the same `to_string`/`changed`/`into`, returning the bool
    each caller folds into its `changed` flag. (The id mutation stays on the
    documented catalog off-bus carve-out — these edit `data_catalogs.*.manual`.)
  - **Verification:** `cargo check -p sectorforge-builder --all-targets` clean;
    `cargo clippy --workspace --all-targets -- -D warnings` clean; builder lib
    **319/319**. Builder-only — golden + map snapshots unaffected (not run). MAP.md
    `roster.rs` row added.
  - **AREA_E is now effectively complete: 16/17 resolved; E11 deferred to its own
    session via [E11.md](E11.md).**

### 2026-06-05 — step 5 (AREA_E E11 — RESOLVED: verbatim `map/context_menu/` dir-module split) · AREA_E COMPLETE 17/17

- **E11 (`context_menu.rs` god-file) — ✅ DONE (Option A, split-only).** Ran the
  dedicated [E11.md](E11.md) playbook. The 1162-LOC
  `builder/src/builder/panels/map/context_menu.rs` is now the
  `map/context_menu/` **dir module**:
  - `mod.rs` (107 L) — module doc, `mod`/re-export glue, `show_sector_context_menu`.
  - `resolve.rs` (123 L) — `resolve_sector_context` + the two pure predicates +
    `menu_anchor_pivot` (the netted pure half).
  - `action.rs` (480 L) — `SectorMenuAction`/`OpenInTarget`,
    `sector_menu_action_label`, `apply_sector_menu_action` (the netted dispatch core).
  - `render.rs` (521 L) — the five `render_*` builders + `stability_label`
    (interactive, un-netted).
- **Table-drive (Option B) declined** — recorded rationale below (next entry).
  Table-driving doesn't make `SectorMenuAction` "more" the SSOT than it already
  is (label + dispatch are already centralised), the 10+ heterogeneous item
  shapes degrade a faithful `MenuItem` into a variant DSL ~as complex as today's
  imperative code, and the builders have no headless net. The split is the
  proportionate fix that addresses the real half of the finding (the file is
  itself a god-file) at zero behaviour risk.
- **Verbatim proof.** Bodies were sliced from `HEAD` at item boundaries (`sed`),
  so every fn/enum body is byte-identical. A trimmed/sorted multiset diff of the
  old file vs `cat context_menu/*.rs` shows **only** `mod`/`use`/import/visibility-
  prefix/module-doc lines as differences — **no** body / string / `format!` /
  match-arm line appears as removed-only.
- **Visibility plan.** Cross-boundary items declared
  `pub(in crate::builder::panels::map)` and re-exported `pub(super)` from
  `context_menu/mod.rs` (equal-visibility → no E0364). `menu_anchor_pivot` stays
  `pub(in crate::builder::panels)` because `panels::system_map` consumes it via
  the `map/mod.rs:27` re-export. The five `render_*` widened private→`pub(super)`
  (called by `show_sector_context_menu` in `mod.rs`). The action facade re-export
  is gated `#[cfg(test)]` — its only out-of-module consumer is the `map/mod.rs`
  `#[cfg(test)]` round-trip tests (the `render::*` builders reach `action::*` via
  the internal path), so gating it avoids an unused-import warning in the
  non-test lib.
- **Gate.** `cargo clippy --workspace --all-targets -- -D warnings` clean;
  `cargo test -p sectorforge-builder --lib` green at **319/319** (the preserved
  re-exports keep the `map/mod.rs` context-menu tests resolving); the 4 new files
  pass `rustfmt --check`. **Golden + map-snapshot suites not run** (builder-only;
  no `sectorforge`/`gui-core` source, no render-path/emit change). Docs:
  `docs/MAP.md` row → 4 rows; `GUIDE.md` path-link repointed;
  `AREA_E_builder_panels.md` E11 → ✅ Resolved. One commit on `main`.
- **AREA_E is now COMPLETE: 17/17 resolved, 0 deferred, 0 pending.**

### 2026-06-05 — step 5, wave 13 (AREA_E E11 — assessed + deferred to a dedicated session)

- **E11 (`context_menu.rs` table-drive) — ⏳ DEFERRED (owner call).** Assessed the
  review's literal fix (table-drive the menus behind `MenuItem`) and judged the
  **full table-drive not worth the churn**: the action⇄label⇄effect mapping is
  already centralised (`sector_menu_action_label` + `apply_sector_menu_action`),
  the `render_*` builders own genuine per-menu *composition* (not boilerplate),
  the items span 10+ heterogeneous shapes (clipboard items that bypass the action
  enum, nested `▸` submenus, an inline DELETE-ALL confirm flow, dynamic `•`
  current-markers, `on_disabled_hover_text`, mode-gated collapse, …) so a faithful
  `MenuItem` degrades into a variant DSL ~as complex as today's code, and the
  interactive builders have **no headless net** (= un-netted behaviour risk).
  - **Net nuance:** the *pure* half — `resolve_sector_context`,
    `apply_sector_menu_action`, `sector_menu_action_label`, the two predicates — **is**
    unit-tested in `map/mod.rs`; only the `render_*` builders are un-netted.
  - **Owner decision:** rather than do the risky table-drive (or unilaterally
    substitute a safe split), wrote a dedicated, in-depth agentic playbook —
    [E11.md](E11.md) — for a separate session. It documents the current file
    inventory (@ `e58ba01`), the test net, the 10+ item shapes, **Option A** (the
    recommended verbatim `map/context_menu/` dir-module split — same playbook as
    E7/E4/E10, zero behaviour risk, addresses the 1162-LOC god-file), **Option B**
    (the table-drive done as safely as possible, incl. the DSL actually required
    and a mandatory interactive-smoke protocol), the visibility/import bookkeeping,
    verification methodology, and ready-to-use subagent prompts. E11 stays
    **pending** until that session runs.

### 2026-06-05 — step 5, wave 13 (AREA_E E10 — filter_bar + control/ dir module)

- **E10 (`filter_bar` + `control/claims.rs`) — ✅ DONE.** Two parts.
  - **(a) `filter_bar` helper.** The §CL1 per-world claims list hand-rolled the
    `Id`-keyed `ui.data_mut` get → `TextEdit` → store-on-change → re-read-from-temp
    filter cycle. Extracted `fn filter_bar(ui, salt, hint) -> String` (in
    `control/claims.rs`) that does the get/render/store and **returns** the current
    value; `show_world_list` now captures that return and drops the redundant
    second `data_mut` re-read. **Behaviour-identical:** `filter_bar` stores the
    freshly-typed value before returning it, so the returned string equals exactly
    what the old store-then-re-read produced (proved by diff: only the filter-cycle
    lines changed; the rows-build / scroll-area / `only_contested` handling are
    untouched). The `only_contested` checkbox keeps its original inline get/store +
    re-read (out of scope; left exactly as-is).
  - **(b) `control.rs` → `control/` dir module.** `git mv control.rs
    control/mod.rs` and carved the §CL1/§CL2 claims block —
    `show_world_list` (now `pub(super)`, called by the parent `show`), plus the
    private `show_world_row` + `show_add_claim_row` + the new `filter_bar` — into a
    new `control/claims.rs`. §CL3 (`contested_worlds`/`show_contested_summary`) and
    §CL4 (`show_bulk_convert`/`count_bulk_matches`/`apply_bulk_convert`) **stay in
    `mod.rs`** (not named by the finding; moving them would widen the visibility
    surface for no gain). The §C presence/system/power editors stay in `mod.rs`.
  - **Verbatim carve (proved by byte-diff vs `git HEAD`):** `show_world_row` +
    `show_add_claim_row` are byte-identical except the **one** repath
    `super::presence_widgets::claim_chip_colours` → `claim_chip_colours` (now a
    `use crate::builder::panels::presence_widgets::claim_chip_colours`).
    `claim_label` / `CLAIM_TYPES` stay private in `mod.rs` and are read by the child
    via `use super::{…}` (ancestor privates are visible to descendants — **no
    `pub(super)` raise needed**). The only other `mod.rs` change: `FactionClaim`
    moved from the module-level `use` (its sole non-test consumer
    `show_add_claim_row` left) into the `#[cfg(test)]` module's import, killing a
    real `unused_imports` warning.
  - **Visibility audit:** no external caller of the three claims fns (grep — the
    `factions.rs::show_filter_bar` and `world/claims.rs::show_add_claim_row`
    namesakes are unrelated). `panels/mod.rs`'s `pub mod control;` resolves to the
    dir unchanged; `show` / `build_overlay_cells` keep their `pub(crate)` surface.
  - **Verification:** `cargo check -p sectorforge-builder --all-targets` clean;
    `cargo clippy --workspace --all-targets -- -D warnings` clean; builder lib
    **319/319** (control's relocated tests still run). Builder-only — golden + map
    snapshots unaffected (not run). MAP.md repointed `control.rs` → `control/mod.rs`
    + added the `control/claims.rs` row.

### 2026-06-05 — step 5, wave 13 (AREA_E E12 — search.rs::show split)

- **E12 (`search.rs::show` split) — ✅ DONE.** The ~307-line `pub fn show`
  mixed §SR4 settings + §SR1 constraint editor + §SR5 preflight + §SR2 run/cancel/
  progress in one body. Carved the four sections into helpers mirroring the
  already-extracted `show_outcome`: `show_search_settings(ui, state, project_seed)`
  (§SR4), `show_constraint_list(ui, state, factions)` (§SR1),
  `preflight_unknown_ids(state, known) -> Vec<String>` (§SR5),
  `show_run_controls(ui, state, preflight_unknown, budget_hint)` (§SR2). `show` is
  now the gather-deps-then-orchestrate entry point.
  - **Verbatim, behaviour-identical (proved by a trimmed-line multiset diff vs
    `git HEAD`):** every widget call / string literal / `format!` / id-salt /
    hover text appears unchanged in both versions. The only deltas are the
    mechanical ones the extraction forces: `budget_hint` is now read via
    `state.search.wishes.as_ref().map_or(1, |w| w.search.budget.max(1))` **before**
    `show_search_settings` (same pre-§SR4-edit value the original captured at the
    top of the `{ wishes }` block); `constraint_editor(.., &factions)` →
    `(.., factions)` (param is now `&[String]`); `Some(project_seed.clone())` →
    `to_owned()` (param is now `&str`); the shared `{ let wishes = …as_mut() }`
    scope split into a per-helper `as_mut().unwrap()` (safe — `show` returns early
    when `wishes` is `None`); the §SR5 loop guarded by `if let Some(wishes) =
    …as_ref()`. No render-order or logic change.
  - **The disjoint-field borrow** (the §SR1 closure mutating `wishes.constraints`
    while also reading/writing `state.search.new_constraint_kind`) holds inside the
    extracted fn exactly as it did inline — edition-2021 disjoint closure captures.
  - **Verification:** `cargo check -p sectorforge-builder --all-targets` clean;
    `cargo clippy -p sectorforge-builder --all-targets -- -D warnings` clean;
    builder lib **319/319**. Builder-only, no `sectorforge`/`gui-core` source — golden +
    map snapshots unaffected (not run). No file moved → MAP.md/GUIDE.md untouched.

### 2026-06-05 — step 5, wave 13 (AREA_E E5 — presence candidate dedup)

- **E5 (`show_add_presence_row`) — ✅ DONE (partial-by-design).** Audited the two
  `show_add_presence_row` editors **before** merging (per the finding's "audit for
  scroll-area-id / section-header / faction-gathering divergence") — they are
  **not** byte-identical duplicates:
  - **CONTROL** (`control.rs`): signature takes `world_id` + a pre-gathered
    `factions` slice; `Buf { faction, tier }` (no dominance); `horizontal` layout;
    influence picker via `influence_label`/`influence_help`; faction combo carries
    an `id: {fid}` hover; id salts `c2_*`; modal "Edit failed".
  - **WORLD** (`world/factions.rs`): signature `(sys_idx, w_idx)` and gathers
    `factions` from `state.sector.factions` internally; `Buf { faction, tier,
    dominance }` with an **extra dominance combo** (`w_add_dom`); `horizontal_wrapped`;
    tier/dominance labels via `Display`; no faction-id hover; id salts `w_add_*`;
    modal "World edit failed".
  A full merge would **change one tab's UX** (the WORLD row's dominance combo is a
  real feature, the influence-help tooltips are CONTROL-only), so the divergence
  is **left in place and noted** — not forced into one parameterised monster.
  - **Extracted only the shared, UX-neutral piece:** `presence_candidates(world,
    factions) -> PresenceCandidates::{NoFactions, AllPresent, Available(Vec<&…>)}`
    in the E14-seeded `panels/presence_widgets.rs`. It folds the byte-identical
    candidate computation (filter the faction list down to those not already
    present on the world); each caller maps the three variants to its **own**
    placeholder strings and renders its **own** picker. `claim_chip_colours`
    (E14) already covers the chip-colour half of the finding.
  - **Behaviour-identical:** same filter source/order; the returned `Vec` borrows
    the caller's `factions` (not `state`), so the later `state.edit_world` mutable
    borrow is unaffected — same shape as before. No `BTreeSet`/`WorldId`/`FactionId`
    import went unused (all have other live uses; clippy `-D warnings` confirms).
  - **Verification:** `cargo check -p sectorforge-builder --all-targets` clean;
    `cargo clippy --workspace --all-targets -- -D warnings` clean; builder lib
    **319/319**. Builder-only — no `sectorforge`/`gui-core` source touched, so
    golden + map snapshots are unaffected (not run). MAP.md presence_widgets row
    updated; AREA_E E5 marked Resolved.

### 2026-06-05 — step 5, wave 14 (AREA_A model/generation — S-effort warm-ups)

Owner picked AREA_A next. Led with the file's suggested local order (A8→A10→A6→
A9→A11→A12→A4→A3) — all S-effort, proportionate, compiler-checked. A11 is the one
golden-gated item (feeds `seed_hash`/`settings_digest`). Verified together:
`cargo clippy -p sectorforge --all-targets -- -D warnings` clean, lib **193→194**,
**golden 15/15 byte-identical**. Single commit on `main`.

- **A8** — `#[must_use]` added to `get_system`/`get_system_mut`/`get_world`/
  `get_worlds_for_system` in `sector_model/mod.rs`. **Not** on `all_worlds` — it
  returns `impl Iterator` (already `#[must_use]`); clippy `double_must_use` rejects
  the redundant attribute, so the review's 5th target was correctly dropped.
- **A10** — both `.unwrap()` in `hidden_routes.rs:455–456` → `.expect("hidden-route
  pair endpoint not in index — invariant: pairs are built from endpoints only")`.
  Behaviour-identical; context on the (unreachable) panic.
- **A6** — the 4 `crate::GENERATOR_{NAME,VERSION}.to_string().into()` double-allocs
  in `gen/generation/mod.rs` (`GeneratedSector` build + `build_manifest`) → single
  `std::sync::Arc::from(…)`; the 5th site (`"not recorded by default".to_string()
  .into()`) folded in. Same `Arc<str>` value, one fewer alloc each.
- **A9** — the 4 `(*self.regions).clone()` + `Arc::new` region edits in
  `mutation.rs` (`add_region`/`remove_region`/`add_region_hex`/`remove_region_hex`)
  → `Arc::make_mut(&mut self.regions)`. Elides the deep `Vec<WarpRegion>` copy when
  refcount==1 (the builder-session common case); clones-on-write otherwise — output
  identical (content, not Arc identity, is serialized).
- **A11** — `rng::hex` swapped `s.push_str(&format!("{b:02x}"))` for `write!(&mut s,
  "{b:02x}")` (`use std::fmt::Write`). Kills the per-byte temp `String` (32/hash).
  **Byte-identical lowercase hex** — proved by golden 15/15 (it feeds
  `seed_hash`/`settings_digest` in `sector.json`).
- **A12** — extracted `GenerationManifest::empty(project_id, seed)` (single source
  for the builder new-sector defaults: zero counts, empty digests, `"unknown"`
  sentinel) and pointed `GeneratedSector::empty`'s inline manifest block at it. The
  generation-pipeline `build_manifest` keeps its **distinct** `"not recorded by
  default"` sentinel (different semantic state — left intentionally divergent, now
  the only other construction site). No golden exposure (empty manifest is
  builder-only).
- **A4** — added exhaustive `parse_tables_cover_all_variants` test in
  `taxonomy.rs`: iterates `VARIANTS` for all four enums and asserts the parse table
  round-trips every variant. Closes the silent-`None`-on-new-variant gap for
  `StarColour`/`Government`/`NotableFeature` (the prior single test covered only one
  `WorldType` variant). `StarColour::Display` is the short code, so its row pairs
  `star_colour_variant_name` with the parser; the other three use the `{:?}`
  variant-name `Display`/`AsRef`. (lib 193→**194**.)
- **A3** — `gen/generation/routes.rs` route-weight blocks no longer hardcode
  `"feature:trade_hub"`-style literals. Added `fn feature_tag(&NotableFeature)`
  mirroring `world_placement::compute_tags`' `format!("feature:{}", snake(f.as_ref
  ()))`, and built the boost/penalty `BTreeSet<String>` **once before** the O(n²)
  pair loop from the `NotableFeature` variants. A renamed variant now updates the
  producer and these comparisons together (no silent weight drift). Tags are
  byte-identical to the old literals (`as_ref()` = variant name, `to_snake_case`
  matches), so route weights — and goldens — are unchanged.
- **Not done (this wave):** **A2** (WorldDto real-enum + serde shim, M) and **A7**
  (BTreeMap accessor index, M) — next. **A5** (157-pub-field visibility, L) stays
  ⏸️ owner-gated like D-S3/D5 (wide builder/viewer/tests cascade, not a clean
  split). No file moved → MAP.md/GUIDE.md untouched.

### 2026-06-05 — step 5, wave 15 (AREA_A A2 — WorldDto real-enum refactor, via workflow)

Owner chose **do A2 via a workflow** + **A7 maintained index** (AskUserQuestion).
A2 first. The DTO `WorldDto` (`<GeneratedWorld>.world`) held 10 stringly-typed
`Arc<str>` fields; replaced with the 9 real `worlds.rs` enums so a renamed
variant is a compile error in every consumer instead of a silent
string-comparison mismatch (the finding's core). **JSON byte-stable** via a
serde shim. ~150 sites across all 4 crates.

- **Foundation (main thread, golden-critical).** `sector_model/mod.rs`: `WorldDto`
  now holds `StarColour/WorldType/Atmosphere/Temperature/Biosphere/Population/
  TechLevel/Government` + `Vec<NotableFeature>`, with `#[serde(into = "WorldDtoRaw",
  try_from = "WorldDtoRaw")]`. The private `WorldDtoRaw` is the **original wire
  schema** (incl. the `star_colour` `short_name()` / `star_colour_code` `code()`
  split, which collapses into one `StarColour` in memory) so `sector.json` is
  unchanged byte-for-byte. `From<WorldDto> for WorldDtoRaw` mirrors the old
  `From<&World>` serialize derivations exactly; `TryFrom<WorldDtoRaw>` parses back —
  star_colour from the **code** (`StarColour: FromStr` parses "O"/"B"/…),
  world_type/government/notable_feature via the `taxonomy::parse_*_variant` fns,
  and atmosphere/temperature/biosphere/population/tech_level via a new
  `find_by_display(VARIANTS, s)` helper. **Why not `FromStr` for those 5:** their
  hand-written `FromStr` accepts the *spaced* display form ("Densely Populated"),
  but the wire string is the `{:?}` variant name ("DenselyPopulated") — caught by
  the new round-trip unit test, which initially failed `invalid population:
  "DenselyPopulated"`. `find_by_display` matches against each enum's `Display`,
  the exact inverse of the serialize side. `From<&World>` is now a field clone.
  Added `world_dto_serde_round_trips_through_wire_shim` (lib 194→**195**).
- **Site sweep (workflow `wf_17b26311-9f1`, 39 agents, ~147 sites).** One agent per
  file applied the conversion rules: literal compares / `match` against
  variant-name strings → **enum compares / matches** (the rename-safe win, incl.
  exhaustive `match` dropping now-unreachable `_ =>` arms in `control.rs`);
  `.to_string()` / `.short_name()` / `.code()` only where a string is genuinely
  consumed (display, `format!`, `Arc<str>` map keys); construction literals →
  variants (dropping the `star_colour_code` field). Agents edited blind (tree
  mid-refactor); the compiler was the oracle in the straggler pass.
- **Straggler fixes (main thread).** (1) Anomalous **test fixtures** carried
  non-variant garbage strings (`Population "Massive"`, `TechLevel "Imperial"`,
  `Government "Imperial"/"ImperialCommander"`, `Biosphere "Standard"`,
  `star_colour "amber"/"Y"`) — mapped to real variants
  (ExtremelyDense/High/MilitaryGovernor/Thriving/…); these fixtures are not
  asserted on, tests stay green. (2) **`combo_enum`/`EnumPicker`** in
  `builder/.../world/mod.rs` was the one genuine **cross-file API redesign** the
  sweep couldn't do in isolation: re-signed `combo_enum<E>(.., &mut E)` (was
  `&mut Arc<str>`) operating on the enum directly, dropped the now-dead
  `debug_key` round-trip + the `Arc` import. (3) `subsectors/summary.rs`
  `any_capital_like` re-signed to `Item = &'a str` (the chained tags `&Arc<str>`
  + `notable_features` `&NotableFeature` no longer share a type; both `.as_ref()`
  to `&str`). (4) one clippy `iter().any(==)` → `contains` in `orbital_assets.rs`.
- **Latent pre-existing bug left as-is (behaviour-preserving):**
  `summary.rs::score_world_as_capital` `matches!` tests government against
  "Republic"/"Corporate"/"Imperial"/"Federation"/"Theocracy" — **none are real
  `Government` variants**, so that +3 capital bonus was dead before and stays
  dead (kept a string compare; not A2's scope to change behaviour).
- **Verification:** `cargo check --workspace --all-targets` clean; `cargo clippy
  --workspace --all-targets -- -D warnings` clean; **golden 15/15 byte-identical**
  (the serde shim proof); lib **195/195**, `it` **93/93**, builder **319/319**,
  viewer **8/8**, gui-core **31/31** + `map_snapshots_match_goldens` un-blessed.
  All 37 changed `.rs` files rustfmt-clean. No file moved → MAP.md untouched.
- **Next:** A7 (maintained BTreeMap index — owner-chosen full fix). A5 still
  ⏸️ owner-gated.

### 2026-06-05 — step 5, wave 16 (AREA_A A7 — accessor index) · AREA_A effectively complete

- **A7 (`get_system`/`get_world` O(n) scans) — ✅ DONE (field-free helper, owner
  call).** The review's literal fix (a maintained index *field* on
  `GeneratedSector`) was put to the owner via AskUserQuestion after I surfaced its
  real cost: a `#[serde(skip)]` field forces `lookup: Default::default(),` into
  **~30 struct-literal sites** (mostly test fixtures) for a LOW-sev item, and any
  on-struct cache risks staleness-on-load (serde-skip → empty after deserialize).
  **Owner chose the field-free helper.**
  - Added `GeneratedSector::build_system_index() -> BTreeMap<SystemId, usize>` and
    `build_world_index() -> BTreeMap<WorldId, (usize, usize)>` (both `#[must_use]`).
    A repeated-lookup caller builds the map **once**, turning an O(n)-per-lookup
    scan into one O(n) build + O(log n) lookups. The map is caller-owned (dropped
    after the loop), so it can never go stale against a mutation and nothing new is
    serialized — sidesteps both hazards of the on-struct design. `get_system`/
    `get_world` keep their scan for one-off use.
  - **Wired through the two genuine repeated-lookup sites** (`export/subsectors/mod.rs`):
    `assign_hex_grid`'s `seed_ids → coord` map and the subsector-skeleton `cells`
    loop both did `seed_ids.iter().map(|id| sector.get_system(id))` (O(seeds·systems));
    now build the index once and index into `sector.systems`. **Same systems →
    byte-identical output** (export is golden-tested).
  - Added `build_indices_resolve_to_the_same_entries_as_scans` (lib 195→**196**).
  - **Verification:** clippy `-D warnings` clean; **golden 15/15 byte-identical**;
    lib **196/196**, `it` **93/93**. (Also fixed a stray import-order nit in
    `scaffold.rs` from the A2 sweep.) No file moved → MAP.md untouched.

- **AREA_A is now effectively complete: 11/12 resolved; only A5 (157-pub-field
  visibility tightening, L) stays ⏸️ owner-gated** alongside D-S3/D5 — a wide
  builder/viewer/tests cascade, not a clean split, explicitly deferrable per the
  review's own sequencing.

### Open decisions / notes
- **B-S2 `merge_manual` alignment — RESOLVED (closed-as-designed, owner call
  2026-06-05).** The cap dedup half (B4) is done. The remaining half is a policy
  divergence: **hooks** (`hooks.rs:176`) dedupes derived entries against manual
  ids, appends the manual block, **then** caps; **missions** (`missions.rs:197`)
  caps **first**, drops `gm_only`, **then** appends manual uncapped and without
  id-dedup. **Owner decision: leave divergent** — manual missions are
  author-curated and intentionally bypass the per-anchor cap; aligning would be a
  behavioural change to `missions.md` / `sector.json` for no functional gain. The
  `*` on the roll-up marks B-S2 as resolved via B4 + this intentional-divergence
  ruling, not a code unification. No further action.
- **B-S1 (`SectorReport` trait, 7 modules) — OWNER-GATED, L / defer.** The
  review itself sequences it last ("defer until other dedup is stable") and notes
  the `render_markdown` signatures vary (some take `cfg`, some don't), needing a
  `Render` associated type or split trait method, plus asymmetric config loading
  (only economy + relations expose `load_*_file`). That is a structural trait
  rewrite across 7 modules — outside the "proportionate, compiler-checked"
  lane and the "no trait rewrites unless trivially clean" preference. Recommend
  an explicit owner go/no-go before attempting; not started.
- **B8 / B10 — LOW, intentionally deferred.** B8 (`insert_top_n` O(top) scan) is
  the right fix for the wrong problem size (`report_top` defaults to 5) — defer
  unless it grows past ~20. B10 (the 9-deep `mul_add` chain in
  `weighted_priority_score`) is a **golden-risk** readability item: a naive dot
  product changes the FMA associativity and breaks byte-stability; left as-is
  (the B5 field-list pass deliberately did **not** touch it).
- **E4 part a (`NotableFeature::as_slug()` swap) — PARKED, behaviour-sensitive.**
  The review's other half of E4 — replacing the 9 `format!("{v:?}")` / `{self:?}`
  key sites in `world/` with a stable `as_slug()` — is **not** done. Those
  Debug-repr strings are load-bearing keys (the feature-weight lookup + the
  `EnumPicker::debug_key` storage keys); a slug whose value differs from the Rust
  variant name would change them. Left for an owner decision (like A5 / D-S3·D5),
  per "don't swap behaviour without asking."
- **Commit cadence:** ~~accumulate~~ → **all step-1–4 work committed & merged via
  PR #3 (`2b274ea`).** Landed as `7a06824` (AREA_D), `56b587b` (E1/E2/E3),
  `5055f3a` (G2), `1b01f28` (C1), `688a378` (B-S3), `7c446bf` (this tracker).
  Working tree clean. The prior "uncommitted/fold into a larger commit" note is
  superseded.
- **G2 file size** (~2.1 MB) — committed as full-file pins in `5055f3a`; revisit
  only if repo leanness later wins over diff-ability.
