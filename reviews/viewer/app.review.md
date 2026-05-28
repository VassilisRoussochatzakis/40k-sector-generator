---
unit_id: U020
crate: viewer
paths:
  - viewer/src/app/mod.rs
  - viewer/src/app/lifecycle.rs
  - viewer/src/app/layout.rs
  - viewer/src/app/sector_view.rs
  - viewer/src/app/system_view.rs
  - viewer/src/app/planner_view.rs
  - viewer/src/app/analytics_views.rs
  - viewer/src/app/export_ui.rs
  - viewer/src/app/trade_view.rs
  - viewer/src/app/editor_views.rs
  - viewer/src/app/factions_view.rs
  - viewer/src/app/regions_view.rs
  - viewer/src/app/relations_view.rs
  - viewer/src/app/segmentum.rs
  - viewer/src/app/types.rs
  - viewer/src/app/ui_helpers.rs
loc_reviewed: 3677
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 11, low: 8, nit: 5 }
top_risks:
  - "Auto-save in update() can panic via serde_json::unwrap() and silently drop write errors (F-020-001)"
  - "Per-frame project I/O on every dirty edit (load_project / TOML parse from disk) inside sector_view + lifecycle (F-020-002)"
  - "Export job errors are stringified and shown only in the toolbar — actionable detail (path, OS errno) is lost (F-020-003)"
  - "expect(\"sector loaded\") on Some+stale invariant in system_view inspector — panics if sector cleared between frames (F-020-004)"
---

# Review: viewer app lifecycle + layout (U020)

## Summary

`viewer/src/app/` is the eframe `App` and its per-view layout routers. The structure is reasonable — one `App` struct that holds tab state, a `View` enum for the central panel, and a `jobs::JobHandle` for background work — but the implementation has accumulated rough edges. The hottest concerns are (a) repeated re-parsing of project config on every dirty edit (a noticeable performance regression, especially in the editor sync block in `mod.rs:198` and `sector_view.rs:670`), (b) several reachable panics in `update()` (`serde_json::to_string_pretty(...).unwrap()` at `mod.rs:211`, `expect("sector loaded")` at `system_view.rs:31`, `unwrap()` at `mod.rs:237`), and (c) export-error reporting that drops the underlying `io::Error`/path context behind `format!("{}", e)`. The `Default for App` body is 47 lines of repeated field initialisation that diverges from `new_segmentum`'s `..Self::default()` shortcut — a maintenance liability as fields grow. No `Drop` is defined for `App`/`JobHandle`, so exporting/preview workers are detached threads that the eframe shutdown path cannot wait for; this is mostly fine but is worth flagging because `FileDialog` and `fs::write` work happens on those workers and a hard quit during export leaves a half-written file with no log.

## Findings

### F-020-001 — [HIGH] [Panics] `serde_json::to_string_pretty(...).unwrap()` in auto-save path
- **Location:** `viewer/src/app/mod.rs:211`
- **Category:** §3.1 Panics & failure surface / §3.4 Error handling
- **Confidence:** High
- **Blast radius:** Reachable on every frame when `editor.auto_save && editor.dirty` and serialisation fails (e.g., NaN floats in a manually-edited world stat).
- **Problem:** The auto-save block does
  ```rust
  let text = serde_json::to_string_pretty(sec).unwrap();
  if fs::write(path, text).is_ok() { ... }
  ```
  Both the JSON encode panic and the `fs::write` failure are silent: a write error is swallowed (clears nothing, leaves `dirty=true`, no `export_status` update), and an encode error crashes the app mid-update.
