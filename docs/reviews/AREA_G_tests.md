# AREA G — tests — verification

Verified 2026-06-05 against live code in `tests/it/` and `viewer/src/app/lifecycle.rs`; all `path:line` citations are exact as of this date.

## Summary table

| ID   | Sev | Status                   | Effort | One-line                                                              |
|------|-----|--------------------------|--------|-----------------------------------------------------------------------|
| G-S1 | —   | ✅ Confirmed (gap real)  | M      | All 4 analysis suites claim "proptest" in doc-comments; zero proptest! macros exist |
| G-S2 | —   | ✅ Confirmed (gap real)  | S      | `fixture_dir()+OnceLock` block duplicated in 4 files after `shared.rs` landed |
| G-S3 | —   | ✅ Confirmed (gap real)  | M      | No committed content golden for `sector.json`/`sector.md`; only structural checks |
| G1   | HIGH | ✅ Confirmed (gap real) | M      | Four suites' module-doc claims "many random seeds (proptest)" — no proptest! exists |
| G2   | HIGH | ✅ Confirmed (gap real) | M      | `golden_generation.rs` checks file existence + structural fields; zero content hash pin |
| G3   | HIGH | ✅ Confirmed (gap real) | S      | All 5 segmentum tests carry `#[ignore]`; none are cheap enough to justify it |
| G4   | MED  | ✅ Confirmed (gap real) | S      | `export_writes_all_expected_files` uses only Json+Markdown; Html/Bitmap never dispatched |
| G5   | MED  | ⚠️ Partial               | S      | 4× dup still live (economy/hooks/personae/relations); `invariants_proptest` uses inline `fixture_dir()` only (no OnceLock) |
| G6   | MED  | 🔄 Moved (line drift)   | M      | `lifecycle.rs` tests exist at lines 342-402 but cover only progress math; `write_sector_to_path` has zero tests |
| G7   | MED  | ✅ Confirmed (gap real) | S      | `system_count` derived from `width×height` formula at line 62; never a proptest dimension |
| G8   | LOW  | ✅ Confirmed (gap real) | S      | `diff_after_ticks` test at line 202 asserts only "doesn't crash + `starts_with`" |
| G9   | LOW  | ✅ Confirmed (gap real) | S      | Metrics `u8` clamp explicitly skipped with a comment at line 91; only JSON round-trip checked |
| G10  | LOW  | ✅ Confirmed (gap real) | S      | SVG test uses substring-only assertions; no blake3 hash pin exists                  |

---

### G-S1 — systemic: doc-vs-reality determinism gap

- **Review sev / bucket:** systemic / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/economy_tests.rs:5`, `tests/it/hooks_tests.rs:5`, `tests/it/personae_tests.rs:5`, `tests/it/relations_tests.rs:5`
- **Evidence:**
  ```rust
  //! 1. Determinism — same sector ⇒ byte-identical `EconomyReport` JSON, across
  //!    many random seeds (proptest).
  ```
  All four files open with identical phrasing. Zero `proptest!` macros exist in any of them — confirmed by grep returning only the doc-comment lines.
- **Why it matters:** A reader trusts the module doc and assumes seed-varying coverage exists. Auditors and future contributors are misled about the actual test surface.
- **Fix:** Either add a `proptest!` block that generates the sector from random seeds and checks byte-identity of the derived report, or replace the claim with "same fixture seed" to describe what the test actually does.
- **Effort:** M
- **Risk / deps:** Proptest runs add CI time (~few seconds per suite at 24 cases); or doc fix is S.

---

### G-S2 — systemic: fixture boilerplate duplicated 5×

- **Review sev / bucket:** systemic / P2
- **Status:** ⚠️ Partial
- **Location:** `tests/it/economy_tests.rs:18-28`, `hooks_tests.rs:20-30`, `personae_tests.rs:20-30`, `relations_tests.rs:20-30`; `shared.rs` already exists at `tests/it/shared.rs:13-23`
- **Evidence:**
  ```rust
  fn fixture_dir() -> Utf8PathBuf {
      Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
  }
  fn fixture_sector() -> &'static GeneratedSector {
      static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
  ```
  Byte-identical block in all four analysis suites. `shared.rs` already provides `fixture_dir()` and `fixture_sector()` but the four files do not use it — they each re-declare private copies.
- **Why it matters:** Each private `OnceLock` generates a redundant sector in the same test process. A change to the fixture path must be made in five places.
- **Fix:** Replace the four private `fn fixture_dir()` + `fn fixture_sector()` blocks with `use crate::shared::{fixture_dir, fixture_sector};` (same pattern `search_and_diff.rs` already uses at line 7).
- **Effort:** S
- **Risk / deps:** None; `shared.rs` exports are already `pub`.

---

### G-S3 — systemic: writer/format coverage gap

- **Review sev / bucket:** systemic / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/golden_generation.rs:64-76`, `tests/goldens/` (only `png_m42_default.blake3` present)
- **Evidence:**
  ```rust
  assert!(tmp_path.join("sector.json").exists());
  assert!(tmp_path.join("sector.md").exists());
  assert_exported_sector_matches(&tmp_path, sector);
  ```
  `assert_exported_sector_matches` (line 184) round-trips `seed`, `systems.len()`, `routes.len()`, `world_count` — structural fields only. No byte-level content pin exists for the text outputs. Only `tests/goldens/png_m42_default.blake3` is committed.
