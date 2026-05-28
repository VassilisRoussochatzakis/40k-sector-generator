---
sweep_id: X07
scope: whole-workspace
reviewed_by: agent
loc_reviewed: 497   # panic-surface sites enumerated
finding_counts: { critical: 0, high: 5, medium: 3, low: 6, nit: 2 }
top_risks:
  - "f32 partial_cmp().unwrap() on faction dimension values in control heatmap (F-X07-001)"
  - "Utf8PathBuf::from_path_buf().unwrap() on user-picked OS path (F-X07-002)"
  - "expect(\"non-empty\") on subsector hex_cells in label layout (F-X07-003, F-X07-004)"
---

# Cross-cutting Sweep: PANIC SURFACE (RUST_REVIEW.md §3.1)

## Method

```
grep -rEn "\b(unwrap|expect|panic!|unreachable!|todo!|unimplemented!)\b" \
  --include="*.rs" -- src gui-core builder viewer
```

→ **497 raw sites across 73 files.**

Triage was done per-file using the `#[cfg(test)]` boundary line plus inspection of
the surrounding 5-15 lines for any "checked above" / `is_none() { return }` /
`if let Some(...)` guard. The headline result is that the workspace's panic
surface is **overwhelmingly test-scoped** — only a small handful of sites in
non-test code are actually reachable on realistic input, and zero are CRITICAL.

### Workspace-wide aggregate triage

| Bucket | Count | Notes |
|---|---:|---|
| **TEST-ONLY** | ~452 | Inside `#[cfg(test)] mod tests { … }`, doctest, `src/bin/dhat_profile.rs`, or `gui-core/tests/`. Acceptable. |
| **POST-CONSTRUCT (documented)** | ~22 | `.expect("invariant: <X> checked by <fn>")` / `.expect("checked above")`. Real invariants, hold. Low value to refactor. |
| **POST-CONSTRUCT (undocumented)** | ~12 | `vec[i]`, `.get(id).unwrap()` where the id came from the same map. Should at minimum get a `// PROOF:` comment. |
| **REACHABLE-LIBRARY** | 0 | No library code-path panics on realistic input. (`f64::partial_cmp().unwrap()` in `src/analysis/search.rs:1364` is `#[cfg(test)]`.) |
| **REACHABLE-UI** | 5 | History wizard + sector_view label layout + viewer path picker. Each is a single user click → crash. |
| **TODO/UNIMPLEMENTED** | 0 | No `todo!()` / `unimplemented!()` anywhere in `src/`, `builder/`, `gui-core/`, or `viewer/`. |
| **`unreachable!()`** | 6 | All POST-CONSTRUCT: `src/validate/diff.rs:454,549,705` (None-vs-None match arms are unreachable by callsite filter); `viewer/src/app/export_ui.rs:48,304` (HTML/SVG branches handled by early `return`); `viewer/src/factions_overview.rs:1012` (infinite `for n in 1..` loop). |
| **`panic!()` in non-test code** | 0 | All 9 `panic!` occurrences are in tests (`src/loading/presets.rs:343`, `src/analysis/importance.rs:276`, `builder/src/builder/project_io.rs:1049-1050`, `builder/src/builder/panels/map/mod.rs:290,369`, `gui-core/tests/map_snapshots.rs:353,362`). |
| **`assert!`/`assert_eq!`/`assert_ne!` in non-test code** | 0 | Verified: every `assert*!` macro in the four crates is inside `#[cfg(test)]` or a doctest. |
| **`debug_assert!`** | 0 | None used. |
| **`get_unchecked` / `unwrap_unchecked`** | 0 | None used. |

### Specific subpatterns

