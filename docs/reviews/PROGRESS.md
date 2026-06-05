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
| A `src/model` + generation | 12 | 1 (A1) | 0 | 11 | 0 |
| B `src/analysis` | 14 | 6 (B-S3,B1,B7,B9,B11,B12) | 0 | 8 | 0 |
| C export/validate/worlds/cli | 13 | 4 (C1,C-S2,C3,C6) | 0 | 9 | 0 |
| D builder command + state | 14 | 12 | 0 | 0 | 2 (D-S3/D5) |
| E builder panels | 17 | 6 (E1,E2,E3,E4,E7,E-S1) | 0 | 11 | 0 |
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
   - Remaining dedup: AREA_B perf (**B1 ✅**, B3/B5/B6), trait/macro dedup
     (B-S1/B-S2, E-S3, C2, F-S1); **C3 ✅** (this session).
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

### Open decisions / notes
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
