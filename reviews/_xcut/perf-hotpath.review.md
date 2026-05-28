---
unit_id: X05
crate: workspace (cross-cutting)
paths:
  - gui-core/src/sector_view.rs
  - gui-core/src/system_view.rs
  - gui-core/src/info_panel.rs
  - gui-core/src/palette.rs
  - gui-core/src/heatmap.rs
  - builder/src/builder/panels/*.rs
  - viewer/src/app/*.rs
  - viewer/src/editor/*.rs
  - src/gen/generation/routes.rs
  - src/gen/faction_style.rs
  - src/analysis/search.rs
  - src/analysis/influence_field.rs
  - src/export/bitmap/primitives.rs
  - src/export/svg_export/primitives.rs
  - src/export/writers.rs
  - src/export/html_export.rs
  - src/validate/diff.rs
  - benches/generation.rs
loc_reviewed: ~6500 (sampled across hot paths)
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 7, low: 4, nit: 3 }
top_risks:
  - "Per-frame String/format! allocation in every panel + sector_view + info_panel (F-X05-001)"
  - "rayon search runs full budget even after a winner is found early (F-X05-002)"
  - "faction_style_by_id linear scan inside per-route render + per-system tint loops (F-X05-003)"
  - "SVG color_hex allocates a fresh String per shape per attribute (F-X05-004)"
---

# Review: Performance hot paths cross-cutting sweep

## Summary

Three perf themes dominate, and each cuts across many files so individual unit reviewers are likely to under-call them:

1. **Per-frame String/`format!` density in egui panels.** Every panel's `show()` callback runs at the egui repaint rate (60–144 Hz when animating, otherwise on-demand). The repository has **439** `.to_string()` calls in panel/viewer/gui-core source, plus **~340** `format!` invocations concentrated in `info_panel.rs` (64), `world.rs` (44), `system.rs` (44), and `history.rs` (31). None of these reuse a scratch `String` or arena. Most are unavoidable for label text, but the dominant pattern is `&format!("{k}: {v}", ...)` going straight into `Ui::label`, where a `RichText`/borrowed-format helper or a thread-local scratch `String` would amortize the allocator hit.

2. **Linear scans of `sector.systems` / `sector.factions` / `sector.routes` from inside render loops.** `faction_style_by_id` (`src/gen/faction_style.rs:218`) is a `factions.iter().find(|f| f.id == id)`, called from `compute_system_tints` (per-system) on every PNG/SVG export and from `draw_route_control_glyph` (per-route) on every GUI frame. With 12–20 factions the constant is small, but with 200 routes × 60 Hz it's still 240K linear scans/sec, all per-frame allocation-adjacent because the `FactionStyleRgb` callee then calls `rgb_to_hex` which `String`-allocates. Same shape recurs in 19 panel `iter().find` sites (sample: `briefing.rs:127`, `orbital.rs:28`, `system_map.rs:390`, `routes.rs:388`, `routes.rs:382`).

3. **Whole-document `to_string_pretty` → `fs::write` for every JSON/SVG/HTML export.** `src/export/writers.rs:54,138,154,184,202,303` and `src/export/html_export.rs:69` all build the entire output as a `String`, then hand it to `fs::write`. For a normal-scale (96-system) sector this is fine; for the 200-system fixture it's tens of MB allocated, copied, then released. A `BufWriter<File>` + `serde_json::to_writer_pretty` would cut peak memory in half and avoid the full-document realloc chain.

Crate-by-crate, the picture is:

- **`gui-core/src/sector_view.rs`** is the right shape (viewport culling, capacity-presized `centers` HashMap, BTreeSet inputs) but allocates a fresh `to_ascii_uppercase()` `String` per system per frame for labels (lines 587, 622, 770) and `pip.to_string()` per system (line 554). At 200 systems × 60 Hz = ~36K `String`s/sec just for labels.
- **`gui-core/src/info_panel.rs`** allocates ~10–40 `String`s per rendered entity (system/world/subsector/route), 80% of them via `format!` then immediately dropped after `ui.label`. This is the worst single-file render-path allocation surface in the workspace.
- **`src/analysis/search.rs`** parallelises with rayon (good) but always processes the full budget even after a winner is found (line 1098 `(0..budget).into_par_iter()`). A long search with a high budget (`default 256`) pays for ~10× more `generate_sector` calls than the sequential version did when the winner is in the first 5%.
- **`src/validate/diff.rs`** is actually well-implemented — every entity match uses `BTreeMap` indices, not nested linear scans. Markdown rendering uses `writeln!` into a reused `String`. The only real cost is the per-`worlds_added` `.iter().map(|w| format!(...)).collect::<Vec<_>>().join(", ")` (line 905–910 and 917–921) which makes a temp Vec just to join — `itertools::join` or a manual `write!` loop would skip the Vec.
- **`src/export/bitmap/primitives.rs`** is tight: `put_pixel` is `#[inline]`, `fill_row` uses one bounds check per row not per pixel, `fill_rect` uses `copy_within` to splat rows. Allocation per shape is zero. **Don't touch this.**
- **`src/export/svg_export/primitives.rs`** is the opposite of the bitmap path — `color_hex` (line 8) returns a fresh `String` on every call, and is called *twice* per shape (fill + stroke). For a 200-system, 600-route SVG that's thousands of throwaway 7-byte allocations.

Bench coverage (from `benches/generation.rs`): generate_sector, validate_project, validate_sector_invariants, render_sector_image, encode_png_bytes are measured. **None** of these cover: GUI frame paint, `analytics::analyze`, `diff_sectors`, `html_export::render_html`, `svg_export::render_sector_svg`, `analysis::search::run_search`. Findings tagged "needs benchmark to validate fix" mean the aggregator should route them to a perf engineer with a Criterion bench added first.

## Findings

### F-X05-001 — [HIGH] [Performance] Per-frame `format!` + `.to_string()` density across all egui panels and `info_panel.rs`
- **Location:** Pervasive. Worst offenders: `gui-core/src/info_panel.rs` (64 `format!`), `builder/src/builder/panels/world.rs` (44), `builder/src/builder/panels/system.rs` (44), `gui-core/src/sector_view.rs:554,587,622,770`. Pattern repeats across all 42 panel files.
- **Category:** Performance / Allocation
- **Confidence:** High (count is direct; cost is inferred from `Ui::label(RichText::new(format!(...)))` pattern)
- **Blast radius:** Per-frame (every panel's `show()` runs at 60–144 Hz when the egui ctx requests repaint)
- **Context:** **per-frame**, GUI render loop
- **Problem:** Each `ui.label(&format!("…: {}", v))` allocates a fresh `String`, hands it to egui (which clones into a galley), then drops both. For info_panel showing a single world that's ~30–40 small `String`s. Across a builder session with one panel + map repainting at 60 Hz, that's 2–5K String allocations/sec for label text alone.
- **Why it matters:** egui already pays a per-frame cost for layouting and glyph atlasing; piling per-label allocator churn on top makes the editor feel less crisp at high zoom (when more labels are visible). Heap fragmentation matters less than the steady allocator chatter — but at 144 Hz on a hi-DPI screen the jank floor is visible.
- **Evidence:** `grep -c "format!" gui-core/src/info_panel.rs` → 64; `grep -rEn "ui\.label.+format!" builder/src/builder/panels/` shows hundreds of sites; sector_view.rs:770 reads `let label = sys.name.to_ascii_uppercase();` per system per frame.
- **Suggested fix:** Three-step, lowest-friction first:
  1. Introduce `pub fn kv_into(ui: &mut Ui, buf: &mut String, k: &str, v: impl fmt::Display)` in `gui_core::info_panel`. Buf is a panel-state field, cleared before each call. The whole `info_panel` API can switch to this without touching call sites if the panel owns the scratch buffer.
  2. For sector_view labels: precompute `Vec<Arc<str>>` of uppercased names in `SectorMapCache` and hand the borrow to the painter. `to_ascii_uppercase` is invariant per-sector — recompute only on sector-key change (the cache key already exists in `SectorOverviewCache`).
  3. Use `RichText::new` directly with `&str` borrows where possible; `RichText: From<&str>` exists and `Ui::label(s)` accepts a `WidgetText` that holds a `Cow<'_, str>`. Calling `ui.label(text)` with `&str` avoids the intermediate `String`.
- **Effort:** L (single sweep, ~1 day, mechanical)
- **Risk of fix:** Low (per-panel buffer; no API/format changes; snapshot tests catch any drift)
- **Bench:** **needs benchmark** — Criterion can't easily measure egui paint cost; recommend wiring `puffin` or `tracing-tracy` and recording before/after with a `cargo bench`-style scripted scroll.

### F-X05-002 — [HIGH] [Performance] `src/analysis/search.rs` always processes full budget even after a winner is found
- **Location:** `src/analysis/search.rs:1098-1123`
- **Category:** Performance / Algorithmic
- **Confidence:** High (code-author comment at line 1083 acknowledges the design)
- **Blast radius:** Per-build (interactive: search is the slowest single user-facing command, runs `generate_sector + analytics::analyze` per candidate)
- **Context:** **per-build** (rare but very expensive — default budget 256 × `generate_sector` ≈ 30–60s)
- **Problem:** The sequential loop the comment refers to (line 1083–1090) used to `break` on first-pass. The rayon rewrite collects every slot first, then bucketises sequentially. With default budget=256 and a winner at n=5, the worker pool keeps running 251 more `generate_sector + analyze + evaluate` cycles whose results are discarded.
- **Why it matters:** A typical interactive use-case finds a winner in the first 5–20 candidates; the current implementation pays the full 256× cost regardless. With 8 cores you save some wall-clock (~8× rayon speedup), but the wasted CPU + battery is large. On a small budget where you'd accept the full enumeration anyway, it's fine.
- **Evidence:** Lines 1141–1147 explicitly normalise `candidates_evaluated` to mimic the sequential count, confirming the author knew the work was happening but kept it for determinism.
- **Suggested fix:** Two options, pick by what determinism guarantee you actually need:
  1. **Cheap fix (preserves byte-stability).** Use `rayon::iter::ParallelIterator::find_map_first` to short-circuit. `find_map_first` returns the lowest-`n` element among `Some(_)` results, identical semantics to a sequential `break`, and is allowed to stop spawning new work once it has a candidate.
  2. **Better fix.** Process candidates in batches of `num_cpus`. After each batch, if a passing report exists, stop. Near-miss collection updates `report_top` incrementally.
- **Effort:** S (option 1: ~30 lines)
- **Risk of fix:** Low (option 1) / Medium (option 2 — need to re-verify byte-stable near-miss ordering)
- **Bench:** **needs benchmark** — no Criterion coverage for `run_search`. Add one with budget=64 and a constraint that passes at n=5 to demonstrate the regression.

### F-X05-003 — [HIGH] [Performance] `faction_style_by_id` linear scan called per-route per-frame and per-system per-export
- **Location:** Definition `src/gen/faction_style.rs:218-228`. Hot call sites: `src/export/bitmap/grid.rs:43` (per-system tint, per-export), `gui-core/src/palette.rs:687-695` → `palette.rs:238` (per-route midpoint glyph), `gui-core/src/info_panel.rs:159,344,894` (per-faction-bucket, per-frame).
- **Category:** Performance / Big-O
- **Confidence:** High
- **Blast radius:** Per-frame **and** per-build
- **Context:** **per-frame** for the palette/info_panel call sites, **per-build** for the bitmap export call site
- **Problem:** Each call is `factions.iter().find(|f| f.id == id)` and then constructs a fresh `FactionStyle` (which itself calls `rgb_to_hex` allocating a 7-byte `String` per faction-style request — see `faction_style.rs` `rgb_to_hex`). With 200 routes × 60 Hz × 1 call per route midpoint glyph = 12K `find` + 12K `String` allocs/sec just for route control glyphs. The same pattern hits during bitmap rendering for every system tinted with a faction colour.
- **Why it matters:** The faction list is short (typically 12–20), so the `find` itself is fast. The real cost is the *repetition*: a single per-frame `FxHashMap<&str, &FactionStyleRgb>` resolved once per frame in `SectorMapCache` would amortize it.
- **Evidence:** `grep -n "faction_style_by_id\|faction_style_rgb_by_id" -r src gui-core builder viewer` finds 20+ call sites across rendering and analytics.
- **Suggested fix:**
  1. Add a `faction_style_index: FxHashMap<FactionId, FactionStyleRgb>` to `SectorMapCache` (`gui-core/src/sector_view.rs:23`). Build once, lookup is O(1).
  2. Change `faction_style_by_id` / `faction_style_rgb_by_id` to accept the precomputed index in the hot-path variants; keep the linear-scan version for one-shot CLI uses.
  3. Inside `compute_system_tints` (`src/export/bitmap/grid.rs:23`), accept the same precomputed index instead of `&[GeneratedFaction]`.
  4. Verify CLAUDE.md: `FxHashMap` is OK here because it's lookup-only — output ordering is driven by the underlying `Vec<GeneratedFaction>`, not the map.
- **Effort:** M
- **Risk of fix:** Low (mechanical signature change; types stay compatible)
- **Bench:** Covered indirectly by `bench_render_png`; add a `bench_faction_glyph` per-frame microbench for the GUI path.

### F-X05-004 — [HIGH] [Performance] SVG `color_hex` allocates a fresh `String` per attribute, per shape
- **Location:** `src/export/svg_export/primitives.rs:8-10` (definition); called from lines 28, 35, 54, 61, 85, 91, 113, 135 of the same file (8 call sites, each per shape).
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Per-build (SVG export); also per-frame if a renderer ever streams SVG live
- **Context:** **per-build** for `write_sector_svg_to`; per-shape rate
- **Problem:** Every `rect`, `circle`, `polygon`, `line`, `text` calls `color_hex(fill)` (and `color_hex(stroke)` when present), each returning a fresh 7-byte `String` that is then immediately interpolated into `write!`. For a 200-system, 600-route SVG with grid + region overlays the count is roughly `(200 * 4) + (600 * 2) + (grid_cells * 1)` = thousands of throwaway `String`s.
- **Why it matters:** Allocator pressure makes the export visibly slower than the bitmap path for sectors that should be SVG's sweet spot (small file, fast). The exact text is 7 bytes: small enough that SSO might catch some on platforms with 23-byte SSO, but Rust's `String` does not do SSO at all — every call goes to the global allocator.
- **Evidence:** `grep -n "format!" src/export/svg_export/primitives.rs` shows `format!("#{:02x}{:02x}{:02x}", ...)` as the only allocation in the file.
- **Suggested fix:** Change `color_hex` to write directly into the buffer:
  ```rust
  // Before:
  // fn color_hex(c: Rgba<u8>) -> String { format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2]) }
  // After:
  fn write_color_hex(s: &mut String, c: Rgba<u8>) {
      let _ = write!(s, "#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2]);
  }
  ```
  Then at every call site, replace `f = color_hex(fill)` with an inline `write!(s, r#"fill=""#)`, `write_color_hex(s, fill)`, `write!(s, r#"" "#)`. Slightly more code per call site, zero allocations. Alternatively, since SVG is large and contiguous, use a `[u8; 7]` byte-buffer and `s.push_str(std::str::from_utf8(&buf).unwrap())`.
- **Effort:** S
- **Risk of fix:** Low (golden tests will catch any whitespace drift; numeric output is unchanged)
- **Bench:** **needs benchmark** — no Criterion for `render_sector_svg`; add one mirroring `bench_render_png`.

### F-X05-005 — [MEDIUM] [Performance] All JSON/MD/HTML writers do `to_string_pretty` + `fs::write` (whole document in RAM)
- **Location:** `src/export/writers.rs:52-56, 138, 154, 184, 202, 303`; `src/export/html_export.rs:62-69`.
- **Category:** Performance / IO
- **Confidence:** High
- **Blast radius:** Per-build, large inputs
- **Context:** **per-build** — peak memory and serialize+write latency
- **Problem:** Every writer follows the pattern `let text = serde_json::to_string_pretty(value)?; fs::write(path, text)?`. For a 200-system sector, `sector.json` is several MB pretty-printed and the `text` `String` is held in full before the single-shot `write` syscall. The `String` itself needs reallocations during `to_string_pretty`'s push-down growth.
- **Why it matters:** Peak RSS for the `sectorforge export` CLI is ~2× what it needs to be on large fixtures. The HTML export is worse because it also embeds the serialised sector in the HTML body and emits a single-shot file write of the combined string.
- **Evidence:** `grep -n "to_string_pretty\|to_writer_pretty" src/export/` shows zero uses of `to_writer_pretty`. `writers.rs:54` is the canonical pattern.
- **Suggested fix:** Replace each `to_string_pretty` + `fs::write` pair with a `BufWriter<File>` + `to_writer_pretty`:
  ```rust
  let file = std::fs::File::create(path).map_err(|e| SectorError::io(path.as_str(), e))?;
  let mut w = std::io::BufWriter::new(file);
  serde_json::to_writer_pretty(&mut w, value).map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
  w.flush().map_err(|e| SectorError::io(path.as_str(), e))?;
  ```
  Add a `write_md_and_json_streaming` helper since the pattern repeats. Wrap the html `text` builder so it streams sections into the BufWriter rather than collecting first.
- **Effort:** S–M (writers.rs is small; html_export needs a bigger refactor)
- **Risk of fix:** Low for JSON (byte-identical output guaranteed by serde); html_export needs golden test re-baseline if buffering changes flush ordering.
- **Bench:** Add `bench_export_json_writer` (current vs streaming) at the `large` scale.

### F-X05-006 — [MEDIUM] [Performance] `gui-core/src/sector_view.rs` rebuilds per-system label `String` every frame; `SectorMapCache` already exists
- **Location:** `gui-core/src/sector_view.rs:587, 622, 770`. `SectorMapCache` defined lines 23–79.
- **Category:** Performance / Recomputation
- **Confidence:** High
- **Blast radius:** Per-frame
- **Context:** **per-frame** rendering
- **Problem:** For every system, every frame, `sys.name.to_ascii_uppercase()` allocates a fresh `String`. For the subsector pass it does the same plus `s.name.strip_prefix("Subsector ")...to_ascii_uppercase()`. None of these depend on per-frame state (zoom level, pan); they're invariant per-sector.
- **Why it matters:** 200 systems × 60 Hz = 12K String allocs/sec for label uppercase alone. Trivial to fix, real on profilers.
- **Suggested fix:** Add `system_labels: Vec<Arc<str>>` and `subsector_labels: Vec<(SubsectorId, Arc<str>)>` to `SectorMapCache`. Populate once in `SectorMapCache::new`. Painter accepts `&str` via `Cow`.
- **Effort:** S
- **Risk of fix:** Low (cache invalidation already keyed; golden test for the cache exists at info_panel.rs:1119)
- **Bench:** Same as F-X05-001 — needs egui frame instrumentation.

### F-X05-007 — [MEDIUM] [Performance] `src/gen/generation/routes.rs` allocates `Vec<&Arc<str>>` + `format!("feature:…")` per pair × per modifier
- **Location:** `src/gen/generation/routes.rs:45-99`
- **Category:** Performance / Allocation in inner loop
- **Confidence:** High
- **Blast radius:** Per-build (route generation is O(systems²))
- **Context:** **per-build** — `i in 0..N, j in i+1..N` with N = system count
- **Problem:**
  1. Line 45–50: `combined_tags: Vec<&Arc<str>> = ... collect()` is allocated per pair (~20K pairs at 200 systems) just to be iterated with `.iter().any()` three times. The `collect` is pure overhead — the iterator can be reconstructed at each `.any()` site, or the three checks can be folded into one pass that sets three booleans.
  2. Lines 77, 83, 89, 95: `let tag = format!("feature:{}", taxonomy::to_snake_case(s));` re-runs `to_snake_case` and re-allocates per pair per modifier. The `taxonomy::to_snake_case(s)` is invariant per-modifier — hoist to a per-modifier precomputed `(prefix, snake_value)` table built once before the pair loop.
- **Why it matters:** With 200 systems × (200·199/2 ≈ 20K) pairs, even a 100-byte `String` allocation per pair-per-modifier-per-`feature:` adds up to MB of allocator churn. `bench_generate` at the 200-system scale will show the difference.
- **Evidence:** Code inspection of lines 33–105; the `combined_tags` collect-then-any-three-times is the explicit hot spot.
- **Suggested fix:**
  1. Replace `combined_tags` `collect()` with a closure `|tag: &str| systems[i].worlds.iter().chain(systems[j].worlds.iter()).flat_map(|wd| wd.tags.iter()).any(|t| t.as_ref() == tag)` and call it once per check, or fold all checks into a single pass:
     ```rust
     let mut boost_hub = false; let mut warp_hazard = false; /* etc */
     for tag in systems[i].worlds.iter().chain(systems[j].worlds.iter()).flat_map(|w| w.tags.iter()) {
         match tag.as_ref() {
             "feature:trade_hub" | "feature:freeport" | ... => boost_hub = true,
             ...
         }
     }
     ```
  2. Pre-compute the modifier tag strings outside the pair loop:
     ```rust
     let modifier_keys: Vec<(Option<String>, Option<String>, Option<String>, Option<RouteType>, f64)> =
         rules.modifiers.iter().map(|m| (
             m.when.notable_feature.as_ref().map(|s| format!("feature:{}", taxonomy::to_snake_case(s))),
             ...
         )).collect();
     ```
- **Effort:** M
- **Risk of fix:** Low (byte-stable output verified by `cargo test --test it -- golden`)
- **Bench:** Covered by `bench_generate` at scale `24x30_200`.

### F-X05-008 — [MEDIUM] [Performance] `info_panel.rs::routes_block` linear-scans all routes per render
- **Location:** `gui-core/src/info_panel.rs:848-906`
- **Category:** Performance / Big-O in render path
- **Confidence:** High
- **Blast radius:** Per-frame when system info is open
- **Context:** **per-frame** (info_panel renders on every selection-related frame)
- **Problem:** `sector.routes.iter().filter(|r| r.from_system_id == sys.id || r.to_system_id == sys.id)` walks every route in the sector for every system info-panel render. For 600 routes that's 600 string comparisons per frame just to find ~5 routes incident to the system.
- **Why it matters:** This is O(N_routes) per info panel show, and 60 Hz × 600 = 36K comparisons/sec. The info panel also `format!`s into the inner loop for each hit, compounding F-X05-001.
- **Suggested fix:** Add `routes_by_system: FxHashMap<SystemId, Vec<RouteId>>` to a shared `InfoPanelCache` (sibling of `SectorOverviewCache`/`HeatmapCache`), keyed on the same sector-key the existing caches use. CLAUDE.md OK: lookup-only, output order driven by sorted RouteIds.
- **Effort:** S
- **Risk of fix:** Low
- **Bench:** needs benchmark.

### F-X05-009 — [MEDIUM] [Performance] `system_history` / `world_history` collect-then-sort per render with full chronicle scan
- **Location:** `gui-core/src/info_panel.rs:364-388, 404-428`
- **Category:** Performance / Recomputation
- **Confidence:** High
- **Blast radius:** Per-frame
- **Context:** **per-frame** (info_panel)
- **Problem:** Both functions scan `sector.chronicle.events` (can be hundreds of entries) and sort the matches each frame. The filter is by `event_mentions_*` which itself iterates `e.entities`. Total per-frame work is O(events × avg_entities_per_event).
- **Suggested fix:** Precompute `events_by_system: FxHashMap<SystemId, Vec<EventId>>` and `events_by_world: FxHashMap<WorldId, Vec<EventId>>` in a chronicle-keyed cache (same pattern as `SectorOverviewCache`). The export render path already does this — see `src/export/render.rs:58-59` for the exact pattern using `FxMap<&str, Vec<&HistoryEvent>>`.
- **Effort:** S
- **Risk of fix:** Low
- **Bench:** needs benchmark.

### F-X05-010 — [MEDIUM] [Performance] `info_panel.rs::subsector_summary` allocates `Vec<_>` + sort for every taxonomy stat block (5 blocks)
- **Location:** `gui-core/src/info_panel.rs:713-761`
- **Category:** Performance / Recomputation
- **Confidence:** High
- **Blast radius:** Per-frame when subsector is selected
- **Context:** **per-frame**
- **Problem:** Five blocks (world types, populations, tech levels, governments, features) each do `let mut v: Vec<_> = ...counts.iter().collect(); v.sort_unstable_by(...); for ... in v.iter().take(N)`. The taxonomy counts on a `Subsector` are invariant for the sector — they're set during generation and only change when the user edits.
- **Suggested fix:** Cache the sorted top-N lists on `Subsector` as `Vec<(String, u32)>` precomputed in `subsectors::Subsector::sort_summary_blocks()`, invoked when subsectors are generated. The current per-frame sort produces deterministic output already — moving it to generation time is a free win.
- **Effort:** S
- **Risk of fix:** Low (move work earlier; golden tests catch any output change)
- **Bench:** needs benchmark.

### F-X05-011 — [MEDIUM] [Performance] `src/validate/diff.rs` `worlds_added`/`worlds_removed` Markdown collect-Vec just to join
- **Location:** `src/validate/diff.rs:901-922`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Per-build (diff is a one-shot CLI)
- **Context:** **per-build / once-per-CLI** — fix is NIT-class but cluster goes here
- **Problem:**
  ```rust
  sd.worlds_added.iter().map(|w| format!("`{}` {}", w.id, w.name)).collect::<Vec<_>>().join(", ")
  ```
  Allocates one `String` per world, then a `Vec<String>`, then the joined `String`. For a diff with many changed systems this can be MB of intermediate allocation just for the markdown report.
- **Suggested fix:**
  ```rust
  use std::fmt::Write as _;
  let mut buf = String::new();
  for (i, w) in sd.worlds_added.iter().enumerate() {
      if i > 0 { buf.push_str(", "); }
      let _ = write!(buf, "`{}` {}", w.id, w.name);
  }
  let _ = writeln!(s, "  - Worlds added: {buf}");
  ```
- **Effort:** S
- **Risk of fix:** Low (output bytes unchanged)

### F-X05-012 — [LOW] [Performance] `HeatmapCache` / `SectorOverviewCache` cache keys do per-call `.to_string()` of `id` + `seed`
- **Location:** `gui-core/src/heatmap.rs:23-52`, `gui-core/src/info_panel.rs:22-46`
- **Category:** Performance / Allocation
- **Confidence:** Medium
- **Blast radius:** Per-frame *if* the key check happens per-frame
- **Context:** **per-frame** (every `get_or_compute` call rebuilds the key)
- **Problem:** `HeatmapCacheKey::from_sector` allocates 2 `String`s (`sector_id`, `seed`) plus 8 `usize`s every time the cache is queried. The key is *only* used for `==` against the previously stored key — it never escapes the function. An `Arc<str>` or borrowing key would compare without allocation.
- **Suggested fix:** Either (a) store the previously seen key as an `(Arc<str>, Arc<str>, u32, u32, ...)` tuple and compare borrows on lookup, or (b) hash the inputs into a `u64` (blake3 / `xxh3`) and compare the hash — both fully sidestep `String::clone`.
- **Effort:** S
- **Risk of fix:** Low (no API change; comparison semantics unchanged for the realistic input space)
- **Bench:** needs benchmark.

### F-X05-013 — [LOW] [Performance] Builder panel `iter().find` over `systems` from `show()` callbacks
- **Location:** Sample: `builder/src/builder/panels/subsectors.rs:104,265`, `system.rs:982,1290,1365`, `routes.rs:382,388`, `map/mod.rs:188,217,743`, `orbital.rs:28`, `system_map.rs:390,831`, `briefing.rs:127`.
- **Category:** Performance / Big-O
- **Confidence:** High
- **Blast radius:** Per-frame for any panel that's open
- **Context:** **per-frame**
- **Problem:** Every `state.sector.systems.iter().find(|s| s.id == id)` is O(systems) per frame. At 200 systems and frequent re-rendering, each open panel pays ~200 string comparisons per lookup. Many panels do 3–5 such lookups per `show()`.
- **Why it matters:** Individual scans are fast (factions are short), but the *count* across panels and the recomputation every frame is wasteful. A `BuilderState::systems_by_id: FxHashMap<SystemId, usize>` index updated by the command bus would make every lookup O(1).
- **Suggested fix:** Extend `BuilderState::derivations` (per CLAUDE.md §panel recipe) with `systems_by_id`, `worlds_by_id`, `routes_by_id`, `factions_by_id` indices. Invalidate on relevant `BuilderCommand` apply. CLAUDE.md OK: lookup-only, output ordering driven by the underlying `Vec`.
- **Effort:** M
- **Risk of fix:** Low–Medium (have to thread invalidation through the command apply path; the existing derivations infrastructure is the right place)
- **Bench:** needs benchmark.

### F-X05-014 — [LOW] [Performance] `top_route_control` in `palette.rs` re-iterates every controls slot 4×
- **Location:** `gui-core/src/palette.rs:203-218`
- **Category:** Performance / Inner loop
- **Confidence:** High
- **Blast radius:** Per-frame (called once per route midpoint glyph)
- **Context:** **per-frame**
- **Problem:** For each `RouteControl c in route.controls`, the inner loop iterates a 4-tuple of `(kind, score)` pairs and runs the same comparison. The outer + inner is O(controls × 4). The 4 score fields per control could be inlined as a max-of-four, eliminating the inner loop's allocation-free but still branchy iteration.
- **Why it matters:** Tiny per-call, but called once per route per frame.
- **Suggested fix:**
  ```rust
  for c in &route.controls {
      let (best_kind, best_score) = if c.interdiction >= c.patrol.max(c.piracy).max(c.toll) {
          (RouteControlKind::Interdiction, c.interdiction)
      } else if c.patrol >= c.piracy.max(c.toll) {
          (RouteControlKind::Patrol, c.patrol)
      } else if c.piracy >= c.toll {
          (RouteControlKind::Piracy, c.piracy)
      } else {
          (RouteControlKind::Toll, c.toll)
      };
      if best.map(|(_, _, s)| best_score > s).unwrap_or(true) {
          best = Some((c.faction_id.as_str(), best_kind, best_score));
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low (golden tests cover bitmap; egui rendering has no golden so visual check needed)
- **Bench:** needs benchmark.

### F-X05-015 — [LOW] [Performance] `bench` profile uses thin LTO; production profile uses fat LTO — bench numbers don't reflect prod
- **Location:** `Cargo.toml:[profile.bench] lto = "thin", codegen-units = 1`; `[profile.release] lto = "fat", codegen-units = 1, panic = "abort"`
- **Category:** Performance / Methodology
- **Confidence:** High
- **Blast radius:** All bench results
- **Context:** **once (config)**
- **Problem:** Criterion benches built under `[profile.bench]` use thin LTO; the production binary uses fat LTO + `panic = "abort"`. The performance characteristics differ — fat LTO can inline across crates (relevant for `gui-core` ↔ `sectorforge` boundary, which is most of the perf surface). Bench numbers may overstate or understate prod by ±20%.
- **Why it matters:** Methodology integrity. A `bench_with_fat_lto` profile (or matching `[profile.bench]` to `[profile.release]`) would make the Criterion numbers actionable for prod decisions.
- **Suggested fix:** Either:
  1. Set `[profile.bench] lto = "fat"` to match release — slower bench build, accurate numbers.
  2. Add a `[profile.bench-fat]` profile and a `bench-fat` Cargo alias; document in `docs/OPTIMIZE.txt` which to use for prod-relevant decisions.
- **Effort:** S
- **Risk of fix:** Low (build time only)
- **Bench:** N/A.

### F-X05-016 — [NIT] [Performance] `gui-core/src/sector_view.rs::hex_vertices` returns a `[Pos2; 6]`, then `draw_hex` does `.to_vec()` to build a `Vec<Pos2>` for `Shape::convex_polygon`
- **Location:** `gui-core/src/sector_view.rs:1099-1110, 1112-1119`
- **Category:** Performance / Allocation
- **Confidence:** Medium (depends on whether egui can take `&[Pos2]`)
- **Blast radius:** Per-hex per-frame
- **Context:** **per-frame**
- **Problem:** `draw_hex` and `draw_hex_fill` build a `[Pos2; 6]` on the stack, then `.to_vec()` it for `Shape::convex_polygon`. For an N×M sector that's N·M allocations per frame just for hex outlines.
- **Suggested fix:** Check egui 0.x's `Shape::convex_polygon` API — if it accepts `impl Into<Vec<Pos2>>` only, the alloc is required. Otherwise replace with `Shape::Path { points: pts.to_vec(), ... }` only when there's an actual stroke; for fill-only and small hex sizes, the `circle_filled` branch already short-circuits. Investigation needed.
- **Effort:** S (investigation)
- **Risk of fix:** Low

### F-X05-017 — [NIT] [Performance] `Vec::new()` in `SectorView::show` for `planet_positions` (system_view.rs) and `obstacles` (sector_view.rs)
- **Location:** `gui-core/src/system_view.rs:216`, `gui-core/src/sector_view.rs:598` (`obstacles` uses `with_capacity`, `placed` does not)
- **Category:** Performance / Allocation
- **Confidence:** Medium
- **Blast radius:** Per-frame
- **Context:** **per-frame**
- **Problem:** `system_view.rs:216: let mut planet_positions: Vec<(usize, Pos2, f32)> = Vec::new();` — capacity is exactly `system.worlds.len()`, known up-front. `sector_view.rs:598: let mut placed: Vec<egui::Rect> = Vec::with_capacity(subs.len());` is fine, but a couple of sibling Vecs nearby (like `cands` at line 645) are not pre-sized.
- **Suggested fix:** `Vec::with_capacity(system.worlds.len())` and similar one-liners. Trivial.
- **Effort:** S
- **Risk of fix:** None

### F-X05-018 — [NIT] [Performance] `gui-core/src/sector_view.rs:600-605` rebuilds `sys_cells: HashSet<(i32, i32)>` per frame inside subsector label placement
- **Location:** `gui-core/src/sector_view.rs:600-605`
- **Category:** Performance / Recomputation
- **Confidence:** High
- **Blast radius:** Per-frame, but the conditional gate (subsector labels visible) trims it
- **Context:** **per-frame** (when subsectors+labels enabled)
- **Problem:** A `HashSet<(i32, i32)>` of every system coordinate is rebuilt every frame inside the subsector label loop. It's already in `SectorMapCache::hex_system` — the cache covers this exact use case but the local fallback path here doesn't consult it.
- **Suggested fix:** When `self.cache` is `Some`, take `sys_cells` from `cache.hex_system.keys()` (already a HashMap, can iterate or expose a derived `HashSet<(i32,i32)>` on the cache).
- **Effort:** S
- **Risk of fix:** Low

## Hot-path inventory

| Path | Context | Current state | Concerns |
|---|---|---|---|
| `gui-core/src/sector_view.rs` | per-frame | viewport-culled, caches via `SectorMapCache`, mostly tight | F-X05-006 label `to_ascii_uppercase` per system, F-X05-016 `to_vec` per hex, F-X05-018 sys_cells rebuilt |
| `gui-core/src/system_view.rs` | per-frame | small (one system) | F-X05-017 `Vec::new` for planet positions |
| `gui-core/src/info_panel.rs` | per-frame | very alloc-heavy | F-X05-001 format! density, F-X05-008 routes scan, F-X05-009 chronicle scan, F-X05-010 subsector counts sort |
| `gui-core/src/palette.rs` | per-frame | `draw_route_*` family is tight, but `top_route_control` re-iters | F-X05-014 (low) |
| `gui-core/src/heatmap.rs` | per-frame (cache check) | `Arc<HeatmapCells>` caching is correct | F-X05-012 cache key alloc |
| `builder/src/builder/panels/*.rs` (×42) | per-frame | identical format!-heavy template across all panels | F-X05-001 (theme), F-X05-013 iter().find scans |
| `viewer/src/app/*.rs`, `viewer/src/editor/*.rs` | per-frame | same template as builder panels | F-X05-001 (theme) |
| `src/gen/generation/routes.rs` | per-build | O(N²) candidate enumeration, deterministic | F-X05-007 combined_tags collect + per-modifier format! |
| `src/gen/faction_style.rs` | both | linear scan, called constantly | F-X05-003 |
| `src/analysis/search.rs` | per-build (rare, expensive) | rayon-parallel but no short-circuit | F-X05-002 |
| `src/analysis/influence_field.rs` | per-build | uses BTreeMap for deterministic anchors; flat Vec for cell scores; preallocated | clean |
| `src/analysis/economy.rs` (1700+ LOC) | per-build | not deeply audited here; mostly BTreeMap | possible per-build wins, sampler-flagged but not at HIGH |
| `src/export/bitmap/primitives.rs` | per-pixel | `#[inline] put_pixel`, `fill_row` 1 bounds check/row, `fill_rect` uses `copy_within` | **exemplary; don't touch** |
| `src/export/bitmap/mod.rs` | per-build | `BufWriter<File>` for PNG output, `Vec::with_capacity` for encode bytes | clean |
| `src/export/svg_export/primitives.rs` | per-shape per-build | mostly write! into reused String | F-X05-004 color_hex alloc per attribute |
| `src/export/writers.rs` | per-build | `to_string_pretty` + `fs::write` everywhere | F-X05-005 whole-doc IO |
| `src/export/html_export.rs` | per-build | same pattern + serialised sector inlined into string | F-X05-005 |
| `src/validate/diff.rs` | per-build (CLI one-shot) | BTreeMap indices, writeln! into reused String | F-X05-011 worlds_added Vec join |
| `benches/generation.rs` | bench scaffolding | covers generate + render PNG + validate | **needs coverage**: render_svg, html_export, run_search, analyze, diff |

## Summary of suggested fixes

- F-X05-001 — HIGH — per-frame format!/String pervasiveness across panels + info_panel — L / Low
- F-X05-002 — HIGH — search.rs runs full budget after winner — S / Low
- F-X05-003 — HIGH — faction_style_by_id linear scan, cache lookup index — M / Low
- F-X05-004 — HIGH — SVG color_hex String per attribute, write directly — S / Low
- F-X05-005 — MEDIUM — writers.rs/html_export use BufWriter + to_writer_pretty — S–M / Low
- F-X05-006 — MEDIUM — cache uppercase system/subsector labels in SectorMapCache — S / Low
- F-X05-007 — MEDIUM — routes.rs combined_tags collect + per-modifier format! hoist — M / Low
- F-X05-008 — MEDIUM — routes_block linear scan, add routes_by_system index — S / Low
- F-X05-009 — MEDIUM — chronicle event-by-system/world cache (mirror render.rs) — S / Low
- F-X05-010 — MEDIUM — precompute subsector summary top-N at gen time — S / Low
- F-X05-011 — MEDIUM — diff.rs worlds_added join via write! into buffer — S / Low
- F-X05-012 — LOW — cache key allocation, switch to Arc<str>/hash — S / Low
- F-X05-013 — LOW — BuilderState systems_by_id/etc. derivations — M / Low–Medium
- F-X05-014 — LOW — top_route_control unroll inner loop — S / Low
- F-X05-015 — LOW — bench profile fat LTO to match release — S / Low
- F-X05-016 — NIT — hex_vertices to_vec per draw_hex — S / Low
- F-X05-017 — NIT — pre-size planet_positions Vec — S / None
- F-X05-018 — NIT — sys_cells rebuilt per frame; reuse SectorMapCache — S / Low

### Bench coverage gaps (route to perf engineer with `needs benchmark`)

- `gui-core` paint loop (no Criterion-friendly harness; recommend `puffin`/`tracing-tracy`)
- `analysis::search::run_search` (no bench; required for F-X05-002 validation)
- `svg_export::render_sector_svg` (required for F-X05-004)
- `html_export::render_html`
- `validate::diff::diff_sectors`
- `analytics::analyze`
- Faction-style index hot-path microbench (F-X05-003)