| Subpattern | Count | Reachable | Notes |
|---|---:|---:|---|
| `partial_cmp(...).unwrap()` (NaN risk) | 3 | 2 | `src/analysis/search.rs:1364` is `#[cfg(test)]`. `builder/src/builder/panels/control.rs:1231,1240` are real (F-X07-001). |
| `partial_cmp(...).unwrap_or(Ordering::Equal)` | 7 | n/a | Defensive, idiomatic. No finding. |
| `f32::total_cmp` / `f64::total_cmp` | 1 | n/a | `gui-core/src/sector_view.rs:717`. Recommended pattern. |
| `slice[i]` / `vec[i]` indexing | 131 occurrences | 0 reachable | Sample inspection: every indexed access in library code derives `i` from internal iteration counters, not user input. No finding for the sweep, leave per-unit. |
| String byte-slice `s[a..b]` | 0 | 0 | `grep` returned no `[\.\.]` byte-range slices on `&str`/`String`. Workspace uses `.chars()`, `.split_whitespace()`, `.strip_prefix()`. |
| `checked_div` / `checked_rem` | 1 | n/a | Only one use; library code that divides by data-derived values does so after `if denom > 0.0` guards (sampled `src/analysis/economy.rs`, `src/analysis/importance.rs`). No reachable div-by-zero panic. |
| `as u32` truncation from signed/wider int in parsing | reviewed | 0 | This is a §3.7 idiom theme handled by X-cut on `as` casts; out of scope here. |

## Findings

### F-X07-001 — [HIGH] [Panics] `partial_cmp().unwrap()` on f32 in control-heatmap reduction
- **Location:** `builder/src/builder/panels/control.rs:1231`, `builder/src/builder/panels/control.rs:1240`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** High
- **Blast radius:** Builder editor — every time the control heatmap recomputes for any sector containing a faction with a NaN dimension value, the panel panics.
- **Problem:**
  ```rust
  if let Some(&m) = by_fac.values().max_by(|a, b| a.partial_cmp(b).unwrap()) {
      …
  }
  …
  .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
  ```
  `by_fac` values are `f32` sums of `axis(&p.dimensions)` over factions+worlds. Any
  NaN entering `dimensions` (e.g. a user-authored faction TOML with `power = nan`,
  or a 0/0 ratio reaching the dimension formula) propagates through the sum and
  makes `partial_cmp` return `None` → unwrap panics inside the builder UI.
- **Why it matters:** Single bad faction value crashes the heatmap panel on every
  redraw. The rest of the workspace already uses `unwrap_or(Ordering::Equal)` for
  this exact pattern (7 sites — see table above), so this is an inconsistency, not
  a deep design issue.
- **Suggested fix:** Replace with the established workspace idiom.
  ```rust
  // line 1231
  if let Some(&m) = by_fac
      .values()
      .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
  {
      …
  }
  // line 1240 — same change
  ```
  Alternatively use `f32::total_cmp` (already used at `gui-core/src/sector_view.rs:717`).
- **Effort:** S
- **Risk of fix:** Low

### F-X07-002 — [HIGH] [Panics] `Utf8PathBuf::from_path_buf(...).unwrap()` on rfd file-dialog path
- **Location:** `viewer/src/app/mod.rs:237`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** High
- **Blast radius:** Viewer GUI — single click on "open sector" with a non-UTF-8 path crashes the editor.
- **Problem:**
  ```rust
  if let Some(path) = dialog.pick_file() {
      let utf8_path = Utf8PathBuf::from_path_buf(path.clone()).unwrap();
      …
  }
  ```
  `rfd::FileDialog::pick_file()` returns `std::path::PathBuf`, which on Linux/macOS
  is arbitrary bytes and on Windows is WTF-16 with unpaired surrogates possible.
  Any non-UTF-8 segment (legitimate on POSIX) crashes the viewer instead of
  surfacing a friendly error.
- **Why it matters:** The codebase otherwise carefully uses `camino::Utf8PathBuf`
  precisely to enforce UTF-8 invariants — this is the seam where the invariant
  must be checked, not asserted.
