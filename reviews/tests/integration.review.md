---
unit_id: U022
crate: tests
paths:
  - tests/it.rs
  - tests/it/analytics_and_presets.rs
  - tests/it/cli_gui_parity.rs
  - tests/it/economy_tests.rs
  - tests/it/golden_generation.rs
  - tests/it/golden_png.rs
  - tests/it/hooks_tests.rs
  - tests/it/imports_test.rs
  - tests/it/invariants_proptest.rs
  - tests/it/invariants_tests.rs
  - tests/it/personae_tests.rs
  - tests/it/relations_tests.rs
  - tests/it/search_and_diff.rs
  - tests/it/segmentum_tests.rs
  - tests/it/svg_export_tests.rs
  - tests/it/validation_tests.rs
  - benches/generation.rs
  - gui-core/tests/map_snapshots.rs
loc_reviewed: 2434
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 9, low: 6, nit: 3 }
top_risks:
  - "Per-test fresh generate_sector + load_project repeated O(60+) times — dominates suite wall-clock (F-022-001)"
  - "Five segmentum tests permanently #[ignore]d → byte-stability never gated on segmentum composition (F-022-003)"
  - "No CLI subcommand has any integration coverage except `generate` via cli_gui_parity (F-022-004)"
  - "PNG golden test doesn't pin a hash — silently tolerates per-platform rasteriser drift (F-022-006)"
  - "file_watcher test sleeps 1.2 s + polls up to 6 s — only slow test in builder unit suite (F-022-002)"
---

# Review: U022 — tests/, benches/, gui-core snapshots

## Summary

The integration suite is small (≈2.3 kLOC across 15 files plus a 700-LOC snapshot test and a 125-LOC bench) and well-organised under a single `tests/it.rs` consolidated binary, which is the correct optimisation (one linker pass vs. fifteen). Determinism is the dominant theme — almost every file has a "same input ⇒ byte-identical JSON" assertion, and four files lift the same property into proptest with `cases: 16`. The trustworthy work is on the **happy path with the m42 fixture**; weaknesses are (a) the same fixture is re-generated dozens of times per run because `OnceLock` caching is only adopted in 5 of 15 files, (b) every segmentum integration test is `#[ignore]`d so segmentum composition is effectively unprotected by CI, (c) CLI subcommands beyond `generate` have zero coverage, and (d) the "golden PNG" test asserts only that two in-process renders agree — it does not pin a hash, so a rendering regression that is *itself* deterministic would pass.

## Baseline & measurement note

`cargo nextest` is **not** installed in this environment, and direct test execution is sandbox-blocked, so wall-clock numbers below are estimated from code shape (counts of `generate_sector`, `compose_segmentum`, proptest `cases`, fixture caching). A real baseline should be captured with:

```bash
cargo nextest run --workspace --no-fail-fast 2>&1 | tee reviews/_baseline/nextest.txt
# or, since this project lacks nextest config:
cargo test --workspace -- -Z unstable-options --report-time --test-threads=1   # nightly
```

The "expected speedup" column below is a code-relative estimate. Land the baseline before applying any of the S-effort fixes — that way the next reviewer can replace the estimates with real numbers.

## Findings

### F-022-001 — [HIGH] [Tests] Fixture not memoised in 10 of 15 integration files
- **Location:** `tests/it/invariants_tests.rs:7-308` (10 calls to `generate_sector`), `tests/it/search_and_diff.rs:13-229` (5 calls + 3 `run_seed_search`), `tests/it/analytics_and_presets.rs:15-110` (3 calls), `tests/it/svg_export_tests.rs:10-41` (2 calls), `tests/it/validation_tests.rs:5-52`
- **Category:** Tests / Speed
- **Confidence:** High
- **Blast radius:** Whole `it` binary
- **Problem:** Five files (`golden_generation.rs`, `personae_tests.rs`, `economy_tests.rs`, `hooks_tests.rs`, `relations_tests.rs`) correctly cache the m42 sector in a `static SECTOR: OnceLock<GeneratedSector>` so the per-file cost is one generation. The other ten files call `sectorforge::load_project(fixture_project())` + `sectorforge::generate_sector(input)` inside **every** test function. `invariants_tests.rs` alone does this 10× per `cargo test` run; `search_and_diff.rs` adds 5 more; `analytics_and_presets.rs` adds 3; `validation_tests.rs` adds another; `svg_export_tests.rs` adds 2. With the project at 24 systems / 8×10 grid each `generate_sector` is non-trivial (this is the dominant cost in `benches/generation.rs:32-56`). Conservatively this is ≥ 25 redundant generations per run.
- **Why it matters:** Slowest 10 % dominates suite time. Tests inside the same binary share statics, so the moment-of-truth fix is one `OnceLock` per file, sharing the same fixture across every `#[test]`. Saves an estimated **40-60 %** of total `it` binary wall-clock.
- **Evidence:** see counts above; cached pattern already established at `tests/it/golden_generation.rs:143-160`, `tests/it/personae_tests.rs:25-31`.
- **Suggested fix:** Promote the cached fixture into a shared module. Add `tests/it/common.rs`:
  ```rust
  use std::sync::OnceLock;
  use camino::Utf8PathBuf;
  use sectorforge::{GeneratedSector, ProjectInput};

  pub fn fixture_dir() -> Utf8PathBuf {
      Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
  }
  pub fn fixture_input() -> &'static ProjectInput {
      static INPUT: OnceLock<ProjectInput> = OnceLock::new();
      INPUT.get_or_init(|| sectorforge::load_project(fixture_dir()).expect("load m42"))
  }
  pub fn fixture_sector() -> &'static GeneratedSector {
      static SECTOR: OnceLock<GeneratedSector> = OnceLock::new();
      SECTOR.get_or_init(|| sectorforge::generate_sector(fixture_input().clone()).expect("generate"))
      }
  ```
  Wire via `#[path = "it/common.rs"] mod common;` in `tests/it.rs`, then `use crate::common::*;` from each test file. Tests that need a *mutated* sector still clone from the cached `GeneratedSector`. Tests that need a *different seed* keep their `sector_with_seed` helper but call `fixture_input().clone()` instead of reloading the project from disk.
