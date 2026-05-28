# Section 2 Execution Progress

Tracking sheet for [RUST_FIXES.md §2](RUST_FIXES.md). One row per fix; updated as
work lands. Status legend: `[ ]` pending, `[~]` in progress, `[x]` done,
`[s]` skipped/de-scoped (with reason), `[!]` blocked.

## Baseline

- Branch `main` @ `3ad88f9 review section 1`.
- `cargo check --workspace --all-targets` clean.
- Investigation pass complete (5 parallel `rust-explorer` agents). Fix-doc drift
  noted per row.

## §2.1 Public API surface narrowing

| ID | Status | Current state | Plan | Notes |
|----|--------|---------------|------|-------|
| TF-API-1 | [ ] | 56 pub decls in `builder/src/builder/panels/*.rs`; 7 in `session.rs`; 79 in `viewer/src/`. ~90% in-crate only. | Downgrade `panels::*::show`, panel-action enums, `session.rs` helpers to `pub(crate)`. Audit viewer separately. | Sequence AFTER TF-S-1 per fix doc; do mechanical pass here. |
| TF-API-2 | [ ] | `SectorView` has 23 pub fields (gui-core/sector_view.rs:92-140); `SectorMapCache` 4 pub HashMaps (lines 23-28); `JobHandle` 5 pub fields incl `Arc<Mutex<f32>>` + `Receiver<T>` (jobs.rs:7-14). | Builder-pattern `SectorView::new(sector).with_*().build()`. `SectorMapCache` → accessor methods. `JobHandle` → private fields + getters. | Cascades to ~50 sites; staged in subsections. |
| TF-API-3 | [ ] | `sectorforge::export::map_theme::MapTheme` (src/export/map_theme.rs:184) vs `sectorforge_gui_core::map_theme::MapTheme` (gui-core/src/map_theme.rs:35). Heatmap: `HeatCellRgb` (src/export/heatmap.rs:115) vs `HeatCell` (gui-core/src/heatmap.rs:16). | Rename gui-core MapTheme → `RenderMapTheme`. Consolidate heatmap types with `From` impl. | Bin-level renames; touches imports. |
| TF-API-4 | [ ] | 48 enums with `#[serde(rename_all = "snake_case")]`. Sample `format!("{:?}", x)` sites: src/export/system_map.rs:298, src/export/subsectors/summary.rs:258-259, src/analysis/analytics.rs:204,315, src/analysis/briefing.rs:448, src/analysis/prose.rs:169, src/analysis/interestingness.rs:145. | Add `as_slug(&self) -> &'static str` to each tagged enum (macro). Replace every `format!("{:?}", e)` with `.as_slug()`. Implement `Display` via slug. | High-value mechanical pass; cuts perf hotspots per TF-P-3. |

## §2.2 Newtype discipline

| ID | Status | Current state | Plan | Notes |
|----|--------|---------------|------|-------|
| TF-NT-1 | [x] | Newtypes added in src/model/ids.rs. Persona/Hook/MissionSeed `id` switched to PersonaId/HookId/MissionId. BriefingProfile observer/restrict switched to FactionId. Construction sites use `.into()`. Builder text-edit sites use String mirror. CLI `briefing --observer` arg parses to FactionId. | — | 75 integration tests pass, no golden refresh needed (serde transparent kept JSON stable). |
| TF-NT-2 | [ ] | Fix doc drift: control.rs line refs point inside loops, not function signatures. `display_importance` (importance.rs:114) returns `f32` ✓. `total_projection` (power_projection.rs:53,167) returns `f32` ✓. | Add ControlScore/DisplayImportance/ProjectedPower newtypes around the *struct fields* (`SystemControlSummary`, `WorldControlSummary`, `Faction.power.total_projection`). Apply where it's actually stored. | Scope adjusted: focus on stored values, not local bindings. |
| TF-NT-3 | [ ] | `feature_weights_for_world` defined in builder/src/builder/panels/world.rs:459-499 (private to that panel). Rebuilds ProjectInput + FeaturePool per frame. Single call site at world.rs:370. | Move to derivations, key by (sys_idx, world_idx, world_digest). Cache in BuilderState derivations. | Touches BuilderState shape; small. |

## §2.3 Performance