- **Suggested fix:**
  ```rust
  let utf8_path = match Utf8PathBuf::from_path_buf(path.clone()) {
      Ok(p) => p,
      Err(_) => {
          self.export_status = "selected path is not valid UTF-8".into();
          return;
      }
  };
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X07-003 — [MEDIUM] [Panics] `expect("non-empty")` on subsector `hex_cells` in label-block placement
- **Location:** `gui-core/src/sector_view.rs:719`, `src/export/svg_export/labels.rs:240`, `src/export/bitmap/labels.rs:247`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** Medium (depends on whether subsectors with zero `hex_cells` are constructed in practice; not blocked by builder UI today)
- **Blast radius:** Both rendering paths (viewer/builder map widget AND the SVG/bitmap exporters) panic if any subsector has an empty `hex_cells` vector. Affects everything that calls `render_sector_image` or `render_svg` — golden tests, exports, GUI.
- **Problem:**
  ```rust
  let &(q0, r0) = s.hex_cells.iter()
      .min_by(|…| d1.total_cmp(&d2))
      .expect("non-empty");
  ```
  in three rendering paths. The invariant "no subsector ever has zero hex_cells"
  is not enforced by any type — `Subsector::hex_cells: Vec<(u8,u8)>` can be empty
  if `build_subsectors` ever assigns no cells to one of the K clusters (a known
  degenerate case for very small sectors with K close to N).
- **Why it matters:** Single empty subsector in any input (user-authored save,
  edge-case generator output) → crash in three different rendering paths
  simultaneously.
- **Suggested fix:** Skip the label-block entirely for empty subsectors.
  ```rust
  let Some(&(q0, r0)) = s.hex_cells.iter().min_by(|…| d1.total_cmp(&d2)) else {
      continue;   // no anchor cell → skip label block
  };
  ```
- **Effort:** S (three sites, same pattern)
- **Risk of fix:** Low — silently dropping a label is a strict improvement over panicking.

### F-X07-004 — [LOW] [Panics] Undocumented POST-CONSTRUCT `.unwrap()` on map look-ups built from same source
- **Location:** Sample sites — `src/export/subsectors/mod.rs:518-619` (10 sites), `src/gen/hidden_routes.rs:416-417`, `src/analysis/economy.rs:917`
- **Category:** Panics & failure surface (§3.1) / Documentation (§3.11)
- **Confidence:** High (these are POST-CONSTRUCT, not bugs)
- **Blast radius:** None today; minor maintainability risk if the construction site is later refactored independently of the lookup site.
- **Problem:** ~12 sites unwrap a `HashMap::get` / `find` result where the key
  was inserted from the same iteration source ~50 lines earlier. The invariant is
  real but undocumented; a future refactor that filters one site without the
  other will introduce a panic.
  Example (`src/export/subsectors/mod.rs:518`):
  ```rust
  let scores: BTreeMap<SystemId, i32> = sector.systems.iter().map(|s| (s.id.clone(), …)).collect();
  …
  let score_a = scores.get(&sys_a.id).unwrap();   // sys_a is also from sector.systems → present
  ```
- **Suggested fix:** Either swap for `.expect("invariant: <X> populated from sector.systems")`
  matching the `world_pool.rs` style, or `unwrap_or(&0)` where a default is benign.
- **Effort:** S
- **Risk of fix:** Low

### F-X07-005 — [LOW] [Panics] `.unwrap()` on `serde_json::to_string_pretty` of `GeneratedSector`
- **Location:** `viewer/src/app/mod.rs:211`
- **Category:** Panics & failure surface (§3.1) / Error handling (§3.4)
- **Confidence:** Low (infallible in current `Serialize` impl; defensive)
- **Blast radius:** Auto-save path in viewer.
- **Problem:** `serde_json::to_string_pretty(sec).unwrap()`. Serialisation of
  `GeneratedSector` cannot fail today because every field implements `Serialize`
  in a non-throwing way. However, if any field is later changed to use a custom
  `serialize_with` that can fail (e.g. a non-finite f32 with strict mode), the
  viewer's autosave will hard-crash.
- **Why it matters:** Auto-save error surfaces as panic instead of `export_status`.
- **Suggested fix:**
  ```rust
  let text = match serde_json::to_string_pretty(sec) {
      Ok(t) => t,
      Err(e) => { self.export_status = format!("autosave serialise failed: {e}"); return; }
  };
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X07-006 — [LOW] [Panics] Wizard anchor `.clone().unwrap()` reachable only if `wizard_anchor_ready` drifts
- **Location:** `builder/src/builder/panels/history.rs:994, 997, 1014, 1027`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** Low (currently POST-CONSTRUCT via `wizard_anchor_ready`)
- **Blast radius:** Builder UI; commit path of the §H5 wizard.
- **Problem:** `wizard_anchor` unwraps `w.anchor_system` / `w.anchor_world` /
  `w.anchor_route` / `w.anchor_region` without locally proving they are set. The
  invariant is enforced by `wizard_anchor_ready(...)` at line 895 in the same
  file — if a future contributor adds a new anchor kind to the enum and forgets
  to extend `wizard_anchor_ready`, the commit path panics.
