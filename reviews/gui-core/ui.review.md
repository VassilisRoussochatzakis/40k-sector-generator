---
unit_id: U002
crate: sectorforge-gui-core
paths:
  - gui-core/src/lib.rs
  - gui-core/src/app_icon.rs
  - gui-core/src/jobs.rs
  - gui-core/src/info_panel.rs
  - gui-core/src/palette.rs
loc_reviewed: 2089
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 9, low: 11, nit: 6 }
top_risks:
  - "info_panel allocates dozens of formatted strings per repaint with no caching — steady-state GUI churn (F-002-001)"
  - "JobContext::set_progress and JobHandle::progress can poison/panic on lock failure (F-002-004)"
  - "Public surface of palette.rs is wide, undocumented, and lacks #[non_exhaustive] / #[must_use] coverage — downstream API fragility (F-002-006)"
---

# Review: gui-core UI primitives (info_panel + palette + crate top-level)

## Summary

`gui-core` is the shared egui widget layer. The code is functional and reasonably
well factored, but it is also the API surface that `builder` and `viewer` both
consume, and the public contract is under-disciplined: very few items have
docs, `RouteControlKind` is a non-`#[non_exhaustive]` public enum that already
duplicates `crate::export::render_core::routes::ControlKind`, and the immediate-mode
info panel rebuilds dozens of `String`s per frame with no per-entity caching.
There are no panics from realistic input on the render paths, no
`unsafe`/concurrency soundness issues, and determinism rules are not violated
(no `FxHashMap` iteration, no RNG). The main themes are:
(1) per-frame allocation in `info_panel` (high), (2) public API hygiene of
`palette.rs` and `jobs.rs` (medium/high), and (3) several subtle correctness
edges in geometry helpers (`top_route_control` deterministic tie-breaking,
`darken`/`fade` premultiplied-alpha mismatch).

## Findings

### F-002-001 — [HIGH] [Performance] `info_panel` repaint allocates O(systems + worlds + factions) strings per frame
- **Location:** `gui-core/src/info_panel.rs:74-192`, `:194-291`, `:454-625`, `:627-785`
- **Category:** Performance / Allocation (hot path)
- **Confidence:** High
- **Blast radius:** Every panel repaint — multiple times per second whenever the side panel is visible.
- **Problem:** Each call to `sector_overview*`, `system_summary`, `world_detail`,
  `subsector_summary` reformats every field into a fresh `String` (via `format!`,
  `to_uppercase`, `short` which itself allocates via `chars().collect::<String>()`).
  A typical sector with ~80 systems and ~300 worlds produces hundreds of
  `String` allocations per frame on the right-side panel. `info_panel.rs:79`
  additionally calls `compute_display_buckets(sector, ...)` once per repaint on the
  legacy `sector_overview` entry point even though the cached variant exists.
- **Why it matters:** egui rebuilds widgets every frame. At 60 fps this is
  steady-state allocator churn that shows up in frame-time outliers (GC-like
  hitches) and in heap profiles.
- **Evidence:** `to_uppercase` and `format!` are called inside every loop body
  in `system_summary` (`info_panel.rs:222-232`, `:280-282`, `:286-289`),
  `world_detail` (`:484-486`, `:498-525`), `subsector_summary`
  (`:689-708`, `:715-718`, `:725-728`, `:737-738`, `:747-748`, `:757-758`).
  `short` (`:1102-1110`) always returns an owned `String` even when the input
  already fits.
- **Suggested fix:**
  1. Cache a *display-formatted* snapshot keyed on
     `SectorOverviewCacheKey` (the cache infrastructure at
     `info_panel.rs:21-72` already exists — extend it from "buckets" to
     "rendered legend rows").
  2. `short` should return `Cow<str>` and only allocate when truncation is
     needed.
  3. Replace `format!("{}: ", k)` inside `kv` (`info_panel.rs:837-846`) with
     two consecutive `ui.label`s that don't reformat.
  4. Drop the un-cached `sector_overview` entry point (`info_panel.rs:74-81`)
     in favour of `sector_overview_with_buckets`, or have it use
     `SectorOverviewCache` internally.
  ```rust
  // before: gui-core/src/info_panel.rs:1102
  fn short(s: &str, max: usize) -> String { /* always allocates */ }
  // after:
  fn short(s: &str, max: usize) -> Cow<'_, str> {
      if s.chars().count() <= max {
          Cow::Borrowed(s)
      } else {
          let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
          out.push('.');
          Cow::Owned(out)
      }
  }
  ```
- **Effort:** M
- **Risk of fix:** Low — purely additive.