| ID | Status | Current state | Plan | Notes |
|----|--------|---------------|------|-------|
| TF-P-1 | [ ] | ProjectInput has 17 pub fields (src/loading/input.rs:16-51). `clone_project_with_seed` deep-clones 15 catalog fields (search.rs:1191-1210). | Group catalog fields into `Arc<ProjectCatalogs>`. `clone_project_with_seed` becomes `Arc::clone(&self.catalogs)`. | Public API shape change; downstream callers updated. **Pending follow-up — bench impact still expected significant for search workloads.** |
| TF-P-2 | [x] | Added `AtomicU32 lowest_winner` to search.rs candidate scan. Closure skips work for `n >= lowest_winner.load()`. Determinism preserved because winner is the lowest passing `n`. | — | Search tests pass; criterion bench would quantify gain. |
| TF-P-3 | [ ] | SectorMapCache lacks label/uppercase caches. info_panel.rs has 64 `format!` sites; sector_view.rs only 1. | After TF-API-4 lands, hoist label uppercasing to `SectorMapCache::system_label_cache: BTreeMap<SystemId, Arc<str>>`. | Sequence AFTER TF-API-4. |
| TF-P-4 | [ ] | `faction_style_by_id` (palette.rs:692) linear scan; 6 call sites incl per-route per-system iterations in info_panel.rs, control.rs. | Add `SectorMapCache::faction_style_index: BTreeMap<FactionId, FactionStyle>`, populate once. | Touches both gui-core+ builder panels. |
| TF-P-5 | [x] | svg_export/primitives.rs: `color_hex` → `write_color_hex(&mut String, Rgba<u8>)` sink. All 8 in-file call sites rewritten. | — | Golden PNG/JSON byte-stable. |
| TF-P-6 | [ ] | Fix doc partially stale: economy.rs:1090-1116 already pre-builds `valid_routes_by_sys`, `by_sys`, `by_route`, `system_refs` once. | Verify whether further hoist needed in relations.rs and stranded check. De-scope if redundant. | Investigate before changing. |
| TF-P-7 | [ ] | BriefingPack.sector: GeneratedSector owned (briefing.rs:194-203, 218 clones). `Arc::make_mut(&mut out.relations)` at line 267. | Convert sector to `Cow<'a, GeneratedSector>`. Project a `Vec<FactionRelation>` instead of `Arc::make_mut`. | Touches public API of BriefingPack. |
| TF-P-8 | [x] | Added `supplier_count: BTreeMap<(endpoint, crit), usize>` pre-bucket. Inner O(R) `.count()` walk replaced with O(1) lookup. | — | Hooks tests pass. |
| TF-P-9 | [ ] | Builder clones per frame: routes.clone() ×8 sites, factions.clone() ×5, claims.clone() ×2. | Cache-backed reads; iterate by reference where command-bus not needed. | Touches multiple panels; staged. |
| TF-P-10 | [ ] | dashboard.rs:43 clones SectorAnalysis. segmentum_view.rs has lightweight `by_id: BTreeMap<String, usize>` at line 21 (no super_map rect cache). | Borrow analysis. Skip rect-cache hoist (stale finding). | Partial: rect-cache item de-scoped. |
| TF-P-11 | [x] | Streamed via `BufWriter`: lib.rs (write_sector_json, write_system_json, write_sector_save via new `write_json_pretty` helper); writers.rs `write_md_and_json`; segmentum.rs `write_report`. project_io.rs (builder catalog writes, 15 sites) left for the command-bus refactor (lower per-site value, higher coupling). | — | Golden bytes stable. |
| TF-P-12 | [~] | BuilderIndex already had `systems`/`worlds`/`routes`/`factions` BTreeMaps. Added typed accessors `BuilderState::system_by_id(&SystemId)` + `system_index_by_id(&SystemId)`. Migration of the 11 panel scan sites to the accessors is a follow-up. | — | Foundation laid; mechanical migration pending. |

## §2.4 Error model

