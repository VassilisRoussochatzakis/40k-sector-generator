---
unit_id: X08
crate: workspace
paths:
  - src/**/*.rs (inline #[cfg(test)] only)
  - gui-core/src/**/*.rs (inline #[cfg(test)] only)
  - builder/src/**/*.rs (inline #[cfg(test)] only)
  - viewer/src/**/*.rs (inline #[cfg(test)] only)
loc_reviewed: ~93000
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 7, low: 6, nit: 2 }
top_risks:
  - "Sleep-based file-watcher test is slow and flake-prone (F-X08-001)"
  - "Wall-clock 500ms assertion in spawn_job test is CI-flaky (F-X08-002)"
  - "Tautological / assertion-free tests give false coverage signal (F-X08-003)"
  - "TOML loaders and parsers have zero property-test coverage (F-X08-004)"
---

# Review: X08 — Testing (inline `#[cfg(test)]` sweep)

## Summary

The workspace has **89 inline test modules with 439 `#[test]` functions** spread across
`src/`, `gui-core/`, `builder/`, `viewer/`. Most modules with non-trivial logic carry at
least a handful of inline tests; the builder command bus, undo/redo, panel action
handlers, and `sector_model::mutation` are well-covered (~250 of the 439 tests live in
`builder/`). The big shortfall is in the **viewer crate (3 tests across 13 142 LOC),**
the **CLI runner crate (0 inline tests across `src/cli/`), and TOML parsers
(`loading/config.rs`, `loading/input.rs` have zero inline tests).** A handful of acute
quality issues — a real `thread::sleep` test in `file_watcher`, a wall-clock latency
assertion in `gui-core/jobs`, an assertion-free smoke test in `visual_tokens` — are
the kinds of failures that erode trust in the suite. No `#[ignore]`d tests, no
`should_panic` without `expected = ...`, no `HashMap`-iteration assertions, no
trivially-true asserts — those classes are clean. There are also no `cargo-fuzz`
targets anywhere in the tree and no doctests except a single one in `src/lib.rs`.

## Findings

### F-X08-001 — [HIGH] [Tests] `file_watcher` test relies on real-time `sleep`s and FS mtime resolution
- **Location:** `builder/src/builder/file_watcher.rs:134-170`
- **Category:** Tests / Flakiness + Speed
- **Confidence:** High
- **Blast radius:** Single test, but it adds ≥1.4 s to every CI run and can flake on slow disks or coarse mtime FS.
- **Problem:** The only test in this module:
  1. Sleeps 1200 ms ("past the filesystem mtime resolution"),
  2. Then busy-polls `try_recv()` 30 × 200 ms = up to 6 s waiting for the watcher's 1000 ms poll loop.
  This makes the test (a) the slowest unit test in the workspace, and (b) sensitive to FS mtime granularity (HFS+/APFS 1 s, ext4 1 ns, FAT32 2 s — Windows CI can drift).
- **Why it matters:** Sleep-based tests are the single biggest source of CI flakiness and unit-test wall-clock time. The brief explicitly calls them out as HIGH.
- **Suggested fix:** Refactor `poll_loop` to take a clock + tick channel injected via a `trait Clock` so the test can drive it deterministically; or hoist the FS scan into a pure `scan_once(root, baseline) -> Vec<FileChange>` function and unit-test that without ever spawning the thread. Sketch:
  ```rust
  pub(crate) fn scan_once(
      root: &Utf8Path, baseline: &mut BTreeMap<String, SystemTime>,
  ) -> Vec<FileChange> { /* moved out of poll_loop */ }

  #[test]
  fn scan_once_reports_mtime_bump() {
      let mut baseline = BTreeMap::new();
      baseline.insert("x.toml".into(), SystemTime::UNIX_EPOCH);
      let dir = tempfile::TempDir::new().unwrap();
      std::fs::write(dir.path().join("x.toml"), b"a").unwrap();
      let events = scan_once(
          &Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap(),
          &mut baseline,
      );
      assert_eq!(events.len(), 1);
  }
  ```
  Keep the spawn-loop integration test under `tests/it/` and `#[ignore]` it by default if it must remain time-based.
- **Effort:** M
- **Risk of fix:** Low

### F-X08-002 — [HIGH] [Tests] Wall-clock 500 ms assertion in `spawn_job` is flake-prone on loaded CI
- **Location:** `gui-core/src/jobs.rs:105-130`
- **Category:** Tests / Flakiness
- **Confidence:** High
- **Blast radius:** Single test; will flake on a busy runner / Windows pause-and-resume / debug builds.
- **Problem:** `spawn_job_dispatch_returns_before_worker_finishes` asserts `start.elapsed() < Duration::from_millis(500)` after calling `spawn_job`. A 500 ms threshold for "did dispatch return" is *also* asserting "machine wasn't paged out for half a second" — an OS scheduling guarantee that does not hold under load.
- **Why it matters:** Wall-clock assertions in unit tests fail intermittently and erode trust. The intent ("dispatch is non-blocking") is correctly testable by checking `handle.receiver.try_recv()` returns `Empty` immediately after `spawn_job` — which the test already does on l. 121. The 500 ms assertion adds nothing functional and only adds flake.
- **Suggested fix:** Delete the elapsed assertion; the `try_recv == Empty` check immediately after `spawn_job` already proves dispatch did not block waiting for the worker.
  ```rust
  // delete: assert!(start.elapsed() < Duration::from_millis(500), ...);
  assert!(matches!(
      handle.receiver.try_recv(),
      Err(std::sync::mpsc::TryRecvError::Empty)
  ));
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X08-003 — [HIGH] [Tests] Assertion-free test gives false confidence
- **Location:** `gui-core/src/visual_tokens.rs:156-170`
- **Category:** Tests / Tautological
- **Confidence:** High
- **Blast radius:** Single test, but the pattern is contagious.
- **Problem:** `region_overlay_tokens_cover_all_conditions` iterates every `RegionConditionKind` and discards the result with `let _ = MapRegionOverlay::from_condition(kind);`. The function being called returns `MapRegionOverlay` (not `Result`), so this only catches a panic — and `from_condition` is a `match` over an `enum` whose exhaustiveness is *already* checked at compile time. The test asserts nothing the compiler doesn't already enforce.
- **Why it matters:** Counts as "covered" in any line-coverage tool but cannot fail on a real regression (e.g. wrong colour for a variant, mis-mapped pattern). The brief flags assertion-free tests as HIGH for precisely this reason.
- **Suggested fix:** Assert a property of the returned overlay — e.g. it has a non-default colour, or use snapshot of the (kind → overlay) mapping:
  ```rust
  for kind in [ /* ... */ ] {
      let overlay = MapRegionOverlay::from_condition(kind);
      assert_ne!(overlay.colour, Color32::TRANSPARENT, "{kind:?} should have a visible colour");
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X08-004 — [HIGH] [Tests] TOML loaders / parsers have zero property-test coverage
- **Location:** `src/loading/config.rs` (436 LOC, 0 tests); `src/loading/input.rs` (268 LOC, 0 tests); `src/worlds_toml.rs:181-270` (4 hand-picked tests, no proptest); `src/loading/presets.rs:299-367` (5 tests, no proptest)
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Every project load goes through these parsers; a malformed TOML in a real preset is a CRITICAL panic / silent default the suite cannot catch.
- **Problem:** All four TOML-facing modules carry only example-based round-trip tests (or none). `loading/config.rs` has 14 `pub struct` / `pub enum` definitions including 3 `OutputFormat::parse_token`-style enums and not a single inline test. `loading/input.rs` has the workspace-critical `load_project` entry-point with no inline coverage. A property test along the lines of "encode → decode is identity for any well-typed config" would catch the entire class of `serde` rename / default-mismatch bugs in one shot.
- **Why it matters:** Loaders are the codebase's main untrusted-input surface; the rubric calls out parsers as the canonical property-test target (§3.10). The recently added §3.10 mandate ("Property tests for parsers/math/layout invariants") is unmet here.
- **Suggested fix:**
  1. Add inline `proptest!` round-trip tests using a `proptest::strategy` per config struct (`prop_compose!` for `GenerationConfig`, `OutputConfig`, `HtmlConfig`).
  2. Add an inline unit test for `OutputFormat::parse_token` covering every variant + at least 3 unknown-token cases.
  3. Add a property test for `WorldsConfig` round-trip in `worlds_toml.rs:181`.
  Sketch:
  ```rust
  proptest! {
      #[test]
      fn output_format_parse_token_roundtrip(fmt: OutputFormat) {
          let s = fmt.token(); // existing
          assert_eq!(OutputFormat::parse_token(s), Some(fmt));
      }
  }
  ```
- **Effort:** M
- **Risk of fix:** Low

### F-X08-005 — [MEDIUM] [Tests] No fuzz targets for parsers / loaders
- **Location:** workspace (no `fuzz/` directory; no `cargo-fuzz` config in `Cargo.toml`)
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Loader code paths that accept arbitrary user TOML, JSON sectors, and segmentum bundles. A maliciously-crafted file can today panic via unchecked indexing / `?` losing context (cross-reference X07 panic-surface sweep).
- **Problem:** §3.10 calls for `cargo-fuzz` targets on `src/loading/` and `src/export/` parsers. None exist. The closest is the proptest in `tests/it/invariants_proptest.rs`, which generates *valid* sectors, not malformed bytes.
- **Why it matters:** Loaders are the most likely place a CRITICAL panic-on-input bug hides; fuzzing is the cheapest way to find them.
- **Suggested fix:** Add a `fuzz/` workspace with three harnesses, all under `cargo +nightly fuzz`:
  ```rust
  // fuzz/fuzz_targets/loader_config.rs
  #![no_main]
  use libfuzzer_sys::fuzz_target;
  fuzz_target!(|data: &[u8]| {
      if let Ok(s) = std::str::from_utf8(data) {
          let _ = toml::from_str::<sectorforge::config::AppConfig>(s);
      }
  });
  ```
  Same shape for `sector_save::load_sector_json` and `worlds_toml::WorldsConfig::from_str`. Document in `GUIDE.md` how to run them in CI weekly (not per-commit).
- **Effort:** M
- **Risk of fix:** Low

### F-X08-006 — [MEDIUM] [Tests] Viewer crate has 3 inline test fns across ~13 100 LOC
- **Location:** `viewer/src/factions_overview.rs:1302` (2 tests), `viewer/src/editor/state.rs:372` (1 test); all other viewer modules have zero inline tests
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Whole viewer crate. Pure-logic helpers like `RoutePlannerState::click_system`, `Plan` cost calc, `SegmentumBundle::child_at`, etc. are exercised only end-to-end through the GUI.
- **Problem:** 798-LOC `segmentum_view.rs`, 471-LOC `route_planner.rs`, 689-LOC `app/sector_view.rs` carry no inline tests. Many of their public helpers are pure functions amenable to unit testing without spinning up egui.
- **Why it matters:** The viewer is the most user-facing crate; regressions in `route_planner.click_system` etc. are felt directly. Coverage-by-screen is expensive; coverage-by-unit is cheap.
- **Suggested fix:** Add inline `#[cfg(test)]` modules to the following targets (priority order):
  1. `viewer/src/route_planner.rs` — `click_system`, `set_metric`, `clear`, `plan(...)`.
  2. `viewer/src/segmentum_view.rs` — `child`, `child_at`, `system_name`, `link_count_for_child`.
  3. `viewer/src/editor/state.rs` — extend with tests for `apply_preview_result(stale)` paths and progress accumulator.
  Each helper above is pure data manipulation — no egui needed.
- **Effort:** L
- **Risk of fix:** Low

### F-X08-007 — [MEDIUM] [Tests] CLI runner crate has zero inline test coverage
- **Location:** `src/cli/*.rs` (≈ 30 files, ~ 2 400 LOC, 0 inline `#[test]` fns)
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Every CLI subcommand. The integration suite (`tests/it/`) exercises some end-to-end behaviour but cannot easily target the small parse helpers.
- **Problem:** `src/cli/common.rs:114` (`parse_heatmap`, 14-arm string match), `src/cli/mod.rs` (625 LOC of clap parsing/dispatch), and every `src/cli/<cmd>.rs` runner are inline-test-free. `parse_heatmap` in particular is exactly the shape of a unit-testable parser — one input string, one output enum, ≥14 cases.
- **Why it matters:** End-to-end CLI tests are slow; a regression in `parse_heatmap` ("supply-vulnerability" mistakenly mapping to `Trade`) would slip past every integration test unless one happens to use that exact spelling.
- **Suggested fix:** Add inline unit tests for the pure parser/printer helpers in `common.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn parse_heatmap_known_tokens() {
          assert!(matches!(parse_heatmap("threat").unwrap(), HeatmapMode::Threat));
          assert!(matches!(parse_heatmap("THREAT").unwrap(), HeatmapMode::Threat));
          assert!(matches!(parse_heatmap("trade_volume").unwrap(), HeatmapMode::TradeVolume));
          assert!(matches!(parse_heatmap("trade-volume").unwrap(), HeatmapMode::TradeVolume));
      }
      #[test]
      fn parse_heatmap_unknown_token_errors() {
          let err = parse_heatmap("not-a-mode").unwrap_err();
          assert!(err.to_string().contains("unknown heatmap mode"));
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X08-008 — [MEDIUM] [Tests] Public modules with rich logic lack inline tests
- **Location:** see coverage-gap list below
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Per-module; some are covered transitively by `tests/it/` (golden + invariants) but the inline signal-to-test ratio is poor.
- **Problem:** Largest untested-inline modules ranked by LOC, with note on whether `tests/it/` exercises them:

  | LOC | Module | Inline tests | `tests/it/` covers? |
  |---:|---|---:|---|
  | 1361 | `src/worlds.rs` | 0 | partial (via golden) |
  | 925 | `src/export/render.rs` | 0 | partial (`invariants_tests.rs` calls `render_sector_markdown` only) |
  | 845 | `src/gen/generation/mod.rs` | 0 | yes (`golden_generation.rs`) |
  | 625 | `src/cli/mod.rs` | 0 | partial |
  | 605 | `src/validate/validation.rs` | 0 | yes (`validation_tests.rs`) |
  | 588 | `src/validate/invariants.rs` | 0 | yes (`invariants_tests.rs`) |
  | 488 | `src/gen/generation/factions.rs` | 0 | partial |
  | 436 | `src/loading/config.rs` | 0 | no |
  | 350 | `src/gen/generation/world_placement.rs` | 0 | partial |
  | 305 | `src/export/writers.rs` | 0 | yes (smoke via CLI) |
  | 293 | `src/export/heatmap.rs` | 0 | no — `compute_rgb` / `score_sector` untested |
  | 268 | `src/loading/input.rs` | 0 | partial (loaded via fixtures) |

- **Why it matters:** `tests/it/` golden tests detect *that something changed* but not *what's wrong*. Inline tests pin down the small invariants that make the golden output stable.
- **Suggested fix:** Add focused inline tests for the worst offenders. Priority:
  - `src/export/heatmap.rs` → unit tests for `compute_rgb` (boundary conditions: empty sector, single system, max-score system) and `score_sector` (deterministic ordering).
  - `src/loading/config.rs` → see F-X08-004.
  - `src/validate/invariants.rs` → already covered by `tests/it/invariants_tests.rs` but the unit `check_regions`, `check_economy` helpers (each ~ 80 LOC) deserve targeted inline cases for each violation code.
- **Effort:** L
- **Risk of fix:** Low

### F-X08-009 — [MEDIUM] [Tests] Doctest coverage on public API is sparse (essentially zero)
- **Location:** `src/lib.rs:34-46` (one doctest in `# Quick start`); `gui-core/src/lib.rs` (no module doc, no doctests); `viewer/src/lib.rs:1-3` (3-line `//!`, no doctests); `builder/src/lib.rs` (no module doc, no doctests)
- **Category:** Tests / Documentation
- **Confidence:** High
- **Blast radius:** Documentation quality + drift detection.
- **Problem:** A single `no_run` doctest in `src/lib.rs`. None of the public APIs (`load_project`, `generate_sector`, `export_sector`, `SectorOverviewCache`, `spawn_job`, ...) have `/// # Examples` blocks that compile.
- **Why it matters:** Doctests are the cheapest form of "API works as documented" testing and rot detection. Missing them is fine; *one* present doctest that goes stale (e.g. `validate_project` signature changes) is worse.
- **Suggested fix:** Add `/// # Examples` doctests with realistic snippets on the top ~ 6 public entry points: `sectorforge::generate_sector`, `sectorforge::validate_project`, `sectorforge::export_sector`, `sectorforge::heatmap::score_sector`, `sectorforge_gui_core::jobs::spawn_job`, `sectorforge_gui_core::heatmap::HeatmapCache::get_or_compute`. Mark cases that can't realistically run as `no_run` but still type-check.
- **Effort:** M
- **Risk of fix:** Low

### F-X08-010 — [MEDIUM] [Tests] Duplicated `sample_sector` fixtures across export tests
- **Location:** `src/export/svg_export/tests.rs:27-72` and `src/export/bitmap/tests.rs:25-70` (~ 45 LOC each, identical)
- **Category:** Tests / Maintainability
- **Confidence:** High
- **Blast radius:** Drift between the two: a new field on `GeneratedSector` requires two updates.
- **Problem:** Two near-identical `sample_sector()` / `empty_manifest()` fixtures exist in sibling test modules. Any change to `GeneratedSector` fields breaks both.
- **Why it matters:** Maintenance tax + drift risk; already happens repeatedly (`influence_field`, `power_projection`, `relations`, `regions`, `economy`, `chronicle`, `id_history` are all set to `Default::default()` in both, in identical order).
- **Suggested fix:** Extract a `pub(crate) fn sample_sector_fixture() -> GeneratedSector` in `src/export/mod.rs` (or in a `#[cfg(test)] mod test_fixtures` shared module) and import from both tests. Same applies to many builder panel tests using `BuilderState::new_blank("t", "T", "seed", 8, 8)` — that helper already exists; the export side hasn't been DRY'd.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-011 — [MEDIUM] [Tests] Bitmap render test asserts only positive dimensions
- **Location:** `src/export/bitmap/tests.rs:72-87`
- **Category:** Tests / Under-asserting
- **Confidence:** Medium
- **Blast radius:** Bitmap export regression undetected at unit level.
- **Problem:** `renders_without_panicking` asserts `img.width() > 0 && img.height() > 0` — basically "did not panic and produced *some* image". `scaled_render_is_larger` asserts only output dimensions scale with input. Neither checks pixel content or hash.
- **Why it matters:** Byte-stability is a project-wide invariant (CLAUDE.md). The golden tests under `tests/it/golden_png.rs` enforce stability end-to-end, but a tighter inline assertion (hash of sample render) would catch local regressions before the golden suite runs.
- **Suggested fix:** Either delete the inline tests as redundant with `tests/it/golden_png.rs`, or strengthen them with a blake3 of the output bytes for a 1-pixel sector — fast and detects unintended changes:
  ```rust
  let img = render(&s, 1, None, RenderOptions::default());
  let bytes = img.as_raw();
  let hash = blake3::hash(bytes);
  assert_eq!(hash.to_hex().as_str(), "<known good>");
  ```
  Document the hash in a const next to the test so an intentional change is a one-line diff.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-012 — [LOW] [Tests] `model::rng::weighted_index` lacks property tests for distribution invariants
- **Location:** `src/model/rng.rs:79-109` (3 tests, none property-based)
- **Category:** Tests / Coverage
- **Confidence:** Medium
- **Blast radius:** Generation determinism — `weighted_index` underpins every weighted draw.
- **Problem:** Existing tests cover (a) single-item pools and (b) zero-weight skipping. Missing: invariants like "weighted_index never returns an index whose weight is zero or NaN", "distribution-of-picks-over-N-draws approximates weights", "result is deterministic for fixed seed".
- **Why it matters:** A regression in the floating-point edge handling (lines 60-67) could silently bias all generation, which the golden tests would only flag via byte-diff long after the fact.
- **Suggested fix:** Add a `proptest!` block:
  ```rust
  proptest! {
      #[test]
      fn weighted_index_never_picks_zero_weight(
          weights in prop::collection::vec(0.0f64..10.0, 2..20),
          seed: u64,
      ) {
          prop_assume!(weights.iter().any(|w| *w > 0.0));
          let pool: Vec<((), f64)> = weights.iter().map(|w| ((), *w)).collect();
          let mut rng = ChaCha8Rng::seed_from_u64(seed);
          let idx = weighted_index(&pool, &mut rng, "test").unwrap();
          assert!(pool[idx].1 > 0.0);
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X08-013 — [LOW] [Tests] `analysis/search::derive_candidate_seed` and `insert_top_n` have only happy-path tests
- **Location:** `src/analysis/search.rs:1334-1367` (2 tests)
- **Category:** Tests / Coverage
- **Confidence:** Medium
- **Blast radius:** Candidate search produces the wrong top-N if `insert_top_n` mis-handles ties.
- **Problem:** `insert_top_n` is tested with all-distinct miss scores. Tie-breaking, empty-buffer, and `n == 0` cases are not covered.
- **Suggested fix:** Add a proptest that for any random input sequence, after all inserts, `buf` contains the `min(n, len)` lowest-miss reports.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-014 — [LOW] [Tests] `info_panel` exposes ~ 9 public formatting fns; only the cache is tested
- **Location:** `gui-core/src/info_panel.rs:1112-1142` (2 tests, both cache-only)
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** All info-panel summaries (world_detail, system_summary, route_summary, subsector_summary, ...) untested at unit level.
- **Problem:** 1142 LOC module with ~ 9 public `fn ...(ui, ...)` helpers that build text into an egui `Ui`. Hard to unit-test because they take `&mut Ui`, but many internally call pure formatters that could be extracted.
- **Suggested fix:** Extract the pure-text-building parts into `fn world_detail_text(w: &GeneratedWorld) -> Vec<DisplayBucket>` (or similar) and have `world_detail` render those buckets. Then the text builders are trivially unit-testable. Mark this as a refactor task — not a fast fix.
- **Effort:** L
- **Risk of fix:** Medium (touches a widely-called API)

### F-X08-015 — [LOW] [Tests] `viewer/src/editor/state.rs` has one test for a 418-LOC module
- **Location:** `viewer/src/editor/state.rs:372-418`
- **Category:** Tests / Coverage Gap
- **Confidence:** High
- **Blast radius:** Editor preview pipeline.
- **Problem:** Single test covers "stale revision dropped". Untested: preview job timeout handling, error result propagation, progress accumulator, `schedule_preview` when no prior job exists.
- **Suggested fix:** Add three more inline tests: `fresh_revision_accepted_when_no_prior_job`, `apply_preview_result_error_clears_in_flight`, `progress_accumulator_clamps_to_one`.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-016 — [LOW] [Tests] `analysis/personae`, `analysis/influence_field` have a single test each
- **Location:** `src/analysis/personae.rs:937` (1 test, 1000+ LOC module); `src/analysis/influence_field.rs:301` (1 test, ~ 300 LOC module)
- **Category:** Tests / Coverage Gap
- **Confidence:** Medium
- **Blast radius:** Analytics outputs the GUI displays directly.
- **Problem:** Both modules are complex pure-computation analytics over a `GeneratedSector` but carry only a smoke test. Property tests of the form "for any sector, output has length == systems.len()" or "all faction influence is in [0,100]" would catch most likely regressions.
- **Suggested fix:** Add 2-3 invariant tests per module. These can be cheap because the inputs are small sample sectors built with `GeneratedSector::empty(...)` + a handful of pushed systems.
- **Effort:** M
- **Risk of fix:** Low

### F-X08-017 — [LOW] [Tests] No coverage report in `reviews/_baseline/`
- **Location:** `reviews/_baseline/`
- **Category:** Tests / Process
- **Confidence:** High
- **Blast radius:** This review.
- **Problem:** `cargo llvm-cov` was not run. Without it, "untested" claims rely on grep + module size and may miss public fns that are exercised transitively by `tests/it/`.
- **Suggested fix:** Add `cargo install cargo-llvm-cov` to the CI and run:
  ```bash
  cargo llvm-cov --workspace --html --output-dir reviews/_baseline/coverage
  cargo llvm-cov --workspace --json --output-path reviews/_baseline/coverage.json
  ```
  Aggregator can then re-rank coverage gaps with real evidence.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-018 — [NIT] [Tests] Mixed use of `HashSet` and `BTreeSet` in tests
- **Location:** e.g. `builder/src/builder/panels/system_map.rs:878` (`std::collections::HashSet`)
- **Category:** Tests / Determinism hygiene
- **Confidence:** Medium
- **Blast radius:** Test code; not user-facing.
- **Problem:** Tests sometimes build `HashSet<_>` from sector ids and assert on `.len()`. Length is order-invariant so this is fine *today*, but contributors who iterate the same `HashSet` for an `assert_eq!` will produce non-deterministic failures.
- **Suggested fix:** Use `BTreeSet` by default in test code too. CLAUDE.md already mandates this for output; extending the convention to tests removes a foot-gun.
- **Effort:** S
- **Risk of fix:** Low

### F-X08-019 — [NIT] [Tests] Existing TOML round-trip tests use `assert_eq!` on the *whole* struct in some places, `.contains(...)` on the *string* in others
- **Location:** `src/gen/factions.rs:199-249` (mixed), `builder/src/builder/preferences.rs:99-105` (whole-struct)
- **Category:** Tests / Style
- **Confidence:** Medium
- **Blast radius:** None functional; consistency only.
- **Problem:** Some round-trip tests parse-then-`assert_eq!`; others write-then-`assert!(toml.contains("..."))`. The latter is brittle (rejects re-ordering / formatting changes).
- **Suggested fix:** Prefer parse-then-`assert_eq!` everywhere; reserve `.contains()` checks for assertions about *skip_serializing_if* (e.g. F-X08 / `optional_style_fields_skip_serialize_when_none`).
- **Effort:** S
- **Risk of fix:** Low

## Per-crate test inventory

| Crate | Inline test fns | Modules with `#[cfg(test)]` | Modules with **zero** inline tests | Biggest untested-inline public fn / module |
|---|---:|---:|---:|---|
| `sectorforge` (src/) | ~ 167 | 47 | ~ 60 | `src/worlds.rs` (1361 LOC, 0 tests); `src/export/render.rs` (925 LOC, 0 tests); `src/loading/config.rs` (436 LOC, 0 tests) |
| `sectorforge-gui-core` | 19 | 7 | 4 | `gui-core/src/info_panel.rs` (only cache tested) |
| `sectorforge-builder` | ~ 250 | 33 | ~ 25 | none material — most state/panel modules have ≥ 1 test; `panels/save_project.rs`, `panels/open_project.rs`, `panels/new_project.rs` have 0 |
| `sectorforge-viewer` | 3 | 2 | ~ 30 | `viewer/src/segmentum_view.rs` (798 LOC, 0 tests); `viewer/src/route_planner.rs` (471 LOC, 0 tests); `viewer/src/app/sector_view.rs` (689 LOC, 0 tests) |
| `tests/it/` *(scoped to U022, listed for reference)* | 15 files / ~ 2150 LOC | — | — | (out of scope here) |

## Coverage-gap list — top 20 public modules with NO inline tests

Ranked by LOC. Many are partially covered by `tests/it/`; that's flagged.

1. `src/worlds.rs` — 1361 LOC, 0 inline. Coverage: partial via golden.
2. `src/export/render.rs` — 925 LOC, 0 inline. `render_sector_markdown` called once in `tests/it/invariants_tests.rs:182`. `render_system_markdown` totally untested.
3. `src/gen/generation/mod.rs` — 845 LOC, 0 inline. Covered by `golden_generation.rs`.
4. `src/cli/mod.rs` — 625 LOC, 0 inline. Partial via CLI integration tests.
5. `src/validate/validation.rs` — 605 LOC, 0 inline. Covered by `tests/it/validation_tests.rs` (52 LOC — light).
6. `src/validate/invariants.rs` — 588 LOC, 0 inline. Covered by `tests/it/invariants_tests.rs`.
7. `src/cli/common.rs` — 484 LOC, 0 inline. `parse_heatmap`, `to_json_pretty`, `print_*` all untested.
8. `src/gen/generation/factions.rs` — 488 LOC, 0 inline.
9. `src/loading/config.rs` — 436 LOC, 0 inline. `OutputFormat::parse_token` not covered anywhere.
10. `src/gen/generation/world_placement.rs` — 350 LOC, 0 inline.
11. `src/export/writers.rs` — 305 LOC, 0 inline. `export_json`, `export_bundle`, `export_all` only via CLI smoke.
12. `src/export/heatmap.rs` — 293 LOC, 0 inline. `score_sector`, `compute_rgb` untested.
13. `src/loading/input.rs` — 268 LOC, 0 inline. `load_project` exercised end-to-end only.
14. `src/gen/generation/routes.rs` — 246 LOC, 0 inline.
15. `src/analysis/history/build.rs` — 228 LOC, 0 inline (covered by `analysis/history/tests.rs` for top-level `derive` only).
16. `src/analysis/history/{model,subsectors,worlds,systems,config,markdown,rules,routes,regions,labels,progress,build,context}.rs` — collectively ~ 2 000 LOC, 0 inline tests outside `tests.rs` smoke.
17. `viewer/src/factions_overview.rs` — 1349 LOC, only 2 tests for `designer_rows_to_factions_file`. ~ 4 other `pub fn` are untested.
18. `viewer/src/segmentum_view.rs` — 798 LOC, 0 inline.
19. `viewer/src/route_planner.rs` — 471 LOC, 0 inline.
20. `viewer/src/app/sector_view.rs` — 689 LOC, 0 inline (much of it is egui glue, but `Plan` arithmetic helpers exist).

## Coordination with U022 (`tests/it/`)

Findings raised **here** (X08): inline-only — sleep test (F-X08-001), wall-clock assert (F-X08-002), assertion-free test (F-X08-003), DRY-violation in export fixtures (F-X08-010), inline coverage gaps (F-X08-006/007/008).

Findings deferred to **U022**: anything about `tests/it/` performance, flakiness, or coverage of `tests/it/` itself (e.g. the lightness of `validation_tests.rs` (52 LOC) for a 605-LOC module).

Findings that are **joint and should be cross-referenced** in the aggregator:
- The TOML-parser proptest gap (F-X08-004) applies to both — proptests can live either inline or in `tests/it/`; U022 should consider where they belong.
- The fuzz-target gap (F-X08-005) is workspace-wide; new `fuzz/` directory belongs at workspace root, not under either reviewer.
- Coverage report absence (F-X08-017) — U022 should be the one to publish the per-test timing baseline (`cargo nextest run --report`), and X08 owns the line-coverage baseline (`cargo llvm-cov`).

## Rubric categories (per-§ closure)

- **§3.1 Panics & failure surface** — see X07; tests sweep notes only: most tests use `.unwrap()` liberally, which is acceptable in test code but means a real panic regression is reported as a test failure, not a typed error. No action.
- **§3.2 unsafe** — N/A in tests. No findings.
- **§3.3 Ownership/borrowing/cloning** — Test fixtures clone freely; acceptable. No findings.
- **§3.4 Error handling** — Tests use `.unwrap()` / `.expect()` consistently; one finding (F-X08-019) on consistency of round-trip assertion style.
- **§3.5 Concurrency** — F-X08-001 (sleep) and F-X08-002 (wall-clock) are the only concurrency-flavoured test issues. No threading data-race risk found inline.
- **§3.6 Performance** — F-X08-001 dominates inline-test wall-clock cost.
- **§3.7 Idiomatic Rust** — F-X08-018 (HashSet vs BTreeSet in tests) is the only call-out.
- **§3.8 Dependencies** — Not in scope; tests don't add new deps.
- **§3.9 Memory/resource** — No findings — tests use `tempfile::TempDir` correctly throughout (good pattern).
- **§3.10 Testing** — The whole sweep. See findings.
- **§3.11 Documentation** — F-X08-009 (doctests on public API).

## Summary of suggested fixes

| ID | Severity | Short description | Effort | Risk |
|---|---|---|---|---|
| F-X08-001 | HIGH | Replace sleep-based `file_watcher` test with injected clock / pure `scan_once` helper | M | Low |
| F-X08-002 | HIGH | Delete wall-clock 500 ms assertion in `spawn_job` test; `try_recv == Empty` already proves non-blocking dispatch | S | Low |
| F-X08-003 | HIGH | Add real assertion to `region_overlay_tokens_cover_all_conditions` (currently `let _ = ...`) | S | Low |
| F-X08-004 | HIGH | Add property-test round-trips for `AppConfig`, `OutputFormat::parse_token`, `WorldsConfig`, and inline unit tests for `loading/config.rs` | M | Low |
| F-X08-005 | MEDIUM | Stand up `fuzz/` workspace with harnesses for `AppConfig`, `WorldsConfig`, `load_sector_json` | M | Low |
| F-X08-006 | MEDIUM | Add inline tests to viewer pure helpers (`route_planner`, `segmentum_view`, `editor/state`) | L | Low |
| F-X08-007 | MEDIUM | Add inline unit tests for CLI parser helpers (`parse_heatmap` etc.) | S | Low |
| F-X08-008 | MEDIUM | Add inline tests to the biggest 0-inline-test modules per the coverage-gap list (priority: heatmap, validation helpers, loading) | L | Low |
| F-X08-009 | MEDIUM | Add `# Examples` doctests to the top ~ 6 public entry points | M | Low |
| F-X08-010 | MEDIUM | Extract shared `sample_sector_fixture()` for export tests | S | Low |
| F-X08-011 | MEDIUM | Strengthen bitmap inline tests with hash-of-output, or delete as redundant with `tests/it/golden_png.rs` | S | Low |
| F-X08-012 | LOW | Add `proptest!` for `weighted_index` distribution invariants | S | Low |
| F-X08-013 | LOW | Add `proptest!` for `insert_top_n` ordering invariants | S | Low |
| F-X08-014 | LOW | Refactor `info_panel` to expose pure text builders for unit testing | L | Medium |
| F-X08-015 | LOW | Extend `viewer/src/editor/state.rs` inline tests (3 additional cases) | S | Low |
| F-X08-016 | LOW | Add invariant tests for `analysis/personae`, `analysis/influence_field` | M | Low |
| F-X08-017 | LOW | Publish `cargo llvm-cov` baseline so coverage gaps can be re-ranked with real data | S | Low |
| F-X08-018 | NIT | Use `BTreeSet` over `HashSet` in test code (project-wide determinism convention) | S | Low |
| F-X08-019 | NIT | Make TOML round-trip assertions consistent (parse-then-`assert_eq!`) | S | Low |