- **Why it matters:** A bug that changes JSON field names or markdown section headers passes all tests as long as the round-trip counts match.
- **Fix:** See G2 (they are the same gap).
- **Effort:** M
- **Risk / deps:** Requires `UPDATE_GOLDEN_JSON=1` / `UPDATE_GOLDEN_MD=1` bootstrap step; gates safe god-file refactors (see G2).

---

### G1 — false "many random seeds" docs in four analysis suites

- **Review sev / bucket:** HIGH / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/economy_tests.rs:5`, `tests/it/hooks_tests.rs:5`, `tests/it/personae_tests.rs:5`, `tests/it/relations_tests.rs:5`
- **Evidence:**
  ```rust
  //! 1. Determinism — same sector ⇒ byte-identical `EconomyReport` JSON, across
  //!    many random seeds (proptest).
  ```
  Confirmed: grep over all four files finds `proptest` only in these doc-comment lines. The actual determinism check (e.g. `economy_tests.rs:151-157`) re-calls `derive_with` twice on the **same** memoized fixture sector — that is idempotency, not seed-varying reproducibility.
- **Why it matters:** A regression in the RNG stage-key pipeline that produces a different-but-internally-consistent output for a different fixture seed goes undetected by these suites. Only `invariants_proptest.rs` exercises multiple seeds, and it does not run the analysis derivations.
- **Fix:** Add a `proptest!` block to each suite:
  ```rust
  proptest! {
      #[test]
      fn economy_derive_is_deterministic_across_seeds(seed in "[a-z0-9]{4,12}") {
          let mut input = load_project(fixture_dir()).unwrap();
          input.config.generation.seed = seed;
          let s = generate_sector(input).unwrap();
          let a = economy::derive_with(&s, &enabled_cfg());
          let b = economy::derive_with(&s, &enabled_cfg());
          prop_assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
      }
  }
  ```
  Or, as a minimal fix, replace the doc-comment claim with "same fixture seed".
- **Effort:** M (4 suites; proptest is already a dev-dependency via `invariants_proptest.rs`)
- **Risk / deps:** Each proptest run adds 24 case × ~20 ms; acceptable in CI.

---

### G2 — no committed content golden for sector.json / sector.md

- **Review sev / bucket:** HIGH / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/golden_generation.rs:64-76`, `tests/it/golden_generation.rs:184-190`
- **Evidence:**
  ```rust
  fn assert_exported_sector_matches(output_dir: &Utf8PathBuf, expected: &GeneratedSector) {
      let exported = sectorforge::load_sector_json(output_dir.join("sector.json")).unwrap();
      assert_eq!(exported.seed.as_ref(), expected.seed.as_ref());
      assert_eq!(exported.systems.len(), expected.systems.len());
  ```
  No blake3 hash of the raw `sector.json` bytes, no line-count or content check of `sector.md`. Only `tests/goldens/png_m42_default.blake3` is committed. `sector.md` is only tested for existence (line 71).