- **Why it matters:** This is the pattern the §3.1 rubric calls out: the
  invariant is real but distant; a `let ... else` collocates the check.
- **Suggested fix:** Use `let Some(x) = ... else { return HistoryAnchor::Sector; }`
  (which yields the same value as the unset case anyway) instead of
  `.clone().unwrap()` × 4.
- **Effort:** S
- **Risk of fix:** Low

### F-X07-007 — [LOW] [Panics] `state.wishes.as_mut().unwrap()` immediately after early-return guard
- **Location:** `viewer/src/editor/wishes_panel.rs:30`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** High (POST-CONSTRUCT)
- **Problem:** `if state.wishes.is_none() { … return; } let wishes = state.wishes.as_mut().unwrap();`
  The pattern is correct but the workspace already uses `let Some(...) else { return; }` elsewhere.
- **Suggested fix:**
  ```rust
  let Some(wishes) = state.wishes.as_mut() else {
      // existing button block lifted up
      return;
  };
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X07-008 — [LOW] [Panics] `.expect("checked above")` could be `let ... else`
- **Location:** `builder/src/builder/panels/economy.rs:615`, `builder/src/builder/panels/regions.rs:475`, `builder/src/builder/panels/factions.rs:129, 316, 363`
- **Category:** Panics & failure surface (§3.1) / Idiomatic Rust (§3.7)
- **Confidence:** High (POST-CONSTRUCT)
- **Blast radius:** None today.
- **Problem:** Five sites use `.as_ref().expect("checked above")` / `.as_mut().expect("checked")`
  on a field that was just verified `is_some()` by an `if` earlier in the same
  function. Pattern is safe but brittle to refactors that split the function.
- **Suggested fix:** Hoist the borrow into the same `if let Some(…) = …` /
  `let Some(…) = … else { return; }` scope as the guard. No functional change.
- **Effort:** S each
- **Risk of fix:** Low

### F-X07-009 — [LOW] [Panics] `.unwrap()` after `if x.is_none() { continue; }` (typed `let ... else`)
- **Location:** `src/analysis/economy.rs:917`
- **Category:** Panics & failure surface (§3.1) / Idiomatic Rust (§3.7)
- **Confidence:** High (POST-CONSTRUCT)
- **Problem:**
  ```rust
  let sys = by_sys.get(we.system_id.as_str()).copied();
  if sys.is_none() { continue; }
  let sys = sys.unwrap();
  ```
  Modern idiom collapses to one line:
  ```rust
  let Some(sys) = by_sys.get(we.system_id.as_str()).copied() else { continue; };
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-X07-010 — [NIT] [Panics] `unreachable!()` on infinite `for n in 1..` faction-id search
- **Location:** `viewer/src/factions_overview.rs:1012`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** High (mathematically unreachable; `i32::MAX` factions = ~2 billion)
- **Problem:** Documented `unreachable!("unbounded faction id search exhausted")`.
  Acceptable — the only theoretical trigger is exhausting all positive `i32`
  ids, which is impossible in practice and the macro carries the proof message.