### F-002-002 — [HIGH] [Idiomatic / API design] `RouteControlKind` duplicates `render_core::routes::ControlKind`; not `#[non_exhaustive]`
- **Location:** `gui-core/src/palette.rs:192-218` (and the duplicate at `src/export/render_core/routes.rs:505-528`).
- **Category:** API design / DRY
- **Confidence:** High
- **Blast radius:** Public surface — both kinds are passed to renderers in `gui-core/src/sector_view.rs:364` and `src/export/bitmap/routes.rs:65`. Drift between the two is silent.
- **Problem:** The `top_route_control` algorithm at `palette.rs:203-218` is a
  byte-for-byte clone of `render_core/routes.rs:512-528`. `RouteControlKind`
  is also a clone of `ControlKind`. If a future variant (e.g. `Smuggling`) is
  added on the export side and forgotten here, the in-app sector view will
  silently render the wrong glyph. Neither enum is `#[non_exhaustive]`, so
  adding a variant *won't* fail to compile downstream.
- **Suggested fix:** Promote `ControlKind` (and the `top_route_control`
  algorithm) into a non-GUI module — e.g. `sectorforge::route_control::TopControl`
  — making it `pub` and `#[non_exhaustive]`. `gui-core::palette` then
  re-uses it directly:
  ```rust
  // gui-core/src/palette.rs
  pub use sectorforge::route_control::{TopControl, top_route_control};
  ```
  This also removes the `String` allocation in `top_route_control`'s return
  type, which is wasteful — callers re-borrow it for `faction_style_by_id`
  immediately.
- **Effort:** S
- **Risk of fix:** Low — same algorithm, mechanical move.

### F-002-003 — [HIGH] [Correctness] `top_route_control` tie-break is sensitive to source order, breaking deterministic glyph picks
- **Location:** `gui-core/src/palette.rs:203-218`
- **Category:** Determinism / Correctness
- **Confidence:** High
- **Blast radius:** Visual output ordering — same input may produce different
  glyphs across builds if route controls iteration order changes (e.g. due to a
  builder edit reordering `route.controls`).
- **Problem:** The comparison is strict `>`: ties keep the first-seen value.
  Iteration is `for c in &route.controls` outer, then a fixed
  `[Interdiction, Patrol, Piracy, Toll]` inner. When two factions have the
  same numeric control score (very common with integer-ish scores), the
  faction that happens to come first in `route.controls` wins. If the
  controls list is rebuilt in a different order (which can happen across
  editor sessions), the rendered glyph color flips.
- **Suggested fix:** Define an explicit tie-break (e.g. faction id ascending,
  then kind index ascending) so the visualization is stable:
  ```rust
  if best.map_or(true, |(id, k, s)| {
      (score, kind as u8, c.faction_id.as_str())
          > (s, k as u8, id)
  }) { ... }
  ```
  (Make `RouteControlKind: Ord` via `#[derive(PartialOrd, Ord, Eq, PartialEq)]`.)
- **Effort:** S
- **Risk of fix:** Low — pure tie-break, scored-larger cases unchanged.

### F-002-004 — [MEDIUM] [Panics] `JobHandle::progress` and `JobContext::set_progress` panic on poisoned mutex
- **Location:** `gui-core/src/jobs.rs:25-28`, `:75-77`
- **Category:** Panics & failure surface
- **Confidence:** High
- **Blast radius:** If any worker thread panics while holding the progress
  lock, every subsequent UI poll of `progress()` panics the render thread —
  catastrophic for a GUI.
- **Problem:** Both methods do `self.progress.lock().unwrap()`. There is no
  panic boundary around `f(job_ctx)` at `jobs.rs:52-55`, so worker panics
  poison the mutex; the next UI frame that reads progress crashes the app.
- **Suggested fix:** Use `Arc<AtomicU32>` instead of `Arc<Mutex<f32>>` — the
  payload is a single `f32` for which atomic load/store is more than
  sufficient, and there's nothing to poison:
  ```rust
  pub progress: Arc<AtomicU32>, // bits of f32
  pub fn progress(&self) -> f32 { f32::from_bits(self.progress.load(Relaxed)) }
  ```
  If keeping `Mutex`, fall back to `.unwrap_or_else(|p| p.into_inner())`.
  Either way, wrap `f(job_ctx)` in `std::panic::catch_unwind` and forward the
  result as `Result<T, JobPanic>` so the GUI can show "job failed" instead of
  crashing.
- **Effort:** S–M
- **Risk of fix:** Low for the atomic swap; M for the `catch_unwind` wrapper
  (needs `T: UnwindSafe`).

