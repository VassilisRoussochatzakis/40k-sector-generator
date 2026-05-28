---
unit_id: U013
crate: sectorforge
paths:
  - src/export/render.rs
  - src/export/render_core/mod.rs
  - src/export/render_core/canvas.rs
  - src/export/render_core/colors.rs
  - src/export/render_core/grid.rs
  - src/export/render_core/options.rs
  - src/export/render_core/routes.rs
  - src/export/map_theme.rs
  - src/export/html_export.rs
  - src/export/heatmap.rs
  - src/export/writers.rs
loc_reviewed: 2870
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 2, medium: 7, low: 11, nit: 5 }
top_risks:
  - "parse_color panics on non-ASCII 6/8-byte input from user TOML themes (F-013-001)"
  - "remove_per_system_json_files leaves orphaned per-system files after sector shrink (F-013-002)"
  - "render_html accepts compute cost of cloning the whole sector for player redaction (F-013-006)"
---

# Review: U013 — `src/export/` part B (render + render_core + theme + html + heatmap + writers)

## Summary

Mostly tight, well-factored output code. `render_core` is a clean Pass-C/D extraction and the determinism guards (BTreeMap for the faction palette, in-order walks over `sector.systems`, sort-then-dedup for the history index) hold up. Real findings cluster around two seams: a UTF-8 byte-slicing panic in `parse_color`, and the `write_per_system_json_files` / `remove_per_system_json_files` pair which is not idempotent across regenerations. Public render/export entry points are missing `# Errors` / `# Panics` docs. No determinism-rule violations — every `FxMap` in `render.rs` is used as a lookup table and consumed in deterministic outer loops.

## Findings

### F-013-001 — [HIGH] [Panics] `parse_color` panics on non-ASCII hex input

- **Location:** `src/export/map_theme.rs:514-536`
- **Category:** 3.1 Panics / failure surface
- **Confidence:** High
- **Blast radius:** User-supplied TOML map-theme file. Reachable via `parse_map_theme_file → resolve_map_theme` and via `outputs.bitmap.theme` in the project config.
- **Problem:** The 6/8-byte arms use `hex.len()` (byte length) but then slice `&hex[idx..idx+2]` at byte indices. A 6-byte input that contains a multi-byte UTF-8 character whose boundary does not align to the even indices will panic (`byte index N is not a char boundary`). Concrete repro: `parse_color("Aé€", "bg")` — "A" (1B) + "é" (2B) + "€" (3B) = 6 bytes; slice `[0..2]` cuts inside `é`. Should return `Err(InvalidColor)` not panic.
- **Why it matters:** A single bad colour string in a user-edited `map_theme.toml` crashes the whole exporter (and any caller of `validate_config` / `export_all`). Validation today (`src/validate/validation.rs:106`) routes through this same function, so the panic surface includes the pre-generation validator.
- **Evidence:** Read of `parse_color`; cross-checked reachability via grep of `resolve_map_theme` call sites.
- **Suggested fix:** Bounce off `as_bytes()` and check ASCII first, or require `hex.bytes().all(|b| b.is_ascii_hexdigit())` before slicing:
  ```rust
  let bytes = hex.as_bytes();
  if !bytes.iter().all(u8::is_ascii_hexdigit) {
      return Err(MapThemeError::InvalidColor { field: field.to_string(), value: s.to_string() });
  }
  let parse_pair = |idx: usize| -> Result<u8, MapThemeError> {
      u8::from_str_radix(std::str::from_utf8(&bytes[idx..idx + 2]).unwrap(), 16)
          .map_err(|_| MapThemeError::InvalidColor { field: field.to_string(), value: s.to_string() })
  };
  ```
  Add a regression test: `assert!(parse_color("Aé€", "x").is_err())`.
- **Effort:** S
- **Risk of fix:** Low

### F-013-002 — [MEDIUM] [Correctness] `remove_per_system_json_files` leaks orphan files

