# AREA C — export / validate / worlds / cli — verification

Verified 2026-06-05 against `main`; scope covers `src/export/`, `src/validate/`, `src/worlds.rs`, `src/cli/`.

## Summary table

| ID    | Sev     | Status             | Effort | One-line |
|-------|---------|--------------------|--------|----------|
| C-S1  | MED     | ✅ Confirmed        | M      | 6 runners inline the project/sector match instead of calling `load_or_regenerate` |
| C-S2  | MED     | ✅ Confirmed        | M      | `diff.rs` re-implements the `RESOURCE_KEYS` match locally with `_ => 0.0` |
| C-S3  | MED     | ✅ Confirmed        | L      | `system_label_visible` predicate + subsector-label placement geometry duplicated in bitmap and svg_export labels.rs |
| C1    | MED     | ✅ Confirmed        | S      | `diff.rs:370–378` re-matches `RESOURCE_KEYS` by string instead of calling `ResourceVector::get`; new resource key silently returns 0.0 in diffs |
| C2    | MED     | ✅ Confirmed (×6)   | M      | 6 runners have inline `(project,sector)` match + the "pass exactly one of" string; `common.rs` has `load_or_regenerate` but 6 callers bypass it |
| C3    | MED     | ✅ Confirmed (×12)  | M      | `if let Some(dir) = out … else if json … else { render_markdown }` triple in 12 runners (not 13: `diff.rs` also has it but uses `args.json` not a `bool` param) |
| C4    | MED     | ✅ Confirmed        | M      | `system_label_visible` fn duplicated verbatim in `bitmap/labels.rs:61` and `svg_export/labels.rs:72`; subsector placement geometry structurally duplicated |
| C5    | LOW     | ✅ Confirmed        | S      | `html_export.rs:257–262` — the `Hidden` branch and the fall-through branch compute identical expressions |
| C6    | LOW     | ✅ Confirmed        | L      | `worlds.rs` is exactly 1371 LOC; mixes taxonomy enums, `FromStr`/`Display`/`VARIANTS` triples, and IO; `worlds_toml` is already a separate module so split is ready |
| C7    | LOW     | ✅ Confirmed        | S      | `segmentum.rs:842` and `diff.rs:1021` put `sector_id` / `FactionId` values into pipe-delimited table cells with no escaping |
| C8    | LOW     | ✅ Confirmed        | S      | `html_export.rs:252` — `unwrap_or(20.0)` is a magic default with no named constant |
| C9    | LOW     | ✅ Confirmed        | S      | `cli/common.rs:112` — wildcard arm on `Severity` even though enum has `#[non_exhaustive]`; future variant gets tag `"UNKNOWN"` |

---

### C-S1 — CLI runner boilerplate: inline project/sector match

- **Review sev / bucket:** MED / P1 #5
- **Status:** ✅ Confirmed
- **Location:** `src/cli/analyze.rs:16`, `src/cli/economy.rs:15`, `src/cli/history.rs:15`, `src/cli/personae.rs:15`, `src/cli/relations.rs:17`, `src/cli/sites.rs:16` (6 files)
- **Evidence:**
  ```rust
  // analyze.rs:16 — representative of all 6
  let (sec, cfg) = match (project, sector) {
      (Some(project), None) => { … }
      (None, Some(sector)) => { … }
      (Some(_), Some(_)) | (None, None) => {
          return Err(… "pass exactly one of …".into());
  ```
- **Why it matters:** The string `"pass exactly one of --project <dir> or --sector <path>"` appears 7 times (6 inline + once in `common::load_or_regenerate`). Runners that need config extraction (`analyze`, `economy`, `history`, `relations`, `personae`, `sites`) cannot call the existing `load_or_regenerate` because they also need the per-runner config from `input.catalogs.*`. A `common::load_or_regenerate_with_cfg<C>` helper or a resolver that returns `(GeneratedSector, Option<SectorInput>)` would reduce all 6 to one call site.
- **Fix:** Add `pub fn resolve_project_or_sector(project, sector) -> Result<SectorInput | GeneratedSector, _>` returning an enum, or add a `load_project_for_run(project, sector) -> Result<(GeneratedSector, SectorInput), _>` where the `sector` path returns a default-config sentinel. Then each runner calls it once.
- **Effort:** M
- **Risk / deps:** `economy.rs` has an extra `enabled: true` override on the config — must not be lost in the refactor.

---

### C-S2 — Stringly-typed economy mirror in diff.rs

> ✅ **RESOLVED 2026-06-05** — fixed together with C1 (`sector_balance.get(*k)`).