- **Why it matters:** A panic inside `eframe::App::update` brings down the binary on the user's next interaction. The silent write failure means the user thinks the auto-save flag worked.
- **Evidence:** Read of `mod.rs:208-217`. Compare with the explicit error surfacing in `lifecycle.rs:179-216`.
- **Suggested fix:** Reuse `write_sector_to_path` from `lifecycle.rs` — it already handles encode + write errors and sets `export_status`.
  ```rust
  if self.editor.auto_save {
      if let Some(path_str) = &self.editor.loaded_from {
          let path = PathBuf::from(path_str);
          self.write_sector_to_path(path); // handles errors + status
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-020-002 — [HIGH] [Performance] Per-edit reload of `worlds.toml` / `sectorforge.toml` from disk
- **Location:** `viewer/src/app/sector_view.rs:669-679`, `viewer/src/app/mod.rs:192-219`, `viewer/src/app/lifecycle.rs:198-211`
- **Category:** §3.6 Performance — hot path (every dirty edit in MAP view)
- **Confidence:** High
- **Blast radius:** Every system add/remove, every route add/remove, every editor-driven mutation re-reads the full project from disk inside `mark_live_sector_dirty` and again on each save. For large projects with many TOML files this is `O(project_size)` per click.
- **Problem:** `mark_live_sector_dirty` (called on every map mutation) does:
  ```rust
  if !self.editor.dirty {
      let mut input = None;
      if let Some(path) = &self.project_dir {
          if let Ok(utf8_path) = camino::Utf8PathBuf::from_path_buf(path.clone()) {
              if let Ok(pi) = sectorforge::input::load_project(&utf8_path) {
                  input = Some(pi);
              }
          }
      }
      self.editor.set_sector(sector.clone(), input, source);
  }
  ```
  The same triple is duplicated verbatim in `lifecycle.rs:57-63` (load), `lifecycle.rs:198-211` (save), and `mod.rs:192-219` (editor sync). `load_project` opens and parses several TOML files synchronously on the UI thread.
- **Why it matters:** Visible UI lag on every edit; the `ProjectInput` is also already known to the editor (it was loaded at startup). Reading it again at edit time is redundant and changes behaviour: an external edit to `worlds.toml` between clicks can silently rewrite the in-memory project under the user.
- **Evidence:** Three identical blocks; the only caller of `set_sector` that needs the freshest TOML is the explicit "open project" flow.
- **Suggested fix:** Cache the `ProjectInput` on `App` (Option) at load time and clone it for `set_sector`. Re-read only on explicit project open / RELOAD button. Extract a `App::cached_project_input(&self) -> Option<&ProjectInput>` helper to deduplicate.
- **Effort:** S
- **Risk of fix:** Low

### F-020-003 — [HIGH] [Error model] Export error reporting loses path/IO context
- **Location:** `viewer/src/app/export_ui.rs:193, 222-223, 249, 299, 344, 388, 442`
- **Category:** §3.4 Error handling
- **Confidence:** High
- **Blast radius:** Every export failure path.
- **Problem:** Every export job collapses to `format!("export failed: {}", e)` and that single string is the only thing the user sees. The PNG/SVG/HTML writers return `sectorforge` errors that already chain through `io::Error`, but by the time it reaches the UI it is a flat string with no path, no errno, and no hint of which file failed. For the all-system export, only the *first* failure is reported and the loop bails — the user never learns which system failed (the loop index is dropped). Equally, when `FileDialog::pick_folder` returns `None` the code returns silently (`export_ui.rs:200, 322, 366`) without resetting `pending_export` after the dialog was already cleared at the call site — fine, but `export_status` is also not reset, so the previous "export already running" message can linger.
- **Why it matters:** When a user clicks "EXPORT SYSTEMS" and one mid-batch system fails, the UI says `export failed: <one-line>`. They cannot tell which system, cannot retry just that one, and the partially-written directory is left as-is.
- **Evidence:** Read of `execute_png_export::AllSystemPngs` arm, `execute_svg_export`, `execute_html_export`, `export_sector_json`.
- **Suggested fix:**
  1. Include the destination path in every `Failed(...)`, e.g. `format!("export failed for {p}: {e}")`.
  2. In the all-system loop, capture failures into a `Vec<(SystemId, String)>` and report `exported N/M (K failed): first <id>: <msg>` so the user sees scale and example.
  3. For `pick_folder`/`save_file` returning `None`, set `self.export_status = "export cancelled".into()` so the toolbar text reflects the cancellation.
- **Effort:** M
- **Risk of fix:** Low

### F-020-004 — [HIGH] [Panics] `expect("sector loaded")` is reachable
- **Location:** `viewer/src/app/system_view.rs:31`
- **Category:** §3.1 Panics
- **Confidence:** Medium-High
- **Blast radius:** Reachable when `View::System { .. }` is the active view but `self.sector` becomes `None` (e.g., set_loaded_sector failure path, future "clear sector" feature, or even a future panel that clones-then-takes the sector). The guard `if let Some(sys) = sys_opt.as_ref()` on line 30 only proves the system_id resolves; it does not prove `self.sector` is still `Some` — but `system_by_id` itself returns `None` if `self.sector` is `None`, so the panic is currently unreachable. However the surrounding code repeatedly calls `self.sector.clone()` (e.g. `system_view.rs:152`, `sector_view.rs:25`), then the `expect` runs after that. Any refactor that clears the sector between those two reads (e.g. a Drop/destructure during preview application) will trip it.
- **Problem:** `let sector = self.sector.as_ref().expect("sector loaded");` inside a frame closure makes a load-bearing assumption that is not enforced by the type system.
- **Why it matters:** A startup or preview-failed transition that clears `self.sector` while `view == System` panics the binary. The other sister views (planner, dashboard, etc.) all guard with `let Some(sector) = ... else { ... return; };` — this one should too.
- **Suggested fix:** Hoist the sector clone above the `SidePanel::show` closure, identical to the other views:
  ```rust
  let Some(sector) = self.sector.clone() else {
      // draw "no sector" placeholder and return
      return;
  };
  // ... then use &sector inside the closure, no expect
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-020-005 — [MEDIUM] [Panics] `Utf8PathBuf::from_path_buf(path).unwrap()` on dialog result
- **Location:** `viewer/src/app/mod.rs:237`
- **Category:** §3.1 Panics
- **Confidence:** High
- **Blast radius:** `open_sector_dialog` — common entry path. Reachable on macOS/Linux where users can name a file with a stray non-UTF-8 byte, or pick a file under a non-UTF-8 ancestor directory.
- **Problem:** `let utf8_path = Utf8PathBuf::from_path_buf(path.clone()).unwrap();` will panic the binary if the OS hands back a non-UTF-8 path. Every other dialog handler in `export_ui.rs:168, 202, 273, 321, 371, 411` already does the right thing with a `Ok(p) = ... else { self.export_status = ...; return; }` pattern.
- **Suggested fix:** Mirror the export_ui pattern:
  ```rust
  let Ok(utf8_path) = Utf8PathBuf::from_path_buf(path.clone()) else {
      self.export_status = "load failed: path is not valid UTF-8".into();
      return;
  };
  ```