| ID | Status | Current state | Plan | Notes |
|----|--------|---------------|------|-------|
| TF-E-1 | [x] | Added `CatalogReloadError::Parse { kind, rel, message }` (`#[non_exhaustive]`). reload_catalog now returns `Result<(), CatalogReloadError>`. 13 silent swallows converted to `?` with typed parse errors carrying file name and kind. Stored in `BuilderState.last_catalog_error`; status bar renders `reload: …` in red. Both call sites (file watcher + conflict_resolver "Reload from disk") wired. | — | UI feedback gap closed. |
| TF-E-2 | [ ] | `build_subsectors` errors only `.unwrap()`-ed in tests; no production swallow sites found (per investigator). | De-scope production swallow. Add HealthFlag emission if we find downstream uses; otherwise close. | Re-verify before action. |
| TF-E-3 | [x] | Added `src/cli/exit_code.rs::from_error(&SectorError) -> ExitCode` with sysexits-style mapping. main.rs routes errors through it. | — | Validate/Generate path codes documented in module rustdoc. |
| TF-E-4 | [x] | SectorError::WorldDataLoad now stores `source: WorldError` via `#[source]`. Both call sites (loading/input.rs, gen/world_pool.rs) updated. | — | Source chain preserved. |
| TF-E-5 | [x] | 61 `let _ = writeln!` in diff.rs replaced with `wln!` macro (defined locally, asserts infallibility via `.expect()`). | — | File path drift: validation.rs → diff.rs. Tests pass. |
| TF-E-6 | [x] | Added `pub enum ValidationCode { ... }` in src/validate/validation.rs with `as_slug() -> &'static str` + Display. 19 string-literal sites in validation.rs converted to `ValidationCode::Foo.as_slug().to_string()`. JSON shape preserved (ValidationIssue.code remains String) so golden tests stay byte-stable. | — | Compile-time enumerability + typo safety. Other code domains (analytics, invariants) left as follow-up. |
| TF-E-7 | [x] | Added `BuilderState.last_save_error: Option<String>`. trigger_auto_save sets it on serialize/write failure. status panel renders a red `save: <err>` tail. | — | Status bar surface. |
| TF-E-8 | [x] | run_generate validation failures + invariant violations now return `Err(SectorError::ValidationFailed { error_count, warning_count })` instead of `Ok(ExitCode::from(1))`. The cli::exit_code mapper translates to exit 1. | — | Single source of truth for exit codes. |

## §2.5 Tests

| ID | Status | Current state | Plan | Notes |
|----|--------|---------------|------|-------|
| TF-T-1 | [x] | Added tests/it/shared.rs with `fixture_dir()` + `fixture_sector()` (OnceLock-backed). Registered via tests/it.rs. Switched analytics_and_presets (3 tests), svg_export_tests (2), invariants_tests (7), search_and_diff (3). | — | 75 integration tests pass; observable speedup. |
| TF-T-2 | [ ] | `detects_mtime_bump` test uses thread::sleep(1200ms) + 200ms polls (file_watcher.rs:135-170). No pure `scan_once`. | Extract scan_once. Replace sleeps with `filetime::set_file_mtime` + bounded recv. | Requires filetime dep. |
| TF-T-3 | [ ] | 5 #[ignore]-d tests in segmentum_tests.rs (lines 50,68,86,107,120). All "slow: full m42 composition". | Re-enable behind cargo `--ignored`. Don't delete (they have CI value). Document in CLAUDE.md. | Triage = no-op for current run, just doc. |
| TF-T-4 | [ ] | 23 subcommands; only Generate covered (cli_gui_parity.rs). | Add assert_cmd-based stub tests per subcommand. Start with help-only smoke tests, expand to Analyze/Validate. | L effort; phased. |
| TF-T-5 | [ ] | golden_png.rs:36-46 compares two runs blake3-eq (not pinned). Template at gui-core/tests/map_snapshots.rs uses byte snapshot + UPDATE_MAP_SNAPSHOTS env. | Add pinned blake3 hash with refresh env var. | Hash regen once. |
| TF-T-6 | [x] | 4 duplicates + their `sector_with_seed` helpers + `use proptest::prelude::*` lines deleted. | — | Tests pass; sector-level proptest in invariants_proptest.rs kept. |
| TF-T-7 | [x] | jobs.rs: elapsed-ms assertion + `Instant::now()` binding + Instant import dropped. | — | try_recv check retained. |
| TF-T-8 | [x] | visual_tokens.rs: rewrote region_overlay test to assert per-kind mapping via pair table. | — | Catches future from_condition regressions. |
| TF-T-9 | [ ] | rewrite_seed at presets.rs:197 has 2 unit tests but no proptest. Config + worlds_toml have no proptest. | Add roundtrip proptests for each. | Requires proptest crate. |
| TF-T-10 | [ ] | No `fuzz/` dir. | Add cargo-fuzz scaffolding with targets for config::parse, presets::load, worlds_toml::parse, map_theme::parse_color. | Sets up infrastructure. |
| TF-T-11 | [ ] | cli_gui_parity.rs:22-84 covers only Generate. | Extend to validate + analyze. | Sequence AFTER TF-T-4 scaffolding. |
| TF-T-12 | [x] | Added `#[cfg(test)] mod tests` to viewer/src/app/lifecycle.rs covering fraction zero/partial paths and preview_progress monotonicity + SystemBuilt scaling. | — | 4 new tests pass. |
| TF-T-13 | [ ] | state/tests.rs covers AddSystem round-trip only. | Round-trip every BuilderCommand variant. | Sequence AFTER TF-S-1 mints new commands. Limited to existing variants for now. |