- **Review sev / bucket:** MED / P1.5
- **Status:** ✅ Confirmed (same root issue as C1)
- **Location:** `src/validate/diff.rs:370–378`
- **Evidence:**
  ```rust
  let pull = |s: &crate::economy::EconomyReport| match *k {
      "ore" => s.sector_balance.ore,
      "promethium" => s.sector_balance.promethium,
      …
      _ => 0.0,
  };
  ```
- **Why it matters:** `ResourceVector::get(&self, key: &str) -> f32` already exists at `src/analysis/economy.rs:112` and does the same match. `diff.rs` duplicates it — a new resource added to `RESOURCE_KEYS` and `ResourceVector` is silently handled in the economic report but silently returns 0.0 in diffs unless both copies are updated together.
- **Fix:** Replace the inline closure with `|s: &EconomyReport| s.sector_balance.get(k)`. One-liner.
- **Effort:** S
- **Risk / deps:** None; `ResourceVector::get` is already `pub`. This is C1's direct fix — see below.

---

### C-S3 — bitmap vs svg_export duplicate label/layout geometry

> ‡†  **PARTIAL ✅ + deeper merge WON'T-FIX (waves 20–21).** The byte-safe slices
> landed in wave 20: `render_core::labels::{system_label_visible (C4, `a580c81`),
> subsector_label_backed (`5cd0b00`)}`. The deeper *placement geometry* merge is
> WON'T-FIX (wave 21): re-confirmed against live source that the bitmap (`i32`,
> `i64` distance, real glyph metrics `text_size`/`GLYPH_*`, `2*g.scale`) and svg
> (`f32`, `mul_add`/`powi` distance, heuristic `chars*font*0.6`, `2.0`) backends
> compute different numbers at every step, with `i32` vs `f32` `Rect`/`MapBounds`.
> A merge needs a generic over both the numeric type AND the glyph-metric source (a
> trait rewrite) with both PNG + SVG blake3 pins held identical — not a pure move,
> against the proportionate preference. See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** MED / P1 structural
- **Status:** ✅ Confirmed
- **Location:** `src/export/bitmap/labels.rs:61–78` vs `src/export/svg_export/labels.rs:72–89`
- **Evidence:**
  ```rust
  // Both files — system_label_visible is byte-identical in logic:
  fn system_label_visible(sys, subsectors, theme, sector) -> bool {
      match theme.label_density {
          LabelDensity::All => true,
          LabelDensity::None => false,
          LabelDensity::ImportantOnly => {
              sector.get_worlds_for_system(sys).len() >= 4
              || !sys.primary_factions.is_empty()
              || subsectors.iter().any(|s| …)
  ```
- **Why it matters:** Changing the importance heuristic (e.g. adding a fourth criterion) requires editing two files. The subsector-label placement algorithm (`draw_subsector_labels`) is also structurally duplicated — ~190 lines each — differing only in coordinate types (`i32` vs `f32`) and drawing primitives.
- **Fix:** Hoist `system_label_visible` into `src/export/render_core/` (already exists, currently holds `RenderOptions`, `grid.rs`, `colors.rs`). The full subsector-placement algorithm is harder to share due to backend-specific drawing calls, but the candidate-selection and obstacle-check logic can be extracted to a backend-agnostic fn returning `Option<(f32,f32)>` with an affine-coordinate abstraction.
- **Effort:** L
- **Risk / deps:** Golden output is not affected by extraction of `system_label_visible` alone (pure logic, no pixel change). The full placement dedup would be a golden-output non-event if drawing calls are unchanged.

---

### C1 — resource diff matches fields by string with `_ => 0.0`

> ✅ **RESOLVED 2026-06-05** — the `diff.rs:370` closure now calls
> `sector_balance.get(*k)`; a new `RESOURCE_KEYS` entry is picked up
> automatically instead of silently diffing as `0.0`. Behaviour-identical for
> the current resource set (diff + golden tests green). Closes **C-S2** too.
> See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** MED / P1.5 (real data-loss class)
- **Status:** ✅ Confirmed
- **Location:** `src/validate/diff.rs:370–378` (line numbers stable)
- **Evidence:**
  ```rust
  let pull = |s: &crate::economy::EconomyReport| match *k {
      "ore" => s.sector_balance.ore,
      …
      "recruits" => s.sector_balance.recruits,
      _ => 0.0,           // ← silently eats any future resource key
  };
  ```