- **Effort:** S
- **Risk of fix:** None

### F-020-006 — [MEDIUM] [Resources / Concurrency] `JobHandle` drop silently abandons worker thread
- **Location:** `viewer/src/app/lifecycle.rs:225-253` (preview spawn), `viewer/src/app/export_ui.rs:447-464` (export spawn); `gui-core/src/jobs.rs:31-66` (no `Drop` impl).
- **Category:** §3.5 Concurrency / §3.9 Memory & resources
- **Confidence:** Medium
- **Blast radius:** A running export/preview job that is dropped — happens on `set_loaded_sector` (which assigns to `self.export_job = None`? actually `app.export_job` is only cleared on completion, so this is mostly the eframe shutdown path) and on `self.editor.preview_job = None` after a `Cancelled` result.
- **Problem:** `JobHandle` has no `Drop`. When the handle is dropped while the worker is still running:
  1. The `Receiver` is dropped — next `tx.send(...)` in the worker is a swallowed `Err(_)` (`jobs.rs:54: let _ = tx.send(...)`), which is fine.
  2. The cancellation flag is never set — the worker continues writing to disk to completion (e.g., a partially-written multi-system export with no UI listener).
  3. `ctx.request_repaint()` is invoked on a possibly-disposed `egui::Context`. Tested empirically — `egui::Context` is internally `Arc<...>` so the call is safe, but spends CPU.
  4. On eframe shutdown, the thread is detached and the OS just kills the process; any half-written file is left on disk.