- **Location:** `src/export/writers.rs:159-173, 121-126`
- **Category:** 3.4 Error handling / correctness
- **Confidence:** High
- **Blast radius:** Any export run where the new sector has fewer systems than a previous run in the same output dir, or where a system was renamed/re-ID'd.
- **Problem:** `remove_per_system_json_files` only iterates `sector.systems` and removes `<id>.json` for **current** systems. Files for systems that no longer exist remain on disk, so consumers walking `systems/` get a mix of current + stale records.
- **Why it matters:** Per-system JSON is documented as "duplicates `sector.json.systems[]`". Stale files break that contract silently.
- **Suggested fix:** Read the directory entries and remove every `*.json` that isn't in the current set (or just unconditionally clear `systems_dir` before re-emitting when `write_per_system_files` is on, then recreate). Sketch:
  ```rust
  fn remove_per_system_json_files(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
      let systems_dir = output_dir.join("systems");
      if !systems_dir.exists() { return Ok(()); }
      let keep: std::collections::HashSet<String> =
          sector.systems.iter().map(|s| format!("{}.json", s.id)).collect();
      for entry in fs::read_dir(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))? {
          let entry = entry.map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
          let name = entry.file_name().to_string_lossy().into_owned();
          if name.ends_with(".json") && !keep.contains(&name) {
              let p = systems_dir.join(&name);
              fs::remove_file(&p).map_err(|e| SectorError::io(p.as_str(), e))?;
          }
      }
      Ok(())
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — add an integration-test fixture that switches `write_per_system_files` on→off.

### F-013-003 — [MEDIUM] [Performance] `format_local_history` clones the hit list inside every world loop

- **Location:** `src/export/render.rs:266-285, 736-872`
- **Category:** 3.3 Cloning / 3.6 Performance
- **Confidence:** High
- **Blast radius:** Once per world per system per sector export. For a ~500-world sector with a dense chronicle, this is `worlds * (clone + sort)` — bounded but unnecessary.
- **Problem:** `format_local_history` does `let mut hits = hits.clone();` then sorts. The original lists were already deduped + sorted-by-id at construction time (lines 95-102); sort order needed at emission time is by `(date, id)`. Sorting once at index-build, then taking `.iter().take(8)`, would avoid the per-world clone+sort.
- **Suggested fix:** In `render_sector_markdown`, after the dedup loop, sort each list by the emission key:
  ```rust
  for list in events_by_system.values_mut() {
      list.sort_by_key(|e| e.id.as_str());
      list.dedup_by_key(|e| e.id.as_str());
      list.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
  }
  ```
  Then `format_local_history` becomes a borrow + `iter().take(8)`. Drop the `let mut hits = hits.clone();` line.
- **Effort:** S
- **Risk of fix:** Low — golden tests will catch any ordering surprise.

### F-013-004 — [MEDIUM] [Performance] `render_core::grid::draw_subsector_borders` rebuilds the owner map every call

- **Location:** `src/export/render_core/grid.rs:52-94`
- **Category:** 3.6 Performance (per-export, not per-frame)
- **Confidence:** Medium
- **Blast radius:** Once per bitmap + once per SVG export. Linear in subsector cell count.
- **Problem:** `owner: HashMap<(i32,i32), &str>` is rebuilt and the nested `for r/for q` loop is then O(width * height * 6) with a HashMap lookup at every neighbour. Use the existing `subsectors[].hex_cells` lookup with `with_capacity(total_cells)` and an `FxHashMap` (lookup-only, no iteration → not a determinism violation). Equally, the outer per-cell `for (i, (dq, dr)) in deltas.iter().enumerate()` does a hash lookup per side even when the cell has no border — fast-path the case where every neighbour is the same id.
- **Suggested fix:** Two cheap wins:
  ```rust
  let total: usize = subsectors.iter().map(|s| s.hex_cells.len()).sum();
  let mut owner: rustc_hash::FxHashMap<(i32, i32), &str> =
      rustc_hash::FxHashMap::with_capacity_and_hasher(total, Default::default());
  ```
  And before the neighbour loop, skip the cell entirely if `deltas.iter().all(|(dq,dr)| owner.get(&(q+dq,r+dr)).copied() == Some(here_id))`. Both are no-functional-change for golden tests.
- **Effort:** S
- **Risk of fix:** Low — strictly lookup-only Fx use is per the CLAUDE.md rules.

### F-013-005 — [MEDIUM] [Performance] `route_thickness_f32` recomputed per-route; minor but consistent waste

- **Location:** `src/export/render_core/routes.rs:50-67`
- **Category:** 3.6 Performance
- **Confidence:** Medium
- **Blast radius:** Per route × per export.
- **Problem:** `route_thickness_f32(&opts.theme, route.stability, hex_size)` is computed inside the route loop, but its inputs only vary by `route.stability` (one of 4 values). Cache the 4 values once before the loop.
- **Suggested fix:**
  ```rust
  use crate::sector_model::RouteStability::*;
  let thick = [
      (Stable, canvas.quantize(route_thickness_f32(&opts.theme, Stable, hex_size)).max(1.0)),
      (Unstable, ...),
      (Hazardous, ...),
      (Perilous, ...),
  ];
  // Inside the loop: `let thickness = thick.iter().find(|(s,_)| *s == route.stability).unwrap().1;`
  ```
  Or simpler: a `match route.stability` inside the loop returning a precomputed `f32`.
- **Effort:** S
- **Risk of fix:** Low (golden-byte stability preserved — same `quantize` boundary).

### F-013-006 — [MEDIUM] [Performance] `render_html` clones the entire sector for every GM-edition render too

- **Location:** `src/export/html_export.rs:80-92`
- **Category:** 3.3 Cloning / 3.6 Performance
- **Confidence:** High
- **Blast radius:** Each HTML export.
- **Problem:** `let view = match cfg.player_observer.as_deref() { Some(observer) => redact_for_observer(...), None => sector.clone(), };` clones the entire `GeneratedSector` even in GM mode where the view is identical. `serde_json::to_string_pretty(&view)` would happily take `&sector` directly.
- **Why it matters:** `GeneratedSector` is large (systems + worlds + relations + economy + chronicle), and the clone is followed by a JSON serialisation that re-walks every byte anyway. Cloning is wasted.
- **Suggested fix:**
  ```rust
  let owned;
  let view: &GeneratedSector = match cfg.player_observer.as_deref() {
      Some(observer) => { owned = redact_for_observer(sector, observer, cfg.player_min_confidence); &owned }
      None => sector,
  };
  let sector_json = if cfg.compact_json { serde_json::to_string(view)? } else { serde_json::to_string_pretty(view)? };
  ```
- **Effort:** S
- **Risk of fix:** Low — same golden output.

### F-013-007 — [MEDIUM] [Idiomatic] `write!` into pre-grown `String` should replace `push_str(&format!(...))` in row loops

- **Location:** `src/export/render.rs:38-54, 119-124, 142-159, 211-226, 250-255, 301-344, 449-459, 472-482, 502-508, 519-531, 541-547, 710-715, 753-869, 911-921`
- **Category:** 3.6 Performance, 3.7 Idiomatic
- **Confidence:** Medium
- **Blast radius:** Markdown export — called once per `export_all`; per-row allocation × thousands of rows for big sectors.
- **Problem:** Pattern `s.push_str(&format!(...))` allocates a temporary `String` per row. `std::fmt::Write` lets `write!(s, "...", ...)` push directly into the existing buffer. The same pattern is already used in `html_export.rs:121` and `:204`.
- **Why it matters:** Reduces allocator churn during the largest export and makes the code shorter. Same byte output.
- **Suggested fix:** `use std::fmt::Write as _;` at the top of the module, then mechanically rewrite `s.push_str(&format!("...", a, b));` → `write!(s, "...", a, b).expect("write! to String");`. Could also pre-size `s` with `String::with_capacity(estimated)` based on system/world counts.
- **Effort:** M (~20 sites, mechanical)
- **Risk of fix:** Low — golden tests cover the bytes.

### F-013-008 — [LOW] [Docs] Public `render_sector_markdown` / `render_system_markdown` missing module + per-fn docs

- **Location:** `src/export/render.rs:1-10, 166-201`
- **Category:** 3.11 Documentation
- **Confidence:** High
- **Problem:** Module doc is one line; the two public fns have no `///` doc beyond a `// Spec §12` line that is on `render_sector_markdown` only. Per the brief: render entry points should carry `# Examples` and, when returning `Result`, `# Errors`. These don't return `Result`, but they should still state determinism + that they never panic.
- **Suggested fix:** Add:
  ```rust
  /// Render `sector` as a deterministic, GM-facing Markdown report.
  ///
  /// Pure; never mutates `sector`. Output bytes depend only on the
  /// sector's serialised fields plus the static template — same input
  /// produces the same string.
  ///
  /// # Panics
  /// Does not panic for any sector produced by `crate::generate_sector`.
  pub fn render_sector_markdown(...) -> String { ... }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-013-009 — [LOW] [Docs] `export_all` / `export_json` / `export_bundle` missing `# Errors`