- **Effort:** S
- **Risk of fix:** Low — pure refactor, no assertion changes. Mutating tests (`invariants_tests.rs:33-93`) already mutate via `.clone()` semantics; they need a `.clone()` of the cached `&GeneratedSector` which is what they do today.

### F-022-002 — [HIGH] [Tests] Sleep-based file watcher unit test slows builder unit-test binary
- **Location:** `builder/src/builder/file_watcher.rs:134-170`
- **Category:** Tests / Speed + Flakiness
- **Confidence:** High
- **Blast radius:** `sectorforge_builder` unit-test binary (one of the two binaries that bound CI wall-clock)
- **Problem:** `detects_mtime_bump` sleeps **1200 ms** (line 149) to clear filesystem mtime resolution, spawns a watcher, then polls up to **30 × 200 ms = 6 000 ms** for an event (lines 161-167). Total worst case ≈ 7.2 s for a single test. On the fast path it's still ≥ 1.4 s. This is also a flakiness exposure: on macOS APFS the mtime granularity is 1 ns; the 1.2 s is overcautious for APFS but undercautious for NFS / SMB. There is no timeout assertion (`expect` returns silently on `None`).
- **Why it matters:** §6.5 speed + trustworthiness double-hit. Single test ≈ 5-30 % of the builder unit-test binary depending on machine.
- **Evidence:** see lines 149, 166. Code uses `std::fs::metadata().modified()` baseline and re-reads after write.
- **Suggested fix:** Two-step.
  1. Replace polling with a `crossbeam_channel::Receiver::recv_timeout(Duration::from_secs(3))` on the watcher's outgoing channel (the watcher already uses `mpsc` per `file_watcher.rs:112`). Total wall-clock drops from up to 7.2 s to actual-detection-latency (often < 50 ms after the rewrite).
  2. Inject a fake clock or pass the mtime directly: factor `FileWatcher::spawn` to take a `now: impl Fn() -> SystemTime` closure (or use `filetime::set_file_mtime` after the rewrite to *force* an mtime > baseline without sleeping). The 1.2 s sleep then disappears entirely.
  ```rust
  // After write:
  filetime::set_file_mtime(target.as_std_path(),
      filetime::FileTime::from_system_time(baseline_mtime + Duration::from_secs(1))).unwrap();
  ```
  Target time: < 100 ms.
- **Effort:** M
- **Risk of fix:** Low — `filetime` is a tiny dep; the channel timeout is the existing channel.