- **Why it matters:** Cancellation semantics are explicit in the export UI (CANCEL EXPORT button) but implicit-drop cancellation is not — a future code path that replaces `export_job` while one is running (e.g. a "queue replace" flow) will leak a writer that races with the new one onto the same path.
- **Suggested fix:** Add `impl<T> Drop for JobHandle<T>` in `gui-core/src/jobs.rs` that sets `self.cancelled.store(true, ...)`. Workers that check `is_cancelled()` (the all-system loop, the preview generation) will then exit promptly. Keep the worker detached — joining on Drop in the UI thread is worse — but at least signal cancellation.
  ```rust
  impl<T> Drop for JobHandle<T> {
      fn drop(&mut self) {
          self.cancelled.store(true, Ordering::SeqCst);
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — the only behaviour change is "drop is now equivalent to cancel"; current call sites either complete-then-drop (no change) or explicitly cancel-then-drop (no change).

### F-020-007 — [MEDIUM] [Performance] `Arc::make_mut` on a multi-MB `GeneratedSector` on every map edit
- **Location:** `viewer/src/app/sector_view.rs:491, 530, 585, 620, 641`, `viewer/src/app/system_view.rs:186, 214`
- **Category:** §3.6 Performance
- **Confidence:** Medium
- **Blast radius:** Every map edit (ADD SYSTEM / REMOVE SYSTEM / ADD ROUTE / REMOVE ROUTE / ADD PLANET / REMOVE PLANET), and at least once more inside `mark_live_sector_dirty`.
- **Problem:** Each of these helpers does `let sector = self.sector.as_mut()...; let sector = Arc::make_mut(sector);` and then `mark_live_sector_dirty` does the same again. Because the editor sync block in `mod.rs:192` does `self.sector = Some(Arc::new(sec.clone()))` whenever `editor.dirty`, the Arc strong count typically drops to 1 between mutations, so `make_mut` is usually cheap — but the editor's preview path and the snapshot machinery (`analytics_views.rs:222: self.history_snapshots.push((name, sector.as_ref().clone()))`) both clone or hold the Arc, which forces a full deep clone on the next mutation. For a sector with a few hundred systems and chronicle data, this is significant.
- **Why it matters:** Performance regressions show up as visible lag immediately after creating a snapshot (the next edit copies the whole sector). The pattern also makes ownership of "the live sector" non-obvious.
- **Suggested fix:** Introduce a single mutator helper, `with_sector_mut(&mut self, f: impl FnOnce(&mut GeneratedSector) -> R)`, that does the `as_mut` + `make_mut` + `mark_dirty` once. Then in `analytics_views.rs:222`, store `Arc<GeneratedSector>` instead of `GeneratedSector` in `history_snapshots` (and clone the Arc, not the sector) — snapshots are read-only.
  ```rust
  pub(super) history_snapshots: Vec<(String, Arc<GeneratedSector>)>,
  // push: self.history_snapshots.push((name, Arc::clone(sector)));
  ```
- **Effort:** M
- **Risk of fix:** Low

### F-020-008 — [MEDIUM] [Error handling] `unwrap_or_default()` silently substitutes empty subsectors
- **Location:** `viewer/src/app/lifecycle.rs:23`, `viewer/src/app/mod.rs:197`, `viewer/src/app/sector_view.rs:667`
- **Category:** §3.4 Error handling
- **Confidence:** High
- **Blast radius:** Subsector display + export.
- **Problem:** `build_subsectors(&sector, SubsectorConfig::default()).unwrap_or_default()` swallows the build error and falls back to an empty `Vec<Subsector>`. The user sees "no subsectors" with no log line — distinguishable from "sector has zero subsectors" only by inspection. Same in three places.
- **Suggested fix:** Surface the error:
  ```rust
  self.subsectors = match build_subsectors(&sector, SubsectorConfig::default()) {
      Ok(s) => s,
      Err(e) => {
          self.export_status = format!("subsector derivation failed: {e}");
          Vec::new()
      }
  };
  ```
  And extract a helper so all three callers go through it.
- **Effort:** S
- **Risk of fix:** Low

### F-020-009 — [MEDIUM] [Performance] `egui::ComboBox` in `planner_view::system_combo` rebuilds a system list every frame
- **Location:** `viewer/src/app/planner_view.rs:150-154, 383-420`
- **Category:** §3.6 Performance — hot path (per-frame while planner view is active)
- **Confidence:** High
- **Blast radius:** Per-frame.
- **Problem:** `draw_planner_panel` builds `let options: Vec<(SystemId, Arc<str>)> = sector.systems.iter().map(|s| (s.id.clone(), s.name.clone())).collect();` every frame, then passes it to two combo boxes. Each `SystemId::clone` is an `Arc` bump but the Vec allocation is a fresh heap reservation per frame, and the `ComboBox::show_ui` closure iterates the options whether or not the popup is open. For a sector with several hundred systems this is hundreds of allocations per frame just to render the planner sidebar.
- **Suggested fix:** Hoist `options` into a cached field invalidated when `self.sector` changes, or use the existing `sector.systems` slice directly without cloning (`SystemId: Clone` because it's an `Arc<str>` newtype, so cloning is cheap but unnecessary). At minimum, only build the `options` Vec when `ComboBox::show_ui` is actually being painted by passing a closure:
  ```rust
  egui::ComboBox::from_id_salt(id)
      .selected_text(...)
      .show_ui(ui, |ui| {
          for s in &sector.systems { /* render */ }
      });
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-020-010 — [MEDIUM] [Performance] `trade_view::routes.sort_by(...)` re-sorts the trade routes every frame
- **Location:** `viewer/src/app/trade_view.rs:129-134`
- **Category:** §3.6 Performance
- **Confidence:** High
- **Blast radius:** Per-frame while TRADE view is open.
- **Problem:** `let mut routes: Vec<_> = sector.economy.routes.iter().collect(); routes.sort_by(...)` — full sort of all economy routes each frame, then `.take(20)`. For sectors with many routes, this is O(n log n) per frame for output that only changes when the sector changes.
- **Suggested fix:** Either (a) cache a `Vec<&TradeRoute>` sorted descending by volume in an `App`-level cache invalidated when the sector changes, or (b) use a partial selection algorithm:
  ```rust
  use std::cmp::Reverse;
  let mut top: Vec<_> = sector.economy.routes.iter().collect();
  top.select_nth_unstable_by(/* 20 */, |a,b| /* ... */);
  top.truncate(20);
  top.sort_by(/* ... */);
  ```
  Option (a) is simpler and consistent with the existing `heatmap_cache` / `sector_overview_cache` pattern.
- **Effort:** S
- **Risk of fix:** Low

### F-020-011 — [MEDIUM] [Idiomatic / API] `View::Clone` cloned every frame just to dispatch
- **Location:** `viewer/src/app/layout.rs:224`
- **Category:** §3.6 Performance / §3.7 Idiomatic
- **Confidence:** High
- **Blast radius:** Per-frame.
- **Problem:** `draw_main_view` does `match app.view.clone()` — it clones the entire `View` enum (which contains a `SystemId = Arc<str>`, cheap) and then deconstructs. The clone exists only because the match needs to call `&mut self` methods on `app`. Cloning an `Arc<str>` per frame is two atomic ops; mostly negligible, but the larger issue is that `View::System { selection: SystemSelection }` may grow to hold non-Copy fields.
- **Suggested fix:** Move the discriminator off `app` first:
  ```rust
  let view = std::mem::replace(&mut app.view, View::Sector); // or take a reference snapshot
  match view {
      View::System { system_id, selection } => app.draw_system_layout(ctx, &system_id, selection),
      other => { /* restore */ app.view = other; ... }
  }
  ```
  Cleaner: split `View` into a "current view" enum copy + a separate "view state" struct so dispatching only needs a `&View`. Acceptable to defer until `SystemSelection` becomes non-Clone.