- **Location:** `src/export/writers.rs:59, 210, 220`
- **Category:** 3.11 Documentation
- **Confidence:** High
- **Problem:** All three return `Result<_, SectorError>` but carry no `# Errors` block. Public API on the crate root via `pub use writers::*;`.
- **Suggested fix:** Add `# Errors` describing the `SectorError::Io` / `SectorError::ExportFailed` / `SectorError::InvalidConfig` cases each can return. Already done correctly on `write_html` / `write_html_to` — mirror that style.
- **Effort:** S
- **Risk of fix:** Low

### F-013-010 — [LOW] [Idiomatic] `JsonFormat::from_flag` keeps the bool ergonomics inverted

- **Location:** `src/export/writers.rs:31-37`
- **Category:** 3.7 Idiomatic
- **Confidence:** Medium
- **Problem:** Comment says "the bool stays in config-land only", but every call site is `JsonFormat::from_flag(cfg.pretty_json)`. An `impl From<bool> for JsonFormat` would let the boolean-to-format mapping live in one place and at call sites read `cfg.pretty_json.into()`.
- **Suggested fix:** Replace with `impl From<bool> for JsonFormat`.
- **Effort:** S
- **Risk of fix:** Low

### F-013-011 — [LOW] [Idiomatic] `RouteViewMode::default()` in `export_all` hardcodes view choice

