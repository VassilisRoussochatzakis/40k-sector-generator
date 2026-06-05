# AREA F — viewer + gui-core — verification

Verified 2026-06-05 against live `main` branch. Scope: `viewer/src/` and `gui-core/src/`. Main god-files: `gui-core/src/sector_view.rs` (1711 LOC), `gui-core/src/info_panel.rs` (1156 LOC), `viewer/src/app/sector_view.rs` (677 LOC).

---

## Summary table

| ID   | Sev  | Status             | Effort | One-line |
|------|------|--------------------|--------|----------|
| F-S1 | HIGH | ✅ Confirmed       | L      | Two parallel editing stacks with distinct dirty flags, save paths, and empty_* constructors |
| F-S2 | MED  | ⚠️ Partial         | M      | 30 hardcoded `Color32::from_rgb` across viewer — ~20 are semantic amber/red; ~10 are intentional data-viz or background fills |
| F-S3 | MED  | ✅ Confirmed       | M      | `SectorView` has 27 `pub` fields, no `Default` impl |
| F1   | HIGH | ✅ Confirmed       | L      | `App` live-edit stack vs `editor::` module are genuinely parallel |
| F2   | HIGH | ✅ Confirmed       | M      | `enum_combo` in `data_editor.rs:287` vs `worlds_editor.rs:363` — structurally identical, minor signature differences |
| F3   | MED  | ✅ Confirmed       | M      | 27 fields at `sector_view.rs:136–184`, 1711 LOC confirmed, no `Default` |
| F4   | MED  | ✅ Confirmed       | S      | `cache: None` at `planner_view.rs:100` and `editor/map_panel.rs:63` confirmed |
| F5   | MED  | 🔄 Moved (count)   | M      | Semantic color sites confirmed; real count is 30 total `Color32::from_rgb`, ~20 semantic |
| F6   | MED  | ✅ Confirmed       | S      | `dialogs.rs:195` — raw `Color32::from_rgb(235, 90, 90)` for SaveAs error |
| F7   | MED  | ✅ Confirmed       | M      | Add-route + distance recompute duplicated in `editor/map_panel.rs:213–230` vs `app/sector_view.rs:561–593` |
| F8   | LOW  | ✅ Confirmed       | M      | `info_panel.rs` is 1156 LOC with formatting fused into render fns |
| F9   | LOW  | ✅ Confirmed       | S      | `editor/factions_panel.rs:308` — local `palette_dim()` returns `TEXT_DIM` constant already available as `palette::chrome_text_dim()` |
| F10  | LOW  | ✅ Confirmed       | M      | `centers` HashMap rebuilt per-frame at `sector_view.rs:382`; `paint_star_dust` also fires every frame |
| F11  | LOW  | ✅ Confirmed       | S      | `factions_overview.rs:399` — `(235, 200, 90)` amber used for both save-ok and save-failed messages |
| F12  | LOW  | ✅ Confirmed       | S      | `stability_color` at `palette.rs:771` overlaps `StatusColors` green/amber/red — intentional split (data-viz vs chrome status) |

---

## Findings