- **Why it matters:** `RESOURCE_KEYS` lives in `economy.rs`; `ResourceVector::get` also lives there and already has an exhaustive `_ => 0.0` fallback. If a seventh resource is added to the struct and `RESOURCE_KEYS` is extended, the diff output silently shows zero delta for that resource — no compile error, no test failure until a human inspects the diff output.
- **Fix:** Replace the closure with `|s: &EconomyReport| s.sector_balance.get(k)`. Optionally make `ResourceVector::get` return `Option<f32>` and add a `debug_assert!` on the `None` path to catch future drift.
- **Effort:** S
- **Risk / deps:** Requires `use crate::economy::EconomyReport;` already present at `diff.rs:10`. No golden-output change (pure logic fix that makes the numbers match what the report renders).

---

### C2 — (project,sector) resolve match duplicated ×6; "pass exactly one of" ×7

- **Review sev / bucket:** MED / P1 #5
- **Status:** ✅ Confirmed (×6 inline runners, ×7 total string occurrences)
- **Location:** `src/cli/{analyze,economy,history,personae,relations,sites}.rs` lines 15–32 in each; plus `src/cli/common.rs:231`
- **Evidence:**
  ```rust
  // relations.rs:17 — one of 6 identical patterns:
  let (sec, cfg) = match (project, sector) {
      (Some(p), None) => { let input = load_project(p)?; … }
      (None, Some(s)) => (load_sector_json(s)?, …::default()),
      _ => return Err(… "pass exactly one of …"),
  };
  ```
- **Why it matters:** Each of the 6 runners that needs a config also repeats error text and the three-way match structure. Drift risk: a runner could diverge (e.g. `economy.rs` has `enabled: true` override not present in others). `load_or_regenerate` exists in `common.rs` but is only used by the 6 runners that don't need config extraction.
- **Fix:** `common::resolve_sector_with_cfg<C: Default>(project, sector, extract: fn(&SectorInput) -> C) -> Result<(GeneratedSector, C), SectorError>`. Each runner becomes two lines: one call + config override.
- **Effort:** M
- **Risk / deps:** `economy.rs` needs its `cfg.enabled = true` override applied post-resolution. `analyze.rs` has a `strict` flag path unrelated to resolution.

---

### C3 — out/json/markdown emit triple ×12

- **Review sev / bucket:** MED / P1 #5
- **Status:** ✅ Confirmed (×12; review said ×13 — `diff.rs` has the pattern too but via `args.json` not a bare `bool`, making the count 12 structurally identical + 1 slight variant)
- **Location:** `src/cli/{analyze,economy,history,hooks,interestingness,missions,personae,prose,regions,relations,sites,search}.rs` — plus `diff.rs` as a variant
- **Evidence:**
  ```rust
  // economy.rs:38–46 — representative:
  if let Some(dir) = out {
      sectorforge::write_economy(dir, &sec.id, &report)?;
      println!("Wrote {dir}/economy.md and {dir}/economy.json");
  } else if json {
      print_json(&report)?;
  } else {
      let md = sectorforge::economy::render_markdown(&sec.id, &report);
      print!("{md}");
  }
  ```
- **Why it matters:** 12 identical structures. Adding a new output path (e.g. TOML, CBOR) requires 12+ edits. The `println!("Wrote …")` success message is also duplicated.
- **Fix:** `common::emit_report<R: Serialize>(out: Option<&Utf8PathBuf>, json: bool, write_fn: impl Fn(&Utf8PathBuf) -> Result<()>, render_md_fn: impl Fn() -> String) -> Result<()>`. Callers pass closures. The `write_fn` already names the files internally so the "Wrote" message can be standardised or moved into `write_*`.
- **Effort:** M
- **Risk / deps:** The "Wrote … .md and … .json" strings differ per runner; standardise or move into the domain-level `write_*` functions. `search.rs` lacks a markdown fallback (only `out` or `json`) — handle as a special case or add the markdown arm.

---

### C4 — `system_label_visible` + placement geometry duplicated in bitmap vs svg_export

- **Review sev / bucket:** MED / P1 structural
- **Status:** ✅ Confirmed
- **Location:** `src/export/bitmap/labels.rs:61–78` and `src/export/svg_export/labels.rs:72–89`
- **Evidence:**
  ```rust
  // svg_export/labels.rs:72 — byte-for-byte same logic as bitmap/labels.rs:61
  fn system_label_visible(sys: &GeneratedSystem, subsectors: &[Subsector],
                           theme: &MapTheme, sector: &GeneratedSector) -> bool {
      match theme.label_density {
          LabelDensity::All => true,
          LabelDensity::None => false,
          LabelDensity::ImportantOnly => {
              sector.get_worlds_for_system(sys).len() >= 4
              || !sys.primary_factions.is_empty()
  ```