### F-002-005 — [MEDIUM] [Resource] `spawn_job` leaks the worker thread when the receiver is dropped
- **Location:** `gui-core/src/jobs.rs:31-66`
- **Category:** Memory / resource management
- **Confidence:** Medium (depends on caller discipline)
- **Blast radius:** A long-running export job whose `JobHandle` is dropped
  before completion keeps consuming CPU until it finishes naturally —
  cancellation is cooperative (`is_cancelled`) and the worker has no
  obligation to poll it.
- **Problem:** `thread::spawn` is detached. The `cancelled` flag is the only
  way to stop the worker; nothing in this file actually checks it. If
  callers (`builder/preview.rs`, `viewer/.../export_ui.rs`) replace a
  `JobHandle` without explicitly calling `.cancel()`, the old worker silently
  continues.
- **Suggested fix:** Make `Drop for JobHandle<T>` set
  `self.cancelled.store(true, …)` so dropping the handle is the documented
  way to stop the job. Document in `JobContext` doc-comment that workers
  *must* poll `is_cancelled` at reasonable intervals.
  ```rust
  impl<T> Drop for JobHandle<T> {
      fn drop(&mut self) { self.cancel(); }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — currently nothing relies on "drop continues".

### F-002-006 — [MEDIUM] [API design] `palette.rs` public surface lacks docs, `#[must_use]`, and visibility minimization
- **Location:** `gui-core/src/palette.rs:10-19, 21, 192-198, 587-664, 673-679, 681-695, 719-797, 799-840, 842-850`
- **Category:** Documentation / API design
- **Confidence:** High
- **Blast radius:** Two downstream crates (`builder`, `viewer`) re-export and
  pattern-match against this surface.
- **Problem:**
  - All ten color constants (`BG`..`PATH_WAYPOINT`) lack `///` docs; readers
    can't tell which is meant for hex fills vs. text vs. dividers without
    grepping.
  - `RouteControlKind` is `pub` but `RouteGeom` is `pub(crate)`-by-default
    private — that's the right call; however `RouteControlKind` should also
    be `#[non_exhaustive]` (see F-002-002).
  - `darken`, `tint`, `contrast_text`, `top_route_control`, `from_rgb`,
    `faction_style*`, the entire `paint_*` family, `STAR_LEGEND` —
    pure functions returning data — should carry `#[must_use]`. Only
    `top_route_control`, `faction_style_by_id`, `faction_style`,
    `faction_style_from_rgb` do today.
  - `STAR_LEGEND` (`palette.rs:842-850`) is exported, undocumented, and
    only used by `gui-core/src/system_view.rs` (per a grep). It should be
    `pub(crate)` or removed.
  - `from_rgb` (`palette.rs:681-683`) is private — good — but its three
    public callers each re-construct a `FactionStyle` from the same
    `FactionStyleRgb` shape; `From<FactionStyleRgb> for FactionStyle`
    would replace `faction_style*`, `faction_style_by_id`, and
    `faction_style_from_rgb` with one impl.
- **Suggested fix:**
  - Add `///` doc-comments to every `pub const`, function, struct, and enum
    in this file. One sentence each. Use `# Panics`/`# Errors` sections
    where applicable (e.g. `darken` does not panic, but the alpha contract
    is non-obvious — see F-002-007).
  - `#[must_use]` every pure function and constant collection.
  - Either `#[non_exhaustive]`-mark `RouteControlKind`, `FactionStyle`, and
    the `RouteControlKind` color helpers, or document that the variant
    space is closed.
  - Implement `impl From<FactionStyleRgb> for FactionStyle`, then
    `faction_style*` become trivial wrappers — or remove them, callers
    can `.into()`.
- **Effort:** M
- **Risk of fix:** Low for docs; Low-M for refactor.

### F-002-007 — [MEDIUM] [Correctness] `darken` produces invalid premultiplied output for translucent inputs; `fade` mismatches premultiplied/unmultiplied
- **Location:** `gui-core/src/palette.rs:587-594` (`fade`), `:635-643` (`darken`)
- **Category:** Correctness / API contract
- **Confidence:** Medium (only observable when the input is not fully opaque)
- **Blast radius:** Translucent overlays (e.g. `theme.rect_select_tint` in
  `map_theme.rs:137` which is `from_rgba_unmultiplied(255, 240, 120, 30)`)
  that pass through `darken`/`fade` will look wrong.