### F-S1 — Two parallel sector-editing stacks
- **Review sev / bucket:** HIGH / P1 #6
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/app/mod.rs:43–88` (App struct) + `viewer/src/editor/state.rs:101–133` (EditorState) + `viewer/src/app/lifecycle.rs:109–235` (App save paths) + `viewer/src/editor/file_ops.rs:74–95` (editor save path)
- **Evidence:**
  ```rust
  // App (lifecycle.rs:190–195): Arc::make_mut + live_dirty flag
  pub(super) fn write_sector_to_path(&mut self, path: PathBuf) {
      let sector = Arc::make_mut(sector);
      …
      self.live_dirty = false;

  // EditorState (file_ops.rs:74): flat fs::write + editor.dirty flag
  pub(crate) fn save_project_sector(name: &str, sector: &GeneratedSector)
  ```
- **Why it matters:** Two dirty flags (`App.live_dirty` + `EditorState.dirty`), two save entry points (`write_sector_to_path` vs `save_project_sector`), two `empty_*` constructor call-sites, two drag-move/add-route implementations. Changes to one path silently fail to propagate to the other — confirmed by the sync bridge in `app/mod.rs:193–230` that copies `editor.sector → self.sector` every frame when `editor.dirty` is set, and by `lifecycle.rs:224–229` re-calling `editor.set_sector(…)` after a successful save-to-path to keep the two in sync. The bridge is load-bearing but fragile.
- **Fix:** Promote `EditorState` as the single source of truth. Remove the duplicated `App.sector: Option<Arc<GeneratedSector>>` write path; route all saves through `EditorState` + one write fn. Extract the `empty_sector/empty_system/empty_world/empty_route` constructors into `sectorforge` (they are pure domain logic, not UI).
- **Effort:** L
- **Risk / deps:** High cascade — `app/mod.rs`, `app/lifecycle.rs`, `app/sector_view.rs`, `editor/dialogs.rs`, `editor/map_panel.rs` all read from one or both stacks. Requires sequential change across the whole App surface. No golden exposure (these are runtime write paths, not render paths).

---

### F-S2 — ~20 hardcoded semantic colors in viewer (real count: 30 total)
- **Review sev / bucket:** MED / P3
- **Status:** ⚠️ Partial (count updated)
- **Location:** Spread across `viewer/src/` — 30 `Color32::from_rgb` calls total
- **Evidence:**
  ```rust
  // layout.rs:212, sector_view.rs:324/329/334, system_view.rs:146,
  // data_editor.rs:114, dashboard.rs:71/135/174/175,
  // planner_view.rs:257/312/313, trade_view.rs:87/163 … (semantic subset)
  // segmentum_view.rs:281/346/454/504 (intentional data-viz/background fills)
  ```
- **Why it matters:** The SPRUCE D7 defect — hardcoded RGB bypasses `StatusColors` theme-awareness. Under the light theme, amber `(235, 200, 90)` and red `(235, 90, 90)` may clash with the parchment accent or fail contrast. The ~20 *semantic* sites (error, warning, unsaved) are the real risk; the ~10 remaining are intentional: dark background fills for banners (`(0, 80, 0)`, `(0, 100, 0)`), segmentum chrome fills, and hue data-viz (region kind label `(220, 160, 60)` — intentional, maps to a specific lore condition rather than a status).
- **Fix:** Replace the ~20 semantic amber/red sites with `palette::warning()` / `palette::danger()` / `palette::success()`. Audit and explicitly comment any remaining hardcoded RGBs as intentional data-viz. See per-finding details for F5, F6, F9, F11.
- **Effort:** M
- **Risk / deps:** No map-snapshot exposure (viewer chrome panels, not `sector_view.rs` render). Low — pure cosmetic substitution.

---

### F-S3 — `SectorView` 27-field god-widget, no `Default`/builder
- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `gui-core/src/sector_view.rs:136–184` (verified)
- **Evidence:**
  ```rust
  pub struct SectorView<'a> {
      pub sector: &'a GeneratedSector,
      pub selected_system: Option<&'a str>,
      pub selected_route: Option<&'a str>,
      pub hex_size: f32,
      pub path_route_ids: Option<…>,
      pub path_waypoints: Option<…>,
      pub subsectors: Option<…>,
      pub cache: Option<&'a SectorMapCache>,
      // … 19 more fields …
      pub show_hover_coord: bool,
  }
  ```
- **Why it matters:** Every call site must supply all 27 fields — adding one field breaks all callers. Currently there are at least 4 call sites (viewer `sector_view.rs`, planner_view.rs, editor `map_panel.rs`, builder map). A `Default` impl with sensible sentinel values (`None`, `false`, `Sense::hover()`) would let new fields be added without cascading edits.
- **Fix:** Add `impl Default for SectorView<'_>` with `sector` left as a required field (or use a builder pattern). All optional fields default to `None`/`false`. The struct literal `SectorView { sector, hex_size, …, ..SectorView::default_with(sector) }` idiom works.
- **Effort:** M
- **Risk / deps:** MAP-SNAPSHOT sensitive — any change to `sector_view.rs` may alter snapshot goldens; regenerate with `UPDATE_MAP_SNAPSHOTS=1`. The `Default` itself is additive, but test the snapshot suite.

---