- **Why it matters:** A deterministic-but-wrong text change (renamed field, reordered JSON key, dropped markdown table row) passes all tests. This is the safety net needed before any god-file split in areas A–F touches serialisation or markdown rendering.
- **Fix:** Mirror `golden_png.rs` pattern: generate once, hash the raw file bytes with blake3, commit the hash. Add `UPDATE_GOLDEN_JSON=1` / `UPDATE_GOLDEN_MD=1` env gates. Blessed files live in `tests/goldens/`.
- **Effort:** M
- **Risk / deps:** GATES god-file splits in areas A–F. Land this first (see Suggested local order).

---

### G3 — all 5 segmentum tests carry #[ignore]

- **Review sev / bucket:** HIGH / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/segmentum_tests.rs:50`, `68`, `86`, `106`, `119`
- **Evidence:**
  ```rust
  #[test]
  #[ignore = "slow: full m42 composition; run with `cargo test --test it segmentum -- --ignored`"]
  fn compose_is_byte_deterministic() {
  ```
  All five tests carry the identical `#[ignore]` rationale. Two of them — `duplicate_child_slot_is_rejected` (line 107) and `different_stitch_seed_can_change_links` (line 87) — perform only structural/error-path assertions that do not require the full two-sector composition that makes `compose_produces_children_and_links` slow.
- **Why it matters:** Byte-determinism of `compose_segmentum` and the duplicate-slot rejection path never run in default CI. A regression in either goes undetected until someone manually passes `--ignored`.
- **Fix:** Split out the two cheap tests: `duplicate_child_slot_is_rejected` calls `compose_segmentum` only to get an error (fast-fail), and `different_stitch_seed_can_change_links` may be slow but only checks manifest metadata. Remove `#[ignore]` from `duplicate_child_slot_is_rejected` immediately; benchmark `different_stitch_seed_can_change_links` and remove `#[ignore]` if it completes in under ~3 s on the CI box.
- **Effort:** S
- **Risk / deps:** CI time increase proportional to composition runs retained without `#[ignore]`.

---

### G4 — Html / Bitmap never exercised through export_sector dispatch

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/golden_generation.rs:64-76`, `tests/it/golden_generation.rs:168-180`
- **Evidence:**
  ```rust
  fn text_export_config() -> OutputConfig {
      let mut cfg = json_export_config(false);
      cfg.formats.push(OutputFormat::Markdown);
      cfg.write_manifest = true;
      cfg
  }
  ```
  `json_export_config` hard-codes `vec![OutputFormat::Json]`; `text_export_config` adds only `Markdown`. `OutputFormat::Html` and `OutputFormat::Bitmap` are never pushed; the `export_sector` code path for those two variants is untouched by the integration suite. `golden_png.rs` tests the bitmap rasteriser directly but bypasses `export_sector` dispatch.
- **Why it matters:** A bug in the `Html` or `Bitmap` arm of `export_sector` (wrong file extension, missing directory creation, panicking encoder) would not trip any test.
- **Fix:** Add two helper configs and two test functions in `golden_generation.rs`:
  ```rust
  fn bitmap_export_config() -> OutputConfig { ... cfg.formats = vec![OutputFormat::Bitmap]; ... }
  fn html_export_config()   -> OutputConfig { ... cfg.formats = vec![OutputFormat::Html];   ... }
  ```
  Assert that `sector.png` / `sector.html` exist and are non-empty (no content pin needed for the initial PR).
- **Effort:** S
- **Risk / deps:** Bitmap render adds ~0.5 s per run; Html is fast.

---

### G5 — fixture OnceLock boilerplate duplicated 4× (partially addressed by shared.rs)

- **Review sev / bucket:** MED / P2
- **Status:** ⚠️ Partial
- **Location:** `tests/it/economy_tests.rs:18`, `hooks_tests.rs:20`, `personae_tests.rs:20`, `relations_tests.rs:20`
- **Evidence:**
  ```rust
  fn fixture_dir() -> Utf8PathBuf {
      Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
  }
  fn fixture_sector() -> &'static GeneratedSector {
      static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
  ```
  `shared.rs` already provides both functions (lines 13-23) and `search_and_diff.rs` already uses `use crate::shared::{fixture_dir as fixture_project, fixture_sector};`. The four analysis suites pre-date the `shared.rs` extraction and were not updated. `invariants_proptest.rs` uses its own inline `fixture_dir()` (no OnceLock), which is intentional (it mutates the input before generating).
- **Why it matters:** Four redundant sector generations per test process; a path change requires five edits.
- **Fix:** In each of the four analysis suites, delete the private `fixture_dir`/`fixture_sector` functions and add `use crate::shared::{fixture_dir, fixture_sector};`. The review counts this as 5× including `invariants_proptest.rs`, but that file's `fixture_dir` is deliberately private (no OnceLock, mutation path) — partial fix of 4/5 is correct.
- **Effort:** S
- **Risk / deps:** None.

---

### G6 — viewer document-write paths have zero tests

- **Review sev / bucket:** MED / P2
- **Status:** 🔄 Moved (line drift)
- **Location:** `viewer/src/app/lifecycle.rs:190-235` (was cited as line 109; `write_sector_to_path` starts at line 190 in the live file)
- **Evidence:**
  ```rust
  pub(super) fn write_sector_to_path(&mut self, path: PathBuf) {
      let Some(sector) = self.sector.as_mut() else { ... };
      let sector = Arc::make_mut(sector);
      Self::refresh_live_manifest_counts(sector);
      let text = match serde_json::to_string_pretty(sector) {
  ```
  The `#[cfg(test)]` block at line 337 contains four tests covering `fraction()` and `preview_progress()` (pure math helpers) — not `write_sector_to_path`, `save_sector_as`, or `set_loaded_sector`. The write path (encode → mkdir → `fs::write`) has no unit test. The review's line 109 citation has drifted; the actual `write_sector_to_path` is at line 190.
- **Why it matters:** Encoding errors, permission errors, or a bad `refresh_live_manifest_counts` mutation only surface at runtime. A round-trip test (serialize sector → write to tempfile → read back → compare fields) would catch serialisation regressions in the viewer's save path independently of the export pipeline.
- **Fix:** Extract a `pub(crate) fn sector_to_json_bytes(sector: &GeneratedSector) -> Result<Vec<u8>, serde_json::Error>` from `write_sector_to_path`, then add a `#[test]` that calls it with a known sector and deserialises the result. The `FileDialog` and `fs::write` callers remain untested (GUI + I/O boundary), but the encode core becomes testable.
- **Effort:** M
- **Risk / deps:** Requires making a helper function `pub(crate)`; no cross-crate changes.

---

### G7 — system_count never fuzzed in invariants_proptest

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/invariants_proptest.rs:60-63`
- **Evidence:**
  ```rust
  let cells = (width as usize) * (height as usize);
  let system_count = (cells.saturating_mul(2) / 5).clamp(2, cells);
  run_one(&seed, width, height, system_count, min_worlds, max_worlds, density)
  ```
  `system_count` is derived deterministically from `width` and `height` — it is not an independent proptest strategy dimension. The `run_one` parameter exists but is always pre-computed; low-density edge cases (1 system, `cells-1` systems) are never exercised.
- **Why it matters:** Generation bugs that only appear at extreme system counts (0-density, fully-packed) pass the suite. The invariant check covers 24 `(seed, width, height)` triples but maps them all to a single density formula.
- **Fix:** Add `system_count in 2usize..=cells` as an explicit strategy in `invariants_hold_across_random_inputs`, or add a second proptest that holds `width`/`height` fixed and varies `system_count` across `[2, cells]`.
- **Effort:** S
- **Risk / deps:** More generation runs per test case; keep `cases: 24` to cap CI time.

---

### G8 — diff_after_ticks asserts only "doesn't crash"

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/search_and_diff.rs:202-214`
- **Evidence:**
  ```rust
  // Tick advancement is allowed to produce zero observable diff if no
  // contested worlds existed; we just assert the call succeeds and the
  // serialisation round-trips.
  let md = sectorforge::render_diff_markdown(&diff);
  assert!(md.starts_with("# Sector Diff"));
  assert!(md.contains("Catalog compatible"));
  ```
  The comment explicitly acknowledges the assertion is a crash-guard only. No check verifies that after 5 ticks some state actually changed; all delta fields (`systems_changed`, `faction_deltas`, etc.) could be empty and the test passes.
- **Why it matters:** If `advance_sector` silently becomes a no-op, this test still passes.
- **Fix:** Use `diff_distinct_seeds_produces_changes` (line 228, which already asserts `total > 0`) as a model. After 5 ticks on the m42 fixture, assert that at least one of `systems_changed` or `faction_deltas` is non-empty — or if the fixture genuinely produces no change, document it and use a fixture that does.
- **Effort:** S
- **Risk / deps:** May require adjusting the fixture if the m42 project has no contested worlds.

---

### G9 — relations metrics 0..=100 clamp not asserted

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/relations_tests.rs:81-95`
- **Evidence:**
  ```rust
  // Metrics are `u8` — already 0..=255; spec uses 0..=100 ranges. Don't
  // hard-assert the inner clamp (allow future tuning) — just ensure
  // they round-trip through JSON.
  let _ = serde_json::to_string(&p.metrics).expect("metric serializes");
  ```
  The comment explicitly skips the documented clamp. The `tension` field is checked for `0.0..=100.0` but the five `u8` metric fields (trust, fear, rivalry, treaty, public) are only round-trip checked.
- **Why it matters:** A future tuning change that produces `u8` values above 100 is invisible to the test suite; downstream rendering that relies on the 0-100 scale breaks silently.
- **Fix:** Add explicit range assertions for each metric field. If the spec intentionally allows `u8` values above 100, update the spec comment and remove the range from the doc; don't leave a discrepancy.
- **Effort:** S
- **Risk / deps:** None; pure assertion addition.

---

### G10 — SVG export uses substring-only assertions, no hash pin

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed (gap real)
- **Location:** `tests/it/svg_export_tests.rs:7-26`
- **Evidence:**
  ```rust
  assert!(svg.starts_with("<?xml"));
  assert!(svg.contains("<svg"));
  assert!(svg.contains("<polygon"), "expected at least one hex polygon");
  assert!(svg.contains("SECTOR:"), "expected legend title to be present");
  ```
  No blake3 hash or byte-level pin. The second test (`writes_svg_file_to_disk`, line 29) asserts only `body.len() > 4096`. In contrast, `golden_png.rs` pins a committed hash in `tests/goldens/png_m42_default.blake3`.
- **Why it matters:** A regression that changes the SVG coordinate system, drops a route element, or silently alters the XML namespace passes as long as the six substrings are present.
- **Fix:** Add a `UPDATE_GOLDEN_SVG=1`-gated blake3 test mirroring `golden_png.rs:87-108`. Commit the initial hash to `tests/goldens/svg_m42_default.blake3`.
- **Effort:** S
- **Risk / deps:** SVG render may be sensitive to font metrics or platform float rounding; verify hash stability across OS before committing.

---

## Suggested local order

1. **G2 first** — pin `sector.json` / `sector.md` content goldens behind `UPDATE_*`. This is the safety net that makes all subsequent god-file splits in areas A–F safe to land. Do not start any A–F refactor without it.
2. **G3** — remove `#[ignore]` from at least `duplicate_child_slot_is_rejected`; benchmark and decide on `different_stitch_seed_can_change_links`. Low effort, immediate CI improvement.
3. **G5** — delete the 4× `fixture_dir`/`fixture_sector` clones in the analysis suites; replace with `use crate::shared::…`. Mechanical, zero risk.
4. **G4** — add `Html` and `Bitmap` export smoke tests; catches the most common class of dispatch bug.
5. **G1** — add seed-varying proptest blocks to the four analysis suites, or fix the doc-comments. Decide based on CI budget.
6. **G7** — make `system_count` an independent proptest dimension.
7. **G6** — extract `sector_to_json_bytes` from viewer lifecycle and add a round-trip test.
8. **G8, G9, G10** — low-effort assertion additions; batch them into one PR.