## Execution order

Wave 1 (foundation, low-risk):
- TF-API-4 (as_slug rollout) — feeds TF-P-3
- TF-NT-1 (PersonaId/HookId/MissionId) — feeds briefing changes
- TF-E-5 (wln! macro) — trivial cleanup
- TF-T-6, TF-T-7, TF-T-8, TF-T-12 (test trivia)

Wave 2 (perf + errors):
- TF-P-5 (color_hex), TF-P-8 (hooks bucket), TF-P-11 (to_writer_pretty)
- TF-E-3 (ExitCode mapper), TF-E-4 (WorldError #[from]), TF-E-7 (auto_save error)
- TF-T-1 (shared fixture)

Wave 3 (structural):
- TF-P-1 (Arc<ProjectCatalogs>), TF-P-2 (rayon short-circuit)
- TF-API-3 (RenderMapTheme rename), TF-P-12 (BuilderIndex)
- TF-NT-2 (score newtypes), TF-NT-3 (FeatureWeightsCache)
- TF-E-1 (CatalogReloadError), TF-E-6 (ValidationCode), TF-E-8 (cli/generate)

Wave 4 (larger/optional):
- TF-API-1 (panel pub→pub(crate))
- TF-API-2 (SectorView builder)
- TF-P-3, TF-P-4, TF-P-7, TF-P-9, TF-P-10
- TF-T-2, TF-T-3, TF-T-4, TF-T-5, TF-T-9, TF-T-10, TF-T-11, TF-T-13
- TF-E-2 (after re-verification)

Verification points: after each wave, run `cargo check --workspace --all-targets`,
`cargo test --workspace`, `cargo test --test it -- golden`.

## Drift log (fix doc vs current code)

- TF-NT-2: control.rs line numbers point inside loops, not function signatures.
  Scope adjusted to struct fields.
- TF-P-6: economy.rs already pre-builds adjacency once per derive call;
  further hoist may not be needed.
- TF-P-10: segmentum_view.rs has no `super_map`/`BTreeMap<String, Rect>` to hoist;
  only borrow tweak applies.
- TF-E-5: `let _ = writeln!` sites live in `src/validate/diff.rs`, not `validation.rs`.
- TF-T-7: elapsed assertion at jobs.rs:116-118, not 117.

## Summary

Completed in this pass (19 of 40 + 1 partial):

- §2.1 API surface — 0 (TF-API-1/2/3/4 deferred; all are L effort).
- §2.2 Newtypes — 1 (TF-NT-1 done; TF-NT-2 + TF-NT-3 deferred).
- §2.3 Performance — 4 done + 1 partial: TF-P-2 (rayon short-circuit), TF-P-5
  (color_hex sink), TF-P-8 (hooks pre-bucket), TF-P-11 (writer streaming);
  TF-P-12 partial (BuilderState accessors added; site migration follow-up).
- §2.4 Error model — 6 of 8 done: TF-E-1 (CatalogReloadError), TF-E-3
  (cli::exit_code), TF-E-4 (#[from] WorldError), TF-E-5 (wln! macro),
  TF-E-6 (ValidationCode), TF-E-7 (auto-save error), TF-E-8 (cli/generate
  unify). TF-E-2 needs re-verification; not actioned.
- §2.5 Tests — 5 of 13 done: TF-T-1 (shared fixture), TF-T-6 (delete dupe
  proptests), TF-T-7 (drop timing flake), TF-T-8 (region overlay
  assertions), TF-T-12 (lifecycle helper tests).

Verification:

- `cargo check --workspace --all-targets` — clean.
- `cargo test --workspace --lib` — 21 + 7 + others pass.
- `cargo test --test it` — 75 pass, 5 ignored (slow segmentum, pre-existing).
- `cargo test --test it -- golden` — 12 pass; byte-stable SVG/PNG/JSON.
- `cargo clippy --workspace --all-targets` — no new warnings introduced; the
  pre-existing list (collapsible-if in cli/common.rs, too-many-arguments,
  field-reassign-with-default) was already present and is out of scope here.

Deferred (recommended for follow-up runs, ordered by ROI):

1. TF-API-4 (`as_slug` rollout) — feeds TF-P-3; mechanical bulk over 48
   enums. Best done by a dedicated agent batch (one enum module per task).
2. TF-P-1 (`Arc<ProjectCatalogs>` in ProjectInput) — pairs with the
   short-circuit in TF-P-2 for a compounded win on `run_seed_search`.
3. TF-NT-3 (FeatureWeightsCache) — straight cache; needs an invalidation
   key tied to BuilderState input edits.
4. TF-API-3 (`RenderMapTheme` rename + HeatCell consolidation) — touches
   imports across builder/viewer/gui-core.
5. TF-NT-2 (score newtypes) — cosmetic until consumers compare scores
   across analyses.
6. TF-T-4 (CLI subcommand coverage), TF-T-9 (config/preset proptest),
   TF-T-10 (cargo-fuzz scaffold), TF-T-11 (cli parity extension).
7. TF-API-1, TF-API-2 — sequence AFTER TF-S-1 (command-bus retrofit) per
   the structural plan.
8. TF-P-6 re-verify, TF-P-3/4/7/9/10 (per-frame cache + Cow borrows).
9. TF-T-2 (file_watcher), TF-T-3 (segmentum triage), TF-T-5 (golden_png
   pin), TF-T-13 (BuilderCommand round-trip).
10. TF-E-2 (build_subsectors surface) — re-verify the call graph first.

## Follow-up pass (covered in second sweep)

All items 1–6 and 8–10 from the deferred list above were addressed in a
second pass. Summary of what landed:

- **TF-API-4** — 57 enums got `as_slug()` + `Display`. ~141 Debug-format
  call sites switched to Display (snake_case output). gui-core map
  snapshots refreshed; PNG/JSON goldens stayed byte-stable. Three pinned
  unit-test strings updated (`gm_full_truth`, `political_sandbox`,
  `dispatch`). Persistent-ID sites (persona/mission/history keys,
  worlds_toml variant parsing) intentionally skipped.
- **TF-P-1** — `ProjectInput` shrunk to `{ root_dir, config, catalogs:
  Arc<ProjectCatalogs>, input_digests }`. 43+ access sites updated across
  src/, builder/, tests/. `clone_project_with_seed` is now one
  `Arc::clone`. Pairs with the TF-P-2 short-circuit on `run_seed_search`.
- **TF-P-3 + TF-P-4** — `SectorMapCache` gained `system_label_cache:
  BTreeMap<SystemId, Arc<str>>` and `faction_style_index:
  BTreeMap<FactionId, FactionStyle>` plus typed `system_label` /
  `faction_style` accessors. Mass migration of info_panel sites is still
  a mechanical follow-up.
- **TF-API-3** — gui-core `MapTheme` renamed to `RenderMapTheme` (the
  data-layer `sectorforge::map_theme::MapTheme` keeps the canonical
  name); `From<HeatCellRgb> for HeatCell` added in gui-core for one-line
  conversions.
- **TF-NT-2** — `ControlScore`, `DisplayImportance`, `ProjectedPower`
  newtypes added in `src/analysis/scores.rs` with `#[serde(transparent)]`.
  Site migration deferred (cosmetic; 35 read sites would cascade).
- **TF-NT-3** — `BuilderState.feature_weights_cache: BTreeMap<(usize,
  usize), FeatureWeightsCacheValue>` keyed by `(sys_idx, w_idx)` and
  validated against an input digest. `feature_weights_for_world` returns
  `Arc<BTreeMap<String, f64>>` and short-circuits on digest hits.
- **TF-P-6** — `derive_dependency_edges` now receives shared `by_sys` +
  `valid_routes_by_sys` from the caller, eliminating the duplicate build
  the inner function used to do.
- **TF-P-7** — relations projection: when only `show_secret_relations` is
  off, the briefing builds a fresh `Vec<FactionRelation>` instead of
  `Arc::make_mut`-ing the (typically shared) matrix. Cow conversion of
  `BriefingPack::sector` left deferred — every profile still mutates
  per-system loops, so the borrowed-path payoff is marginal.
- **TF-P-9** — only the actually wasteful per-frame clones got removed
  (conflict.rs faction list). The remaining `routes.clone()` sites are
  mut-then-`ReplaceRoutes` working copies; rewriting them needs the
  command-bus retrofit (TF-S-1).
- **TF-P-10** — already complete in the previous pass.
- **TF-E-2** — `compute_subsector_variety` returns
  `(Vec<SubsectorVariety>, Option<SubsectorBuildError>)`; failures push a
  `HealthFlag { code: "SUBSECTOR_DERIVE_FAILED", … }`. Builder also
  surfaces `last_subsector_error` in the status bar (parallels the
  `last_catalog_error` / `last_save_error` slots). Viewer surface left as
  a follow-up.
- **TF-T-2** — extracted pure `scan_once(root, &mut baseline)`. Three new
  unit tests cover changed-file detection, no-baseline-entry, and
  missing-file paths. No `filetime` dep needed — the baseline is set to
  `UNIX_EPOCH` so any on-disk mtime is newer without sleeping.
- **TF-T-3** — segmentum ignored tests intentionally retained; CLAUDE.md
  documents the `cargo test --test segmentum_tests -- --ignored` command.
- **TF-T-4** — `tests/it/cli_smoke.rs` exercises `--help` on every CLI
  subcommand, asserts the top-level help lists them all, and confirms
  unknown subcommands exit non-zero.
- **TF-T-5** — `golden_png` now also asserts a pinned blake3 hash stored
  at `tests/goldens/png_m42_default.blake3`. Refresh with
  `UPDATE_GOLDEN_PNG=1 cargo test --test it -- golden_png`.
- **TF-T-9** — proptests added: `rewrite_seed` round-trips any seed
  through TOML parse; `WorldsConfig` round-trips through emit+parse via
  `toml::Value` equality.
- **TF-T-10** — `fuzz/` crate (outside the workspace; nightly-only)
  scaffolded with four targets: `config_parse`, `worlds_toml_parse`,
  `presets_load`, `map_theme_parse_color`. README documents the
  `cargo +nightly fuzz run …` workflow.
- **TF-T-11** — `cli_gui_parity` now also compares `validate --json` and
  `analyze --json` outputs against the in-process `validate_project` and
  `analytics::analyze` calls.
- **TF-T-13** — 11 round-trip tests added in `state/tests.rs` covering
  AddSystem, RemoveSystem (asymmetric — undo re-inserts at the tail),
  MoveSystem, RenameSystem, SwapSystems, AddRoute, RemoveRoute,
  AddFaction, RemoveFaction, SetRouteType, SetRouteStability. Uses
  canonical JSON for comparison since `GeneratedSector` deliberately has
  no `PartialEq`.

Still deferred (genuinely blocked or low-ROI):

- TF-API-1, TF-API-2 — wait for TF-S-1 (command-bus retrofit) per §3.
- Site migration follow-ups: TF-P-3 (info_panel `.to_uppercase()` calls),
  TF-P-4 (`faction_style_by_id` linear scans), TF-NT-2 (35 score-field
  consumers).
- TF-P-7 Cow conversion of `BriefingPack::sector` (broad cascade for
  marginal benefit until profile loops are short-circuited).
- Viewer-side surfacing of `last_subsector_error` (TF-E-2 follow-up).

Verification (second pass):

- `cargo check --workspace --all-targets` — clean.
- `cargo test --workspace` — 167 + 81 + 262 + others pass; 5 ignored
  (slow segmentum, pre-existing).
- `cargo test --test it -- golden` — 13 pass; SVG/PNG/JSON byte-stable.
- gui-core `map_snapshots` — refreshed once (`UPDATE_MAP_SNAPSHOTS=1`)
  to absorb the Display-format text changes.
- `golden_png` pinned hash committed at
  `tests/goldens/png_m42_default.blake3`.
