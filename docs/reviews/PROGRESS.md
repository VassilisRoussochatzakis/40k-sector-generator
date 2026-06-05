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
| B `src/analysis` | 14 | 5 (B-S3,B7,B9,B11,B12) | 0 | 9 | 0 |
| C export/validate/worlds/cli | 13 | 4 (C1,C-S2,C3,C6) | 0 | 9 | 0 |
| D builder command + state | 14 | 12 | 0 | 0 | 2 (D-S3/D5) |
| E builder panels | 17 | 5 (E1,E2,E3,E7,E-S1) | 0 | 12 | 0 |
| F viewer + gui-core | 15 | 0 | 0 | 15 | 0 |
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
     B11 ✅ · A1 ✅ · C6 ✅ · E7 ✅. Remaining splits: E4 (split-only),
     F3/F8 (D3/D5 deferred).
   - Remaining dedup: AREA_B perf (B1/B3/B5/B6), trait/macro dedup
     (B-S1/B-S2, E-S3, C2, F-S1); **C3 ✅** (this session).

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

### Open decisions / notes
- **Commit cadence:** ~~accumulate~~ → **all step-1–4 work committed & merged via
  PR #3 (`2b274ea`).** Landed as `7a06824` (AREA_D), `56b587b` (E1/E2/E3),
  `5055f3a` (G2), `1b01f28` (C1), `688a378` (B-S3), `7c446bf` (this tracker).
  Working tree clean. The prior "uncommitted/fold into a larger commit" note is
  superseded.
- **G2 file size** (~2.1 MB) — committed as full-file pins in `5055f3a`; revisit
  only if repo leanness later wins over diff-ability.
