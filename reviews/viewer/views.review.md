---
unit_id: U019
crate: viewer
paths:
  - viewer/src/factions_overview.rs
  - viewer/src/segmentum_view.rs
  - viewer/src/route_planner.rs
  - viewer/src/dashboard.rs
  - viewer/src/data_editor.rs
  - viewer/src/preset_gallery.rs
loc_reviewed: 3512
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 8, low: 7, nit: 4 }
top_risks:
  - "Per-frame deep clone of SectorAnalysis in dashboard render path (F-019-001)"
  - "Per-frame allocation storm in segmentum super_map (F-019-002)"
  - "Viewer crate exposes a 600-line mutation surface (show_editor) that is dead and contradicts the read-only invariant (F-019-003)"
---

# Review: viewer primary view modules (U019)

## Summary

The view modules are functional but consistently leak allocations on the per-frame hot
path: `state.analysis.clone()`, fresh `HashMap`/`BTreeMap`/`HashSet`/`Vec` per show,
and `format!` strings for every label/cell. Read-only safety holds at the dispatch
layer (`app/*`) but `factions_overview::show_editor` plus ~400 lines of `&mut
GeneratedSector` mutation helpers are still compiled in as dead, public API — they
both contradict the crate's read-only invariant and silently rot. Most file-level
helpers (`stat`, `chip`, `kv`, `fixed_text`, `field_label`, `text_edit`,
`share_bar`) duplicate patterns that should live in `gui-core::palette`/`info_panel`
so the builder and viewer share one source of truth.

## Findings

### F-019-001 — [HIGH] [Performance] Per-frame deep clone of `SectorAnalysis`
- **Location:** `viewer/src/dashboard.rs:43`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — every frame the Dashboard tab is visible
- **Problem:** `let Some(a) = state.analysis.clone() else { ... };` deep-clones a
  `SectorAnalysis` (eight `BTreeMap<Arc<str>, u32>`, three `Vec<...>`, plus
  nested `top_factions: Vec<FactionShare>`, `articulation_point_ids:
  Vec<SystemId>`, etc.) on every redraw, even though the rendering only ever
  borrows `&a`.
- **Why it matters:** Steady-state allocator churn at 60 Hz for a panel the user
  may leave open indefinitely. `BTreeMap` clone is O(n) plus internal node
  allocation; the deep struct includes ~10 collections.
- **Evidence:** `analytics.rs:68-90` shows the field set; `dashboard.rs:43` is the
  clone site; nothing downstream needs ownership — every consumer (`share_bar`,
  `dist_block`, iteration over `health_flags`) takes references.
- **Suggested fix:** Borrow instead of clone.
  ```rust
  // before
  let Some(a) = state.analysis.clone() else { ... };
  // after
  let Some(a) = state.analysis.as_ref() else {
      ui.label(RichText::new("analysis unavailable").color(TEXT_DIM).monospace());
      return;
  };
  ```
  All `a.field` accesses below already work on a borrow.
- **Effort:** S
- **Risk of fix:** Low

### F-019-002 — [HIGH] [Performance] Allocation storm in `super_map` per frame
- **Location:** `viewer/src/segmentum_view.rs:234-411`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — segmentum overview, every frame
- **Problem:** `super_map` builds a `BTreeMap<String, Rect>` (`child_rects`,
  l.256) and a `HashMap<(String, SystemId), Pos2>` (`centers`, l.257), then
  for every system clones `meta.id` and `sys.id` into the key (l.317, l.320-321,
  l.335, l.355-356). For routes and links the keys are built again with
  `.clone()` per probe. Click hit-test (l.395-399) does another
  `(child.clone(), sys.clone())`. Then in `super_grid` /
  `child_table` / `link_table`, `bundle.link_count_for_child` (l.481, l.546)
  is a linear scan of all inter-sector links re-run for every grid cell — O(n·m)
  per frame.
- **Why it matters:** Even a small segmentum (e.g. 3×3 with ~120 systems) drives
  thousands of `String` allocs and Arc bumps per frame. The hash-map key type
  `(String, SystemId)` forces an owned `String` for every probe.
- **Evidence:** read of the function body; `LoadedSegmentumChild.id: String`
  means probing requires owning.