- **Problem:**
  - `egui::Color32` stores **premultiplied** channels. `darken` constructs the
    result via `from_rgba_premultiplied(c.r() * s, c.g() * s, c.b() * s, c.a())`
    — it scales RGB but **not** alpha, breaking the premultiplied invariant
    if `c.a() < 255` and would produce a brighter-than-expected image.
  - `fade` reads the channels and feeds them to `from_rgba_unmultiplied`
    while scaling only the alpha. Color32's accessors return *premultiplied*
    components, so this only round-trips correctly when `c.a() == 255`.
  - The truncating `as u8` (`palette.rs:592, 638-640, 649`) silently
    saturates above-255 floats (e.g. `(amount * 255).round()` where amount
    came from a clamped 0..=1 — currently safe, but the `as u8` should be
    `.clamp(0.0, 255.0) as u8` or `.min(255.0) as u8` to make the contract
    explicit. Some sites already do this (`palette.rs:592, 649`), some don't
    (`palette.rs:638-640`).
- **Suggested fix:** Document the alpha contract at the function level
  ("assumes opaque input"), then either (a) assert `c.a() == 255` in debug,
  or (b) scale alpha along with RGB in `darken`, and have `fade` divide by
  the original alpha before applying.
  ```rust
  pub fn darken(c: Color32, amount: f32) -> Color32 {
      let s = (1.0 - amount).clamp(0.0, 1.0);
      let scale = |v: u8| ((f32::from(v) * s).round()).clamp(0.0, 255.0) as u8;
      Color32::from_rgba_premultiplied(scale(c.r()), scale(c.g()), scale(c.b()), c.a())
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — the common path (opaque palette colors) is
  unchanged.

### F-002-008 — [MEDIUM] [Performance] `routes_block` and `system_history` re-scan all routes/events per call
- **Location:** `gui-core/src/info_panel.rs:404-428` (history filter+sort),
  `:848-906` (routes filter+sort)
- **Category:** Performance / repeat scan
- **Confidence:** Medium
- **Blast radius:** Per-frame full sector scans whenever the system info panel
  is open. With 80 systems × ~120 routes, that's ~9600 string comparisons per
  frame on the routes filter alone, plus the inner `c.faction_id.to_uppercase`
  loop in `routes_block`.
- **Problem:** Linear scans of `sector.routes` and `sector.chronicle.events`
  with allocation-y filter closures, sorted, then iterated for display. Same
  result every frame the panel is open with the same selection.
- **Suggested fix:** Extend `SectorOverviewCache` (or add a sibling
  `SystemDetailCache`) keyed by `(sector_key, system_id)` that pre-computes
  `Vec<&Route>` and `Vec<&HistoryEvent>` once per selection change. Tie its
  invalidation to the same `SectorOverviewCacheKey` rev.
- **Effort:** M
- **Risk of fix:** Low — caching layer is additive; key already exists.

### F-002-009 — [MEDIUM] [Idiomatic] `info_panel` API takes `&str` for IDs but the underlying types are typed (`SystemId`, `WorldId`)
- **Location:** `gui-core/src/info_panel.rs:363` (`world_history(... world_id: &str)`),
  `:404` (`system_history(... system_id: &str)`), `:430-452`
- **Category:** Idiomatic Rust / Type safety
- **Confidence:** High
- **Blast radius:** Public API of `info_panel`; viewer call-sites pass
  `w.id.as_str()` / `sys.id.as_str()` (`viewer/src/app/system_view.rs:36`).
- **Problem:** The module already imports `sectorforge::ids::SystemId`
  (used at `:389`); accepting `&str` discards type information and
  invites mismatching the wrong kind of id (e.g. passing a faction id by
  accident). Compare endpoints types are checked at `:431-451` via
  string `==`.
- **Suggested fix:** Change signatures to take `&SystemId` / `&WorldId`,
  drop the `.as_str()` calls at the call sites. Inside the function,
  use `.as_str()` only at the `==`-comparison line.
- **Effort:** S
- **Risk of fix:** Low — straight type tightening.

### F-002-010 — [MEDIUM] [Idiomatic] `SectorOverviewCacheKey` over-collects, leaving cache potentially stale on edits
- **Location:** `gui-core/src/info_panel.rs:21-46`
- **Category:** Correctness / cache key
- **Confidence:** Medium
- **Blast radius:** Builder edits that don't change counts but do change
  per-faction `system_presence`/`world_presence` (e.g. transferring a system
  between factions) will leave the cache stale — `display_buckets` won't
  recompute even though the visible legend should change.
- **Problem:** The key tracks `seed`, dimensions, total system/world/route/faction
  counts and the sector id. A redistribution between existing factions
  keeps all counts identical.
- **Suggested fix:** Include a per-faction
  `(faction_id, system_presence_len, world_presence_len)` digest, or
  derive a `u64` content hash via `blake3` over the bucket inputs
  (`compute_display_buckets`'s inputs). Alternative: switch to an explicit
  invalidate-on-mutation, called from `BuilderCommand` apply.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-011 — [LOW] [Idiomatic] `app_icon::load_app_icon` swallows decode errors silently
- **Location:** `gui-core/src/app_icon.rs:12-21`
- **Category:** Error handling
- **Confidence:** High
- **Blast radius:** A build with a broken `sectorpic.png` launches with the
  OS default icon and no log line; debugging is annoying. (The bytes are
  `include_bytes!`'d at compile time, so realistic risk is low.)
- **Problem:** `image::load_from_memory(ICON_PNG).ok()?` discards the error.
- **Suggested fix:** Log the error path via `eprintln!` or `log::warn!`
  before returning `None`:
  ```rust
  let img = match image::load_from_memory(ICON_PNG) {
      Ok(img) => img,
      Err(e) => { eprintln!("app icon decode failed: {e}"); return None; }
  };
  ```
- **Effort:** XS
- **Risk of fix:** None.

### F-002-012 — [LOW] [Idiomatic] `lib.rs` only re-exports `entity_link`; downstream callers reach into modules directly
- **Location:** `gui-core/src/lib.rs:1-13`
- **Category:** API design / module discoverability
- **Confidence:** Medium
- **Problem:** The single `pub use nav::entity_link;` is inconsistent —
  builder/viewer code reaches in via `sectorforge_gui_core::palette::...`,
  `sectorforge_gui_core::jobs::...`, `sectorforge_gui_core::info_panel::...`
  (see grep results across both crates). Either go all-in on flat
  re-exports or document the module-as-namespace convention. A short
  crate-level `//!` doc explaining what each module owns would help new
  contributors.
- **Suggested fix:** Add a `//!` doc on the crate, with a one-line
  description of each module. Promote the most-used items
  (`palette::{TEXT, TEXT_DIM, PANEL_BG, BG}`, `palette::faction_style_by_id`,
  `info_panel::SectorOverviewCache`) to crate-level re-exports if you want
  a stable "blessed" surface.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-013 — [LOW] [Idiomatic] `info_panel::sector_overview` duplicates `sector_overview_with_buckets` + un-cached bucket compute
- **Location:** `gui-core/src/info_panel.rs:74-81`
- **Category:** Dead-code / cleanup
- **Confidence:** High
- **Problem:** The function recomputes `compute_display_buckets` every call
  and exists only as a convenience wrapper. The cached path
  (`sector_overview_with_buckets` + `SectorOverviewCache`) is what
  downstream `viewer/src/app/sector_view.rs:127` uses. A grep finds no
  caller of `sector_overview` itself outside this module.
- **Suggested fix:** Delete `sector_overview`. If you want a "one-shot,
  uncached" entry point for tests, mark it `#[cfg(test)]`.
- **Effort:** XS
- **Risk of fix:** Low — verify with `cargo check --workspace`.

### F-002-014 — [LOW] [Idiomatic] `legend_control_row` uses stringly-typed `kind: &str` dispatch
- **Location:** `gui-core/src/info_panel.rs:1053-1100`
- **Category:** Idiomatic Rust / type safety
- **Confidence:** High
- **Problem:** `match kind { "PATROL" => ..., "TOLL" => ..., ..., _ => {} }`.
  A typo at any call site (`info_panel.rs:141-144`) silently renders a row
  with no glyph. The kind set is already modeled as
  `RouteControlKind` in `palette.rs:192-198`.
- **Suggested fix:** Replace the `&str` parameter with `RouteControlKind`,
  add a `pub fn label(self) -> &'static str` on the enum, exhaustive match.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-015 — [LOW] [Idiomatic] `kv` allocates `"{k}:"` on every call
- **Location:** `gui-core/src/info_panel.rs:837-846`
- **Category:** Performance / micro-allocation
- **Confidence:** High
- **Problem:** `format!("{k}:")` allocates a `String` per row. Across a
  full system summary that's ~30+ rows per frame.
- **Suggested fix:** Either use two adjacent `ui.label` calls (label +
  separator + value) or accept `k: &'static str` and pre-suffix at
  compile time. Or use `egui`'s built-in `ui.monospace(k).label(":")`
  pattern.
- **Effort:** XS
- **Risk of fix:** Low.

### F-002-016 — [LOW] [Idiomatic] `short` chars-pass + chars-collect walks the string twice and ignores grapheme clusters
- **Location:** `gui-core/src/info_panel.rs:1102-1110`
- **Category:** Performance / correctness
- **Confidence:** Medium
- **Problem:** `s.chars().count()` walks the entire string, then
  `s.chars().take(max-1).collect::<String>()` walks it again, even when
  no truncation is needed. Also splits in the middle of grapheme clusters
  (e.g. emoji + skin tone). Allocates unconditionally (see also F-002-001).
- **Suggested fix:** Single pass:
  ```rust
  fn short(s: &str, max: usize) -> Cow<'_, str> {
      let mut count = 0;
      for (i, _) in s.char_indices() {
          count += 1;
          if count == max + 1 {
              let mut out = String::with_capacity(i + 1);
              out.push_str(&s[..i]);
              out.push('.');
              return Cow::Owned(out);
          }
      }
      Cow::Borrowed(s)
  }
  ```
  If grapheme correctness matters, add `unicode-segmentation`.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-017 — [LOW] [Performance] `routes_block` reformats and resorts per repaint
- **Location:** `gui-core/src/info_panel.rs:848-906`
- **Category:** Performance
- **Confidence:** High
- **Problem:** Filter → `collect::<Vec<_>>` → `sort_unstable_by` → iterate
  per repaint, and the inner `parts.iter().filter(...).map(format!).collect::<Vec<String>>()` allocates a
  fresh `Vec<String>` per control per frame.
- **Suggested fix:** See F-002-008 (cache by selection). Locally, replace
  the inner `Vec<String>` with a single owned `String` built by
  `write!` into a reusable scratch.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-018 — [LOW] [Idiomatic] `Color32::from_rgb` `as u8` truncation in `darken`/`tint` lacks an explicit clamp
- **Location:** `gui-core/src/palette.rs:638-640`
- **Category:** Idiomatic Rust / silent truncation
- **Confidence:** Medium
- **Problem:** `(f32::from(c.r()) * s) as u8`. `s` is clamped to `0..=1`
  earlier so the maximum is < 256, but a future edit that loosens the clamp
  would silently wrap. Sibling helpers `tint` (`:649`) and `fade` (`:592`)
  do clamp explicitly.
- **Suggested fix:** Use the same `clamp(0.0, 255.0) as u8` idiom as `fade`
  to make the contract uniform.
- **Effort:** XS
- **Risk of fix:** None.

### F-002-019 — [LOW] [Idiomatic] `legend_row`/`legend_route_row`/`legend_control_row` re-build `RichText` and `FontId` per row
- **Location:** `gui-core/src/info_panel.rs:1032-1100`
- **Category:** Performance / repaint cost
- **Confidence:** Medium
- **Problem:** `RichText::new(text).color(TEXT).font(mono(12.0))` constructs
  fresh `FontId` (cheap, cloneable) and `RichText` per row. Fine
  individually, but combined with the per-frame `format!` callers this
  amplifies the allocation count.
- **Suggested fix:** Hoist `static MONO_12: FontId = ...` (`FontId` is not
  `const`-friendly; use `LazyLock<FontId>`) and reuse across calls.
- **Effort:** XS
- **Risk of fix:** None.

### F-002-020 — [LOW] [Idiomatic] `jobs::spawn_job::progress` is `Arc<Mutex<f32>>` — wrong primitive for the job
- **Location:** `gui-core/src/jobs.rs:11`
- **Category:** Idiomatic concurrency
- **Confidence:** High
- **Problem:** A `Mutex<f32>` for a single, monotonic progress value is
  textbook over-locking — `AtomicU32` carrying `f32::to_bits` is the
  textbook fix (and dodges F-002-004).
- **Suggested fix:** See F-002-004.
- **Effort:** S
- **Risk of fix:** Low.

### F-002-021 — [LOW] [Docs] `app_icon`, `jobs`, `lib.rs`: no `//!` module/crate doc
- **Location:** `gui-core/src/jobs.rs:1` (no `//!`), `gui-core/src/lib.rs:1` (no `//!`)
- **Category:** Documentation
- **Confidence:** High
- **Problem:** `app_icon` has a one-line `//!`; `jobs` and `lib.rs` have
  none. The crate-level doc would orient a new reader on
  what `gui-core` owns vs. `builder`/`viewer`.
- **Suggested fix:** Add 3–6 line `//!` to each.
- **Effort:** XS
- **Risk of fix:** None.

### F-002-022 — [NIT] [Style] Long fully-qualified paths everywhere in `info_panel`
- **Location:** `gui-core/src/info_panel.rs:104-114, 121-134, 297, 307-329, 430-451, 849, 923, 943-987, 1013`
- **Category:** Style
- **Confidence:** High
- **Problem:** `sectorforge::sector_model::RouteViewMode::Detailed`,
  `sectorforge::archetypes::NecronPhase::default()`, etc. appear inline
  with no `use` aliasing, making lines >100 chars and the `match`es
  visually noisy.
- **Suggested fix:** Add `use` imports at the top for `RouteViewMode`,
  `RouteType`, `RouteKind`, `RouteStability`, `HistoryAnchor`,
  `HistoryEntityKind`, `ArchetypeState`, `NecronPhase`, `TyranidStage`,
  `GscStage`, `TauSphereBand`, `StabilityState`, `ConflictState`.
- **Effort:** XS
- **Risk of fix:** None.

### F-002-023 — [NIT] [Style] Inconsistent `Color32::from_rgb` vs `Color32::from_rgba_*` decisions
- **Location:** `gui-core/src/palette.rs:21-32, 596-633`
- **Category:** Style
- **Confidence:** Medium
- **Problem:** All colors are opaque sRGB tuples; using `from_rgb` is
  fine, but the long match tables would be easier to scan as
  `const NAME: Color32 = ...` table at top of file (the `BG`/`TEXT`
  block already does this for theme colors). A `const` lookup table
  also makes color-blind audits one PR.
- **Suggested fix:** Promote each `Color32::from_rgb(...)` literal in
  `star_color`, `world_type_color`, `stability_color` to a named `const`,
  group them, then match yields the constant.
- **Effort:** S
- **Risk of fix:** None.

### F-002-024 — [NIT] [Style] `mono(size)` helper hides intent
- **Location:** `gui-core/src/info_panel.rs:816-818`
- **Category:** Style
- **Confidence:** Low
- **Problem:** Single-line `fn mono(size: f32) -> FontId { FontId::monospace(size) }`
  saves five characters per use; inlining at call sites is clearer.
- **Suggested fix:** Remove `mono`, write `FontId::monospace(12.0)` directly.
  (Or do the inverse and make `mono` the canonical typography
  function — but pick one.)
- **Effort:** XS
- **Risk of fix:** None.

### F-002-025 — [NIT] [Style] `info_panel::short` `.` truncation marker is unusual
- **Location:** `gui-core/src/info_panel.rs:1102-1110`
- **Category:** Style / UX
- **Confidence:** Low
- **Problem:** A single `.` reads more like punctuation than truncation.
  Convention is `…` (single char) or `...` (three dots).
- **Suggested fix:** `out.push('…');` — also adjust `max.saturating_sub(1)`
  remains correct (`…` is one char).
- **Effort:** XS
- **Risk of fix:** None.

### F-002-026 — [NIT] [Style] `jobs.rs` test `job_handle_carries_revision_and_cancel_flag` uses real-time `recv_timeout(1s)`
- **Location:** `gui-core/src/jobs.rs:90-103`
- **Category:** Testing
- **Confidence:** Medium
- **Problem:** A 1s timeout makes the test brittle on slow CI; reduces
  test isolation. The job is synchronous so the receive should be
  near-instant.
- **Suggested fix:** Use `recv()` (blocks indefinitely) or
  `recv_timeout(Duration::from_millis(100))`.
- **Effort:** XS
- **Risk of fix:** Low.

### F-002-027 — [NIT] [Style] `legend_control_row` `match kind { ... _ => {} }` silently drops unknown kinds
- **Location:** `gui-core/src/info_panel.rs:1062-1096`
- **Category:** Robustness / debug
- **Confidence:** High
- **Problem:** Wildcard arm hides typos (see F-002-014).
- **Suggested fix:** With the F-002-014 enum conversion, drop the wildcard
  arm — exhaustive `match` is the goal.
- **Effort:** XS
- **Risk of fix:** None.

## Per-category status (§3 coverage)

- **3.1 Panics & failure surface:** F-002-004 (Mutex panic), no other
  unwrap/expect/`as` truncation panics on realistic input. Numeric
  routines in `palette.rs` divide by `len.max(1.0)` / `total > 0.0` checks
  (`:260, :303`). `app_icon::load_app_icon` returns `Option` — F-002-011.
- **3.2 unsafe & soundness:** No `unsafe` blocks. ✓
- **3.3 Ownership/borrow/clone:** `info_panel.rs:1102` `s.to_string()`,
  `:1106` `chars().collect()`, every `.to_uppercase()` allocation (covered
  by F-002-001 and F-002-016). `top_route_control` allocates `String` in
  its return tuple even though every caller re-borrows it
  (`palette.rs:217`, F-002-002 covers).
- **3.4 Error handling:** Only error path in this unit is decode in
  `app_icon` (F-002-011). Mutex poisoning is F-002-004. No
  `Box<dyn Error>` in public API.
- **3.5 Concurrency & async:** No async. Threading via `thread::spawn` in
  `jobs.rs`; soundness issues F-002-004, F-002-005, F-002-020.
- **3.6 Performance:** F-002-001 (hot), F-002-008, F-002-015, F-002-017,
  F-002-019 — the info panel is the main offender. `palette.rs` route
  draw helpers iterate the route once each, fine.
- **3.7 Idiomatic / API design:** F-002-002, F-002-006, F-002-009,
  F-002-012, F-002-013, F-002-014, F-002-018, F-002-020.
- **3.8 Cargo hygiene:** Cargo.toml is minimal and clean; no findings.
  All imports used.
- **3.9 Memory & resource management:** F-002-005 (detached worker on
  handle drop).
- **3.10 Testing:** Inline tests cover `SectorOverviewCache` invalidation
  (`info_panel.rs:1118-1141`) and `JobHandle` basics
  (`jobs.rs:90-130`). No tests for `palette.rs` color helpers,
  `top_route_control` tie-breaking (F-002-003), `darken/fade` alpha
  behaviour, or `info_panel` panic-freeness on empty sectors.
- **3.11 Documentation & maintainability:** F-002-006, F-002-012,
  F-002-021. No TODO/FIXME found in this unit. Magic numbers throughout
  `palette.rs` route-drawing helpers (e.g. `geom.unit * 5.0`,
  `thickness * 1.7`) are tuning constants; consider naming
  the most-used ones (`STRIDE_MULTIPLIER`, `JAGGED_AMPLITUDE`, etc.) for
  one-knob tuning.

## Project-specific invariants

- **No `FxHashMap` iteration:** ✓ This unit holds no `Fx*` types.
- **RNG centralization:** ✓ No RNG draws.
- **Output writer byte-stability:** N/A (no golden writers in this unit;
  the in-app sector view is not golden-tested).
- **Builder command-bus discipline:** N/A (gui-core writes nothing).

## Summary of suggested fixes

| ID | Severity | Short | Effort / Risk |
|---|---|---|---|
| F-002-001 | HIGH | Cache formatted strings & buckets in info_panel; `Cow<str>` for `short` | M / Low |
| F-002-002 | HIGH | Dedup `RouteControlKind` with `ControlKind` + `#[non_exhaustive]` | S / Low |
| F-002-003 | HIGH | Deterministic tie-break in `top_route_control` | S / Low |
| F-002-004 | MED | Replace `Mutex<f32>` with `AtomicU32`; wrap worker in `catch_unwind` | S–M / Low |
| F-002-005 | MED | `Drop for JobHandle` should cancel | S / Low |
| F-002-006 | MED | Doc + `#[must_use]` + visibility audit on `palette.rs` public surface | M / Low |
| F-002-007 | MED | Document/correct premultiplied-alpha contract in `darken`/`fade` | S / Low |
| F-002-008 | MED | Cache per-system route+history lookups | M / Low |
| F-002-009 | MED | `info_panel` history APIs should take `&SystemId`/`&WorldId` | S / Low |
| F-002-010 | MED | Expand `SectorOverviewCacheKey` or switch to explicit invalidate | S / Low |
| F-002-011 | LOW | Log decode error in `load_app_icon` | XS / None |
| F-002-012 | LOW | Add crate-level `//!` doc and decide re-export policy | S / Low |
| F-002-013 | LOW | Delete duplicate `sector_overview` entry point | XS / Low |
| F-002-014 | LOW | Replace `&str` kind in `legend_control_row` with enum | S / Low |
| F-002-015 | LOW | Avoid `format!("{k}:")` per row in `kv` | XS / Low |
| F-002-016 | LOW | Single-pass `short` returning `Cow<str>` | S / Low |
| F-002-017 | LOW | Avoid per-frame `Vec<String>` in `routes_block` | S / Low |
| F-002-018 | LOW | Clamp in `darken`'s `as u8` to match sibling helpers | XS / None |
| F-002-019 | LOW | Cache `FontId::monospace(12.0)` in `LazyLock` | XS / None |
| F-002-020 | LOW | `Arc<Mutex<f32>>` → `Arc<AtomicU32>` for job progress | S / Low |
| F-002-021 | LOW | Add `//!` to `jobs.rs` and `lib.rs` | XS / None |
| F-002-022 | NIT | Add top-of-file `use`s to shorten path noise in `info_panel` | XS / None |
| F-002-023 | NIT | Promote color literals to named `const`s | S / None |
| F-002-024 | NIT | Drop `mono(size)` trivia helper or rename | XS / None |
| F-002-025 | NIT | Use `…` instead of `.` for truncation marker | XS / None |
| F-002-026 | NIT | Tighten `recv_timeout` in `job_handle_carries_*` test | XS / Low |
| F-002-027 | NIT | Drop wildcard arm in `legend_control_row` after F-002-014 | XS / None |
