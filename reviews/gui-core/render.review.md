---
unit_id: U001
crate: sectorforge-gui-core
paths:
  - gui-core/src/sector_view.rs
  - gui-core/src/system_view.rs
  - gui-core/src/heatmap.rs
  - gui-core/src/map_theme.rs
  - gui-core/src/visual_tokens.rs
  - gui-core/src/nav.rs
loc_reviewed: 2392
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 9, low: 8, nit: 6 }
top_risks:
  - "SectorView / MapTheme public structs lack #[non_exhaustive]; adding a field is a workspace-wide breaking change (F-001-001, F-001-002)"
  - "Per-frame heap traffic in the main render loop: per-system uppercase Strings, per-hex Vec, per-subsector HashSets (F-001-005, F-001-006, F-001-007)"
  - "Region-label anchor diverges between cached and uncached paths due to as i32 truncation of fractional axial centroid (F-001-004)"
---

# Review: gui-core rendering primitives (sector_view, system_view, heatmap, map_theme, visual_tokens, nav)

## Summary

The gui-core widgets are functionally complete and structurally readable: `SectorView::show` is a long but mostly linear pipeline (cull, hex pass, route pass, system pass, label pass, click dispatch), and `SystemView` mirrors it on a smaller scale. The two biggest themes are (a) **API ergonomics / forward-compatibility** — every public widget struct and every visual token enum is a `pub` struct literal with no `#[non_exhaustive]`, and `SectorView` carries 19 mandatory positional fields, so any addition ripples into the builder and viewer crates with no migration cushion — and (b) **per-frame allocation hygiene** in the render hot path, which builds several `HashSet`s, `Vec`s, and uppercased `String`s once per visible system / subsector / hex. None of the findings are correctness-fatal: there is no `unsafe` (confirmed), no `unwrap`/`expect` reachable on realistic input, and no `FxMap` iteration leaks into byte-stable output paths. Two correctness items worth attention: the cached `region_centroids` projection truncates fractional axial coordinates with `as i32`, which produces a visibly different label position than the uncached fallback, and the public `HeatmapCells` type alias hard-codes `std::HashMap`, locking external code into SipHash through gui-core's public surface.

## Findings

### F-001-001 — [HIGH] [API design] `SectorView`, `SectorMapCache`, `MapTheme`, `HeatmapCache`, `SectorGeom`, `SystemGeom`, `SystemView`, `HeatCell` are all `pub` with no `#[non_exhaustive]`
- **Location:** `gui-core/src/sector_view.rs:23,92,911`; `gui-core/src/system_view.rs:12,277`; `gui-core/src/map_theme.rs:19,35`; `gui-core/src/heatmap.rs:16,55`.
- **Category:** Idiomatic Rust / API design (§3.7)
- **Confidence:** High
- **Blast radius:** Two downstream crates (builder, viewer) plus tests. Builder constructs `SectorView { ... }` literally at `builder/src/builder/panels/map/interactions.rs:94`; viewer constructs it at `viewer/src/app/sector_view.rs:393`, `viewer/src/app/planner_view.rs:98`, `viewer/src/editor/map_panel.rs:53`.
- **Problem:** gui-core is the foundational shared widget library — its public surface is the contract every other GUI crate compiles against. Adding **any** new field (e.g. an extra theme token, a new optional overlay) is currently a breaking change with no compile-error cushion from `#[non_exhaustive]`, and a literal struct constructor in every caller forces a same-PR sweep of all sites.
- **Why it matters:** Future-proofing the foundational widgets is the single highest-leverage API change for this crate; every panel/tab added downstream pays the tax of struct-literal churn.
- **Evidence:** Read of `pub struct SectorView<'a> { … 19 pub fields … }` (no attribute), and the four caller sites above.
- **Suggested fix:** Annotate every widget input struct and every theme token enum with `#[non_exhaustive]`, then add a `SectorView::new(sector: &GeneratedSector)` constructor returning a builder with `.with_theme(...)`, `.with_cache(...)`, `.with_selection(...)`, etc. Same shape for `SystemView`.
  ```rust
  #[non_exhaustive]
  pub struct SectorView<'a> { /* fields stay pub for read */ }

  impl<'a> SectorView<'a> {
      pub fn new(sector: &'a GeneratedSector) -> Self { /* defaults */ }
      pub fn with_theme(mut self, theme: &'a MapTheme) -> Self { self.theme = Some(theme); self }
      // …
  }
  ```
- **Effort:** M (touches every caller, but they all just need to switch from struct literal to builder calls — mechanical).
- **Risk of fix:** Low. The compiler will identify every site.