- **Effort:** S
- **Risk of fix:** Low

### F-020-012 — [MEDIUM] [Idiomatic] `Default for App` is a 47-line transcription that diverges from `new_segmentum`
- **Location:** `viewer/src/app/mod.rs:92-140, 155-160`
- **Category:** §3.7 Idiomatic / maintainability
- **Confidence:** High
- **Problem:** `Default for App` lists every field explicitly. `new_segmentum` uses `..Self::default()`. When fields are added, both places must be kept in sync — but `new_segmentum` will silently pick up the new default while `Default` requires touching the giant list. The risk is reversed for `App::new()` / `new_with_source` / `new_empty` — they call `Self::default()` but then do nothing with new fields. This pattern has already produced churn (see `editor`, `data_editor`, `dashboard`, `preset_gallery` initialised in two different styles in `analytics_views.rs:236` snapshot revert path).
- **Suggested fix:** `#[derive(Default)]` is not possible because of the many non-`Default` egui types, but the body can be tightened by introducing per-subsystem default helpers:
  ```rust
  fn default_view_state() -> ViewState { ... }
  ```
  and grouping related fields into sub-structs (`SectorViewState`, `PlannerViewState`, `ExportState`, `EditState`). That also reduces the field count on `App` proper from ~30 to ~10 and makes derivations obvious. Defer to a focused refactor task; flag as MEDIUM because the maintainability tax compounds.
- **Effort:** L
- **Risk of fix:** Medium — touches every panel.

### F-020-013 — [MEDIUM] [Idiomatic / API] Top bar dispatch is a wall of `selectable_label / if clicked`
- **Location:** `viewer/src/app/layout.rs:43-122`
- **Category:** §3.7 Idiomatic
- **Confidence:** High
- **Problem:** 80 lines of nine near-identical `if ui.selectable_label(matches!(...), "...").clicked() { app.view = View::X; }` blocks. Adding a new view requires editing the enum, the layout match, and a hand-written button — easy to miss.
- **Suggested fix:** Drive the tab strip from a table:
  ```rust
  const TABS: &[(&str, View)] = &[("SECTOR", View::Sector), ...];
  for (label, view) in TABS {
      let on = std::mem::discriminant(&app.view) == std::mem::discriminant(view);
      if ui.selectable_label(on, *label).clicked() { app.view = view.clone(); }
  }
  ```
  Tabs that need extra logic (System needs a `sector_selected`) stay hand-written; the rest collapse. Reduces the file by ~60 lines.
- **Effort:** S
- **Risk of fix:** Low

### F-020-014 — [LOW] [Cloning] `sector.clone()` then `Arc::new(sector.clone())` in `set_loaded_sector`
- **Location:** `viewer/src/app/lifecycle.rs:23, 28, 66`
- **Category:** §3.3 Cloning
- **Confidence:** High
- **Problem:** The function takes `sector: GeneratedSector` by value, then does:
  ```rust
  self.subsectors = build_subsectors(&sector, ...).unwrap_or_default();
  self.sector_map_cache = Some(SectorMapCache::new(&sector, &self.subsectors));
  self.sector = Some(Arc::new(sector.clone()));      // <-- clone
  ...
  self.editor.set_sector(sector, input, source_path); // <-- moves the original
  ```
  The `Arc::new(sector.clone())` deep-clones the sector when the original is right there. The editor and the live `Arc` could share the same `Arc<GeneratedSector>`.
- **Suggested fix:**
  ```rust
  let sector_arc = Arc::new(sector);
  self.subsectors = build_subsectors(sector_arc.as_ref(), ...).unwrap_or_default();
  self.sector_map_cache = Some(SectorMapCache::new(sector_arc.as_ref(), &self.subsectors));
  self.sector = Some(Arc::clone(&sector_arc));
  self.editor.set_sector_arc(sector_arc, input, source_path); // new API, takes Arc
  ```
  Requires `EditorState::set_sector_arc` accepting an Arc (or have it take owned `GeneratedSector` and only the call site clones once via `(*sector_arc).clone()`).
- **Effort:** S
- **Risk of fix:** Low

### F-020-015 — [LOW] [Idiomatic] Duplicate "no sector loaded" placeholder is copy-pasted across seven views
- **Location:** `viewer/src/app/sector_view.rs:27-38`, `planner_view.rs:9-20`, `analytics_views.rs:8-19, 52-63`, `trade_view.rs:6-17`, `factions_view.rs:6-17`, `regions_view.rs:6-17`, `relations_view.rs:6-17`, `segmentum.rs:10-21`
- **Category:** §3.7 Idiomatic / DRY
- **Confidence:** High
- **Problem:** Eight identical 12-line blocks render the "no sector loaded" placeholder. Future styling change must touch all eight.
- **Suggested fix:** Extract to `ui_helpers::no_sector_placeholder(ctx: &egui::Context)` and call once per view that needs the early return.
- **Effort:** S
- **Risk of fix:** None