- **Location:** `src/export/writers.rs:85`
- **Category:** 3.7 Idiomatic
- **Confidence:** Medium
- **Problem:** `RenderOptions { ..., route_view_mode: RouteViewMode::default() }` — `OutputConfig::bitmap` carries `faction_fill`, `heatmap`, `theme` but no `route_view_mode`, so the CLI export can never select an alternate view. Either the field is dead weight on `RenderOptions` for the CLI path, or `BitmapConfig` should plumb it.
- **Suggested fix:** If reachable from the GUI but not from CLI by design, add a `// only the GUI exporter overrides this` comment. Otherwise add the field to `BitmapConfig` with `#[serde(default)]`.
- **Effort:** S
- **Risk of fix:** Low

### F-013-012 — [LOW] [Idiomatic] `MapTheme::print_mono`/`imperial_archive`/... are `fn`, but `gm_dark` is `pub fn`

- **Location:** `src/export/map_theme.rs:218, 252, 286, 320, 354, 388`
- **Category:** 3.7 API design
- **Confidence:** High
- **Problem:** Asymmetric visibility for what is the same conceptual constructor. Either every preset is `pub` (so callers can request a specific named theme without going through `builtin`/`resolve_map_theme`) or none of them are.
- **Suggested fix:** Make them all `pub(crate)` and expose only `MapTheme::builtin(name)` as the public entry; or make all six `pub` for symmetry. (Note: `MapTheme::builtin` is currently `fn builtin(...) -> Option<Self>` not `pub`, but called via `resolve_map_theme` — so leaning toward "all `pub(crate)`".)
- **Effort:** S
- **Risk of fix:** Low