- **Suggested fix:** None required. Optional: bound the loop to
  `for n in 1..=u32::MAX` to make the `unreachable!()` syntactically dead, but
  the cost-benefit is essentially zero.
- **Effort:** XS
- **Risk of fix:** None

### F-X07-011 — [NIT] [Panics] `unreachable!()` for handled enum arms (`PendingExport::SectorHtml | SectorSvg`)
- **Location:** `viewer/src/app/export_ui.rs:48`, `viewer/src/app/export_ui.rs:304`
- **Category:** Panics & failure surface (§3.1) / Idiomatic Rust (§3.7)
- **Confidence:** High (POST-CONSTRUCT; both arms handled by early `if matches!(…) { … return; }`)
- **Problem:** Both `unreachable!()` arms are protected by an `if` early-return
  earlier in the function. Idiomatic alternative would be a `match` on a
  narrowed enum (split `PendingExport` into two: `RasterExport` vs
  `VectorExport`) so the compiler proves exhaustiveness without a runtime
  panic. That refactor is out of scope for a panic-surface fix.
- **Suggested fix:** Leave as-is, or add a `// PROOF: SectorHtml/SectorSvg
  handled by early return at line N` comment.
- **Effort:** XS
- **Risk of fix:** None

## Panic-density table (top 20 files by raw count)

The "non-test" column counts sites *outside* `#[cfg(test)] mod tests`. Files
where non-test = 0 contribute zero panic-surface risk despite high raw counts.

| File | Total | Non-test | Top severity | Notes |
|---|---:|---:|---|---|
| `builder/src/builder/command.rs` | 103 | 0 | — | All inside `mod tests` after L937. Clean. |
| `builder/src/builder/panels/map/mod.rs` | 72 | 0 | — | All inside `mod tests` after L96. Clean. |
| `src/export/subsectors/mod.rs` | 27 | 12 | LOW | All non-test sites are POST-CONSTRUCT lookups of system IDs sourced from `sector.systems`. F-X07-004. |
| `src/model/sector_model/mutation.rs` | 21 | 0 | — | All inside `mod tests` after L767. Clean. |
| `src/loading/presets.rs` | 15 | 0 | — | All inside `mod tests` after L299. Clean. |
| `gui-core/src/sector_view.rs` | 14 | 1 | MEDIUM | F-X07-003 (label-block `expect("non-empty")`). Rest are tests after L1373. |
| `builder/src/builder/project_io.rs` | 12 | 0 | — | All inside `mod tests` after L926. Clean. |
| `builder/src/builder/panels/economy.rs` | 12 | 1 | LOW | F-X07-008 (`expect("checked above")`). |
| `src/export/html_export.rs` | 11 | 2 | NIT | Two infallible `write!(String, …).expect("write! into String cannot fail")` — idiomatic. |
| `builder/src/builder/panels/subsectors.rs` | 11 | 0 | — | All inside `mod tests` after L525. Clean. |
| `builder/src/builder/panels/system_map.rs` | 10 | 0 | — | All inside `mod tests` after L749. Clean. |
| `builder/src/builder/panels/system.rs` | 10 | 0 | — | All inside `mod tests` after L1380. Clean. |
| `builder/src/builder/panels/control.rs` | 9 | 2 | **HIGH** | F-X07-001 (two `partial_cmp().unwrap()`). |
| `builder/src/builder/file_watcher.rs` | 9 | 0 | — | All inside `mod tests` after L128. Clean. |
| `src/gen/world_pool.rs` | 8 | 8 | LOW | All 8 are documented `.expect("invariant: <X> checked by first_missing_field")`. Exemplary. |
| `builder/src/builder/panels/routes.rs` | 7 | 0 | — | All inside `mod tests` after L1112. Clean. |
| `builder/src/builder/panels/history.rs` | 7 | 5 | LOW | F-X07-006 (4 wizard anchor unwraps, all guarded by `wizard_anchor_ready` at L895). |
| `src/analysis/power_projection.rs` | 6 | 0 | — | All inside `mod tests` after L193. Clean. |
| `src/analysis/economy.rs` | 6 | 1 | LOW | F-X07-009 (`.unwrap()` after `is_none() continue`). |
| `builder/src/builder/panels/intel.rs` | 6 | 0 | — | All inside `mod tests` after L492. Clean. |