### F-022-003 — [HIGH] [Tests] All five segmentum integration tests are `#[ignore]`d → composition is effectively untested in CI
- **Location:** `tests/it/segmentum_tests.rs:49-131` (all five `#[test]` fns carry `#[ignore = "slow: full m42 composition…"]`)
- **Category:** Tests / Trustworthiness + Coverage
- **Confidence:** High
- **Blast radius:** `src/export/segmentum.rs` (1,168 LOC per inventory) — byte-stability, child-slot collision detection, `compose_segmentum` deterministic hash, write-artifact emission. None of these are gated by a default `cargo test --workspace`.
- **Problem:** The CLAUDE.md determinism contract claims byte-stable output writers; `compose_segmentum` produces `segmentum.json` / `super_manifest.json` that are part of that contract. The current state means a PR that breaks segmentum byte stability passes CI silently. The `#[ignore]` rationale ("slow: full m42 composition") is real but unbounded: there is no shrunken fixture and no smoke variant.
- **Why it matters:** §6.5 explicitly: "each `#[ignore]`d test is either dead weight to delete or a real gap to re-enable; never leave it ambiguous". Five `#[ignore]`s in one file is a strong signal — segmentum testing has been parked rather than dropped.
- **Evidence:** every `#[test]` at lines 49, 67, 85, 105, 118 is preceded by an `#[ignore]`. The default test run executes zero segmentum assertions.
- **Suggested fix:**
  1. Build a `mini_project` fixture: 2×1 super-grid of 4-system children (the bench's "tiny" scale, `benches/generation.rs:27`). Per-child generation ≈ 1/6 of m42, so two children compose in ~⅓ the time of one m42 today.
  2. Convert `duplicate_child_slot_is_rejected` (lines 105-116) to **not need composition at all** — it asserts on an error path that fires before any work; remove the ignore.
  3. Keep one `compose_is_byte_deterministic` running on the mini fixture **un-ignored**; gate the full-m42 variant behind `#[ignore]` so `cargo test -- --ignored` retains the heavy check.
- **Effort:** M
- **Risk of fix:** Medium — needs a new fixture under `examples/`; if the mini fixture diverges from m42 in some asserted-on way (e.g., faction inventory) the tests must be adapted.

### F-022-004 — [HIGH] [Tests] CLI subcommands other than `generate` have zero integration coverage
- **Location:** `src/cli/*.rs` (20 files: `analyze.rs`, `briefing.rs`, `compose.rs`, `diff.rs`, `economy.rs`, `history.rs`, `hooks.rs`, `interestingness.rs`, `missions.rs`, `personae.rs`, `presets.rs`, `prose.rs`, `regions.rs`, `relations.rs`, `search.rs`, `sites.rs`, `validate.rs`); the only CLI test is `tests/it/cli_gui_parity.rs:22-84` which spawns `sectorforge generate`.
- **Category:** Tests / Coverage
- **Confidence:** High
- **Blast radius:** Every `sectorforge <subcommand>` invocation. A clap argument refactor, an output-path change, or an exit-code regression in any of 18 subcommands ships unnoticed.
- **Problem:** The library functions that back each subcommand (`analyze_sector`, `derive_personae`, `compose_segmentum`, …) **are** tested at the library boundary, but the clap wiring, argument validation, exit codes, and error-path stderr messages are not. The `cli_gui_parity.rs` shape is exactly the right template — `env!("CARGO_BIN_EXE_sectorforge")` + a tempdir + `Command`.
- **Suggested fix:** One parameterised CLI smoke test that walks every subcommand against the m42 fixture with `--help` (cheap, ~1 ms per subcommand) plus a small number of full invocations on the hot-path subcommands (`analyze`, `diff`, `validate`, `search`, `relations`, `economy`, `personae`, `hooks`). Use a table:
  ```rust
  const SMOKE: &[&str] = &[
      "analyze", "validate", "relations", "economy", "personae", "hooks", "interestingness",
  ];
  #[test]
  fn cli_subcommands_succeed_on_fixture() {
      let bin = env!("CARGO_BIN_EXE_sectorforge");
      let tmp = tempfile::tempdir().unwrap();
      for cmd in SMOKE {
          let out = tmp.path().join(cmd);
          let s = Command::new(bin).args([cmd, "--project", FIXTURE, "--out",
              out.to_str().unwrap(), "--allow-warnings"]).status().unwrap();
          assert!(s.success(), "{cmd} exited {s:?}");
      }
  }
  ```
  Hint: keep the spawned binaries serial (`#[test]` is fine, but make sure the parent tests don't allocate `tempfile::TempDir` per iteration; one tempdir is enough). Pin the budget at ~5 s total wall-clock by skipping subcommands that themselves run `generate` from scratch.
- **Effort:** M
- **Risk of fix:** Low — `cli_gui_parity.rs` proves the spawn-and-assert pattern works.

### F-022-005 — [MEDIUM] [Tests] Single integration binary swallows `--list` filters that fail per-file expectations
- **Location:** `tests/it.rs:1-33`
- **Category:** Tests / Organisation
- **Confidence:** Medium
- **Blast radius:** Developer ergonomics only
- **Problem:** Consolidating into one `it` binary is correct for linker time (note in `tests/it.rs:1-2`), but `cargo test --test it -- foo_bar` resolves against all modules' test names. Two modules with identically-named tests (`derive_is_deterministic_for_fixture` exists in `economy_tests`, `hooks_tests`, `personae_tests`, `relations_tests`) match together. CLAUDE.md says `cargo test --test it -- golden` is the byte-stability gate — that filter currently matches `golden_generation` *and* `golden_png` which is fine, but a developer expecting `cargo test --test it -- determinism_holds_across_random_seeds` to pick one will get four proptests run in series.
- **Why it matters:** Not a correctness issue, but a 4× slowdown when isolating a single property. Also documents poorly: the CLAUDE.md command `cargo test --test it -- golden` works by accident of naming, not by structure.
- **Suggested fix:** Either (a) prefix module test names (`#[test] fn golden_png_export_is_deterministic_for_fixed_seed`), or (b) document in CLAUDE.md that the golden gate is `cargo test --test it -- ::golden_generation:: ::golden_png::`. (a) is cheap and removes ambiguity.
- **Effort:** S
- **Risk of fix:** Low — rename only.

### F-022-006 — [MEDIUM] [Tests] `golden_png` test doesn't pin a hash — does not catch deterministic-but-wrong rasteriser regressions
- **Location:** `tests/it/golden_png.rs:21-54`
- **Category:** Tests / Trustworthiness
- **Confidence:** High
- **Blast radius:** `src/export/bitmap/`, `src/export/svg_export/` byte-stability gate
- **Problem:** The test renders the m42 sector twice in one `cargo test` invocation and asserts the two BLAKE3 hashes agree. That catches **nondeterminism inside a single run** (HashMap iteration leak, RNG drift, timestamp in PNG). It does **not** catch a regression that is itself deterministic — e.g., a theme change that uniformly tints every hex 5 % brighter, or a `f32 → i32` rounding change that shifts every sprite one pixel. The header comment at lines 12-15 explicitly acknowledges this choice ("A pinned hash is intentionally NOT baked in"). The reasoning ("cosmetic tweaks would force a hash bump") is real but bypasses the CLAUDE.md byte-stability invariant.
- **Why it matters:** §6.5 explicitly flags "tests whose assertion can't actually fail" on regressions of interest. The `gui-core/tests/map_snapshots.rs` test (excellent — uses `UPDATE_MAP_SNAPSHOTS=1` to bless) is the right pattern; the project already knows how to do this.
- **Suggested fix:** Mirror the `map_snapshots.rs` pattern. Persist `tests/goldens/sector_png.blake3` next to the test; allow `UPDATE_GOLDEN_PNG=1 cargo test golden_png` to refresh. Keep the two-run determinism check as a separate `#[test]` so a single-run nondeterminism regression still surfaces as a distinct failure.
  ```rust
  let hash = blake3::hash(&png_a).to_hex().to_string();
  let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/sector_png.blake3");
  if std::env::var_os("UPDATE_GOLDEN_PNG").is_some() {
      std::fs::write(&golden, format!("{hash}\n")).unwrap();
      return;
  }
  let expected = std::fs::read_to_string(&golden).unwrap();
  assert_eq!(expected.trim(), hash);
  ```
- **Effort:** S
- **Risk of fix:** Low.

### F-022-007 — [MEDIUM] [Tests] Five identical proptest determinism blocks duplicate the same property across files
- **Location:** `tests/it/personae_tests.rs:181-198`, `tests/it/economy_tests.rs:166-185`, `tests/it/hooks_tests.rs:165-182`, `tests/it/relations_tests.rs:144-162`, `tests/it/invariants_proptest.rs:67-78`
- **Category:** Tests / Redundancy + Speed
- **Confidence:** High
- **Blast radius:** ~5 × 16 = 80 fresh sector generations per `cargo test` run (each `cases: 16`), all on the same seed alphabet `[a-z0-9-]{4,12}`
- **Problem:** Every derivation module reproduces the same property — *"same seed ⇒ byte-identical derive output"* — by generating the sector twice and asserting JSON equality. The sector-level proptest in `invariants_proptest.rs:67-78` already proves `generate_sector` is deterministic; the four downstream proptests therefore prove only that *each derive function is referentially transparent on the same `&GeneratedSector`*, which is what their **non-proptest** `derive_is_deterministic_for_fixture` test (at `personae_tests.rs:172-179` and siblings) already asserts. The duplication adds 80 sector generations per run for no incremental signal.
- **Why it matters:** §6.5 redundancy + speed. The proptest framework's value here is **input-space exploration**, not deterministic-repeat.
- **Suggested fix:** Pick one of two consolidations:
  1. **Drop the four downstream determinism proptests.** Keep the per-fixture determinism test (already present) plus the sector-level proptest. Saves 64 of the 80 sector generations.
  2. **Replace them with a property that the derivation actually owns** — e.g., for `relations`: "every present-faction pair appears exactly once and `a < b` canonical ordering holds across all generated seeds" (this is `matrix_covers_every_present_faction_pair` lifted into proptest). That keeps proptest's value while shedding the per-iteration sector generation by sharing a `sector_with_seed` LRU.
- **Effort:** S
- **Risk of fix:** Low — net assertion strength rises, not falls.

### F-022-008 — [MEDIUM] [Tests] `fixture_project`/`fixture_dir`/`fixture_input`/`fixture_sector` are duplicated across 13 files
- **Location:** every `tests/it/*.rs` except `cli_gui_parity.rs`, `imports_test.rs`, `validation_tests.rs`, `golden_png.rs` defines its own fixture helper
- **Category:** Tests / Maintainability
- **Confidence:** High
- **Blast radius:** Test maintainability — a fixture rename (e.g., `examples/m42_project` → `examples/m42`) touches every file
- **Problem:** Same code, same constant, repeated 13×. `invariants_tests.rs` calls it `fixture_project`; `personae_tests.rs` calls it `fixture_dir`. Half use `env!("CARGO_MANIFEST_DIR")`, half use `std::env::var("CARGO_MANIFEST_DIR").expect(...)`. Both work but inconsistency masks the genuine difference between compile-time and runtime resolution.
- **Suggested fix:** Promote into `tests/it/common.rs` as F-022-001's suggested fix already does. Single canonical `fixture_dir()` using `env!` (compile-time, no allocation, no `expect`).
- **Effort:** S
- **Risk of fix:** Low.

### F-022-009 — [MEDIUM] [Tests] Tests silently `return` on missing fixture / no routes — no skip signal
- **Location:** `tests/it/invariants_tests.rs:38-42` (`if let Some(r) = sector.routes.first_mut() { ... } else { return; }`), `tests/it/analytics_and_presets.rs:74-77, 90-93` (`if !dir.exists() { eprintln!("presets/ dir missing — skipping"); return; }`), `tests/it/relations_tests.rs:78-80` (`let Some(first) = matrix.pairs.first() else { return; }`)
- **Category:** Tests / Trustworthiness
- **Confidence:** High
- **Blast radius:** Tests pass without asserting anything when their precondition is unmet
- **Problem:** Three places "skip" by returning successfully. The standard test harness doesn't have a SKIPPED state, so a future refactor that accidentally generates zero routes or removes the `presets/` directory would silently pass these tests. The eprintln is invisible in CI logs unless `--nocapture` is set.
- **Suggested fix:** Convert each silent skip to an assertion that the precondition holds. If it ever fails, that **is** the regression the test should catch:
  ```rust
  // tests/it/invariants_tests.rs:38
  let r = sector.routes.first_mut().expect("m42 fixture must produce at least one route");
  // tests/it/analytics_and_presets.rs:74
  assert!(dir.exists(), "presets/ dir must exist for this test; check repo state");
  ```
  For `relations_tests.rs:78`, the matrix may legitimately be empty for a sector with one faction, but the m42 fixture has multiple — assert the precondition the same way.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-010 — [MEDIUM] [Tests] Property-test `cases: 16` is too low for input-space coverage of `invariants_hold_across_random_inputs`
- **Location:** `tests/it/invariants_proptest.rs:44-65`
- **Category:** Tests / Coverage
- **Confidence:** Medium
- **Blast radius:** §11.11 invariant fuzzing surface
- **Problem:** `cases: 24` (line 46) with 6-dimensional input space (`seed`, `width 4..=20`, `height 4..=20`, `density 0.05..=0.6`, `min_worlds 1..=3`, `extra_worlds 0..=5`) gives extremely sparse coverage — fewer than two cases per dimensional combination. Each case is full `generate_sector` ⇒ this is the right place to spend a budget. The other proptests in personae/economy/hooks/relations correctly use `cases: 16` because they're determinism repeats (and the F-022-007 fix removes them); freeing CPU for a higher count here.
- **Suggested fix:** Bump `invariants_hold_across_random_inputs` to `cases: 64` once F-022-007 has removed the redundant proptests. Also set `failure_persistence` so flakes have a regression file. Add `prop_assume!(min_worlds <= max_worlds)` instead of computing `max_worlds = min_worlds + extra_worlds`, which loses signal by widening the spread.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-011 — [LOW] [Tests] `imports_test.rs` constructs a hand-rolled empty sector via `..Default::default()` — fragile against `GeneratedSector` field additions
- **Location:** `tests/it/imports_test.rs:6-43`
- **Category:** Tests / Maintainability
- **Confidence:** High
- **Problem:** `empty_sector()` lists 16 explicit fields then `..Default::default()`. Any new mandatory field on `GeneratedSector` will silently pick up `Default` here (good), but the existing explicit fields duplicate `GenerationManifest::default()` work — and the same shape appears at `src/loading/sector_save.rs:160-220` and `gui-core/tests/map_snapshots.rs` uses `GeneratedSector::empty(...)`.
- **Suggested fix:** Use `GeneratedSector::empty("test", "Test", "seed", 1, 1)` (the constructor exists per `gui-core/tests/map_snapshots.rs:48`). 35 lines → 1. Same effect.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-012 — [LOW] [Tests] `diff_after_ticks_reports_changes_when_conflict_state_evolves` asserts almost nothing
- **Location:** `tests/it/search_and_diff.rs:167-179`
- **Category:** Tests / Trustworthiness
- **Confidence:** High
- **Problem:** The test ticks 5 turns then renders markdown and only checks `md.starts_with("# Sector Diff")` and `md.contains("Catalog compatible")`. The comment (lines 173-175) admits the diff can be empty. So the test mostly verifies that `diff_after_ticks` doesn't panic and that the markdown header exists. The header check is the same as `diff_writers_emit_both_files` would catch. Net: this is a smoke test masquerading as a behavioural one.
- **Suggested fix:** Either (a) rename to `diff_after_ticks_does_not_panic` so the contract is honest, or (b) seed the sector with a guaranteed-contested setup (force a faction stance change) and assert at least one entry in `diff.systems_changed`. Option (b) is the test the file is trying to write.
- **Effort:** M
- **Risk of fix:** Low.

### F-022-013 — [LOW] [Tests] Bench is run only via `cargo bench`, not protected by CI; no `--profile-time` smoke
- **Location:** `benches/generation.rs:117-125`
- **Category:** Tests / Coverage
- **Confidence:** Medium
- **Problem:** Criterion benches are great for local runs but are not regression-tested. A change that 10×-slows `generate_sector` won't fail any test. Some Rust projects add a `cargo test --release --bench generation -- --test` mode to keep benches compiling, which is already implicit in `cargo build --workspace --all-targets`; but the project has no "the bench compiles + runs one iter" smoke.
- **Suggested fix:** Optional — add `--test` invocation to CI: `cargo test --bench generation -- --test` (Criterion supports the harness flag; runs each bench once). Catches API drift in the bench file without spending bench time.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-014 — [LOW] [Tests] No fuzz targets for `loading::` TOML parsers
- **Location:** `src/loading/` (sector_save.rs, presets.rs); also `src/worlds_toml.rs:1`
- **Category:** Tests / Coverage
- **Confidence:** Medium
- **Problem:** §6.5 explicitly calls out "Fuzz targets (cargo-fuzz) for anything in `builder`/`viewer` that parses untrusted bytes". The TOML loaders in `src/loading/` parse user input; there is no `fuzz/` directory in the workspace. Existing inline unit tests at `src/loading/presets.rs:300-359` cover happy paths only.
- **Suggested fix:** Add `cargo-fuzz` targets for `load_project` and `presets::scaffold` against random TOML byte strings. Lowest-cost variant: a proptest that generates valid+invalid TOML fragments around the `[generation]` keys and asserts no panic in `load_project`. (Not critical because TOML is parsed by `toml` crate which is itself extensively fuzzed — but project-specific deserialization layers in `worlds_toml.rs` are project code.)
- **Effort:** M
- **Risk of fix:** Low.

### F-022-015 — [LOW] [Tests] `svg_export_tests.rs:33-41` asserts only that "body > 4096 bytes" — does not validate SVG structure
- **Location:** `tests/it/svg_export_tests.rs:33-41`
- **Category:** Tests / Trustworthiness
- **Problem:** `writes_svg_file_to_disk` reads the file back and only asserts `body.len() > 4096`. The `renders_m42_sector_as_well_formed_svg` test above it already validates structure on the in-memory string — that test could be extended to also write-and-reread, eliminating the second test or strengthening it.
- **Suggested fix:** Either delete `writes_svg_file_to_disk` (subsumed by structural test + adding `let body = std::fs::read_to_string(&path).unwrap();` at the end of the in-memory test), or extend it with the same `<svg`, `<polygon`, `</svg>` checks.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-016 — [NIT] [Tests] `tests/it/cli_gui_parity.rs` doesn't capture stderr on CLI failure
- **Location:** `tests/it/cli_gui_parity.rs:32-43`
- **Problem:** `Command::new(bin).args(...).status()` discards stdout/stderr; on failure the panic message only contains the exit code. A failed CI run gives the developer no immediate clue why `sectorforge generate` failed.
- **Suggested fix:** Use `.output()` and embed `String::from_utf8_lossy(&out.stderr)` in the assertion message.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-017 — [NIT] [Tests] Proptest configs hard-code `max_shrink_iters: 16` — too low for meaningful shrinking
- **Location:** `tests/it/personae_tests.rs:184`, `tests/it/economy_tests.rs:169`, `tests/it/hooks_tests.rs:168`, `tests/it/relations_tests.rs:147` (and `invariants_proptest.rs:47` at 32)
- **Problem:** `max_shrink_iters: 16` rarely produces a minimal counter-example for a sector generated from a 6-dimensional config. If F-022-007 is taken, these go away anyway. If not, raise to default (≥ 1024) — shrinking only runs on failure so it costs nothing on the happy path.
- **Suggested fix:** Delete the field; let `ProptestConfig::default()` apply.
- **Effort:** S
- **Risk of fix:** Low.

### F-022-018 — [NIT] [Tests] `analytics_and_presets.rs:38-54` reads `analysis.json` and parses but only checks `sector_id`
- **Location:** `tests/it/analytics_and_presets.rs:48-53`
- **Problem:** The JSON deserialise to `serde_json::Value` only validates that one field is present. The `write_analysis` writer at `src/analysis/mod.rs` produces a documented schema — at minimum, verify `analysis_md.starts_with("# Sector Analysis")` against the on-disk file (currently checked only on the in-memory string at line 33-35).
- **Suggested fix:** Round-trip parse `analysis.json` into `sectorforge::SectorAnalysis` (the typed struct) instead of `serde_json::Value`. That catches schema breakage in addition to file existence.
- **Effort:** S
- **Risk of fix:** Low.

## Slowest-tests table (estimated — no nextest baseline)

| Rank | Test (binary :: name) | Estimated time | Why expensive | Proposed optimisation | Expected speedup |
|---|---|---|---|---|---|
| 1 | `it::invariants_proptest::invariants_hold_across_random_inputs` | ≥ 24 × generate_sector ≈ 2-6 s | 24 full generations across random sizes | Already correct shape; keep. Optional: lift `load_project` out of the loop (currently in `run_one` line 24) — `load_project` once, clone `ProjectInput`. | 10-20 % |
| 2 | `it::invariants_proptest::determinism_holds_across_random_seeds` | ≥ 24 × 2 generate_sector ≈ 2-6 s | 48 generations | Same load_project hoist; share input via cell. | 10-20 % |
| 3 | `it::economy_tests::determinism_holds_across_random_seeds` (proptest) | 16 × 2 generate_sector ≈ 2-4 s | Redundant with sector-level proptest | Delete per F-022-007 | 100 % of this test |
| 4 | `it::hooks_tests::determinism_holds_across_random_seeds` (proptest) | 16 × 2 generate_sector ≈ 2-4 s | Same | Delete per F-022-007 | 100 % of this test |
| 5 | `it::personae_tests::determinism_holds_across_random_seeds` (proptest) | 16 × 2 generate_sector ≈ 2-4 s | Same | Delete per F-022-007 | 100 % of this test |
| 6 | `it::relations_tests::determinism_holds_across_random_seeds` (proptest) | 16 × 2 generate_sector ≈ 2-4 s | Same | Delete per F-022-007 | 100 % of this test |
| 7 | `it::search_and_diff::search_story_beat_constraints` | 10 generates (budget=10) ≈ 0.5-2 s | Search budget unbounded | Reduce budget to 4; constraint coverage doesn't depend on a winner | 60 % |
| 8 | `it::cli_gui_parity::cli_and_library_produce_identical_sector_json` | binary spawn + generate ≈ 0.3-1 s | Process spawn unavoidable for this test's purpose | Keep | n/a |
| 9 | `sectorforge_builder::file_watcher::tests::detects_mtime_bump` | ≥ 1.4 s steady, up to 7.2 s | Real wall sleeps | Replace with `filetime::set_file_mtime` + channel timeout per F-022-002 | 95 % |
| 10 | `it::invariants_tests::*` (10 functions, no cache) | 10 × generate_sector ≈ 1-4 s | Fixture not memoised | OnceLock per F-022-001 | 80-90 % across this file |

Total estimated suite wall-clock today: **20-45 s** (workspace `cargo test --workspace`, default flags, no `--ignored`, modern laptop). Estimated post-fix: **8-15 s**, a **~50 % suite-time reduction**.

## Flakiness ledger

| Test | Risk | Cause | Suspected failure rate |
|---|---|---|---|
| `sectorforge_builder::file_watcher::tests::detects_mtime_bump` | High | Wall-clock sleep + filesystem mtime granularity (NFS/SMB exotic FS); polls 30×200ms without an explicit timeout assertion. | Will flake on slow CI or networked filesystems; ≥ 0.5 % expected on typical CI. |
| `it::invariants_proptest::*` | Low | Proptest seed not pinned in config; default RNG. If a failure occurs, regressions are stored to `proptest-regressions/` (not yet committed — check `.gitignore`). | Deterministic by case-count; flakes only on real bugs. |
| `it::cli_gui_parity::cli_and_library_produce_identical_sector_json` | Low | Spawns the CLI binary — depends on prior `cargo build` for `CARGO_BIN_EXE_sectorforge`. Cargo handles this, but on heavily-loaded CI a stale binary could in theory be invoked. | Effectively zero. |
| `gui-core::map_snapshots::map_snapshots_match_goldens` | Low | Software rasteriser at `gui-core/tests/map_snapshots.rs:451-506` uses `f32` edge functions with epsilon tolerance — platform-specific FPU behaviour theoretically possible. | Effectively zero on x86_64/aarch64 with default rounding mode. |

No tests use `tokio`, `thread::spawn` for concurrency assertions, `rand::thread_rng`, or unordered `HashMap` iteration in assertion paths (grep confirmed). No `should_panic` without `expected = "..."`.

## Coverage-gap list (public items / error paths with no integration test)

| Crate / path | Gap | Severity |
|---|---|---|
| `src/cli/{analyze,briefing,compose,diff,economy,history,hooks,interestingness,missions,personae,presets,prose,regions,relations,search,sites,validate}.rs` | 17 subcommands, zero CLI-spawn coverage. Only `generate` is tested via `cli_gui_parity.rs`. | HIGH — F-022-004 |
| `src/export/segmentum.rs` (1,168 LOC) | Five `#[ignore]`d tests in `tests/it/segmentum_tests.rs`. No default-suite gating of segmentum byte stability, super-grid collision rejection, child digest, or writer artifacts. | HIGH — F-022-003 |
| `src/export/html_export.rs` | No test file (grep: no `html_export` reference in `tests/it/`). Inline `#[cfg(test)]` only. | MEDIUM |
| `src/export/system_map.rs` | No integration test. Inline test only. | MEDIUM |
| `src/export/bitmap/regions.rs` | No integration test for region overlay output bytes. `golden_png.rs` only exercises `render_sector_image`. | MEDIUM |
| `src/export/subsectors/mod.rs` | No integration test. | MEDIUM |
| `src/loading/sector_save.rs` (split/merge save format) | Inline tests cover happy path + one error; no integration test for end-to-end save/reopen via the CLI or library. | MEDIUM |
| `src/loading/presets.rs` | Integration test `scaffold_starter_preset_produces_loadable_project` exists but silently skips if `presets/` is absent (F-022-009). No coverage for malformed preset TOML. | LOW |
| `src/analysis/conflict.rs`, `src/analysis/intel.rs`, `src/analysis/missions.rs`, `src/analysis/briefing.rs`, `src/analysis/prose.rs`, `src/analysis/interestingness.rs`, `src/analysis/history/`, `src/analysis/importance.rs`, `src/analysis/influence_field.rs`, `src/analysis/power_projection.rs`, `src/analysis/route_control.rs`, `src/analysis/stability.rs`, `src/analysis/control.rs`, `src/analysis/regions.rs` | Only inline unit tests; no integration coverage of `derive`+`render_markdown`+writer like the personae/economy/hooks/relations files have. | MEDIUM (cumulative) |
| `src/validate/diff.rs` (1,308 LOC) | `search_and_diff.rs` exercises `diff_sectors` and `advance_sector`, but the deep diff cases (faction_deltas with mixed presence shifts, route stability transitions, system addition/removal) are not specifically asserted. The test at `search_and_diff.rs:167-179` admits empty-diff results pass silently. | MEDIUM — F-022-012 |
| `src/worlds.rs` (1,361 LOC) | No direct integration test. Covered only transitively through `generate_sector` runs. | MEDIUM |
| `viewer/src/` (~10 kLOC) | Inline tests in `viewer/src/editor/state.rs` and `viewer/src/factions_overview.rs` only. No widget-level snapshot tests, no app-level smoke. The `gui-core/tests/map_snapshots.rs` pattern (offline rasteriser) is directly applicable to the viewer's segmentum view, route planner, factions overview. | MEDIUM |
| `builder/src/builder/panels/*.rs` (26 panels, ~20 kLOC) | Inline `#[cfg(test)]` modules in most panels, but zero integration coverage of the `BuilderState` + `BuilderCommand` round-trip (apply / undo / redo). Per CLAUDE.md the command-bus invariant (§R4) is load-bearing and not gated by a test. | MEDIUM |
| `builder/src/builder/project_io.rs` (1,053 LOC) | Inline test only. No round-trip save→load→assert-identity at integration level. | MEDIUM |
| `gui-core/src/sector_view.rs` and `system_view.rs` | `map_snapshots.rs` covers `sector_view`; `system_view` has only inline tests. | LOW |
| Doctests | 6 `no_run` doctests in `src/lib.rs` (lines 34, 195, 213, 234, 270, 295). None elsewhere in the workspace. `no_run` means they compile-check but don't execute — better than nothing, but executable examples on `generate_sector`, `analyze_sector`, `diff_sectors`, `run_seed_search`, and the typed public errors would catch API-surface drift. | LOW |
| Fuzz targets | None. §6.5 specifically asks for fuzz on TOML / export writers. | LOW — F-022-014 |

## Estimate of total-suite-time reduction if findings applied

| Group | Saving |
|---|---|
| F-022-001 (cache fixture across files) | 40-60 % of `it` binary |
| F-022-002 (kill sleeps in file_watcher test) | -1.4 s to -7 s of `sectorforge_builder` unit binary |
| F-022-007 (drop 4 redundant determinism proptests) | -64 sector generations per run, ≈ 8-15 s saved |
| F-022-010 (load_project hoist in `run_one`) | 10-20 % of the two heaviest proptests |
| F-022-003 (mini segmentum fixture, default-run subset) | net **increase** in coverage; small fixture chosen so total cost is ≤ 1 s |

**Net projection: workspace `cargo test --workspace` drops from an estimated 20-45 s to 8-15 s, a ~50 % reduction.** Coverage simultaneously improves (segmentum no longer skipped, CLI subcommands covered).

## Rubric checklist

- **3.1 Panics/failure surface** — `unwrap()`/`expect()` in tests are standard practice; flagged only F-022-016 (loss of stderr context). No findings.
- **3.2 unsafe** — none in tests. No findings.
- **3.3 Ownership/clones** — `fixture_input().clone()` pattern is correct (sharing the cached input). The few `.to_string()` / `.into()` calls in test setup are noise. No findings.
- **3.4 Error handling** — tests `unwrap()` on Results, which is the test-code idiom. F-022-009 covers silent skips.
- **3.5 Concurrency/async** — none in tests; suite is single-threaded except for the cargo test parallel runner. No findings.
- **3.6 Performance** — F-022-001, F-022-002, F-022-007, F-022-010.
- **3.7 Idiom/API** — F-022-005 (test name namespacing), F-022-011 (use the existing `GeneratedSector::empty` constructor).
- **3.8 Dependencies/Cargo** — `dev-dependencies` minimal (`tempfile`, `proptest`, `criterion`). `criterion` correctly uses `default-features = false, features = ["cargo_bench_support"]`. No findings.
- **3.9 Memory/resource** — `tempfile::tempdir()` correctly drops on scope exit. No findings.
- **3.10 Inline `#[cfg(test)]` coverage** — see coverage-gap list. Each analysis derivation has 1-6 inline tests; CLI has zero; export modules each have at least one. F-022-014 (no fuzz).
- **3.11 Documentation** — Module-doc headers in tests/it/*.rs are excellent (every file opens with `//! …` explaining purpose). Bench is well-commented. `imports_test.rs:1-5` is the one file without a `//!` preamble. NIT.

## Summary of suggested fixes

- F-022-001 — HIGH — Cache fixture in shared `tests/it/common.rs` (OnceLock) across 10 uncached files — S / Low
- F-022-002 — HIGH — Replace sleep+poll in `file_watcher` test with `filetime::set_file_mtime` + channel `recv_timeout` — M / Low
- F-022-003 — HIGH — Add `examples/mini_project`; un-ignore segmentum byte-stability + slot-collision tests on the mini fixture — M / Medium
- F-022-004 — HIGH — Add CLI smoke test that runs each subcommand against the m42 fixture — M / Low
- F-022-005 — MEDIUM — Prefix module-local test names so filter-by-name picks the intended test — S / Low
- F-022-006 — MEDIUM — Pin a BLAKE3 golden for PNG export with `UPDATE_GOLDEN_PNG=1` blessing path — S / Low
- F-022-007 — MEDIUM — Drop or rewrite four redundant determinism proptests (personae/economy/hooks/relations) — S / Low
- F-022-008 — MEDIUM — Promote duplicated `fixture_*` helpers into `tests/it/common.rs` — S / Low
- F-022-009 — MEDIUM — Convert silent `return` skips to explicit asserts so missing preconditions fail loudly — S / Low
- F-022-010 — MEDIUM — Hoist `load_project` out of `run_one`, raise `invariants_hold_across_random_inputs` to `cases: 64` — S / Low
- F-022-011 — LOW — Use `GeneratedSector::empty` in `imports_test.rs` — S / Low
- F-022-012 — LOW — Strengthen `diff_after_ticks_*` to assert concrete diff content, or rename to reflect its smoke-test scope — M / Low
- F-022-013 — LOW — Add `cargo test --bench generation -- --test` to CI to keep benches building — S / Low
- F-022-014 — LOW — Add a proptest or `cargo-fuzz` target for `load_project` against random TOML — M / Low
- F-022-015 — LOW — Merge or remove the redundant SVG file-write test in `svg_export_tests.rs` — S / Low
- F-022-016 — NIT — Capture stderr from spawned CLI in `cli_gui_parity.rs` for diagnosability — S / Low
- F-022-017 — NIT — Remove the explicit `max_shrink_iters: 16` overrides; let default apply — S / Low
- F-022-018 — NIT — Parse `analysis.json` into typed `SectorAnalysis`, not `serde_json::Value` — S / Low