### F-013-013 — [LOW] [API] `HeatmapMode` and `LabelDensity`/`LegendStyle`/`SymbolSet` lack `#[non_exhaustive]`

- **Location:** `src/export/heatmap.rs:13-41`, `src/export/map_theme.rs:147-176`
- **Category:** 3.7 API design
- **Confidence:** Medium
- **Problem:** `HeatmapMode` has grown five variants since first cut (`Tension`, `TradeVolume`, `FoodOutput`, `TitheStress`, `SupplyVulnerability`, `ConflictIntensity` are all marked `§4 NEW2.md` / `§12`). Each addition is a breaking change for downstream `match` exhaustiveness. Same for the four theme-axis enums.
- **Suggested fix:** Add `#[non_exhaustive]` to all five.
- **Effort:** S
- **Risk of fix:** Low (consumers in the workspace already wildcard or list every variant).

### F-013-014 — [LOW] [Idiomatic] `HeatmapMode::label` uses `match` over the same variants twice (with `ALL`)

- **Location:** `src/export/heatmap.rs:44-101`
- **Category:** 3.7 Idiomatic
- **Confidence:** Low
- **Problem:** Three independent enumerations of the variants: `ALL`, `label`, `base_color_rgb`. Easy to forget one when adding a variant; the labels for `Tension`, `ConflictIntensity` etc. are now hand-maintained mirrors. A const lookup table tied to the variant indices avoids drift.
- **Suggested fix:** Optional — keep the matches but add a `#[cfg(test)] fn label_for_each_variant_in_all()` to prove parity.
- **Effort:** S
- **Risk of fix:** Low

### F-013-015 — [LOW] [Idiomatic] `colors::short` does `chars().count()` twice for a single-pass truncate

- **Location:** `src/export/render_core/colors.rs:81-89`
- **Category:** 3.6 Performance, 3.7 Idiomatic
- **Confidence:** High
- **Problem:** `s.chars().count() <= max` walks the whole string; then on the truncation arm `s.chars().take(max-1).collect()` walks it again. For long-running labels this is 2× the work, and for any short label it adds a redundant pass.
- **Suggested fix:**
  ```rust
  pub(crate) fn short(s: &str, max: usize) -> String {
      let mut iter = s.chars();
      let head: String = iter.by_ref().take(max).collect();
      if iter.next().is_none() { head } else {
          let mut out: String = head.chars().take(max.saturating_sub(1)).collect();
          out.push('.');
          out
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-013-016 — [LOW] [Allocation] `format_subfaction`/`format_force` allocate even when both fields are `None`

- **Location:** `src/export/render.rs:874-890`
- **Category:** 3.3 Cloning
- **Confidence:** Medium
- **Problem:** Returns `String` even for the `(None, None)` case where it could return `Cow<'_, str>` (`Cow::Borrowed("")` for the empty arm, `Cow::Owned(format!(...))` otherwise). Called inside the world-faction-table loop.
- **Suggested fix:** Return `Cow<'a, str>` from these two helpers.
- **Effort:** S
- **Risk of fix:** Low (caller is `write!`-context — both arms format fine).

### F-013-017 — [LOW] [Idiomatic] `parse_map_theme_file`'s error message stitches two TOML errors