### F-020-016 — [LOW] [Idiomatic] `ScrollArea::show_rows` in `relations_view` is correct but neighbours use unbounded `ScrollArea::vertical`
- **Location:** `viewer/src/app/relations_view.rs:42-56` vs `trade_view.rs`, `regions_view.rs`, `factions_view.rs`
- **Category:** §3.6 / §3.7
- **Problem:** Only the relations view uses `show_rows` (lazy row creation) — the others render every row every frame. Sectors with many factions / regions / trade routes will spend frame budget building widgets the user cannot see.
- **Suggested fix:** Convert `trade_view.rs:135-156` (top trade lanes table) and `regions_view.rs:45-84` (regions grid) to `ScrollArea::vertical().show_rows(...)` once row height is determined. Particularly important for the trade-routes table, which can have hundreds of rows.
- **Effort:** S
- **Risk of fix:** Low

### F-020-017 — [LOW] [Cloning] `app.sector.clone()` at the top of every view
- **Location:** `viewer/src/app/planner_view.rs:9`, `sector_view.rs:25`, `analytics_views.rs:8, 52`, `trade_view.rs:6`, `factions_view.rs:6`, `regions_view.rs:6`, `relations_view.rs:6`, `segmentum.rs:10`
- **Category:** §3.3 Cloning
- **Problem:** Every view starts with `let Some(sector) = self.sector.clone() else { ... };`. The clone is cheap (Arc bump) but unnecessary because most views only read from `sector`. The `clone()` exists to satisfy the borrow checker when the view subsequently calls `&mut self` methods — but several of the views (`trade_view`, `regions_view`, `relations_view`, `factions_view`) never mutate App while holding the Arc, so a borrow would suffice.
- **Suggested fix:** Split each `ui(app, ctx)` into a short "borrow and dispatch" outer function and an inner `fn render(sector: &GeneratedSector, ctx: &egui::Context, ...)` that does not need `App`. Eliminates the Arc bumps for the read-only views.
- **Effort:** M
- **Risk of fix:** Low

### F-020-018 — [LOW] [Idiomatic] `History snapshots` stored as full `GeneratedSector` deep-clones in a `Vec`
- **Location:** `viewer/src/app/mod.rs:88`, `viewer/src/app/analytics_views.rs:222`
- **Category:** §3.3 / §3.9
- **Problem:** `history_snapshots: Vec<(String, GeneratedSector)>` — each snapshot is a full deep clone. With many snapshots and a large sector, memory grows linearly with no cap. There is no eviction policy. The "revert" path also clones again at `analytics_views.rs:235`.
- **Suggested fix:** Store `Arc<GeneratedSector>`; clone the Arc on snapshot and on revert (the revert site already calls `set_loaded_sector` which takes `GeneratedSector` by value — adapt that to take `Arc<GeneratedSector>` or `(*arc).clone()`). Add a cap on `history_snapshots` (e.g. ring buffer with limit 16) and surface that visually.
- **Effort:** S
- **Risk of fix:** Low

### F-020-019 — [LOW] [Idiomatic] `HeatmapMode::ALL` iterated via `super::HeatmapMode` instead of `sectorforge::heatmap::HeatmapMode::ALL`
- **Location:** `viewer/src/app/sector_view.rs:267`
- **Category:** §3.7 Idiomatic
- **Problem:** Minor — `super::HeatmapMode` resolves via the re-export in `types.rs`/`mod.rs`. It works but obscures that this is a `sectorforge` constant. Consider naming the import explicitly for grep-ability:
  ```rust
  use sectorforge::heatmap::HeatmapMode;
  ```
  in the file rather than via `super::*` indirection.
- **Effort:** trivial
- **Risk of fix:** None

### F-020-020 — [LOW] [Documentation] Module-level docs absent on most files
- **Location:** `viewer/src/app/lifecycle.rs:1` (no `//!`), `layout.rs:1`, `sector_view.rs:1`, `planner_view.rs:1`, `analytics_views.rs:1`, `trade_view.rs:1`, `factions_view.rs:1`, `regions_view.rs:1`, `relations_view.rs:1`, `segmentum.rs:1`, `types.rs:1`, `ui_helpers.rs:1`
- **Category:** §3.11 Documentation
- **Problem:** Only `mod.rs` and `export_ui.rs` have module-level docs. The other files have no `//!` header explaining their role. This unit is the natural place where a new contributor lands; module docs would orient them.
- **Suggested fix:** Add a one-paragraph `//!` to each file describing what it owns and what `App` fields it touches. Cross-link the responsible section of `GUIDE.md`/`docs/MAP.md`.
- **Effort:** S
- **Risk of fix:** None

