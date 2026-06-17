# Over-engineering audit — 2026-06-17

Repo-wide `ponytail-audit` (complexity only — no correctness/security/perf).
13 parallel subtree scanners + a dependency scanner → 48 findings; top-20
high-impact `delete:`/`yagni:` claims were adversarially re-verified against the
whole tree (`src builder viewer gui-core tests benches fuzz`, `old/` excluded).

**Verdict: the repo is lean.** Every scope came back "heavily doc-justified",
the dependency set is clean (0 removable), total cuttable ≈ **1,150 lines of
~138k (<1%)**. Waste clusters in three speculative modules + duplicated test
setup.

Status: `✓` adversarially verified safe · `⚠` verified but the as-written plan
breaks the build (don't cut verbatim — see note) · `·` scanner-confident, not
double-checked.

Determinism / command-bus / square-geometry invariants were passed to every
scanner as "do not flag" — nothing below touches them (re-confirmed for each `✓`).

---

## ⚠ Read before cutting these two

- **session.rs base64** — the dead code is real, but the plan misses the
  struct-literal at `builder/src/lib.rs:46` (`files: Vec::new()`), and
  `SessionFile` is public API backing `state_from_generated_sector` (consumed by
  `builder/benches/builder_mutations.rs:59`). Add lib.rs:46 to the edit; correct
  the from_state call-site list (`session.rs:187`, `system_map.rs:1123`,
  `map/mod.rs:1146/1477/1654` — `smoke_test.rs` calls `save_session`, not from_state).
- **lib.rs `derive_*` facades** — only **8 of 9** are dead.
  `derive_interestingness` has 2 live callers in `benches/generation.rs:147,182`
  (deleting it breaks the bench compile). Cut the other 8, keep that one.

---

## Structural cuts (dead code / single-use abstractions)

- [x] `yagni` ✓ **−290** `src/gen/world_ecs.rs` — speculative bevy_ecs adapter, zero consumers. Delete file + `pub mod` (gen/mod.rs:23) + lib.rs:107,188 re-exports + slug-parity reg (macros.rs:235-236) + MAP.md:82.
- [x] `yagni` ✓ **−175** `src/loading/sector_save.rs` — §13-planned, never built against. Delete module + mod.rs:11 + lib.rs:93,177,504,515 + doc refs (GUIDE.md, MAP.md, TEST_GAPS.md).
- [x] `delete` ⚠ **−90** `builder/src/builder/session.rs` — hand-rolled base64 + EmbeddedFile + `files` field, all dead. **See ⚠ note above.**
- [x] `delete` ✓ **−60** `src/analysis/scores.rs` — `score_newtype!` types never adopted. Delete file + mod.rs:38.
- [x] `yagni` ✓ **−55** `gui-core/src/visual_tokens.rs` — `MapRouteVisual` vestigial; `.pattern()` == `route_type.pattern(mode)` (own test proves it). Inline at view.rs:515,641; drop enum+test; edit GUIDE.md:2533. Keep `MapRegionOverlay`.
- [x] `native` · **−50** `viewer/src/data_editor.rs` + `editor/file_ops.rs` — `ScratchDir` hand-rolls temp-dir-on-drop, duplicated; comment "no tempfile dep" is false (`tempfile.workspace=true`). Use `tempfile::TempDir`. (Keep `CwdGuard`.)
- [x] `delete` ✓ **−35** `src/export/map_theme.rs:161` (+heatmap.rs, segmentum.rs) — 7 dead `Display` impls (RouteLineMode/LabelDensity/LegendStyle/SymbolSet/HeatmapMode/StabilityDimension/BorderOrientation). `as_slug()` already serves output. Keep FactionMode's.
- [x] `yagni` ⚠ **−24** `src/lib.rs:694…` — 8 of 9 `derive_*` no-config facades are dead. **See ⚠ note above** (keep `derive_interestingness`).
- [x] `yagni` ✓ **−27** `viewer/src/app/layout.rs` — `TopBar`/`MainView` single-field `&mut App` delegators, one call site each. Make free fns `pub(crate)`, call directly at app/mod.rs:191-192.
- [x] `yagni` ✓ **−25** `builder/src/builder/panels/factions.rs:660` — `preferred_picker_*` 3 single-caller `pick_multi` shims, called back-to-back at :449-451. Inline (preserve salt literals).
- [x] `delete` ✓ **−20** `builder/src/builder/derivation_cache.rs:43` — `DerivationCache get/put/invalidate` always-empty husk superseded by §39 ledger (status-bar "cache: N" always 0). Full removal touches state/mod.rs:180, 7 no-op `.clear()`, status.rs:52. Keep `digest_input` + the ledger.
- [x] `delete` ✓ **−15** `src/loading/config.rs:175` — 3 dead `Display` impls (PlacementMode/WorldSelectionMode/HtmlTheme). Keep OutputFormat's.
- [x] `delete` ✓ **−14** `gui-core/src/widgets.rs:189` — `toggle_with_label`, zero callers (only bare `toggle` used).
- [x] `delete` ✓ **−11** `gui-core/src/ui_kit.rs:99` — `field`, superseded by `labeled`; test-only ref.
- [x] `delete` ✓ **−11** `gui-core/src/design.rs:250` — `vertical_gradient` Mesh helper nothing uses (the 2 gradient surfaces deliberately avoid it).
- [x] `delete` ✓ **−8** `gui-core/src/design.rs:60` — `rounding_sm` / `rounding_lg` (only `rounding_md` used). Keep the consts.
- [x] `yagni` ✓ **−7** `src/analysis/history/mod.rs:58` — `derive_with_progress` 1 internal caller; inline into `derive_with`, relink 2 doc-links.
- [x] `yagni` ✓ **−6** `src/loading/presets.rs:33` — `PresetMeta.tags` + `default_seed` deserialized, never read.
- [x] `delete` ✓ **−6** `builder/src/builder/file_watcher.rs:72` — `root()` + backing field (poll_loop uses its own clone). Also drop unused `Utf8Path` import.
- [x] `yagni` ✓ **−6** `viewer/src/app/types.rs:14` — `ExportJobResult` 3 variants nothing discriminates → `struct ExportJobResult(String)`. Keep builder's (it IS discriminated).
- [x] `yagni` ✓ **−5** `src/export/system_map.rs:44` — `RESOLUTION_720P/1440P/4K` test-only; inline 1/2/3. Keep `MAX_SCALE`.
- [x] `delete` · **−5** `src/validate/validation.rs:712` — `ValidationCode` `Display` impl (always built via `.as_slug().to_string()`).
- [x] `yagni` · **−5** `builder/src/builder/panels/map/dialogs.rs:243` — `CollisionAction` single-variant enum = glorified bool.
- [x] `yagni` · **−5** `src/model/sector_model/routes_view.rs:143` — `RouteType::pattern_key` one-line wrapper over `key()`, 1 caller.
- [x] `delete` · **−4** `builder/src/builder/workspace.rs:72` — `BuilderWorkspace::get(idx)`, no caller; doc references a method that doesn't exist.
- [x] `delete` · **−4** `builder/src/builder/derivation_cache.rs:355` — redundant match arm (dead `!stale.contains` guard; next arm identical).
- [x] `yagni` · **−4** `src/model/sector_model/routes_view.rs:30` — `GeneratedRoute::pattern` no-salt shim, 0 callers.
- [x] `delete` · **−3** `builder/src/builder/panels/interestingness.rs:589` — `_force_report_use` no-op that only silences an unused import; drop both.

## Duplication / shrink (quality, scanner-confident)

- [x] `shrink` · **−33** `builder/src/builder/panels/relations.rs:653` — `attitude_combo`/`treaty_combo` identical bar enum → one generic combo.
- [x] `shrink` · **−30** `builder/src/builder/panels/economy.rs:921` — `show_tech_rows`/`show_pop_rows` byte-identical bar map field → one helper.
- [x] `shrink` · **−25** `tests/it/golden_png.rs:87` + `svg_export_tests.rs` — inlined blake3-pin block → use existing `assert_blake3_golden`, promote to shared.rs.
- [x] `shrink` · **−15** `tests/it/relations_tests.rs:25` (+ personae/hooks/economy) — `gen_sector` copied verbatim in 4 files → shared.rs.
- [x] `shrink` · **−12** `tests/it/loading_tests.rs:12` + `cli_behavior.rs` — `copy_dir_all` defined twice → shared.rs.
- [x] `shrink` · **−12** `tests/it/route_monotonicity.rs:25` (+3) — `fixture_dir`/`fixture_project` re-defined → import shared.
- [ ] `shrink` · **−10** `tests/it/random_sector_tests.rs:324` — 3rd hand-rolled recursive `walkdir`; flat `read_dir` if presets aren't nested. **NOT APPLIED: presets dir IS nested (flat `read_dir` won't work), and after batch-2 dedup this is the SOLE remaining recursive walker — no duplication left to remove. Minimal 1-caller helper kept as-is.**
- [x] `shrink` · **−8** `tests/it/analytics_and_presets.rs:7` — `presets_dir`/`goldens_dir` each defined twice → shared.rs.
- [x] `shrink` · **−8** `src/model/sector_model/mod.rs:660` — `RouteType::key()` duplicates `as_slug()` slugs. **DONE (per request, after determinism analysis): made `RouteType::as_slug()` `const` so the `const ROUTE_TYPES` table still compiles, swapped all 10 callers, deleted `key()`. Byte-identical (`as_slug()==key()` for every variant) — golden confirms the route-pattern hash at `routes_view.rs` is unchanged. Note: `as_slug()` now feeds that hash → golden output, so renaming a RouteType slug shifts PNG/SVG/HTML goldens.**
- [x] `shrink` · **−6** `builder/src/builder/state/selection.rs:66` — nav-stack cap idiom triplicated (`remove(0)` is O(n)).
- [x] `shrink` · **−6** `builder/src/builder/project_io.rs:273` — `toml_err(&str)` subsumes `parse_err(&'static str)`.