- **Location:** `src/export/map_theme.rs:499-508`
- **Category:** 3.4 Error handling
- **Confidence:** Medium
- **Problem:** When both parse attempts fail, the message is `"{with_table_err}; also failed as bare theme: {bare_err}"`. The user gets two TOML diagnostics that often point at different lines, making it hard to tell which form is the right one. Prefer: attempt the table form; if it fails *with the table key missing*, fall back; otherwise return the table-form error verbatim.
- **Suggested fix:**
  ```rust
  match toml::from_str::<MapThemeFile>(text) {
      Ok(file) => Ok(file.map_theme),
      Err(e) if e.to_string().contains("missing field `map_theme`") =>
          toml::from_str::<MapThemeConfig>(text).map_err(|b| MapThemeError::InvalidFile(b.to_string())),
      Err(e) => Err(MapThemeError::InvalidFile(e.to_string())),
  }
  ```
  Brittle on the error-message contains; safer is to test for `map_theme.is_some()` via a `serde_json::Value` peek, or document the dual-form behaviour and surface only the bare-form error.
- **Effort:** S
- **Risk of fix:** Low

### F-013-018 — [LOW] [Maintainability] `MapThemeConfig::overlay` macro is order-sensitive and unverified

- **Location:** `src/export/map_theme.rs:101-140`
- **Category:** 3.11 Maintainability
- **Confidence:** Medium
- **Problem:** Every `MapThemeConfig` field has to be repeated in `overlay`, `MapTheme` field list, `resolve_map_theme` overrides, and the `Default` derive. The four lists drift in lock-step today (verified by reading) but the next field add will silently miss `overlay`. Add a `#[cfg(test)]` test that round-trips overlay vs. manual field-by-field setting for every field, or move to a derive macro.
- **Suggested fix:** Add this test:
  ```rust
  #[test] fn overlay_preserves_all_set_fields() {
      let base = MapThemeConfig::default();
      let full = MapThemeConfig { background: Some("#000000".into()), /* every field set */ ..Default::default() };
      let merged = base.overlay(full.clone());
      assert_eq!(format!("{merged:?}"), format!("{full:?}"));
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-013-019 — [NIT] `events_by_system` / `events_by_world` could pre-size with `with_capacity`

- **Location:** `src/export/render.rs:58-59`
- **Category:** 3.6 Performance
- **Confidence:** Medium
- **Problem:** Both are `FxMap::default()` with no capacity. Allocations resize as the index grows. Once-per-export, but cheap to fix.
- **Suggested fix:** `FxMap::with_capacity_and_hasher(sector.systems.len(), Default::default())` for `events_by_system`, and `sector.systems.iter().map(|s| s.worlds.len()).sum::<usize>()` for `events_by_world`.
- **Effort:** S

### F-013-020 — [NIT] `format_sector_map` could `String::with_capacity`

- **Location:** `src/export/render.rs:349-395`
- **Category:** 3.6 Performance
- **Problem:** `out: String = String::new()` then a per-cell push. Roughly `(width*2 + 2) * height + ~64` bytes of static framing — easy to pre-allocate.
- **Suggested fix:** `let mut out = String::with_capacity(((sector.width * 2 + 1) * sector.height) as usize + 64);`
- **Effort:** S

### F-013-021 — [NIT] `region_glyph` uses a non-glyph fallback set inconsistent with the legend line

- **Location:** `src/export/render.rs:392, 552-564`
- **Category:** 3.11 Maintainability
- **Problem:** Glyphs `+`, `I`, `%` are returned for `NecropolisDrift`, `BeaconChain`, `EmpyricBleed`, but the legend printed at line 392 only lists `~`, `^`, `=`, `#`, `*`. A user seeing `+` in the map has no legend entry.
- **Suggested fix:** Extend the legend line, or have `region_glyph` route those three to one of the documented glyphs.
- **Effort:** S