- **Suggested fix:**
  1. Hoist scratch maps to `SegmentumBundle` (cleared, not reallocated) **or**
     pass `&mut Scratch { child_rects, centers }` from the caller.
  2. Change the key from `(String, SystemId)` to `(&str, &str)` — values held
     in `SegmentumChild` and `GeneratedSystem` live longer than the closure.
  3. Memoize `link_count_for_child` into a `BTreeMap<String, usize>` populated
     once at load time inside `SegmentumBundle` (it never changes after load).
- **Effort:** M
- **Risk of fix:** Low

### F-019-003 — [HIGH] [Idiomatic / Ownership] Dead mutation API contradicts the read-only invariant
- **Location:** `viewer/src/factions_overview.rs:343-413` (`show_editor`),
  `:897-936` (`rebuild_all_summaries_from_world_data`), `:938-990`
  (`remove_faction_everywhere`), `:1004-1013` (`next_faction_id`),
  `:992-1002` (`clear_conflict`, `clear_option`)
- **Category:** Idiomatic Rust / Ownership / Dead code
- **Confidence:** High (verified via `grep -rn` over the workspace)
- **Blast radius:** Whole crate — declares mutation surface in a "read-only"
  view, plus drift hazard (mutation helpers go stale without anyone noticing
  because they're never invoked)
- **Problem:** `pub fn show_editor(ui: &mut Ui, sector: &mut GeneratedSector) ->
  bool` is exported but has zero callers anywhere in `viewer/` or `builder/`.
  Same for its support helpers `remove_faction_everywhere`,
  `rebuild_all_summaries_from_world_data`, `next_faction_id`. They take
  `&mut GeneratedSector` — exactly the API the crate is claimed not to have.
  `next_faction_id` even contains an `unreachable!("unbounded faction id
  search exhausted")` at `:1012` which is reachable in principle (default
  `i32` overflow on `for n in 1..`) — see also F-019-006.
- **Why it matters:** A read-only crate that ships writers is a lie that
  invites a future caller to use them and bypass the builder's command bus
  (which would break §R4 undo/redo). It also bloats the binary and pulls in
  `Arc::make_mut` paths on otherwise-shared data.
- **Suggested fix:** Either
  - delete `show_editor` and its mutation helpers entirely (preferred — viewer
    is read-only), or
  - move them into `builder/src/builder/panels/factions.rs` reworked to route
    through `BuilderCommand` (preserves the determinism/undo contract).
- **Effort:** M (deletion) / L (port to builder command bus)
- **Risk of fix:** Low (deletion); the only public callers are within the file.

### F-019-004 — [MEDIUM] [Performance] `factions_view` allocates `HashSet` + `Vec` every frame
- **Location:** `viewer/src/route_planner.rs:117-129`, called from
  `viewer/src/app/planner_view.rs:68-69`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — every frame on the Planner tab
- **Problem:** `highlighted_route_ids()` and `waypoint_set()` build a new
  `HashSet` per call by iterating and cloning the entire path. The accessor
  is called twice per frame.
- **Why it matters:** Two `HashSet` allocations + N `Arc` bumps per frame for
  data that only changes on plan recomputation.
- **Suggested fix:** Cache the sets on the `Plan` struct (populate during
  `plan_route`) and return `&HashSet<…>`, e.g.
  ```rust
  pub struct Plan {
      ...
      route_id_set: HashSet<RouteId>,
      hop_set: HashSet<SystemId>,
  }
  impl RoutePlannerState {
      pub fn highlighted_route_ids(&self) -> &HashSet<RouteId> {
          self.plan.as_ref().map_or(&EMPTY_ROUTES, |p| &p.route_id_set)
      }
  }
  ```
  Use `OnceLock<HashSet<…>>` for the empty fallback or return `Option<&…>`.
- **Effort:** S
- **Risk of fix:** Low

### F-019-005 — [MEDIUM] [Performance] `cached.clone()` of full preset list every frame
- **Location:** `viewer/src/preset_gallery.rs:139-150`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — every frame the gallery panel is open
- **Problem:** `let entries = match state.cached.clone() { ... }` clones a
  `Result<Vec<PresetEntry>, _>` (and every contained `String`/path) every frame
  just to satisfy the borrow checker — the loop below only reads.
- **Why it matters:** Per-frame `Vec` + string allocation for read-only access.
- **Suggested fix:** Borrow:
  ```rust
  let entries = match state.cached.as_ref() {
      Some(Ok(v)) => v.as_slice(),
      Some(Err(e)) => { ui.label(RichText::new(format!("load failed: {e}"))…); return; }
      None => return,
  };
  ```
  Move the click handler's call to `presets::scaffold` after splitting state
  mutation off (or take a local `dir = state.resolved_dir()` copy before the
  borrow) to avoid simultaneous `&` and `&mut state` aliasing.
- **Effort:** S
- **Risk of fix:** Low — needs minor refactor of the click handler to release
  the borrow before mutating `state.status`/`state.pending_open`.

### F-019-006 — [MEDIUM] [Panic] `unreachable!` is reachable via integer overflow
- **Location:** `viewer/src/factions_overview.rs:1004-1013`
- **Category:** Panics & failure surface
- **Confidence:** Medium (only reachable through `show_editor`, which is
  currently dead — see F-019-003)
- **Blast radius:** GUI thread panic if `show_editor` is ever wired
- **Problem:** `for n in 1..` uses inferred `i32` by default; iteration to
  `i32::MAX` returns `None` from `next`, falling through to
  `unreachable!`. With ~2 billion existing factions this is theoretical, but
  the bug is also that `n` could trivially wrap a different way if someone
  later changes the prefix logic.
- **Why it matters:** Library-style panic in a UI callback.
- **Suggested fix:** Use a typed wide counter and explicit error:
  ```rust
  for n in 1u64..=u64::from(u32::MAX) {
      let id = format!("faction_{n}");
      if !used.contains(id.as_str()) { return FactionId::new(id); }
  }
  // unreachable in practice; return a sentinel error instead of panicking.
  FactionId::new(format!("faction_{}", used.len() + 1))
  ```
  Better still: delete with F-019-003.
- **Effort:** S
- **Risk of fix:** Low

### F-019-007 — [MEDIUM] [Performance] `link_count_for_child` is O(L) and called from O(N·M) loops
- **Location:** `viewer/src/segmentum_view.rs:105-111`, used at `:481`, `:546`,
  `:705`, and once for every `LOCAL BORDER LINKS` row
- **Category:** Performance / Algorithm
- **Confidence:** High
- **Blast radius:** Hot path — every frame, super_grid and child_table both
  call this per cell/row
- **Problem:** Each invocation linearly scans `inter_sector_links` (full slice
  scan, no early exit). For a `cols × rows` super_grid plus the child_table,
  it's `O(grid · L)` per frame.
- **Why it matters:** Quadratic-ish behavior on a render hot path. Trivial to
  precompute.
- **Suggested fix:** Cache once in `SegmentumBundle` at load time:
  ```rust
  pub struct SegmentumBundle {
      ...
      link_counts: BTreeMap<String, usize>,
  }
  // populate in load_segmentum_bundle by iterating inter_sector_links once
  pub fn link_count_for_child(&self, id: &str) -> usize {
      self.link_counts.get(id).copied().unwrap_or(0)
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-019-008 — [MEDIUM] [Idiomatic / Duplication] Six helper widgets duplicate gui-core patterns
- **Location:** `viewer/src/factions_overview.rs:1270-1300` (`fixed`,
  `fixed_text`, `field_label`, `text_edit`); `viewer/src/segmentum_view.rs:760-798`
  (`stat`, `chip`, `kv`, `endpoint_label`, `orientation_label`);
  `viewer/src/dashboard.rs:247-303` (`share_bar`, `dist_block`)
- **Category:** Idiomatic / Code reuse
- **Confidence:** High
- **Blast radius:** Maintainability across viewer + builder
- **Problem:** Each view module rolls its own monospace label/chip/key-value
  widget. The builder almost certainly redefines the same primitives. They
  belong in `gui-core::info_panel` / `gui-core::palette` so the brand-styled
  rendering stays consistent.
- **Why it matters:** Any palette change ("kv label width is now 96") has to
  be reapplied N places. Today fonts/colors already drift between
  `dashboard::share_bar` (uses `Color32::from_gray(35)`, `Color32::from_gray(70)`)
  and the rest of the views (which use `palette::HEX_OUTLINE` / `palette::BG`).
- **Suggested fix:** Extract to `gui-core::widgets`:
  ```rust
  // gui-core/src/widgets.rs
  pub fn fixed_label(ui: &mut Ui, width: f32, text: &str, color: Color32) { … }
  pub fn field_label(ui: &mut Ui, text: &str) { … }
  pub fn kv(ui: &mut Ui, key: &str, value: &str) { … }
  pub fn chip(ui: &mut Ui, text: &str, fill: Color32) { … }
  pub fn stat(ui: &mut Ui, label: &str, value: usize) { … }
  ```
  Have viewer and builder both `use sectorforge_gui_core::widgets::*;`.
- **Effort:** M
- **Risk of fix:** Low — pure extraction; one PR per crate.

### F-019-009 — [MEDIUM] [Performance / Idiomatic] `text_edit<T: AsRef<str> + From<String>>` allocates an `Arc<str>` per keystroke
- **Location:** `viewer/src/factions_overview.rs:1285-1300`
- **Category:** Performance / Allocation
- **Confidence:** Medium (only allocates on `changed()` events, not idle)
- **Blast radius:** Faction designer editor — text fields on `Arc<str>` fields
- **Problem:** `*value = T::from(buf)` reallocates an `Arc<str>` for every
  character typed. The widget is generic, so designer rows (which are
  `String`-backed) pay only a `String` move, but the editor row variant
  called on `&mut GeneratedFaction` fields (currently unreachable, see
  F-019-003) would allocate per keystroke.
- **Why it matters:** Typing in egui fires `changed()` per character; an
  `Arc<str>` per char is unnecessary heap traffic.
- **Suggested fix:** Two specialisations:
  ```rust
  fn text_edit_string(ui: &mut Ui, value: &mut String, width: f32) -> bool { … }
  fn text_edit_arc(ui: &mut Ui, value: &mut Arc<str>, width: f32) -> bool {
      let mut buf = value.to_string();
      let r = ui.add_sized([width, 22.0], TextEdit::singleline(&mut buf).font(…));
      if r.changed() && buf.as_str() != value.as_ref() {
          *value = Arc::from(buf);
      }
      r.changed()
  }
  ```
  Or commit the new value only on `lost_focus()` to amortise.
- **Effort:** S
- **Risk of fix:** Low

### F-019-010 — [MEDIUM] [Performance] `format!` per cell/label in render hot loops
- **Location:** `viewer/src/factions_overview.rs:529, 564, 574, 580, 617, 627, 633, 710`
  (and many more `format!`s in headers/footers);
  `viewer/src/segmentum_view.rs:307, 374-378, 463, 471, 479, 540, 723`;
  `viewer/src/dashboard.rs:53, 59, 86, 117, 152, 160, 170, 205, 239, 270, 287, 298`;
  `viewer/src/route_planner.rs:433, 468` (these are off the per-frame path)
- **Category:** Performance / Allocation
- **Confidence:** High for render-hot files, lower for `route_planner.rs`
  (called from `plan_route`, not per-frame)
- **Blast radius:** Hot path — every visible row/header allocates a `String`
- **Problem:** Egui labels accept `impl Into<String>`, but `format!` always
  heap-allocates. With dozens of `format!("{}S {}W", …)` calls in `show_edit_row`
  / `show_readonly_row` running once per faction per frame, this is steady-state
  churn.
- **Why it matters:** A 30-faction sector with edit rows costs ~120
  `format!` allocations per frame on just this panel.
- **Suggested fix:** Reuse a thread-local `String` scratch buffer:
  ```rust
  thread_local! { static SCRATCH: RefCell<String> = RefCell::new(String::with_capacity(64)); }
  fn fmt_sw(s: usize, w: usize) -> String {
      SCRATCH.with(|b| {
          let mut b = b.borrow_mut(); b.clear();
          use std::fmt::Write; write!(b, "{s}S {w}W").unwrap();
          b.clone() // egui needs an owned String — at least the buffer is reused
      })
  }
  ```
  Or, lower-risk: precompute these strings once per data change and stash in
  a sibling derived state (mirrors the builder's `state/derivations.rs`
  pattern).
- **Effort:** M
- **Risk of fix:** Low

### F-019-011 — [LOW] [Idiomatic] Misleading comment about NaN handling in `HeapNode::cmp`
- **Location:** `viewer/src/route_planner.rs:237-245`
- **Category:** Idiomatic / Documentation accuracy
- **Confidence:** High
- **Blast radius:** Local correctness reasoning
- **Problem:** The comment says "NaN-safe via total_cmp on the bits" but the
  implementation uses `partial_cmp(...).unwrap_or(Equal)` — those are
  different. If a NaN ever lands in `cost`, the heap silently treats it as
  equal-to-everything and can loop or order incorrectly.
- **Why it matters:** Documentation lies about safety. Weights today are
  always finite (`edge_weight` only multiplies/adds constants), but adding a
  new term like `weight / dist` could introduce a NaN unnoticed.
- **Suggested fix:** Either really use `total_cmp`:
  ```rust
  other.cost.total_cmp(&self.cost)
  ```
  or update the comment to "weights are constructed finite; NaN would
  violate Ord". Add `debug_assert!(self.cost.is_finite());` in `push`.
- **Effort:** S
- **Risk of fix:** Low

### F-019-012 — [LOW] [Idiomatic] Public API leaks `sectorforge::ids::…` through fully-qualified path
- **Location:** `viewer/src/route_planner.rs:27-28, 49, 117, 124, 251-253, 295-297, 331-334, 351-353`
- **Category:** Idiomatic Rust / API hygiene
- **Confidence:** High
- **Blast radius:** Readability across the module
- **Problem:** Every reference is written `sectorforge::ids::SystemId` /
  `sectorforge::ids::RouteId` — ~25 occurrences. The module already does
  `use sectorforge::sector_model::{…}`; add the ids module to the same
  import block.
- **Suggested fix:**
  ```rust
  use sectorforge::ids::{RouteId, SystemId};
  ```
  Replace fully-qualified uses with the short names.
- **Effort:** S
- **Risk of fix:** None

### F-019-013 — [LOW] [Ownership] `observed.get(...).cloned().unwrap_or_default()` clones a sub-struct that is then read-only
- **Location:** `viewer/src/factions_overview.rs:394`
- **Category:** Ownership / Allocation
- **Confidence:** High
- **Blast radius:** Edit loop iteration — once per faction per frame
- **Problem:** `let obs = observed.get(&fac_id).cloned().unwrap_or_default();`
  clones a `PresenceStats` (four `BTree…` containers) to satisfy the borrow
  checker because `sector.factions[i]` is later borrowed mutably.
- **Why it matters:** Per-faction per-frame deep clone of the presence map. The
  caller only reads `obs` from `show_edit_row`.
- **Suggested fix:** Split the borrow:
  ```rust
  for i in order {
      let fac_id = sector.factions[i].id.clone(); // Arc bump
      let obs = observed.get(&fac_id); // Option<&PresenceStats>
      let fac = &mut sector.factions[i];
      dirty |= show_edit_row(ui, fac, obs.unwrap_or(&PresenceStats::default()), ...);
      ...
  }
  ```
  Hoist a `const DEFAULT: PresenceStats = PresenceStats { … };` (or
  `OnceLock`) so the empty case borrows a static reference.
  (Becomes moot if F-019-003 deletes the editor.)
- **Effort:** S
- **Risk of fix:** Low

### F-019-014 — [LOW] [Concurrency / API] `selected_link: Arc<str>` is allocated per click
- **Location:** `viewer/src/segmentum_view.rs:605`, `:118`, `:138`
- **Category:** Idiomatic / Allocation on input event
- **Confidence:** Medium
- **Blast radius:** One click → one `Arc<str>` allocation
- **Problem:** `*selected_link = Some(Arc::from(l.id.as_str()));` allocates a
  fresh `Arc<str>` even though `InterSectorLink.id` is already a `String`/`Arc`-
  style identifier. The selection could store the index, an `Arc::clone` of an
  existing arc, or a plain `String`.
- **Why it matters:** Tiny but reveals a type smell — `&mut Option<Arc<str>>`
  in the function signature suggests sharing, but no other holder exists.
- **Suggested fix:** If `InterSectorLink.id` is already `Arc<str>`, store
  `Arc::clone(&l.id)`. Otherwise, change `selected_link` to
  `Option<usize>` indexing `bundle.segmentum.inter_sector_links`.
- **Effort:** S
- **Risk of fix:** Low

### F-019-015 — [LOW] [Error handling] `data_editor` swallows partial save failures
- **Location:** `viewer/src/data_editor.rs:74-86`
- **Category:** Error handling
- **Confidence:** Medium
- **Blast radius:** Project save flow
- **Problem:** `save` does not write atomically — `fs::write(path, text)` will
  truncate `worlds.toml` then re-fill it. A crash between truncate and
  populate (or a quota error mid-write) leaves the user with an empty file.
  A short-running editor that "loses unsaved" is fine; one that destroys the
  prior on-disk state is not.
- **Why it matters:** This is the only persistence point for `worlds.toml`;
  losing it costs the user real authoring time.
- **Suggested fix:** Write-then-rename atomically:
  ```rust
  let tmp = path.with_extension("toml.tmp");
  fs::write(&tmp, text)?;
  fs::rename(&tmp, path)?;
  ```
  Or use the `tempfile::NamedTempFile::persist` pattern (already in the
  workspace's dev-deps for tests, would need promotion to runtime dep for
  viewer).
- **Effort:** S
- **Risk of fix:** Low

### F-019-016 — [LOW] [Error handling / Idiomatic] `extract_world_data_dir` defines `Mini` structs inside the function body for every call
- **Location:** `viewer/src/data_editor.rs:89-100`
- **Category:** Idiomatic Rust
- **Confidence:** High
- **Blast radius:** Compile-time only (no runtime cost — `serde` codegen
  happens once)
- **Problem:** Inline `#[derive(serde::Deserialize)] struct Mini` is a
  documentation smell — it hides what subset of `sectorforge.toml` the
  viewer cares about. Easier to find and reuse if hoisted to a private
  module-level type.
- **Suggested fix:** Move `Mini`/`MiniInputs` to file scope with a
  doc-comment explaining "minimal projection of project config for
  world_data_dir discovery".
- **Effort:** S
- **Risk of fix:** None

### F-019-017 — [LOW] [Performance] `factions_view` collects two full `Vec<SystemId>` / `Vec<WorldId>` per frame in edit mode
- **Location:** `viewer/src/factions_overview.rs:383-388`
- **Category:** Performance / Allocation
- **Confidence:** Medium (gated by `show_editor` being live — see F-019-003)
- **Blast radius:** Edit hot path
- **Problem:** `all_systems` and `all_worlds` are rebuilt every frame regardless
  of whether any "ALL SYS" / "ALL WORLDS" button is hovered or clicked.
- **Suggested fix:** Lazily compute only on the click branch; clone the IDs
  (cheap — Arc) only into the target field on the actual button event.
- **Effort:** S
- **Risk of fix:** Low

### F-019-018 — [NIT] [Idiomatic] `HashMap` instead of `BTreeMap` for centers/by_id is fine here, but determinism context deserves a comment
- **Location:** `viewer/src/segmentum_view.rs:257` (`HashMap` for `centers`),
  `viewer/src/route_planner.rs:170, 259, 304, 352, 391, 395`
- **Category:** Idiomatic / Project-specific
- **Confidence:** High
- **Blast radius:** None functionally — these maps are not iterated for output
- **Problem:** `CLAUDE.md` makes a hard rule about not iterating Fx/HashMap
  for output, but allows them for "internal lookup". The viewer uses
  `HashMap` for lookup only; **no output** depends on iteration order.
  However a one-line comment per site would help the next reviewer (and the
  rust-explorer subagent) skip these quickly.
- **Suggested fix:** Add `// lookup only — not iterated for output, see
  CLAUDE.md determinism rules`. No code change.
- **Effort:** S
- **Risk of fix:** None

### F-019-019 — [NIT] [Idiomatic] `egui::Frame::none()` is deprecated in newer egui
- **Location:** `viewer/src/segmentum_view.rs:437, 492, 777`,
  `viewer/src/preset_gallery.rs:182` (uses `.group()` which still wraps a
  Frame internally)
- **Category:** Idiomatic / Dependency-tracking
- **Confidence:** Medium (depends on the pinned egui version)
- **Blast radius:** Future egui upgrade pain
- **Problem:** Recent egui releases moved `Frame::none()` to `Frame::new()`
  and `Frame::default()` while removing or deprecating the no-arg variant in
  some patch versions. This is cross-cutting if/when the workspace bumps
  egui.
- **Suggested fix:** Centralise via `gui-core::widgets::card_frame(fill,
  stroke)` so a single edit handles the upgrade.
- **Effort:** S
- **Risk of fix:** Low

### F-019-020 — [NIT] [Docs] Magic numbers and palette values are not named constants
- **Location:** `viewer/src/dashboard.rs:71` (`Color32::from_rgb(235, 200, 90)`
  — duplicates the "warning amber" used in `data_editor.rs:118` and
  `preset_gallery.rs:99-101, 246-250`),
  `viewer/src/factions_overview.rs:478` (same color),
  `viewer/src/segmentum_view.rs:274` (`Color32::from_rgb(40, 36, 52)`
  selection bg), `:439` (`Color32::from_rgb(42, 38, 52)` — note the values
  differ between sites that should match)
- **Category:** Documentation / Maintainability
- **Confidence:** High
- **Blast radius:** Visual inconsistency
- **Problem:** Brand colors are duplicated as literals; the two "active
  child cell" tints already drift by 2 rgb units between super_map and
  super_grid.
- **Suggested fix:** Expose in `gui-core::palette`:
  ```rust
  pub const WARNING_AMBER: Color32 = Color32::from_rgb(235, 200, 90);
  pub const ERROR_RED: Color32 = Color32::from_rgb(235, 90, 90);
  pub const ACTIVE_CELL_BG: Color32 = Color32::from_rgb(42, 38, 52);
  pub const SUCCESS_GREEN: Color32 = Color32::from_rgb(120, 220, 130);
  ```
- **Effort:** S
- **Risk of fix:** None

### F-019-021 — [NIT] [Docs] `factions_overview.rs` module doc claims "broad summary edits" but show_editor is dead
- **Location:** `viewer/src/factions_overview.rs:1-5`
- **Category:** Documentation
- **Confidence:** High
- **Blast radius:** Reader confusion
- **Problem:** Module doc advertises an edit surface that no caller uses.
- **Suggested fix:** Either reinstate the caller (then the doc is honest) or
  delete the edit half along with F-019-003 and rewrite the doc to "read-only
  faction rollups + a TOML designer scratchpad".
- **Effort:** S
- **Risk of fix:** None

## Per-category coverage (rubric §3)

- **3.1 Panics & failure surface:** F-019-006 (`unreachable!` in
  `next_faction_id`). No other reachable unwraps on user-controlled input were
  found. The other `unwrap_or_default()`/`unwrap_or` sites are on `Option`
  control flow and are correct.
- **3.2 unsafe & soundness:** No findings. Zero `unsafe` blocks.
- **3.3 Ownership, borrowing, lifetimes, cloning:** F-019-001, F-019-002,
  F-019-004, F-019-005, F-019-013, F-019-014, F-019-017. The biggest theme.
- **3.4 Error handling:** F-019-015 (non-atomic save), F-019-016 (`Mini`
  hoist). `route_planner` and `factions_overview` return typed errors
  (`thiserror`) — consistent with library convention.
- **3.5 Concurrency & async:** No findings. Single-threaded UI; no rayon or
  spawn.
- **3.6 Performance:** F-019-001, F-019-002, F-019-004, F-019-005, F-019-007,
  F-019-009, F-019-010, F-019-017 — all per-frame allocation issues. The
  dominant theme.
- **3.7 Idiomatic Rust & API design:** F-019-003 (dead mutation API),
  F-019-008 (helper duplication), F-019-011 (misleading comment), F-019-012
  (path-qualification), F-019-014 (`Arc<str>` selection token), F-019-019
  (Frame::none deprecation surface).
- **3.8 Dependencies & Cargo hygiene:** No findings at unit level. Imports
  are tight; no unused `use` statements.
- **3.9 Memory & resource management:** No findings. `Drop` not used;
  `Arc::make_mut` usage in `remove_faction_everywhere` is correct (but the
  whole function is dead — see F-019-003). No growing caches.
- **3.10 Testing:** `factions_overview` has two unit tests for the designer
  export path (`tests` mod, l.1302-1349). None of the render functions are
  tested (egui makes that hard). `data_editor`, `dashboard`, `route_planner`,
  `preset_gallery`, `segmentum_view` ship **zero** `#[cfg(test)]` blocks —
  no property tests for the pathfinding monotonic-cost invariant in
  `route_planner::dijkstra`, no roundtrip test for `worlds.toml`
  load/save in `data_editor`. Recommend adding at least:
  - `route_planner`: prop_test that `Safest` cost is monotone non-decreasing
    along the returned path; that no path through `Perilous` is selected if
    a non-Perilous alternative exists.
  - `data_editor`: load → mutate → save → load roundtrip preserves all
    non-defaulted fields.
- **3.11 Documentation & maintainability:** F-019-020 (magic colors),
  F-019-021 (module doc lies). Module docs exist on every file (good).
  Public functions are mostly undocumented (`pub fn show_overview`,
  `pub fn plan_route`, `pub fn show`) — add `///` summaries with `# Panics` /
  `# Errors` where appropriate. Lifting to gui-core (F-019-008) is the right
  forcing function to land those.

## Project-specific invariants

- **No `FxHashMap`/`FxHashSet` iteration for output.** Verified — all
  output-shaped iterations use `BTreeMap`/`BTreeSet`. The `HashMap` uses in
  `segmentum_view::super_map` and `route_planner::dijkstra`/`bfs` are pure
  lookups (F-019-018 nit recommends a one-line comment to make this explicit
  for future reviewers).
- **All RNG draws via `src/model/rng.rs`.** Verified — no `rand::thread_rng()`
  / `SeedableRng` / `rand::random()` in any of the six files. The
  pattern-selection in `palette::draw_route_line_clipped` is given a stable
  `key` string built deterministically (`segmentum_view.rs:374-378`).
- **Output writers are byte-stable.** N/A — these files render to egui, not
  to disk. `data_editor::save` uses `cfg.to_toml_string()`; check the
  determinism of that against the workspace golden tests (delegated to
  X-cut review).
- **Builder mutations through the command bus.** Not applicable to viewer;
  but see F-019-003 — `factions_overview::show_editor` is the only place in
  this unit that takes `&mut GeneratedSector`, and it should not exist.

## Summary of suggested fixes

- F-019-001 — HIGH — borrow `state.analysis.as_ref()` instead of cloning per frame — S/Low
- F-019-002 — HIGH — hoist `super_map` scratch maps, use `&str` keys, memoize `link_count_for_child` — M/Low
- F-019-003 — HIGH — delete dead `show_editor` + mutation helpers (or port to builder cmd bus) — M/Low
- F-019-004 — MEDIUM — cache `HashSet`s on `Plan`, return references — S/Low
- F-019-005 — MEDIUM — borrow `state.cached.as_ref()` in preset gallery — S/Low
- F-019-006 — MEDIUM — replace `unreachable!` with typed counter and graceful sentinel — S/Low
- F-019-007 — MEDIUM — precompute `link_counts: BTreeMap` once at load — S/Low
- F-019-008 — MEDIUM — extract `kv`/`chip`/`stat`/`fixed_text`/`field_label`/`share_bar` into `gui-core::widgets` — M/Low
- F-019-009 — MEDIUM — split `text_edit` into `&mut String` / `&mut Arc<str>` variants, or commit on `lost_focus` — S/Low
- F-019-010 — MEDIUM — replace per-frame `format!` cells with cached strings or thread-local scratch — M/Low
- F-019-011 — LOW — use `total_cmp` in `HeapNode::cmp` and add `debug_assert!(finite)` — S/Low
- F-019-012 — LOW — `use sectorforge::ids::{RouteId, SystemId};` in `route_planner.rs` — S/None
- F-019-013 — LOW — borrow `observed.get(...)` instead of `.cloned().unwrap_or_default()` — S/Low
- F-019-014 — LOW — clone existing `Arc<str>` or store `usize` index for `selected_link` — S/Low
- F-019-015 — LOW — atomic write-then-rename in `data_editor::save` — S/Low
- F-019-016 — LOW — hoist `Mini`/`MiniInputs` to file scope with a doc-comment — S/None
- F-019-017 — LOW — defer `all_systems`/`all_worlds` Vec building to click handler — S/Low
- F-019-018 — NIT — add "lookup-only" comment near `HashMap` declarations — S/None
- F-019-019 — NIT — centralise `Frame::none()` via `gui-core::widgets::card_frame` — S/Low
- F-019-020 — NIT — promote duplicated rgb literals to `gui-core::palette` named constants — S/None
- F-019-021 — NIT — rewrite `factions_overview.rs` module doc to match reality (after F-019-003) — S/None