- **Why it matters:** Changing the `ImportantOnly` heuristic requires editing two files. The function is `pub(super)` in each, so it is invisible to `render_core`. See also C-S3 for the deeper subsector-placement duplication.
- **Fix:** Move `system_label_visible` to `src/export/render_core/mod.rs` as `pub(super)` visible to both backends, or to a new `src/export/render_core/labels.rs`. No pixel change → no golden re-bless required.
- **Effort:** M (just the predicate move; full placement dedup is L)
- **Risk / deps:** Both backends import from their local `super::` — adjust imports only.

---

### C5 — dead branch in `redact_for_observer`

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/export/html_export.rs:257–262`
- **Evidence:**
  ```rust
  if matches!(p.influence, FactionInfluence::Hidden) {
      let conf = (p.dimensions.visibility * observer_vis) / 100.0;
      return (conf as u8) >= min_conf;
  }
  let conf = (p.dimensions.visibility * observer_vis) / 100.0;
  (conf as u8) >= min_conf
  ```
- **Why it matters:** Both branches compute the identical expression — the `if Hidden` early-return is dead code; control always falls through to the same result. The original intent was presumably to treat `Hidden` differently from visible presences, but both arms are the same formula. This is either a logic bug (the `Hidden` arm should use a different formula) or dead code (the `if` block can be deleted).
- **Fix:** If intent is to treat Hidden presences the same as visible ones: delete the `if` block. If the intent was to use a stricter threshold for Hidden: restore the different formula (e.g., confidence only from `observer_vis` alone). Clarify with a comment either way.
- **Effort:** S
- **Risk / deps:** HTML export is exercised by golden tests — any behavioural change needs `cargo test --test it -- golden`. If just deleting the dead branch (no logic change), goldens are unaffected.

---

### C6 — `worlds.rs` god-file mixes taxonomy enums + IO

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed
- **Location:** `src/worlds.rs` — exactly 1371 LOC as stated
- **Evidence:**
  ```rust
  // worlds.rs:1 — module doc
  /// World parameter types. The enum set in this module is the authoritative
  /// taxonomy; project data is loaded from `worlds.toml` via [`load_worlds_data`].
  // worlds.rs:439 — IO function deep inside the enum file:
  pub fn load_worlds_data(data_dir: impl AsRef<Path>) -> Result<WorldsLoad, WorldError> { … }
  ```
- **Why it matters:** Nine enums each have hand-written `FromStr` + `Display` + `VARIANTS` triples (lines 393–1126). Adding a new enum variant requires touching 3 separate impl blocks. `worlds_toml` is already a separate module (`src/worlds_toml/`); the `load_worlds_data` IO function and `WorldsLoad`/`WorldError` types could move there, leaving `worlds.rs` as pure taxonomy. A `enum_slug!` macro (see B-S3) would then collapse each triple to one declaration.
- **Fix:** Phase 1 (mechanical): move `WorldError`, `WorldsLoad`, and `load_worlds_data` into `src/worlds_toml/`. Phase 2 (optional): add `enum_slug!` or use `strum` to collapse the `FromStr`/`Display` triples.
- **Effort:** L
- **Risk / deps:** All callers of `load_worlds_data` must update their import path. No golden-output risk for the structural split; `enum_slug!` addition has no output impact.

---

### C7 — unescaped `|` / newline in markdown table rows

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/export/segmentum.rs:842–851` (children table) and `src/validate/diff.rs:1021` (stance-changes table)
- **Evidence:**
  ```rust
  // segmentum.rs:842 — sector_id is a free-form String from sectorforge.toml [project].id:
  "| {} | ({}, {}) | {} | `{}` | {} | {} | {} |\n",
  c.id, c.column, c.row, c.sector_id, c.seed, …
  // diff.rs:1021 — FactionId is Arc<str>, no validation prevents a pipe in the id:
  wln!(s, "| {} | {} | {:?} | {:?} |", c.a, c.b, c.before, c.after);
  ```
- **Why it matters:** `sector_id` comes from `[project].id` in `sectorforge.toml`, which is a free-form `String` (validated only as non-empty, not as pipe-free). `FactionId` is an `Arc<str>` newtype with no character constraint. A value containing `|` would break the markdown table. `c.seed` uses backtick quoting already (`\`{}\``), which helps for that field only.
- **Fix:** Escape `|` as `\|` and newlines as `<br>` in user-supplied string fields before interpolation. Add a helper `md_cell(s: &str) -> String` in `segmentum.rs` and `diff.rs` (or in a shared export util). **Golden-output change:** any fix here changes `.md` output → run `cargo test --test it -- golden` and re-bless with `UPDATE_*=1` if needed.
- **Effort:** S
- **Risk / deps:** Golden-output change. The fix is safe to apply first on a branch with golden re-bless. No logic change, only output sanitisation.