### F-013-022 — [NIT] `RouteGeom::at` clamps `t` silently

- **Location:** `src/export/render_core/routes.rs:126-132`
- **Category:** 3.7 Idiomatic
- **Problem:** `let t = t.clamp(0.0, self.total);` swallows what would otherwise be a programmer error (e.g. `dot_clusters` adds `local` offsets that can push `t` out of range). Silent clamping means the offending pattern draws on the endpoint and you don't notice.
- **Suggested fix:** Add a `debug_assert!(t >= -EPSILON && t <= self.total + EPSILON)` before the clamp so test runs catch over-shoots.
- **Effort:** S

### F-013-023 — [NIT] `top_route_control` builds `String` for a `&str` faction id

- **Location:** `src/export/render_core/routes.rs:512-528`
- **Category:** 3.3 Cloning
- **Problem:** Returns `Option<(String, ControlKind, f32)>` even though the source is `c.faction_id` (an `Arc<str>`-like / `FactionId` newtype). Per-route per-export allocation.
- **Suggested fix:** Return `Option<(FactionId, ControlKind, f32)>` (clone the typed id) or `Option<(&'a str, ControlKind, f32)>` if lifetime carries.
- **Effort:** S

## Determinism check (CLAUDE.md hard rule)

- `render.rs` uses `FxMap` for `events_by_system`, `events_by_world`, `at`, `region_at`. Every consumer is `.get(key)` inside a deterministic outer loop (`for sys in &sector.systems`, `for r in 0..height; for q in 0..width`). The two `values_mut()` loops at 95-102 sort + dedup the Vec values but do not iterate map keys for output. **No determinism violations.**
- `render_core/routes.rs:45-67` uses `HashMap<&str, (f32, f32)>` for centers; consumed by `.get(...)` keyed on the deterministic `route.from_system_id` iteration. **OK.**
- `render_core/grid.rs:52-94` uses `HashMap<(i32,i32), &str>` for subsector owner; consumed by `.get(...)` inside `for r/for q`. **OK.**
- `heatmap::compute_rgb` returns `HashMap<SystemId, HeatCellRgb>`; consumed in `bitmap/grid.rs:32` and `svg_export/mod.rs:90` purely via `.get(&sys.id)`. **OK.**
- `html_export::build_faction_palette_json` correctly uses `BTreeMap` (lines 181, 196). **OK.**
- `html_export::redact_for_observer` does `sys.intel.by_observer.retain(...)` — `by_observer` is a `BTreeMap` (confirmed `src/analysis/intel.rs:21`). **OK.**

## HTML escaping check

- All user strings inlined into the HTML body go through `html_escape` (`html_export.rs:100-102`): `view.title`, `view.id`, and `edition`. Faction names + ids inside the JSON payload go through `serde_json::to_string` (`html_export.rs:201, 207, 208`), which produces correctly-escaped JSON-string literals.
- Client-side `renderer.js:698` defines a matching `esc(...)` and uses it on every interpolation of sector text into `innerHTML`.
- One spot uses raw template substitution into a style attribute (`renderer.js:727`): `style="background:${pal.fill}"`. `pal.fill` comes from `format!("#{:02x}{:02x}{:02x}", ...)` in `build_faction_palette_json` — strictly `#` + 6 hex chars, no injection vector.
- **No HTML-injection findings.**

## Empty-category accounting (rubric §3)