### F-020-021 — [LOW] [Idiomatic] `route_view_mode` is mirrored on `App` and `EditorState`
- **Location:** `viewer/src/app/mod.rs:89`, `viewer/src/app/layout.rs:138-150`, `viewer/src/app/editor_views.rs:82`
- **Category:** §3.7 Idiomatic
- **Problem:** `App::route_view_mode` and `EditorState::route_view_mode` are kept in sync manually: the top bar writes both (`layout.rs:138-150`), and `draw_edit_layout` copies one to the other (`editor_views.rs:82`). Future code can desync them. The top-bar handler "wins" until the edit view runs, so the displayed sector may use a stale mode for one frame.
- **Suggested fix:** Single source of truth on `App`. `EditorState` accepts the mode as a parameter to `editor::show_map` / `editor_toolbar` rather than caching it. If the editor needs its own copy for offline rendering, expose a `&mut RouteViewMode` borrowed from `App`.
- **Effort:** S
- **Risk of fix:** Low

### F-020-022 — [NIT] `'a` lifetime on `TopBar`/`MainView` is unused
- **Location:** `viewer/src/app/layout.rs:6-32`
- **Category:** §3.7 / clippy `needless_lifetimes`
- **Problem:** The structs are constructed and immediately consumed by `.show()` — they don't need to be named. They could be free functions and the file would be 26 lines shorter.
- **Suggested fix:** Delete `TopBar`/`MainView` wrappers; call `layout::draw_top_bar(self, ctx)` and `layout::draw_main_view(self, ctx)` directly from `mod.rs::update`. (Clippy already flags this.)
- **Effort:** trivial
- **Risk of fix:** None

### F-020-023 — [NIT] `String::from` vs `.into()` — `format!`-then-`.into()` allocations
- **Location:** `viewer/src/app/lifecycle.rs:119`, `sector_view.rs:493, 561`, several export status assignments
- **Category:** §3.7 / cosmetic
- **Problem:** Inconsistent use of `.into()`, `String::from`, `to_string()`. Doesn't affect codegen but reads inconsistently.
- **Suggested fix:** Pick one style per file. Defer.
- **Effort:** trivial

### F-020-024 — [NIT] Magic numbers in `zoom_to_fit`
- **Location:** `viewer/src/app/lifecycle.rs:77-87`
- **Category:** §3.11
- **Problem:** `800.0`, `5.0`, `250.0` and the hex metric constants are unnamed. The comment mentions UI size isn't known — but the constants should at least be `const ZOOM_FIT_TARGET_PX`, `const HEX_SIZE_MIN`, `const HEX_SIZE_MAX`.
- **Suggested fix:** Promote to `const`s at the top of the file. Reuse `HEX_SIZE_MIN` / `HEX_SIZE_MAX` everywhere the clamp `5.0..=250.0` recurs (`sector_view.rs:256, 376, 383`, `planner_view.rs:55, 83, 89`).
- **Effort:** S
- **Risk of fix:** None

### F-020-025 — [NIT] `// Always set the sector in the editor...` comment cargo-culted
- **Location:** `viewer/src/app/lifecycle.rs:65`
- **Category:** §3.11
- **Problem:** Comment says "always" but the function is only called from explicit load paths — the comment narrates code instead of explaining why.
- **Suggested fix:** Replace with a why-comment ("Editor mirrors the canonical sector; this resets its dirty flag because the user explicitly chose this content.") or delete.
- **Effort:** trivial

### F-020-026 — [NIT] `// We could chain them, but pending_export only holds one. For now, let's just do PNG.`
- **Location:** `viewer/src/app/layout.rs:197-198`
- **Category:** §3.11 / TODO inventory
- **Problem:** The "SAVE & EXPORT ALL" button does not do what its label says — it saves and exports PNG only. The inline comment notes this is a known limitation. This should be a tracked TODO with an issue link, not a buried comment in a button handler. Worse, the label misleads the user.
- **Suggested fix:** Either implement a `pending_export: Vec<PendingExport>` queue, or rename the button to "SAVE & EXPORT PNG" to match behaviour. The former is the correct fix; the latter is a quick win.
- **Effort:** S (rename) / M (queue)
- **Risk of fix:** Low

## Rubric coverage