---

### C8 — magic `20.0` observer-visibility default

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/export/html_export.rs:252`
- **Evidence:**
  ```rust
  .map(|p| p.dimensions.visibility)
  .unwrap_or(20.0);
  ```
- **Why it matters:** `20.0` is the assumed observer visibility when the observer has no presence on a world. This value is not documented or named anywhere. If the visibility scale or confidence formula changes, this magic constant will be missed.
- **Fix:** `const OBSERVER_DEFAULT_VISIBILITY: f32 = 20.0;` at the top of `html_export.rs` (or in a shared constants module) with a doc comment explaining the convention.
- **Effort:** S
- **Risk / deps:** None. No output change — same value, just named.

---

### C9 — wildcard arm on `#[non_exhaustive]` `Severity` enum

> † **WON'T-FIX (language-mandated; re-confirmed wave 21).** The review's "remove
> the `_ => "UNKNOWN"` arm" is impossible: `mod cli` compiles into the **bin**
> crate while `Severity` is `pub use`'d from the **lib** and is
> `#[non_exhaustive]`, so across that crate boundary an exhaustive match is illegal
> and the wildcard is mandatory — dropping it is `rustc E0004` (confirmed
> empirically in waves 18–19). The wildcard stays. See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/cli/common.rs:107–113` (note: review cited `:107`, actual match is on line 108)
- **Evidence:**
  ```rust
  fn severity_tag(s: Severity) -> &'static str {
      match s {
          Severity::Error => "ERROR",
          Severity::Warning => "WARN",
          Severity::Info => "INFO",
          _ => "UNKNOWN",
      }
  }
  ```
- **Why it matters:** `Severity` is `#[non_exhaustive]` (`src/validate/validation.rs:30`), so the `_` arm is required for external consumers. However, this consumer is within the same workspace — the `_` arm silently maps a future `Severity::Debug` or `Severity::Hint` variant to the string `"UNKNOWN"` in CLI output, which is confusing. Since this crate is an internal consumer, it can match exhaustively and will get a compile error when a variant is added, prompting a deliberate tag choice.
- **Fix:** Remove the `_ =>` arm. If `Severity` is `#[non_exhaustive]`, the compiler will still require the wildcard in *external* crates, but in the same workspace a full match is possible. Alternatively, add `#[allow(unreachable_patterns)]` with a comment, or switch to `Severity::as_slug()` which already exists on the enum.
- **Effort:** S
- **Risk / deps:** Check whether `sectorforge-builder` or `sectorforge-viewer` also call `severity_tag` transitively — they don't; this fn is `fn` (private). Use `s.as_slug().to_uppercase()` as the simplest fix.

---

## Non-issues spot-checked

**BTree ordering (determinism):** `diff.rs` uses `BTreeMap`/`BTreeSet` throughout for all indexed lookups (`before_idx`, `after_idx`, `b_stranded`, `a_stranded`) and explicitly sorts `stance_changes` by `(a, b)` pair before emission. Clean.

**`exit_code::from_error` wired:** `src/main.rs:15` calls `cli::exit_code::from_error(&e)` in the top-level error handler. The `exit_code.rs` module maps all `SectorError` variants to stable codes. Clean.

---

## Suggested local order

1. **C1 / C-S2** — one-liner fix (`s.sector_balance.get(k)`); zero risk; fixes a real data-loss class in diff output. Do first.
2. **C9** — replace `_ => "UNKNOWN"` with `s.as_slug().to_ascii_uppercase()` (uses the existing method); zero risk.
3. **C8** — name the `20.0` constant; zero risk; improves C5 investigatability.
4. **C5** — decide and document whether `Hidden` presences should use a different formula; if not, delete the duplicate branch.
5. **C7** — add `md_cell` escaping for `|` in segmentum and diff markdown tables; re-bless golden output (`cargo test --test it -- golden` + `UPDATE_*=1`). Do on a dedicated branch so the golden delta is reviewable.
6. **C4 / C-S3** — move `system_label_visible` to `render_core`; no golden impact; unlocks the deeper placement dedup.
7. **C2 / C-S1** — introduce `resolve_sector_with_cfg` helper; eliminates 6 duplicate match blocks and 7 duplicate strings. Medium mechanical effort, no logic risk.
8. **C3** — introduce `emit_report` helper; depends on C2 being settled so the signatures are stable.
9. **C6** — move `WorldError`/`WorldsLoad`/`load_worlds_data` into `worlds_toml`; do after the god-file split is blocked on a content golden (per `G2`).