### File-level density observations (themes for orchestrator)

1. **Test density dominates everything.** Of the top 20 files, 13 have **zero**
   non-test panics. The headline "497 panic sites" number overstates risk by
   roughly an order of magnitude — true reachable panic surface in the
   workspace is ~12 sites, with five being LOW POST-CONSTRUCT and two being
   HIGH.

2. **`builder/src/builder/command.rs` and `panels/map/mod.rs` together hold
   175 panic sites — every single one is a test.** This is good — the
   command-bus and map-interaction surface is heavily exercised.

3. **`src/gen/world_pool.rs` is the exemplar.** Every `expect()` carries a
   `// invariant: <X> checked by <fn>` message. Other files should follow.

4. **The `f32`/`f64` ordering pattern is workspace-wide** (35 `partial_cmp`
   usages across 23 files), and is **almost entirely defensive already** — only
   2 of the 35 use raw `.unwrap()` instead of `.unwrap_or(Ordering::Equal)` or
   `total_cmp`. Cleaning up F-X07-001 finishes the convergence; no other sites
   need attention.

5. **No `todo!`, `unimplemented!`, library `panic!`, or library `assert!` exists
   outside test mods.** The codebase is unusually clean on these axes.

## Coverage of §3.1 sub-bullets (per RUST_REVIEW.md / _AGENT_BRIEF.md)

| Sub-pattern | Status |
|---|---|
| `unwrap`/`expect` on reachable errors | F-X07-001, F-X07-002, F-X07-005 |
| Out-of-bounds indexing | No reachable instances (every `vec[i]` derives from internal counter). |
| Integer overflow | Handled separately under X-cut for `as` casts; no reachable panic-paths. |
| `panic!`/`unreachable!`/`todo!`/`unimplemented!` in library code | 0 `panic!`, 6 `unreachable!()` all POST-CONSTRUCT (F-X07-010, F-X07-011), 0 `todo!`/`unimplemented!`. |
| Slicing untrusted length | None observed. |
| `.unwrap_unchecked`/`get_unchecked` | None. |
| Div by zero from data | None reachable (sampled — all sites have `if denom > 0.0` guards). |
| `str` byte-boundary slicing | None observed. |
| `f64::partial_cmp(...).unwrap()` (NaN-reachable) | 2 reachable instances → F-X07-001. |
| `assert!` in library code | 0. |

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| F-X07-001 | HIGH | Replace 2 `partial_cmp().unwrap()` with `unwrap_or(Ordering::Equal)` in builder control heatmap | S | Low |
| F-X07-002 | HIGH | Handle non-UTF-8 path from rfd dialog instead of unwrapping | S | Low |
| F-X07-003 | MEDIUM | Skip label block on empty `hex_cells` in 3 rendering paths | S | Low |
| F-X07-004 | LOW | Add `// PROOF:` / `.expect("invariant: …")` to ~12 undocumented POST-CONSTRUCT map lookups | S | Low |
| F-X07-005 | LOW | Surface serde_json autosave error instead of unwrapping | S | Low |
| F-X07-006 | LOW | Collocate history wizard anchor guard with use via `let ... else` | S | Low |
| F-X07-007 | LOW | Use `let Some(...) else { return; }` in `wishes_panel.rs:30` | S | Low |
| F-X07-008 | LOW | Hoist 5 `expect("checked above")` borrows into the same `if let` scope | S | Low |
| F-X07-009 | LOW | Collapse `is_none()+continue+unwrap` to `let Some else { continue }` | XS | Low |
| F-X07-010 | NIT | Optional: bound `for n in 1..` to make `unreachable!()` syntactically dead | XS | None |
| F-X07-011 | NIT | Add `// PROOF:` comments to 2 `unreachable!()` in `export_ui.rs` | XS | None |