## Stdlib / native micro-swaps

- [x] `native` · **−6** `builder/src/builder/panels/map/theme.rs:755` — `parse_hex` → `Color32::from_hex` (low: clippy paint-ban may block).
- [x] `shrink` · **−3** `builder/src/builder/project_io.rs:811` — `blake3_digest` wrapper (adds `blake3:` prefix; 1 caller).
- [x] `shrink` · **−3** `viewer/src/factions_overview.rs:932` — `join_static_list` → inline `slice::join`.
- [x] `shrink` · **−2** `src/analysis/personae.rs:382` — `node_presence<S,W>` generics are noise → take 2 `usize`.
- [x] `stdlib` · **−2** `builder/src/builder/panels/map/cache.rs:18` — `.map(..).unwrap_or(true)` → `Option::is_none_or` (already the idiom here).
- [x] `stdlib` · **−1** `builder/src/builder/panels/invariants.rs:313` — `splitn(2,'.')`+`.next()?`×2 → `str::split_once`.
- [x] `stdlib` · **−0** `src/gen/world_pool.rs:376` — `partial_cmp().unwrap_or(Equal)` → `f64::total_cmp` (drops an import; deterministic NaN; net 0 lines).

---

**net: ≈ −1,150 lines, −0 deps.**

Lowest-risk batch to start: the gui-core dead fns + the two speculative modules
(`world_ecs`, `sector_save`) — ≈ 580 lines, all `✓`, no downstream risk. Run
`cargo test --test it -- golden` after anything that touches `src/export`.