### F1 — Second editing+save stack: `app/editor_views.rs` + `editor/map_panel.rs`
- **Review sev / bucket:** HIGH / P1 #6
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/app/editor_views.rs:30` (dispatches to editor module) + `viewer/src/editor/map_panel.rs:155–185` (drag-move) + `viewer/src/app/lifecycle.rs:190` (`write_sector_to_path`) + `viewer/src/editor/file_ops.rs:74` (`save_project_sector`)
- **Evidence:**
  ```rust
  // editor_views.rs:30 — tab dispatch that routes into the editor:: module
  let sel = self.editor.selection.clone();
  match self.editor.tab { … }

  // editor/file_ops.rs:74 — editor's own save path (examples/<name>/out/)
  pub(crate) fn save_project_sector(name: &str, …) -> Result<String, …>
  ```
- **Why it matters:** The review correctly identifies this as the dominant viewer hazard. The editor module has its own `EditorState.dirty` flag and `Dialog::SaveAs` path (`save_project_sector` → `examples/<name>/out/sector.json`). The `App` has `live_dirty` + `write_sector_to_path` (arbitrary path via `rfd::FileDialog`). They are bridged every frame by a sync block in `app/mod.rs:193–230`, which copies `editor.sector` → `Arc<App.sector>` when `editor.dirty` is set, and `lifecycle.rs:224` re-syncs the editor state after a successful `write_sector_to_path`. The bridge works but is fragile — any path that writes one side without updating the other will silently diverge.
- **Fix:** Same as F-S1 — unify on one source of truth, one dirty flag, one save fn. The editor module's `save_project_sector` should become the only write path, extended to accept arbitrary paths (dropping the `examples/<name>` convention assumption).
- **Effort:** L
- **Risk / deps:** Ordering: fix F1 before F7 (drag-move dedup depends on the unified data model). No map-snapshot exposure.

---

### F2 — `enum_combo` duplicated: `data_editor.rs:287` vs `worlds_editor.rs:363`
- **Review sev / bucket:** HIGH / P1 #6
- **Status:** ✅ Confirmed (line numbers drifted from review)
- **Location:** `viewer/src/data_editor.rs:287` (verified, review cited :141 — that is where the grid call site is) + `builder/src/builder/panels/worlds_editor.rs:363`
- **Evidence:**
  ```rust
  // viewer data_editor.rs:287 — F: Fn(&T) -> String (allocating)
  fn enum_combo<T, F>(ui, id, value: &mut Option<T>, variants, label_of: F) -> bool
  where T: Clone + PartialEq, F: Fn(&T) -> String

  // builder worlds_editor.rs:363 — F: Fn(&T) -> &'static str (zero-alloc)
  fn enum_combo<T, F>(ui, id, value: &mut Option<T>, variants, label_of: F) -> bool
  where T: Clone + PartialEq + std::fmt::Debug, F: Fn(&T) -> &'static str
  ```
- **Why it matters:** The logic is identical (selectable_label for None + variants); differences are cosmetic: the builder version adds `.on_hover_text(format!("key: {v:?}"))` and uses `&'static str` labels instead of `String`. Any fix to one must be manually mirrored to the other. The entire `worlds.toml` grid widget (`egui::Grid`, column headers, row layout) is also duplicated between the two files.
- **Fix:** Extract `gui_core::widgets::enum_combo` accepting `F: Fn(&T) -> impl Into<egui::WidgetText>` — covers both callers. Separately, extract the full `edit_rows` grid widget into `gui_core::widgets::worlds_grid(ui, cfg, &mut any_change, &mut delete_request)`.
- **Effort:** M
- **Risk / deps:** Cross-crate: `gui-core` must not depend on `sectorforge-builder` or `sectorforge-viewer` (it doesn't today — moving the widget there is fine). The builder `worlds_editor.rs` is a builder panel (`panel-implementer` agent territory).

---

### F3 — `SectorView` 27-field struct, 1711 LOC, no `Default`
- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `gui-core/src/sector_view.rs:136` (struct open) — 27 `pub` fields spanning lines 137–184; file is 1711 LOC confirmed
- **Evidence:**
  ```rust
  pub struct SectorView<'a> {   // line 136
      pub sector: &'a GeneratedSector,          // required
      pub selected_system: Option<&'a str>,
      …
      pub show_hover_coord: bool,               // line 183 — field 27
  }
  ```
- **Why it matters:** Call-site construction requires all 27 fields every time; compiler catches missing fields but not semantically wrong `None` defaults. The `show()` body is also split across routes, systems, subsectors, labels, hit-testing — ripe for sub-fn extraction.
- **Fix:** `impl Default for SectorView<'_>` with a dummy/placeholder `sector` (or a separate `SectorViewBuilder` that requires `sector`). Split `show()` body into `render_routes`, `render_systems`, `render_labels`, `render_hit_test` private fns.
- **Effort:** M
- **Risk / deps:** MAP-SNAPSHOT sensitive — `sector_view.rs` renders the map snapshots. The `Default` impl alone does not change rendering; body splits might. Run `UPDATE_MAP_SNAPSHOTS=1` after any split.

---

### F4 — `cache: None` callers hit O(systems·regions) fallback per frame
- **Review sev / bucket:** MED / P2 hot path
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/app/planner_view.rs:100` and `viewer/src/editor/map_panel.rs:63`
- **Evidence:**
  ```rust
  // planner_view.rs:100
  crate::sector_view::SectorView {
      cache: None,          // falls back to O(regions*hexes) scan per visible hex
      …
  }

  // sector_view.rs:264–283 — the fallback path
  for reg in self.sector.regions.iter() {
      if reg.hexes.iter().any(|h| h.q == q && h.r == r) { … }
  }
  ```
- **Why it matters:** The `None` fallback at `sector_view.rs:264` does an O(regions × hexes) scan for every hex in the viewport per frame. With 20+ regions and 8×8 = 64 hexes this is ~1280 iterations/frame minimum, hitting on every mouse move.
- **Fix:** Build a `SectorMapCache` in `RoutePlannerState` and `EditorState` and keep it updated on sector change. The planner already has `app.sector_map_cache` — thread a reference through `planner_view::ui`. The editor map panel should build a local cache on first use and invalidate on `state.dirty`.
- **Effort:** S
- **Risk / deps:** MAP-SNAPSHOT sensitive — `cache: Some(…)` vs `None` takes different code paths in `sector_view.rs`. The cache path is logically equivalent but snapshot tests should be run to confirm. No cross-crate change needed.

---

### F5 — Viewer: ~20 semantic hardcoded amber/red sites
- **Review sev / bucket:** MED / P3
- **Status:** 🔄 Moved (count differs — review says ×11 sites, actual is ~20 semantic sites in ~30 total)
- **Location:** `viewer/src/` — spread across `app/layout.rs:212`, `app/planner_view.rs:257/312/313`, `app/sector_view.rs:324/329/334`, `app/system_view.rs:146`, `app/trade_view.rs:87/163`, `data_editor.rs:114`, `dashboard.rs:71/135/174/175`, `editor/dialogs.rs:118/195`, `editor/generation_panel.rs:300`, `editor/settings_panel.rs:60`, `editor/wishes_panel.rs:94`, `factions_overview.rs:399`, `preset_gallery.rs:114/158/245/247`
- **Evidence:**
  ```rust
  // layout.rs:212 — amber "unsaved" / export status
  RichText::new(&app.export_status).color(Color32::from_rgb(235, 200, 90))
  // dashboard.rs:174–175 — severity colors for dashboard flags
  FlagSeverity::Error   => Color32::from_rgb(235, 90, 90),
  FlagSeverity::Warning => Color32::from_rgb(240, 200, 90),
  ```
- **Why it matters:** These bypass `palette::warning()` / `palette::danger()` / `palette::success()` — theme-unaware. The semantic amber `(235, 200, 90)` is close to the dark theme's `warning()` `(0xE0, 0x91, 0x3A)` but diverges in the light theme. Note: the 4 `segmentum_view.rs` sites and preview-mode green banner (`(0, 80, 0)`) are intentional — background fills / chrome, not status indicators.
- **Fix:** Mechanical substitution: `Color32::from_rgb(235, 90, 90)` → `palette::danger()`, `(235, 200, 90)` / `(240, 200, 90)` → `palette::warning()`, `(120, 220, 130)` → `palette::success()`. Leave segmentum fills and background banners as-is.
- **Effort:** M
- **Risk / deps:** No map-snapshot exposure (all viewer chrome panels). Low risk. The viewer has a `clippy.toml` banning raw paint primitives — but these `colored_label` / `RichText::color(…)` sites may not be caught by that lint (the ban targets `Painter` calls, not `Color32` literals). Check `viewer/clippy.toml` to confirm scope.

---

### F6 — `editor/dialogs.rs:195` — SaveAs error raw `Color32`
- **Review sev / bucket:** MED / P3
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/editor/dialogs.rs:195` (verified)
- **Evidence:**
  ```rust
  if let Some(e) = error.as_ref() {
      ui.colored_label(egui::Color32::from_rgb(235, 90, 90), e);
  }
  ```
- **Why it matters:** Error display in the SaveAs dialog is hardcoded red, bypassing `palette::danger()`. Theme-unaware; would fail contrast on the light theme.
- **Fix:** `ui.colored_label(palette::danger(), e);` — one-liner substitution.
- **Effort:** S
- **Risk / deps:** None — isolated to the dialog widget.

---

### F7 — Drag-move/add-route distance logic duplicated
- **Review sev / bucket:** MED / P1 #6
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/editor/map_panel.rs:154–185` (drag-move) + `213–230` (add-route) vs `viewer/src/app/sector_view.rs:561–593` (`add_route_between`) + `471–504` (`add_system_at`)
- **Evidence:**
  ```rust
  // editor/map_panel.rs:179 — drag finalize, distance recompute
  r.distance = sectorforge::sector_model::hex_distance(from_coord, to_coord);

  // app/sector_view.rs:587 — add_route_between, same recompute
  route.distance = sectorforge::sector_model::hex_distance(a, b);
  ```
- **Why it matters:** Both stacks implement the same: "find coord of both endpoints, call `hex_distance`, update `route.distance`". A future invariant change (e.g. distance weighting) must be applied in both places. The `App` path handles ID remapping via `reindex_ids` after any mutation; the `editor::` path does not — a subtle behavioral divergence.
- **Fix:** Extract `fn recompute_route_distances(sector: &mut GeneratedSector)` in `sectorforge` (or in a shared viewer util). Both stacks call it after any system move or route add. The `App` path's `reindex_ids` call and the editor's absence of it is a separate correctness gap that should be documented or unified.
- **Effort:** M
- **Risk / deps:** Depends on F1/F-S1 — fixing the two stacks first simplifies this extraction. Sequential.

---

### F8 — `info_panel.rs` (1156 LOC) — formatting fused with render
- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed
- **Location:** `gui-core/src/info_panel.rs` — 1156 LOC confirmed
- **Evidence:**
  ```rust
  // info_panel.rs:1 — single module, no sub-modules
  //! Right-side info panel. One pure render fn per entity kind so layout is easy
  //! to tweak in isolation.
  ```
- **Why it matters:** String formatting helpers (route stability labels, system archetype text, economy summaries) are interleaved with `egui::Ui` render calls. Extracting pure formatting fns would enable unit-testing content without a UI context, and make the render fns shorter.
- **Fix:** Extract `info_panel/format.rs` with pure `fn route_summary_text(route, sector) -> String` helpers. Keep `info_panel/mod.rs` as thin render wrappers calling format fns.
- **Effort:** M
- **Risk / deps:** MAP-SNAPSHOT sensitive — `info_panel` is used in the `sector_view` snapshot suite via the builder's info panel. Any behavioral change to formatting would alter snapshot output. Purely additive extraction with no behavioral change is safe.

---

### F9 — `editor/factions_panel.rs:308` — local `palette_dim()` shadows `chrome_text_dim()`
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/editor/factions_panel.rs:308` (verified)
- **Evidence:**
  ```rust
  fn palette_dim() -> Color32 {
      Color32::from_rgb(150, 145, 165)
  }
  ```
- **Why it matters:** `Color32::from_rgb(150, 145, 165)` is exactly `palette::TEXT_DIM` (palette.rs:15) and matches the value returned by `chrome_text_dim()` in the dark theme. This local fn is theme-unaware — it hardcodes the dark-theme value. Under the light theme the chrome dim color shifts, but this call site stays pinned to the dark value.
- **Fix:** Delete `palette_dim()`. Replace the one call site (`factions_panel.rs:176`) with `crate::palette::chrome_text_dim()`.
- **Effort:** S
- **Risk / deps:** None — isolated to factions_panel. The import `use crate::palette::{…}` already exists in the file.

---

### F10 — Per-frame `centers` HashMap rebuild + star-dust repaint
- **Review sev / bucket:** LOW / P2 hot path
- **Status:** ✅ Confirmed
- **Location:** `gui-core/src/sector_view.rs:382` (`centers` map) and `sector_view.rs:1016` (`paint_star_dust`)
- **Evidence:**
  ```rust
  // sector_view.rs:382 — rebuilt every call to show()
  let mut centers: HashMap<&str, Pos2> =
      HashMap::with_capacity(self.sector.systems.len());
  for sys in &self.sector.systems { … centers.insert(…); }

  // sector_view.rs:222–224 — star-dust painted every frame
  if framed && dark_map {
      paint_star_dust(&painter, rect);
  }
  ```
- **Why it matters:** `centers` is built even when a `SectorMapCache` is present (because `centers` also applies the drag-override). `paint_star_dust` iterates up to 540 circles every frame using a hash-based deterministic PRNG — deterministic but not free. Both could be memoized into `SectorMapCache` (centers as pixel coords keyed on origin + hex_size) and a pre-built `Vec<Shape>` for star-dust (keyed on rect dimensions).
- **Fix:** Add `star_dust_shapes: Vec<egui::Shape>` to `SectorMapCache` (or a separate `MapRenderCache` keyed on `(rect_width as u32, rect_height as u32)`). Move `paint_star_dust` shapes into the cache, extend with painter at render time.
- **Effort:** M
- **Risk / deps:** MAP-SNAPSHOT sensitive — `paint_star_dust` fires on live-render only (not export paths), so snapshot goldens are not affected. But the `centers` HashMap includes the drag-override, which is dynamic — only the static (non-drag) centers can be cached.

---

### F11 — `factions_overview.rs:399` — amber for both success and failure
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `viewer/src/factions_overview.rs:399` (verified)
- **Evidence:**
  ```rust
  if !state.status.is_empty() {
      ui.label(RichText::new(&state.status).color(egui::Color32::from_rgb(235, 200, 90)));
  }
  // state.status is set to "saved <path>" on success AND "save failed: <e>" on error
  // (factions_overview.rs:392–395)
  match choose_and_save_designer_toml(…) {
      Ok(Some(path)) => state.status = format!("saved {}", path),
      Err(e)         => state.status = format!("save failed: {e}"),
  }
  ```
- **Why it matters:** Both the save-ok and save-failed messages render in the same amber, making errors indistinguishable from success at a glance. A failed save looks identical to a successful one.
- **Fix:** Add a `status_is_error: bool` field to `FactionDesignerState`, set it on `Err`, clear on `Ok`. Render with `if state.status_is_error { palette::danger() } else { palette::success() }`.
- **Effort:** S
- **Risk / deps:** None — isolated to `factions_overview.rs` and `FactionDesignerState`.

---

### F12 — `palette.rs:771` — `stability_color` greens/ambers overlap `StatusColors`
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed — intentional split, documented here
- **Location:** `gui-core/src/palette.rs:771`
- **Evidence:**
  ```rust
  pub fn stability_color(s: RouteStability) -> Color32 {
      match s {
          RouteStability::Stable    => Color32::from_rgb(110, 210, 130), // data-viz green
          RouteStability::Unstable  => Color32::from_rgb(240, 200, 90),  // data-viz amber
          RouteStability::Hazardous => Color32::from_rgb(235, 90, 90),   // data-viz red
          RouteStability::Perilous  => Color32::from_rgb(165, 100, 215),
          _                         => Color32::from_rgb(150, 150, 150),
      }
  }
  ```
- **Why it matters:** The amber `(240, 200, 90)` and red `(235, 90, 90)` numerically resemble `StatusColors` warning/danger, but `stability_color` is *data-visualization*: it encodes a lore-specific domain value (route stability tier) onto the map canvas, not a UI state. Switching to `palette::warning()` would couple route coloring to theme status semantics and break under light themes where `warning()` shifts to orange-brown. The split is correct.
- **Fix:** No code change needed. Add a comment to `stability_color` documenting the intentional split: `// Data-viz palette — intentionally NOT palette::warning()/danger(). Route stability encodes domain lore, not UI state; these values are fixed regardless of theme.`
- **Effort:** S
- **Risk / deps:** MAP-SNAPSHOT sensitive — any change to `stability_color` would alter route colors in golden PNGs. Comment-only change is safe.

---

## Suggested local order

1. **F6, F9, F11, F12** — S-effort, no deps, safe warm-ups. F6/F9 are one-liner substitutions. F11 adds a bool field. F12 is a comment.
2. **F5 (remaining semantic sites)** — after F6/F9/F11 are done, do the bulk sweep of `Color32::from_rgb` → `palette::warning/danger/success` across the ~20 remaining viewer files. Mechanical; low risk.
3. **F4** — wire `SectorMapCache` into `planner_view` and `editor/map_panel`. S-effort per call site; run map snapshot suite after.
4. **F3 / F-S3** — add `Default` impl to `SectorView`. No behavioral change; run map snapshot suite to confirm.
5. **F2** — extract `gui_core::widgets::enum_combo` + worlds grid. Needs coordinated change in both builder and viewer panels.
6. **F8** — split `info_panel.rs` formatting. Additive; no behavioral change.
7. **F10** — memoize star-dust into cache. Requires cache key on rect dimensions; MAP-SNAPSHOT consideration.
8. **F7** — extract shared distance-recompute helper. Depends on F1/F-S1 decision about which stack survives.
9. **F1 / F-S1** — the dominant refactor. Unify editing stacks. Block on F7 being prepped. Requires full viewer regression pass.