- **3.1 Panics & failure surface:** F-020-001 (auto-save unwrap, HIGH), F-020-004 (expect, HIGH), F-020-005 (Utf8 unwrap, MEDIUM).
- **3.2 unsafe & soundness:** No findings. No `unsafe` in any of the assigned files.
- **3.3 Ownership / cloning:** F-020-014, F-020-017, F-020-018, plus reference in F-020-007.
- **3.4 Error handling:** F-020-001, F-020-003 (HIGH), F-020-008.
- **3.5 Concurrency / async:** F-020-006 (`JobHandle` Drop). No async in workspace. Worker threads correctly use `Arc<AtomicBool>` and `Arc<Mutex<f32>>`; no lock-ordering hazards.
- **3.6 Performance:** F-020-002 (HIGH), F-020-007, F-020-009, F-020-010, F-020-011, F-020-016.
- **3.7 Idiomatic / API:** F-020-011, F-020-012, F-020-013, F-020-015, F-020-019, F-020-021, F-020-022.
- **3.8 Dependencies / Cargo hygiene:** No findings. Each file's imports are tight; `rfd` and `camino` are used appropriately. Note: `use rfd::FileDialog;` is duplicated as a free `use` at `mod.rs:223` after an `impl` block — minor style nit, no functional impact.
- **3.9 Memory & resources:** F-020-006 (JobHandle Drop), F-020-018 (snapshot growth, no eviction). No `static mut`. No reference cycles. `Arc<GeneratedSector>` ownership is consistent.
- **3.10 Testing:** No inline `#[cfg(test)]` in any of the assigned files — the entire layer is untested at the unit level. The behaviour is largely "UI dispatcher + glue", which is admittedly hard to unit-test, but functions like `preview_progress` (`lifecycle.rs:277-309`) and `fraction` (`lifecycle.rs:311-317`) are pure and trivially testable. Flag: no tests **at all** in this unit.
- **3.11 Documentation:** F-020-020 (no module docs), F-020-024 (magic numbers), F-020-025, F-020-026.

## Determinism invariants (CLAUDE.md)

- **No FxMap iteration for output**: None of the assigned files iterate Fx*-collections for output. Subsector / heatmap / overview iteration goes through cached helpers in `sectorforge::*` or `crate::*`. Pass.
- **RNG draws via `src/model/rng.rs`**: No RNG use in any assigned file. Pass.
- **Byte-stable writers**: Export paths delegate to `sectorforge::bitmap`, `svg_export`, `html_export` (out of scope for this unit). Pass.
- **Command bus**: This is the viewer, not the builder — the command-bus rule does not apply. The viewer mutates `GeneratedSector` directly via `Arc::make_mut`, which is correct for this crate.

## Summary of suggested fixes

- F-020-001 — HIGH — replace `serde_json::unwrap()` + ignored write error in auto-save with `write_sector_to_path` — S / Low
- F-020-002 — HIGH — cache `ProjectInput` once at project-open; stop re-parsing TOML on every dirty edit — S / Low
- F-020-003 — HIGH — preserve path + multi-failure context in export error messages — M / Low
- F-020-004 — HIGH — replace `expect("sector loaded")` in `system_view` with `let-else` guard — S / Low
- F-020-005 — MEDIUM — replace `Utf8PathBuf::from_path_buf(...).unwrap()` in `open_sector_dialog` with let-else — S / None
- F-020-006 — MEDIUM — add `Drop for JobHandle` that sets cancel flag — S / Low
- F-020-007 — MEDIUM — single `with_sector_mut` mutator; store snapshots as `Arc<GeneratedSector>` — M / Low
- F-020-008 — MEDIUM — surface `build_subsectors` errors instead of `unwrap_or_default()` — S / Low
- F-020-009 — MEDIUM — stop allocating planner combo `options` Vec each frame — S / Low
- F-020-010 — MEDIUM — cache or partial-sort trade-route top-N — S / Low
- F-020-011 — MEDIUM — avoid full `View::clone` in `draw_main_view` — S / Low
- F-020-012 — MEDIUM — group `App` fields into sub-structs; shrink the 47-line `Default` — L / Medium
- F-020-013 — MEDIUM — drive top-bar tab strip from a `&[(&str, View)]` table — S / Low
- F-020-014 — LOW — fold redundant `Arc::new(sector.clone())` in `set_loaded_sector` — S / Low
- F-020-015 — LOW — extract `no_sector_placeholder` helper used by all eight views — S / None
- F-020-016 — LOW — switch trade/regions tables to `ScrollArea::show_rows` — S / Low
- F-020-017 — LOW — split read-only views so they borrow `&sector` instead of cloning the Arc — M / Low
- F-020-018 — LOW — store `history_snapshots` as `Vec<(String, Arc<GeneratedSector>)>` with size cap — S / Low
- F-020-019 — LOW — import `HeatmapMode` directly instead of `super::HeatmapMode` — trivial / None
- F-020-020 — LOW — add `//!` module docs to every file in `viewer/src/app/` — S / None
- F-020-021 — LOW — collapse `route_view_mode` mirror between `App` and `EditorState` — S / Low
- F-020-022 — NIT — drop unused `'a` on `TopBar`/`MainView` wrappers — trivial / None
- F-020-023 — NIT — unify `.into()` / `String::from` / `to_string()` per file — trivial
- F-020-024 — NIT — name `5.0`, `250.0`, `800.0` constants for zoom/hex-size — S / None
- F-020-025 — NIT — drop the cargo-culted "Always set" comment in `set_loaded_sector` — trivial
- F-020-026 — NIT — rename misleading "SAVE & EXPORT ALL" button or implement export-queue — S/M / Low