### F-001-002 — [HIGH] [API design] Public enums lack `#[non_exhaustive]`; adding a system kind / route mode / region condition silently breaks downstream `match`
- **Location:** `gui-core/src/sector_view.rs:142` (`SectorClick`); `gui-core/src/system_view.rs:21,31,40,50` (`SystemSelection`, `SystemLayout`, `SystemClick`, `SystemPick`); `gui-core/src/visual_tokens.rs:14,67,115` (`MapSystemGlyph`, `MapRouteVisual`, `MapRegionOverlay`).
- **Category:** Idiomatic Rust / API design (§3.7)
- **Confidence:** High
- **Blast radius:** All three visual-token enums map from `sectorforge::sector_model::*` enums in the lib crate. The lib model is the one place a designer adds a new system kind / region condition — once added, every `match` in builder and viewer (and the renderer in this crate) breaks until updated.
- **Problem:** These are growable token enums by design (the renderer's whole job is to map model → glyph), yet they appear with no `#[non_exhaustive]`.
- **Why it matters:** A model PR that adds, say, a new `SystemKind::Megastructure`, compiles cleanly through `visual_tokens.rs` (which uses exhaustive matching of its own input) but silently leaves the renderer/builder/viewer with the wrong glyph until every site is updated. Marking the enums non-exhaustive forces the downstream `match` to use `_ =>`, which is precisely the safety net you want here.
- **Suggested fix:** Add `#[non_exhaustive]` to each enum listed above. Convert internal matches that previously relied on exhaustiveness to use `_ => default_glyph()` or explicit fallbacks. (The two `match`es in `from_system` and `from_route_type` should remain exhaustive on `SystemKind` / `RouteType` from the lib — the change is on the *output* enums.)
- **Effort:** S
- **Risk of fix:** Low

### F-001-003 — [HIGH] [API design / determinism] `HeatmapCells` public alias hard-codes `std::HashMap`; `HeatmapCache::get_or_compute` returns it through gui-core's public surface
- **Location:** `gui-core/src/heatmap.rs:21`, `gui-core/src/heatmap.rs:66`.
- **Category:** API design / determinism (§3.7, project invariant)
- **Confidence:** High
- **Blast radius:** Any caller storing the `Arc<HeatmapCells>` will iterate a `HashMap` if they ever need to enumerate samples — gui-core is publishing that as the canonical type. Today `SectorView` only `.get()`s by key (`sector_view.rs:193-196`), but the public alias invites future iteration in builder code that would silently be non-deterministic.
- **Problem:** The project invariant in `CLAUDE.md` says "Never iterate FxMap/HashMap for output — use BTreeMap or sort keys explicitly". This rule has to apply harder to public *type aliases*, because they propagate the choice of map to every downstream module.
- **Why it matters:** Determinism is a workspace-wide invariant; a public `HashMap` alias is a latent foot-gun even when current call sites are safe.
- **Suggested fix:** Switch the public alias to a deterministic map, or keep it private and hand callers an opaque accessor.
  ```rust
  pub type HeatmapCells = BTreeMap<sectorforge::ids::SystemId, HeatCell>;
  // or — better — keep HashMap internally for O(1) `.get()` but only expose a `.get(&SystemId)` API:
  pub struct HeatmapCells(HashMap<SystemId, HeatCell>);
  impl HeatmapCells { pub fn get(&self, id: &SystemId) -> Option<HeatCell> { … } }
  ```
  Either choice prevents callers from iterating the inner HashMap.
- **Effort:** S
- **Risk of fix:** Low — `compute()`'s body changes one collect-target; the only `.get()` call site keeps working.

### F-001-004 — [MEDIUM] [Correctness] Cached region-label anchor diverges from uncached fallback because `cp.x as i32` truncates fractional axial centroid
- **Location:** `gui-core/src/sector_view.rs:67-71` (storage) and `gui-core/src/sector_view.rs:1295-1303` (consumer).
- **Category:** Correctness / consistency
- **Confidence:** High
- **Blast radius:** Visual only — region labels jump position when the cache is/isn't present, and for centroids whose fractional parts round up (e.g. `(3.7, 4.5)`), labels render at the hex one row/column earlier than the screen-space-average that the fallback computes at `region_label_anchor` (`sector_view.rs:1330-1347`).
- **Problem:** The cache stores the fractional axial-coord centroid in a `Pos2` (`sx/n, sy/n`), but the consumer at line 1299 does `cp.x as i32, cp.y as i32`, which truncates toward zero. The uncached fallback path correctly averages screen-space hex centers, yielding a different (and more accurate) anchor.
- **Why it matters:** Quietly visual-inconsistent between two execution paths that are meant to be equivalent. Whoever toggles the cache for a snapshot test will see labels move, and bug reports will look like "label position drifts after window resize."
- **Suggested fix:** Either (a) project once at cache-build time and store screen-space pixel coords on the cache, or (b) round when projecting from axial. Option (a) is faster, but option (b) is the smaller change:
  ```rust
  let q = cp.x.round() as i32;
  let r = cp.y.round() as i32;
  hex_center(q, r, g) + origin.to_vec2()
  ```
  Even better: store the screen-space anchor directly on `SectorMapCache::region_centroids` and remove the conditional altogether.
- **Effort:** S
- **Risk of fix:** Low

### F-001-005 — [MEDIUM] [Performance / hot path] `sys.name.to_ascii_uppercase()` allocates a `String` for every system every frame (twice when subsector labels are on)
- **Location:** `gui-core/src/sector_view.rs:587` (subsector-label obstacle pass), `gui-core/src/sector_view.rs:770` (system-label pass).
- **Category:** Performance — render hot path
- **Confidence:** High
- **Blast radius:** Steady-state heap churn proportional to `systems × frame_rate`. With 300 systems at 60 fps this is 18 000 small `String` allocations per second.
- **Problem:** The uppercase form is recomputed every frame from immutable `sys.name`. Egui's painter takes a `String` for `layout_no_wrap`, so the allocation can't be avoided at the call site, but it can be hoisted onto `SectorMapCache` (which is already the per-snapshot cache).
- **Why it matters:** Per-frame allocator pressure surfaces as GC-like hitches under load and dominates the frame budget on debug builds.
- **Suggested fix:** Add `pub uppercase_name: Box<str>` (or `Arc<str>`) to a new per-system cache entry on `SectorMapCache`, populated once in `SectorMapCache::new`. Renderer reads `cache.system_meta[&sys.id].uppercase_name.clone()` (Arc clone is `O(1)`); fallback path keeps the per-frame uppercase.
  ```rust
  pub struct SystemRenderMeta { pub uppercase_name: Arc<str> }
  pub system_meta: HashMap<SystemId, SystemRenderMeta>,  // built in `new()`
  ```
- **Effort:** S
- **Risk of fix:** Low (golden tests cover the rendered output — any byte-identical change passes).

### F-001-006 — [MEDIUM] [Performance / hot path] `hex_vertices(...).to_vec()` allocates a fresh 6-element `Vec<Pos2>` for every hex every frame
- **Location:** `gui-core/src/sector_view.rs:1104` (`draw_hex`), `gui-core/src/sector_view.rs:1117` (`draw_hex_fill`).
- **Category:** Performance — render hot path
- **Confidence:** High
- **Blast radius:** One `Vec<Pos2>` per visible hex per frame across the full grid. For a 32×24 map fully visible that's 768 allocations/frame, plus the rect-select pass and the subsector-selected pass.
- **Problem:** `convex_polygon` takes `Vec<Pos2>` by value (egui ≤ 0.29 API), and the helper builds a fresh one each call from the `[Pos2; 6]` array.
- **Why it matters:** Per-frame allocator traffic in the most-called rendering helpers in this crate.
- **Suggested fix:** Switch the hex passes to use `painter.add(egui::Shape::Path(epaint::PathShape::convex_polygon(... )))` with a pooled `Vec<Pos2>` reused across hexes — or, since the geometry is a regular hexagon, drop the polygon entirely and call `painter.add(epaint::CircleShape { center, radius: size, … })` when `size < ~8.0`, falling back to a custom path for larger sizes. Cheapest concrete change: hoist a `scratch: Vec<Pos2>` onto a render-state struct (analogous to `SectorMapCache`) and `clear() + extend_from_slice(&hex_vertices(...))` per hex; pay the allocation once for the highest-zoom workload, never again.
  ```rust
  let scratch = &mut self.scratch_pts;   // field, not local
  scratch.clear();
  scratch.extend_from_slice(&hex_vertices(c, size));
  painter.add(egui::Shape::convex_polygon(std::mem::take(scratch), fill, stroke));
  // Note: convex_polygon consumes the Vec; if egui ever exposes a borrowing
  // API switch to that instead.
  ```
- **Effort:** M (egui's `convex_polygon` consumes its `Vec`, so reuse needs `mem::take` ping-pong — verify a perf win with criterion before merging).
- **Risk of fix:** Low — golden tests catch any pixel-level drift.

### F-001-007 — [MEDIUM] [Performance / hot path] Subsector-label loop rebuilds `sys_cells` and per-subsector `cells` `HashSet`s every frame
- **Location:** `gui-core/src/sector_view.rs:600-605` (`sys_cells`), `gui-core/src/sector_view.rs:612-616` (per-subsector `cells`), and the obstacle `Vec` at `gui-core/src/sector_view.rs:578-595`.
- **Category:** Performance — render hot path
- **Confidence:** High
- **Blast radius:** Per frame, `O(systems)` for the obstacle `Vec`, `O(systems)` again for `sys_cells`, `O(hex_cells)` per subsector. Across all sectors with subsectors enabled (the common case) this is a steady stream of HashMap allocations.
- **Problem:** `SectorMapCache` already memoises the cell→subsector map; the same data should drive these label-placement caches.
- **Why it matters:** Same as F-001-005 — allocator churn on a path that runs every frame.
- **Suggested fix:** Move `sys_cells` and the per-subsector `cells` into `SectorMapCache` (they're stable across frames as long as `subsectors`/`sector.systems` don't change). Same for the static obstacle bboxes for system-marker hex rects, since hex centers don't move within a frame.
  ```rust
  pub sys_cells_set: FxHashSet<(i32, i32)>,                                // built once
  pub subsector_cells: BTreeMap<String, FxHashSet<(i32, i32)>>,            // built once
  ```
  (Use `Fx*` aliases since these are read-only lookup, never iterated for output.)
- **Effort:** S
- **Risk of fix:** Low

### F-001-008 — [MEDIUM] [Performance / hot path] `centers` `HashMap<&str, Pos2>` built every frame, used only for `.get()`
- **Location:** `gui-core/src/sector_view.rs:318-328`.
- **Category:** Performance — render hot path
- **Confidence:** Medium
- **Blast radius:** One HashMap per frame sized `systems.len()`.
- **Problem:** `centers` is built fresh every frame purely to map system id → screen pos. The same data is already trivially reproducible from `sector.systems` since `hex_center(sys.coord.q, sys.coord.r) + origin.to_vec2()` is cheap; the only reason to cache it is `drag_override`.
- **Why it matters:** Same allocator concern as F-001-005/007. Note this also indexes by `&str`, so the HashMap stores borrowed keys with a `'a`-equivalent lifetime — fine, but it's another SipHash table on the hot path.
- **Suggested fix:** Drop the HashMap; replace `centers.get(sys.id.as_str())` with an `O(systems)` walk plus a special-case for `drag_override`, or — better — precompute `Vec<Pos2>` parallel to `sector.systems` so lookup is `O(1)` by index. Renderer is already iterating `sector.systems` in order, so the parallel `Vec` is the natural choice.
  ```rust
  let mut centers: Vec<Pos2> = Vec::with_capacity(self.sector.systems.len());
  for sys in &self.sector.systems {
      let mut c = hex_center(sys.coord.q, sys.coord.r, &g) + origin.to_vec2();
      if let Some((drag_id, drag_pos)) = self.drag_override.as_ref() {
          if drag_id.as_str() == sys.id.as_str() { c = *drag_pos; }
      }
      centers.push(c);
  }
  // Route loop: need a name → index map, but it can be a Fx lookup constructed
  // once outside the route loop (still cheaper than HashMap<&str, Pos2>).
  ```
- **Effort:** M (route lookup needs a small rework).
- **Risk of fix:** Low

### F-001-009 — [MEDIUM] [API design / type safety] `selected_system: Option<&'a str>` and `selected_route: Option<&'a str>` lose the newtype safety of `SystemId`/`RouteId`
- **Location:** `gui-core/src/sector_view.rs:94-95`.
- **Category:** Idiomatic Rust / API design (§3.7)
- **Confidence:** High
- **Blast radius:** Cross-crate. Callers in `builder/` and `viewer/` already have `SystemId`/`RouteId` values; converting to `&str` and back is friction that subverts the newtype.
- **Problem:** Every other id field on this struct is correctly typed (`path_route_ids: &HashSet<RouteId>`, `multi_selected: &BTreeSet<SystemId>`, `pinned: &BTreeSet<SystemId>`, `drag_override: (SystemId, Pos2)`), but the two scalar selected ids are `&str`. The renderer immediately calls `.as_str()` on every `SystemId` for comparison (`sector_view.rs:468`) and bypasses any compile-time check that the str is actually a system id.
- **Why it matters:** Newtypes are a workspace-wide invariant for ids — see `sectorforge::ids::SystemId`/`RouteId`. Bypassing them in the foundational widget API is a smell.
- **Suggested fix:** Change to `Option<&'a SystemId>` / `Option<&'a RouteId>`. Compare with `sys.id == sel` instead of `sys.id.as_str() == Some(sel)`. The borrow stays the same lifetime, so no caller change beyond removing `.as_str()`.
- **Effort:** S
- **Risk of fix:** Low — compile errors at all call sites are mechanical.

### F-001-010 — [MEDIUM] [Performance / hot path] `selected_route` linear scan of `sector.routes` instead of a direct find
- **Location:** `gui-core/src/sector_view.rs:421-458`.
- **Category:** Performance / clarity
- **Confidence:** High
- **Blast radius:** O(routes) per frame even though only one route can be selected.
- **Problem:** `for route in &self.sector.routes { if route.id.as_str() != sel { continue; } … }` iterates every route to find the one matching `sel`. Use `find`. Also the `glow` `Color32` is rebuilt inside the body (only hits once, but the body sits inside the loop visually as if it were per-route).
- **Why it matters:** Tiny in isolation, but combined with similar patterns (see F-001-006/007/008) the per-frame budget adds up.
- **Suggested fix:**
  ```rust
  if let Some(sel) = self.selected_route {
      if let Some(route) = self.sector.routes.iter().find(|r| r.id.as_str() == sel) {
          // hoist `glow` out of the loop-body; paint once.
      }
  }
  ```
  (Combine with F-001-009: change `sel: &SystemId/&RouteId` and the comparison drops `.as_str()`.)
- **Effort:** S
- **Risk of fix:** Low

### F-001-011 — [MEDIUM] [Performance / per-click] `SectorGeom::pick_hex` is `O(W × H)` even when `pick_hex` could analytically invert the axial transform
- **Location:** `gui-core/src/sector_view.rs:952-964` (`pick_hex`), `gui-core/src/sector_view.rs:840-895` (the per-click fallback that mirrors the same O(W·H) scan).
- **Category:** Performance — per-click
- **Confidence:** High
- **Blast radius:** Per-click only (`pick_hex` is not called per frame from `show`). For a 32×24 grid that's ~768 iterations per click — acceptable. Larger sectors (200×100) would feel laggy under rapid clicks.
- **Problem:** Pointy-top axial coords have a closed-form inverse from screen coords (`q = (sqrt(3)/3 * x - 1/3 * y) / size`, then round to the nearest valid axial coord). Brute-forcing every cell is unnecessary.
- **Why it matters:** Future-proofing against larger sectors; cleaner code.
- **Suggested fix:** Replace the loops with the closed-form `pixel_to_axial` + axial-rounding helper. Standard formula (offset-r adjustment included since the grid is row-offset, not pure axial):
  ```rust
  pub fn pick_hex(&self, p: Pos2, width: u32, height: u32) -> Option<HexCoord> {
      let local = p - self.origin.to_vec2() - Vec2::new(self.margin, self.margin + self.hex_size);
      let r = (local.y / (self.hex_size * 1.5)).round() as i32;
      let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
      let q = (local.x / (self.hex_size * 3f32.sqrt()) - row_shift - 0.5).round() as i32;
      let coord = HexCoord { q, r };
      let centre = self.hex_center(q, r);
      ((centre - p).length() <= self.hex_size * 0.95
          && (0..width as i32).contains(&q)
          && (0..height as i32).contains(&r)).then_some(coord)
  }
  ```
  Same idea for the in-`show` fallback at lines 851-895.
- **Effort:** M (axial rounding for offset-r is finicky — add a unit test).
- **Risk of fix:** Medium — geometric inversion bugs are easy to introduce; gate behind a feature flag and run the existing pick tests.

### F-001-012 — [MEDIUM] [Performance / startup] `SectorMapCache::new` uses `std::HashMap` for build-time tables; SipHash isn't free
- **Location:** `gui-core/src/sector_view.rs:32,39,44,45,67`.
- **Category:** Performance — once-per-cache (not per-frame)
- **Confidence:** Medium
- **Blast radius:** Run once per cache invalidation. With 300 systems and ~700 hex cells across regions, this is single-digit milliseconds; not a frame issue, but cheap to fix and consistent with workspace style.
- **Problem:** These are pure internal-lookup tables — exactly the case `FxHashMap` exists for in this workspace.
- **Why it matters:** Cheap consistency win; lines up with the rest of the lib crate's internal-lookup conventions.
- **Suggested fix:** Switch internal `HashMap` to `sectorforge::FxMap` for the four lookup tables on `SectorMapCache`. Determinism is fine because none of these are iterated for output (selected-subsector wash and region centroid iteration are commutative paint or already feed into deterministic per-region anchors).
- **Effort:** S
- **Risk of fix:** Low

### F-001-013 — [LOW] [Idiom] `pip.to_string()` allocates inside the per-system loop; the same digit text recurs every frame
- **Location:** `gui-core/src/sector_view.rs:554`.
- **Category:** Performance / idiom
- **Confidence:** Medium
- **Blast radius:** One small String per system per frame (pip count is always small).
- **Problem:** `pip.to_string()` formats `usize` into a heap-allocated `String`. For small counts the SSO doesn't help (egui's `text()` takes `String`).
- **Suggested fix:** Either keep as-is (the win is marginal compared to F-001-005/006), or batch: pre-render an Arc<str> for "1".."20" once on `SectorMapCache` and look up.
- **Effort:** S
- **Risk of fix:** Low

### F-001-014 — [LOW] [Idiom] `planet_positions` `Vec<(usize, Pos2, f32)>` is built only to be re-iterated for click resolution in the same `show()` call
- **Location:** `gui-core/src/system_view.rs:216-263`.
- **Category:** Performance / idiom
- **Confidence:** High
- **Blast radius:** Per-frame, per-system-view. Allocates a `Vec<(usize, Pos2, f32)>` sized to `worlds.len()`.
- **Problem:** The Vec is populated during the planet-paint loop and consumed only by the click handler at lines 252-256. Click happens at most once per frame; the Vec is built unconditionally. Either inline the click-resolution into the paint loop, or recompute world anchors in the click branch (the cost is `O(worlds)` * `O(1)` math — cheap).
- **Suggested fix:** Drop the `Vec`; inside the `if response.clicked()` branch, walk `self.system.worlds` and recompute anchors with `world_anchor`:
  ```rust
  let planet_hit = self.system.worlds.iter()
      .map(|w| {
          let p = world_anchor(&g, star, self.layout, i32::from(w.orbit.max(1)), w.index);
          (w.index, (p - pos).length())
      })
      .filter(|(_, d)| *d <= g.planet_r * 1.25)
      .min_by(|a, b| a.1.total_cmp(&b.1));
  ```
- **Effort:** S
- **Risk of fix:** Low — geometry math is shared with `world_anchor`, so the two paths can't drift.

### F-001-015 — [LOW] [Idiom] `SystemGeom::new` redundant `.max(1).max(1)`
- **Location:** `gui-core/src/system_view.rs:288`.
- **Category:** Idiom (§3.7)
- **Confidence:** High
- **Suggested fix:** Drop one `.max(1)`. `sys.worlds.iter().map(|w| w.orbit).max().unwrap_or(1)` already returns `≥ 1`.
- **Effort:** S
- **Risk of fix:** None

### F-001-016 — [LOW] [Idiom] `ScaledSize` constructors and getters could be `#[inline]` for cross-crate use
- **Location:** `gui-core/src/map_theme.rs:24-32`.
- **Category:** Performance / idiom (§3.7)
- **Confidence:** Medium
- **Problem:** `ScaledSize::new` and `ScaledSize::px` are tiny cross-crate helpers called many times per frame inside `SectorView::show`. Without `#[inline]` LLVM may decline to inline them across crate boundaries.
- **Suggested fix:** Add `#[inline]` to both. Also add `#[inline]` to `MapTheme::region_color`.
- **Effort:** S
- **Risk of fix:** None

### F-001-017 — [LOW] [Idiom] `SectorMapCache::new` rebuilds `region_hex_counts` only to immediately drop it into `region_centroids`
- **Location:** `gui-core/src/sector_view.rs:45,63,67-71`.
- **Category:** Idiom
- **Confidence:** High
- **Problem:** Two HashMaps where one would do. The `(sx, sy, n)` tuple is never used outside the second loop; merge them.
- **Suggested fix:**
  ```rust
  let mut region_centroids = HashMap::with_capacity(sector.regions.len());
  for reg in sector.regions.iter() {
      let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
      for h in &reg.hexes { /* … */ sx += h.q as f32; sy += h.r as f32; }
      if !reg.hexes.is_empty() {
          let n = reg.hexes.len() as f32;
          region_centroids.insert(reg.id.to_string(), Pos2::new(sx / n, sy / n));
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** None

### F-001-018 — [LOW] [Idiom] `region_label_text` allocates `String` per region per frame; cache on `SectorMapCache`
- **Location:** `gui-core/src/sector_view.rs:1349-1358` (also called from `draw_region_labels` line 1313).
- **Category:** Performance / idiom
- **Suggested fix:** Cache `Arc<str>` per region id on `SectorMapCache`, like F-001-005. Same pattern.
- **Effort:** S
- **Risk of fix:** Low

### F-001-019 — [LOW] [API] `entity_link` allocates `format!("→ {}", widget.text())` and re-reads the original text via `WidgetText::text()` which returns `Cow<str>` (allocates if rich text)
- **Location:** `gui-core/src/nav.rs:10-22`.
- **Category:** API / Performance
- **Confidence:** Medium
- **Problem:** `widget.text()` may allocate for rich widget text; the `format!` then allocates again. For a button that fires once per click this is fine, but it's reachable per frame for every visible link.
- **Suggested fix:** Take `label: impl Into<String>` and prefix on the input directly, or take `&str` and let the caller handle rich text:
  ```rust
  pub fn entity_link(ui: &mut egui::Ui, label: &str, with_arrow: bool) -> egui::Response {
      if with_arrow {
          ui.link(format!("→ {label}"))
      } else {
          ui.link(label)
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — but callers using `RichText` would need a separate overload.

### F-001-020 — [LOW] [Doc] No `//!` module-level doc on `nav.rs` past the §LINK4 line; missing `# Panics`/`# Errors` on most `pub` items
- **Location:** `gui-core/src/nav.rs` (no expansion), `gui-core/src/sector_view.rs` (most `pub fn` lack rustdoc), `gui-core/src/map_theme.rs` (no docs on `ScaledSize::new`, `ScaledSize::px`, `MapTheme::region_color`), `gui-core/src/heatmap.rs` (`HeatmapCache::get_or_compute` and `::invalidate` undocumented).
- **Category:** Documentation (§3.11)
- **Suggested fix:** Add one-sentence doc comments on each public method. Special call-out for `SectorView`'s 19 fields — every one is exposed without rustdoc except via inline comments embedded in the struct body (which IDEs don't surface).
- **Effort:** M
- **Risk of fix:** None

### F-001-021 — [NIT] [Style] `hex_center` is a private wrapper over `hex_center_xy` with no behavioural difference
- **Location:** `gui-core/src/sector_view.rs:1060-1071`.
- **Suggested fix:** Delete `hex_center`, rename `hex_center_xy` to `hex_center`.
- **Effort:** S
- **Risk of fix:** None

### F-001-022 — [NIT] [Style] `hex_pick` is also a wrapper that constructs a fresh `SectorGeom` to delegate to `SectorGeom::pick_hex`
- **Location:** `gui-core/src/sector_view.rs:1073-1088`.
- **Suggested fix:** Inline the construction at the (single) call site `sector_view.rs:781`, then delete `hex_pick`.
- **Effort:** S
- **Risk of fix:** None

### F-001-023 — [NIT] [Style] `selected_route` glow `Color32` rebuilt inside the loop body
- **Location:** `gui-core/src/sector_view.rs:438-443`.
- **Suggested fix:** Hoist above the loop (combined with F-001-010).
- **Effort:** S
- **Risk of fix:** None

### F-001-024 — [NIT] [Style] `cargo clippy -W clippy::cast_possible_truncation` flags ~18 `f32 as i32`/`f32 as u8` sites
- **Location:** `gui-core/src/sector_view.rs:175,176,179,180` (viewport scan); `:1273` (heat blend); `:1299` (region centroid); `:720` (fallback anchor); etc.
- **Category:** §3.7 (silent `as` truncation)
- **Suggested fix:** Where the input range is bounded (alpha blending in `blend_heat`, the `0..=255` rounding case is provably in-range), keep as-is and add a comment. Where the input is genuinely unbounded (`viewport scan as i32` from rect bounds), the `.max(0)`/`.min(...)` clamp already saves us; an explanatory comment beats a `try_from` here. F-001-004 is the one truncation that is a real bug.
- **Effort:** S
- **Risk of fix:** None

### F-001-025 — [NIT] [Style] `Vec::new()` then `.push()` in `system_view::show` without `with_capacity`
- **Location:** `gui-core/src/system_view.rs:216`.
- **Suggested fix:** `Vec::with_capacity(self.system.worlds.len())`. (Folds into F-001-014 if that's adopted.)
- **Effort:** S
- **Risk of fix:** None

### F-001-026 — [NIT] [Style] `painter.layout_no_wrap("SUBSECTOR".to_string(), …)` allocates a fresh `String` per subsector per frame
- **Location:** `gui-core/src/sector_view.rs:624`.
- **Suggested fix:** This is forced by egui's API (`layout_no_wrap(text: String, …)`), but pre-laying-out the galley once and reusing it across subsectors would side-step it. Folded into the broader hot-loop refactor in F-001-007.
- **Effort:** S
- **Risk of fix:** None

## Per-category coverage

### 3.1 Panics & failure surface
- `gui-core/src/sector_view.rs:464` `centers[sys.id.as_str()]` — safe because `centers` is populated from `self.sector.systems` at lines 320-328 right above.
- `gui-core/src/sector_view.rs:719` `.expect("non-empty")` — gated by `if s.hex_cells.is_empty() { continue; }` at line 608. Currently unreachable, but a future refactor removing the guard would panic. Consider replacing with `if let Some(...) = … .min_by(...)`.
- `system_view.rs:288` `.unwrap_or(1).max(1)` — defensive, fine.
- No `panic!`, `unreachable!`, `todo!`, or `unimplemented!` in library code. No out-of-bounds slicing. No reachable arithmetic overflow (all `as` casts on bounded inputs or saturating).

### 3.2 unsafe & soundness
**No findings.** Confirmed via `grep -nR "unsafe" gui-core/src/{sector_view,system_view,heatmap,map_theme,visual_tokens,nav}.rs` — zero occurrences, as expected for this workspace.

### 3.3 Ownership, borrowing, lifetimes, cloning
- `sector_view.rs:35,54,63,70` `reg.id.to_string()`, `s.id.as_ref().to_string()` — allocations are once-per-cache-build, acceptable.
- `sector_view.rs:587,622,770` `to_ascii_uppercase` per frame — see F-001-005.
- `sector_view.rs:976,1007,1010,1016` `s.id.clone()`, `route.id.clone()` inside the hit-test closures — `RouteId`/`SystemId` are `Arc<str>`-backed (cheap clone), so this is fine.
- `system_view.rs:236` `short_upper(&w.name, 14)` allocates a `String` per planet per frame — minor, but mirrors F-001-005.
- `entity_link` (`nav.rs:10`) — see F-001-019.

### 3.4 Error handling
**No findings.** This is a pure rendering crate; no `Result` returns, no errors propagated. The error model is "draw what you can and skip the rest" (e.g. `centers.get(...).is_none()` paths skip drawing the route silently), which is appropriate for a GUI widget.

### 3.5 Concurrency & async
**No findings.** No threading, no async, no shared-mutable state. `HeatmapCache` is owned by the app and accessed single-threaded.

### 3.6 Performance
See F-001-005 through F-001-013, F-001-016. The bulk of the substance in this review is here: the render hot path has multiple per-frame heap allocations that can be hoisted to `SectorMapCache`.

### 3.7 Idiomatic Rust & API design
See F-001-001, F-001-002, F-001-003, F-001-009, F-001-019. Public-surface forward-compatibility is the highest-priority theme for gui-core.

### 3.8 Dependencies & Cargo hygiene
**No findings at unit level.** Imports are tight; no unused-import warnings. `gui-core/src/sector_view.rs:3` imports `std::collections::{BTreeSet, HashMap, HashSet}` — all three are used. `gui-core/src/heatmap.rs:6` `Arc` is used. No over-broad feature flags visible in this unit (Cargo.toml itself is X06's territory).

### 3.9 Memory & resource management
- `HeatmapCache::cells: Option<Arc<HeatmapCells>>` — `Arc` is correctly used to share with the renderer borrow; no cycle risk because `HeatCell` holds no `Arc`.
- `HeatmapCache::invalidate` clears `key` and `cells` but not `mode` — minor (the next `get_or_compute` resets it anyway), but inconsistent. NIT.
- No growing caches without bound; no `static mut`.

### 3.10 Testing & verification
- `sector_view.rs` has tests for `point_segment_distance`, three for `hit_route` (basic, miss, tie-break), and one for `SectorMapCache::region_for_hex`. No tests for `pick_hex`, `hit_system`, `SectorMapCache::new` cache contents, or the per-frame `show()` behaviour (the latter is hard without an egui context but `SectorGeom` accessors are unit-testable).
- `system_view.rs` has seven `pick_world` tests covering star/world/empty-orbit/background for both layouts. Good coverage.
- `heatmap.rs` has two cache tests (reuse and Off-mode). Adequate.
- `visual_tokens.rs` covers every variant for both routes and regions. Adequate.
- `map_theme.rs` and `nav.rs` have **no tests at all**.
- No `#[ignore]`s; no sleep-based tests.
- **Recommended:** add a `pick_hex` round-trip property test (`hex_center(q,r) → pick_hex → Some((q,r))`) using `proptest` — that would catch F-001-011's geometric inversion bugs cheaply.

### 3.11 Documentation & maintainability
See F-001-020. Module-level `//!` exists on every file (good), but per-item `///` coverage is thin on `sector_view.rs` (most `pub fn`/`pub struct` fields are bare). Magic numbers appear inline (`* 0.55`, `* 1.25`, `* 1.5`, `+ 3.0`) — some have inline comments, most don't. Recommend extracting layout offsets to named constants on `MapTheme` (which already does this for the big ones).

## Summary of suggested fixes

| ID | Severity | Short | Effort/Risk |
|---|---|---|---|
| F-001-001 | HIGH | Add `#[non_exhaustive]` + builder to `SectorView`/`SystemView`/`MapTheme`/etc. | M / Low |
| F-001-002 | HIGH | Add `#[non_exhaustive]` to public token enums | S / Low |
| F-001-003 | HIGH | Stop publishing `HashMap` as `HeatmapCells` public alias | S / Low |
| F-001-004 | MEDIUM | Fix `as i32` truncation of cached region centroid (round, or project once) | S / Low |
| F-001-005 | MEDIUM | Cache uppercased system names on `SectorMapCache` | S / Low |
| F-001-006 | MEDIUM | Pool the per-hex `Vec<Pos2>` in `draw_hex`/`draw_hex_fill` | M / Low |
| F-001-007 | MEDIUM | Cache `sys_cells`/per-subsector `cells` HashSets on `SectorMapCache` | S / Low |
| F-001-008 | MEDIUM | Replace per-frame `HashMap<&str, Pos2> centers` with parallel `Vec<Pos2>` | M / Low |
| F-001-009 | MEDIUM | `selected_system/route` should be `Option<&SystemId>`/`Option<&RouteId>` | S / Low |
| F-001-010 | MEDIUM | Replace `selected_route` linear scan with `iter().find` | S / Low |
| F-001-011 | MEDIUM | Replace `pick_hex` O(W·H) scan with closed-form axial inverse | M / Medium |
| F-001-012 | MEDIUM | Switch `SectorMapCache` internal HashMaps to `FxHashMap` | S / Low |
| F-001-013 | LOW | Pre-render `pip.to_string()` digits on cache | S / Low |
| F-001-014 | LOW | Inline `planet_positions` Vec into click branch in `SystemView::show` | S / Low |
| F-001-015 | LOW | Drop redundant `.max(1).max(1)` in `SystemGeom::new` | S / None |
| F-001-016 | LOW | `#[inline]` on `ScaledSize::{new,px}` and `MapTheme::region_color` | S / None |
| F-001-017 | LOW | Merge `region_hex_counts`/`region_centroids` into a single pass | S / None |
| F-001-018 | LOW | Cache `region_label_text` on `SectorMapCache` | S / Low |
| F-001-019 | LOW | Make `entity_link` borrow `&str` instead of `impl Into<WidgetText>` | S / Low |
| F-001-020 | LOW | Add rustdoc to public items (especially `SectorView` fields) | M / None |
| F-001-021 | NIT | Delete `hex_center` wrapper, rename `hex_center_xy` | S / None |
| F-001-022 | NIT | Inline `hex_pick` (only one caller) | S / None |
| F-001-023 | NIT | Hoist `selected_route` glow Color32 out of the loop body | S / None |
| F-001-024 | NIT | Audit/comment 18 `f32 as i32`/`f32 as u8` clippy sites | S / None |
| F-001-025 | NIT | `Vec::with_capacity` for `planet_positions` | S / None |
| F-001-026 | NIT | Reuse `"SUBSECTOR"` galley across subsectors | S / None |