- 3.1 Panics & failure surface: F-013-001.
- 3.2 unsafe & soundness: No findings (no `unsafe` in scope).
- 3.3 Ownership / cloning: F-013-003, F-013-006, F-013-016, F-013-023.
- 3.4 Error handling: F-013-002, F-013-017.
- 3.5 Concurrency & async: No findings (no concurrency in scope).
- 3.6 Performance: F-013-003, F-013-004, F-013-005, F-013-007, F-013-015, F-013-019, F-013-020.
- 3.7 Idiomatic Rust & API: F-013-010, F-013-011, F-013-012, F-013-013, F-013-014, F-013-022, F-013-023.
- 3.8 Dependencies: No findings — imports are tight; `std::collections::HashMap` is correctly used over `FxHashMap` since the data is keyed by `&str`/`SystemId` and the API surface is shared with `render_core` (no determinism risk because no iteration).
- 3.9 Memory & resource management: No findings — `fs::write` releases the file handle deterministically; no caches grow without bound.
- 3.10 Testing: No findings — `html_export::tests` covers escaping, byte-stability, and theme switching; `map_theme::tests` covers theme resolution and overlays. Gap noted (F-013-018) is upgraded into a suggested test, not a finding by itself. `parse_color` should grow a non-ASCII regression test as part of F-013-001's fix.
- 3.11 Documentation & maintainability: F-013-008, F-013-009, F-013-018, F-013-021.

## Summary of suggested fixes

- F-013-001 — HIGH — `parse_color` panics on non-ASCII 6/8-byte input; ASCII-check before slicing — S / Low.
- F-013-002 — MEDIUM — `remove_per_system_json_files` leaves stale files; sweep the dir not the new system list — S / Low.
- F-013-003 — MEDIUM — `format_local_history` clones+sorts the hit list per world; sort once at index-build — S / Low.
- F-013-004 — MEDIUM — `draw_subsector_borders` rebuilds + queries owner map naively; pre-size + skip cells with uniform neighbours — S / Low.
- F-013-005 — MEDIUM — `route_thickness_f32` recomputed per route; cache the four stability values — S / Low.
- F-013-006 — MEDIUM — `render_html` clones whole sector for GM edition; borrow when no redaction — S / Low.
- F-013-007 — MEDIUM — `push_str(&format!(...))` row pattern in `render.rs`; switch to `write!` into the buffer — M / Low.
- F-013-008 — LOW — Add `///` + `# Panics` to `render_sector_markdown` / `render_system_markdown` — S / Low.
- F-013-009 — LOW — Add `# Errors` blocks to `export_all` / `export_json` / `export_bundle` — S / Low.
- F-013-010 — LOW — Replace `JsonFormat::from_flag` with `impl From<bool>` — S / Low.
- F-013-011 — LOW — `route_view_mode` plumbed nowhere; either remove from `RenderOptions` or expose on `BitmapConfig` — S / Low.
- F-013-012 — LOW — `MapTheme::print_mono` etc are private but `gm_dark` is `pub`; pick one — S / Low.
- F-013-013 — LOW — `#[non_exhaustive]` on `HeatmapMode`, `LabelDensity`, `LegendStyle`, `SymbolSet`, `RouteLineMode` — S / Low.
- F-013-014 — LOW — Add a test that every `HeatmapMode::ALL` variant has a `label` + `base_color_rgb` — S / Low.
- F-013-015 — LOW — `colors::short` walks the string twice; single-pass via `iter.next()` peek — S / Low.
- F-013-016 — LOW — `format_subfaction`/`format_force` always allocate; return `Cow<'_, str>` — S / Low.
- F-013-017 — LOW — `parse_map_theme_file` stitched-error message confuses users; choose one diagnostic — S / Low.
- F-013-018 — LOW — Add a parity test for `MapThemeConfig::overlay` covering every field — S / Low.
- F-013-019 — NIT — `with_capacity` for `events_by_system`/`events_by_world` — S / Low.
- F-013-020 — NIT — `with_capacity` for `format_sector_map`'s output — S / Low.
- F-013-021 — NIT — Region glyph legend doesn't cover three new glyphs (`+`, `I`, `%`) — S / Low.
- F-013-022 — NIT — `RouteGeom::at` silently clamps `t`; add `debug_assert!` — S / Low.
- F-013-023 — NIT — `top_route_control` returns owned `String`; use the typed `FactionId` — S / Low.
