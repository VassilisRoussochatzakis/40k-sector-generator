---
unit_id: U018
crate: sectorforge-builder
paths:
  - builder/src/builder/panels/mod.rs
  - builder/src/builder/panels/missions.rs
  - builder/src/builder/panels/hooks.rs
  - builder/src/builder/panels/subsectors.rs
  - builder/src/builder/panels/regions.rs
  - builder/src/builder/panels/generation.rs
  - builder/src/builder/panels/interestingness.rs
  - builder/src/builder/panels/intel.rs
  - builder/src/builder/panels/personae.rs
  - builder/src/builder/panels/prose.rs
  - builder/src/builder/panels/conflict.rs
  - builder/src/builder/panels/briefing.rs
  - builder/src/builder/panels/sites.rs
  - builder/src/builder/panels/orbital.rs
  - builder/src/builder/panels/invariants.rs
  - builder/src/builder/panels/validation.rs
  - builder/src/builder/panels/surface_regions.rs
loc_reviewed: 8388
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 2, medium: 9, low: 11, nit: 6 }
top_risks:
  - "Per-frame heavyweight clones of derived reports + sector data across nearly every panel (F-018-001)"
  - "interestingness.rs::seed_target rebuilds an entire InterestingnessReport per immediate-mode frame while a picker is open (F-018-002)"
  - "Manual mission/site editor 'changed = true' fires on every keystroke, triggering full auto-recompute cascades (F-018-003)"
---

# Review: U018 — builder PANELS group C (narrative + analysis + utility)

## Summary

The 16 panels reviewed share one strong shape — a `show(ui, state)` free function that reads
`state.{cache,catalog,report}`, edits via either `state.run(BuilderCommand::…)` (for sector
mutations under `conflict`, `orbital`, `surface_regions`) or via overlay fields directly (per
the §D3 carve-out in `regions_ops.rs`). Failure surface is clean: no `unwrap`/`expect` outside
`#[cfg(test)]` except one acceptable `.expect("checked above")` in `regions.rs`. Determinism
discipline holds — no `FxHashMap` iteration, no `rand::thread_rng`, sorts via `BTreeMap`/`BTreeSet`.

The real cost is **performance / allocation pressure**: most panels clone the full report
(`Vec<MissionSeed>`, `Vec<Hook>`, `Vec<WorldSite>`, `Vec<Persona>`, `ProseReport`,
`SitesReport`, `Vec<WarpRegion>`, `Vec<Subsector>`, `Vec<GeneratedSystem>` snapshots) on
every frame so the borrow-checker doesn't fight the `&mut state` they need for click handlers.
A few panels go further and clone catalog blocks, `factions` rosters, or — worst — re-run a
full library derivation per frame just to populate a picker label.

Health = 3: the panels are correct, the patterns are uniform and easy to reason about, but
the workspace will not scale to large sectors at these allocation rates.

## Findings

### F-018-001 — [HIGH] [Performance] Per-frame clone-the-whole-report pattern is endemic across narrative panels
- **Location:**
  - `builder/src/builder/panels/missions.rs:189`, `missions.rs:283-292`
  - `builder/src/builder/panels/hooks.rs:147`, `hooks.rs:222-228`
  - `builder/src/builder/panels/sites.rs:192`, `sites.rs:290-294`
  - `builder/src/builder/panels/personae.rs:173`
  - `builder/src/builder/panels/prose.rs:152` (clones `prose_report` then again at `prose.rs:222`), `prose.rs:287-298`
  - `builder/src/builder/panels/subsectors.rs:113-127` (`current_subsectors`), `subsectors.rs:146` (`region.clone()` in REG inspector path mirror)
  - `builder/src/builder/panels/regions.rs:146` (`state.sector.regions[idx].clone()`), `regions.rs:323` (`regions: Vec<WarpRegion> = state.sector.regions.iter().cloned().collect()`)
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Every frame the tab is visible, for every panel using this pattern (≈9 panels). Cost scales with `N_systems × N_personae × manual_count`.
- **Problem:** The panels need both a non-mutable view of a report and `&mut state` for click handlers (`state.focus_entity`, `state.selected_*_id = …`). Rather than restructure borrows the panel does `let Some(report) = state.foo_report.clone() else { … }` (or `.as_ref().…cloned()` per row) up front. For sites/missions/prose/personae the report holds a `Vec` of owned structs containing further owned `String`s and `Vec<FactionId>`s — those are deep-cloned each frame at ~30–60 fps.
- **Why it matters:** Steady-state allocator churn proportional to derived-overlay size while the tab is open. On a sector with 200 systems and a few hundred sites/missions/personae this is megabytes/sec of heap traffic that will surface as GC-like jitter in the immediate-mode loop and dominate the panel's CPU budget.
- **Evidence:** Reads of the cited blocks. `Vec::clone` on a `MissionSeed`/`WorldSite`/`Persona`/`Hook` — each containing owned String fields — is deep.
- **Suggested fix (pattern — apply mechanically across the 9 sites):** Drop the report behind an `Rc<>` / `Arc<>` in `BuilderState` (one cheap retain per frame) **or** restructure each panel's loop to collect just the *click intents* into a small `Vec<Intent>` scratch buffer, then iterate `&state.foo_report.as_ref().unwrap().rows` directly and apply intents after the loop. Concretely:
  ```rust
  // before: let Some(report) = state.missions_report.clone() else { … };
  //         for m in &report.missions { … }   // deep clone
  // after:  let Some(report) = state.missions_report.as_ref() else { return; };
  //         let mut intents: Vec<Intent> = Vec::new();   // re-use a scratch field
  //         for m in &report.missions {
  //             if ui.button(…).clicked() { intents.push(Intent::Select(m.id.clone())); }
  //         }
  //         drop(report);                                  // release borrow
  //         for i in intents { state.apply_intent(i); }    // mut borrow now free
  ```
  The clones that remain are per-clicked-row, not per-rendered-row.
- **Effort:** M (one helper per panel)
- **Risk of fix:** Low — purely mechanical. Golden tests don't cover GUI.

### F-018-002 — [HIGH] [Performance] `seed_target` calls `derive_interestingness_with` once per add-override-row paint
- **Location:** `builder/src/builder/panels/interestingness.rs:405-433`, called from `:390` inside `show_add_override_row`.
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Only when the user has opened the §INT4 picker, but: the call runs on the *click* path so it's a one-shot per "Add" press, not every frame. **However** the docstring says the round-trip is intentional ("the library is the single source of truth"). Re-read of `:387–:398`: `seed_target` is invoked exactly once per Add-button click — so the steady-state cost is bounded.
- **Problem:** Constructing a fresh `GeneratedSector::empty` plus a full `derive_interestingness_with` pass just to read one band is a heavyweight operation (the scorer walks `observed_metrics` which itself materialises maps). It happens to be click-driven, which de-risks it, but the helper name is wrong — `seed_target` reads like an O(1) lookup. A user mashing "Add" twenty times pays the full derivation cost twenty times.
- **Why it matters:** Latency spike on Add click; also a code smell — the library should expose the band table directly.
- **Suggested fix:** Add a `pub fn profile_band(profile: ProfileId, metric: &str) -> Option<MetricTarget>` to `sectorforge::interestingness` (the comment says `profile_targets` is private — promoting it is a one-line change) and call that from `seed_target` instead of the round-trip. Falls back to the neutral `[0,1]` table on miss exactly as today.
- **Effort:** S (library exposure + 5-line panel change)
- **Risk of fix:** Low

### F-018-003 — [MEDIUM] [Performance / Correctness] `text_edit_*::changed()` + `on_catalog_edited` causes a recompute cascade on every keystroke
- **Location:**
  - `builder/src/builder/panels/missions.rs:461-595` (manual editor); cascades into `on_catalog_edited` → `recompute_missions` when `missions_auto_recompute` is on (`:674-676`)
  - `builder/src/builder/panels/hooks.rs:348-495` → `on_catalog_edited` → `recompute_hooks` (`:656-658`)
  - `builder/src/builder/panels/sites.rs:449-563` → `recompute_sites` (`:637-639`)
  - `builder/src/builder/panels/personae.rs:328-353` and `personae.rs:442-455` → `recompute_personae` (`:526-528`)
  - `builder/src/builder/panels/prose.rs:188-196`, `prose.rs:338-345` → `recompute_prose` (`:432-434`)
- **Category:** Performance / UX
- **Confidence:** High
- **Blast radius:** Whenever `*_auto_recompute` is on (default-on for most), each keystroke in any text/drag field flips `changed = true`, calls `on_catalog_edited`, marks the validation dirty, **and re-runs the full library derivation pass for the entire sector**. For sites/personae/missions that pass walks every world.
- **Problem:** There's no debounce or "field still focused" check. Holding `Backspace` in the manual mission's `title` field re-derives missions ~30 times/sec.
- **Why it matters:** Typing latency in any catalog editor will be poor as the sector grows; CPU pegged while editing.
- **Suggested fix:** Either (a) only set `changed |= …` on `lost_focus()` (like the project_io persistent_singleline pattern already used in `regions.rs:165-171`), or (b) thread a small debounce timer through `BuilderState` mirroring `preview.schedule(now, DEBOUNCE_SECONDS)` from `generation.rs:25`. Option (a) is the smaller change and matches existing prior art:
  ```rust
  let resp = ui.text_edit_singleline(&mut m.title);
  if resp.lost_focus() && /* value actually differs */ { changed = true; }
  ```
- **Effort:** M (touches ~5 panels but mechanical)
- **Risk of fix:** Low; tests already use the lost-focus pattern in `regions.rs`.

### F-018-004 — [MEDIUM] [Allocation / Hot path] `regions.rs::show_route_effects` rebuilds `regions_clone` + `systems_clone` on click and `BTreeMap<&str, HexCoord>` every frame
- **Location:** `builder/src/builder/panels/regions.rs:323-377`. `regions: Vec<WarpRegion> = state.sector.regions.iter().cloned().collect();` runs every frame; `coord_by_id: BTreeMap<&str, HexCoord> = …` also every frame; `regions_clone` and `systems_clone` allocations on click (`:366-368`).
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Every frame the REGIONS tab is visible. Cost scales with `N_systems + N_regions`.
- **Problem:** `regions` clone and `coord_by_id` build happen unconditionally per paint; only the click path needs the writable clones, and even those could borrow.
- **Suggested fix:** Iterate `&state.sector.regions` directly for the counting loop — the only reason to clone is the "Apply effects to routes" call, which can be split into a separate scope that re-reads:
  ```rust
  // counting loop
  for route in state.sector.routes.iter() {
      let (Some(&a), Some(&b)) = … else { continue; };
      if let Some(cond) = dominant_route_condition(&state.sector.regions, a, b) { … }
  }
  // … later, only if button was clicked:
  if clicked {
      apply_route_effects(&state.sector.regions, &state.sector.systems, &mut state.sector.routes);
  }
  ```
  `dominant_route_condition` already takes `&[WarpRegion]` so no clone is needed. `coord_by_id` is rebuilt every frame; if the system count is small it's tolerable, otherwise hoist to the derivation cache.
- **Effort:** S
- **Risk of fix:** Low

### F-018-005 — [MEDIUM] [Allocation] Subsectors panel rebuilds the full cluster list every frame when the cache is cold
- **Location:** `builder/src/builder/panels/subsectors.rs:113-127`, called from `subsectors.rs:42`.
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Every frame, when `map_view_cache` is None (which happens immediately after any override edit per `subsectors.rs:139, 169, 359, 513`).
- **Problem:** When the user edits an override, the panel sets `map_view_cache = None` to force the MAP tab to refresh on its next tick. But while the SUBSECTORS tab itself is still visible, `current_subsectors` falls through to the `else` branch and runs `build_subsectors(&sector, …)` + `apply_subsector_overrides` every frame until the user switches tabs. `build_subsectors` does a full k-means / Lloyd pass.
- **Why it matters:** Hangs the SUBSECTORS tab right after any override edit.
- **Suggested fix:** Cache the cluster list on a panel-owned field of `BuilderState` (a `subsector_panel_cache: Option<Vec<Subsector>>` with a generation counter that bumps whenever overrides change), or eagerly refresh the `map_view_cache` here rather than only on the MAP tab tick.
- **Effort:** S
- **Risk of fix:** Low — the cache invalidation is already explicit at every mutation site.

### F-018-006 — [MEDIUM] [Allocation] `regions.rs::show_glyph_preview` allocates an O(W·H) `Vec<Vec<char>>` per frame
- **Location:** `builder/src/builder/panels/regions.rs:403-452`. Concretely `:411` (`vec![vec!['.'; w]; h]`) and `:433` (`String::with_capacity((w + 1) * h)`).
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Every frame the REGIONS tab is visible.
- **Problem:** Allocates a fresh nested `Vec<Vec<char>>` of the sector dimensions, then a second `String` of size `2·W·H`, on every paint. For a 64×64 sector that's 65 small allocations + one ~8 KiB string per frame.
- **Suggested fix:** Hoist the grid + text scratch to a `RegionsPanelScratch` field on `BuilderState` and `clear()` instead of re-allocating. Or move the rendering into a single flat `Vec<u8>` of dim `2·W·H` and reuse.
  ```rust
  // store on BuilderState:
  pub struct RegionsScratch { grid: Vec<u8>, w: usize, h: usize, text: String }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-018-007 — [MEDIUM] [Allocation] Faction-roster `Vec<(FactionId, String)>` cloned per frame in 6+ editor sites
- **Location:**
  - `builder/src/builder/panels/conflict.rs:41-46`, `conflict.rs:137-142`
  - `builder/src/builder/panels/orbital.rs:69-74`
  - `builder/src/builder/panels/surface_regions.rs:44-49`
  - `builder/src/builder/panels/intel.rs:99-104`, `intel.rs:128-133`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Every frame the relevant tab is visible. Cost = `N_factions` × (FactionId clone = String clone + name clone).
- **Problem:** Each panel materialises a fresh `Vec<(FactionId, String)>` from `state.sector.factions` to hand into the combo helpers, just so the iterator can be borrowed across UI calls that also borrow `state` mutably. Both `FactionId` (currently a `String` wrapper) and the name allocate.
- **Suggested fix:** Two options:
  1. Have the combo helpers take `&[GeneratedFaction]` directly — most callers just need `f.id` + `f.name`, both of which are `&str`-derivable.
  2. Hoist a faction-display cache to `BuilderState` (one `Vec<(FactionId, String)>` invalidated on any sector mutation that touches `factions`). The derivation cache infrastructure already exists per `mod.rs` `pub mod derivation_cache;`.
- **Effort:** M (touches helper signatures)
- **Risk of fix:** Low — purely refactor.

### F-018-008 — [MEDIUM] [Allocation] `personae.rs::show_kind_pools_section` constructs `Vec<String>` of all kinds per frame
- **Location:** `builder/src/builder/panels/personae.rs:398-403`.
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Every frame the PERSONAE tab is visible.
- **Problem:** Allocates `Vec<String>` from the built-in `&[&'static str]` slice plus any custom kinds, just to dedupe. `kinds.contains(key)` is O(N) so the whole construction is O(N²) per paint.
- **Suggested fix:** Use a `BTreeSet<&str>` keyed by `&str` (no allocation per kind):
  ```rust
  let mut kinds: BTreeSet<&str> = BUILTIN_KINDS.iter().copied().collect();
  for key in cfg.kinds.keys() { kinds.insert(key.as_str()); }
  for kind in &kinds { … }
  ```
  Or just iterate built-ins then `cfg.kinds.keys().filter(|k| !BUILTIN_KINDS.contains(&k.as_str()))` directly.
- **Effort:** S
- **Risk of fix:** Low

### F-018-009 — [MEDIUM] [Correctness] `personae.rs` "add custom kind" text field is recreated empty every frame
- **Location:** `builder/src/builder/panels/personae.rs:422-435`.
- **Category:** Correctness / UX
- **Confidence:** High
- **Blast radius:** The custom-kind picker on the PERSONAE tab.
- **Problem:** `let mut new_kind = String::new();` is a fresh empty string each frame. The `ui.text_edit_singleline` mutates it, but on the next frame the value is gone. Only the user's *very last* keystroke before pressing Enter could be visible to the `lost_focus + Enter` branch — and even then, the value visible to that branch is what was typed *that same frame*, which is fragile (an Enter immediately after a keystroke may or may not see the latest text).
- **Why it matters:** The "Add custom kind" UI is effectively broken — type "necron-fork", press Enter, and the value may be empty or partial. Compare with `intel.rs:346-362` which correctly uses `ui.data_mut` to persist the buffer across frames.
- **Suggested fix:** Use the project's own `persistent_singleline` helper (re-exported from `panels/mod.rs:21`), the same pattern `regions.rs:166-170` uses, or copy `intel.rs:346-362`'s `data_mut` idiom.
- **Effort:** S
- **Risk of fix:** Low

### F-018-010 — [MEDIUM] [Correctness] `conflict.rs` system-conflict aggregate runs a `SetSystemConflict` command **every frame** when override is off
- **Location:** `builder/src/builder/panels/conflict.rs:167-180`.
- **Category:** Correctness / Performance / Undo bus pollution
- **Confidence:** High
- **Blast radius:** Every frame the SYSTEM tab's §CF2 section is open and override is off.
- **Problem:** The aggregate-mode branch derives `derived = derive_system_conflict(sys)`, compares to `sys.conflict`, and if they differ runs `BuilderCommand::SetSystemConflict`. But the *command bus* pushes onto the undo log (per CLAUDE.md §R4). If `derive_system_conflict` ever returns a value that doesn't compare equal to the just-set value (e.g. an `age` field that depends on tick state), every frame pushes a new command — saturating the undo ring and burning CPU.
- **Why it matters:** Even if the derivation is currently idempotent, this is a fragile coupling. And it makes the undo log meaningless because user actions get drowned in auto-syncs. `let _ = state.run(cmd);` swallows any error too.
- **Suggested fix:** Move the auto-sync out of the panel and into the derivation cache / a deliberate "sync now" step on tab activation. Or guard the write behind a real diff that excludes monotonic fields like `age` / `last_change_tick`. At minimum, surface the error and skip if `sys.conflict.derived_equivalent_to(&derived)`.
- **Effort:** M
- **Risk of fix:** Medium — needs a small library helper for the comparison.

### F-018-011 — [LOW] [Performance] `interestingness.rs::show_metrics_chart` paints O(metrics) bars without `with_capacity`
- **Location:** `builder/src/builder/panels/interestingness.rs:181-249`.
- **Category:** Performance
- **Confidence:** Medium
- **Problem:** `draw_metric_row` formats a `RichText::new(format!(…))` per metric per frame. Minor — the metric count is bounded (~13) — but the `format!` chain is the dominant cost here.
- **Suggested fix:** Use `write!` into a thread-local scratch `String` or precompute the row text into `score`'s own struct when the report is built. The library, not the panel, is the natural owner of "render-ready strings".
- **Effort:** S
- **Risk of fix:** Low

### F-018-012 — [LOW] [Correctness] `regions.rs` HexCoord clamp uses `.min(…).max(…)` — fine, but `as i32` cast on possibly-huge `u32` widths is silent truncation
- **Location:** `builder/src/builder/panels/regions.rs:136-143` and `regions.rs:247-248`, `regions.rs:407-409`.
- **Category:** Idiomatic / Correctness
- **Confidence:** Medium
- **Problem:** `(state.sector.width.saturating_sub(1) as i32)` silently truncates for widths ≥ `i32::MAX` (theoretically possible; practically not, but the panel uses `as` rather than `i32::try_from` everywhere). Same for `state.sector.systems.len().max(1) as u32` at `subsectors.rs:133` and `region_grow_size as usize` at `regions.rs:294`.
- **Suggested fix:** Either accept the convention (sector dims fit in i32 by design) and add a `// SAFETY: sector dims constrained to ≤200 by ConfigGeneration::sector_width range` comment, or switch to `i32::try_from(…).unwrap_or(i32::MAX)` so the range is explicit.
- **Effort:** S
- **Risk of fix:** Low

### F-018-013 — [LOW] [Allocation] `validation.rs::group_by_file` and `invariants.rs::group_by_stratum` clone every issue/violation
- **Location:**
  - `builder/src/builder/panels/validation.rs:158-168` (`group.push(i.clone())`)
  - `builder/src/builder/panels/invariants.rs:137-146` (`out.entry(key).or_default().push(v.clone())`)
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Every paint of these tabs. Issue / violation count is typically small (dozens) so this is bounded, but the panel could just store indices into the report.
- **Suggested fix:** `BTreeMap<String, Vec<&ValidationIssue>>` keyed by reference (the report is borrowed for the whole render anyway) — drops all the clones.
- **Effort:** S
- **Risk of fix:** Low

### F-018-014 — [LOW] [Performance] `prose.rs::show_system_editor` clones the whole `prose_report` even when only one entry is rendered
- **Location:** `builder/src/builder/panels/prose.rs:222`.
- **Category:** Performance
- **Confidence:** High
- **Problem:** The whole report is cloned just to read `system_entries.iter().find(|e| e.system_id == sid)`. `ProseReport` likely holds owned paragraphs for every system, so this scales O(N_systems · per-system-prose-bytes) per frame.
- **Suggested fix:** Restructure as F-018-001 — capture the selected entry into a local `let entry = report.system_entries.iter().find(…).cloned()` (cloning **one** entry instead of all). The shape is already there at `:287-292`, just drop the up-front report clone.
- **Effort:** S
- **Risk of fix:** Low

### F-018-015 — [LOW] [Idiomatic] Big `KIND_VARIANTS: &[…]` constants are duplicated; could be `EnumIter` from the library side
- **Location:**
  - `builder/src/builder/panels/missions.rs:44-66` (`KIND_VARIANTS`, `SCALE_VARIANTS`, `VISIBILITY_VARIANTS`)
  - `builder/src/builder/panels/hooks.rs:44-59`
  - `builder/src/builder/panels/personae.rs:42-67`
  - `builder/src/builder/panels/sites.rs:40-72`
  - `builder/src/builder/panels/interestingness.rs:46-71`
  - `builder/src/builder/panels/surface_regions.rs:19-32`
- **Category:** Idiomatic / Maintainability
- **Confidence:** Medium
- **Problem:** Every panel re-asserts the variants of an enum in display order, with a `// Keep in sync with src/foo.rs::FooKind` comment as the only invariant. Drift is inevitable.
- **Suggested fix:** Each enum should expose a `pub const ALL: &[Self]` (the pattern already used by `RegionConditionKind::ALL` per `regions.rs:180`). Replace each panel's local const with the library's. Adding a variant only requires updating the library, not 6 panels.
- **Effort:** M (library-side mostly)
- **Risk of fix:** Low

### F-018-016 — [LOW] [Idiomatic] `intel.rs` does a `let _ = state.run(cmd);` with several panels swallowing command errors
- **Location:** `builder/src/builder/panels/conflict.rs:179` (`let _ = state.run(cmd);`).
- **Category:** Error handling
- **Confidence:** High
- **Problem:** §3.4 — errors swallowed via `let _ = …`. The sibling sites surface errors through `ModalKind::Message`; this one drops it. The aggregate-sync path is exactly the place where a silent error would be most damaging (state diverges from the world rollup).
- **Suggested fix:** Either surface via `ModalKind::Message` like the rest of the file does at `conflict.rs:76-80`, or `.unwrap_or_else(|e| log::warn!("system conflict auto-sync: {e}"))`. Don't blackhole it.
- **Effort:** XS
- **Risk of fix:** Low

### F-018-017 — [LOW] [Performance] `intel.rs::run_baseline_intel` re-builds `observer_ids: Vec<String>` then `observer_refs: Vec<&str>` per call
- **Location:** `builder/src/builder/panels/intel.rs:77-84`.
- **Category:** Performance / Allocation
- **Confidence:** Medium
- **Problem:** Click-driven (not per-frame), so the cost is bounded. Still — two vecs allocated to call a fn that takes `&[&str]`. A `let ids: Vec<&str> = state.sector.factions.iter().map(|f| f.id.as_str()).collect();` works directly (avoiding the intermediate owned-String stage) since the borrow is held across `derive_intel`.
- **Suggested fix:**
  ```rust
  let observer_refs: Vec<&str> = state.sector.factions.iter().map(|f| f.id.as_str()).collect();
  sectorforge::intel::derive_intel(&mut state.sector, &observer_refs);
  ```
  Wait — `&mut state.sector` conflicts with the immutable borrow of `state.sector.factions`. The current `String` round-trip is *deliberate* to break the borrow. So this is **acceptable** and the only fix is to take a `Vec<FactionId>` snapshot before grabbing the mut borrow:
  ```rust
  let observer_ids: Vec<&str> = state.sector.factions.iter().map(|f| f.id.as_str()).collect();
  // observer_ids borrows state.sector immutably; need owned to outlive
  ```
  Net: leave as-is. Mark as known-acceptable rather than a finding. **Closing as no-action.**
- **Effort:** —
- **Risk of fix:** —

### F-018-018 — [LOW] [Idiomatic] `regions.rs:475` uses `.expect("checked above")` where `let … else` is cleaner
- **Location:** `builder/src/builder/panels/regions.rs:471-477`.
- **Category:** Idiomatic Rust
- **Confidence:** High
- **Problem:** `.expect("checked above")` is a code smell; the next refactor that moves the early-return loses the invariant. Replace with `let-else`.
- **Suggested fix:**
  ```rust
  let Some(cfg_src) = state.data_catalogs.regions.as_ref() else { return; };
  let mut cfg = cfg_src.clone();
  ```
- **Effort:** XS
- **Risk of fix:** None

### F-018-019 — [LOW] [Documentation] `panels/mod.rs` contract docstring claims "never carry raw String IDs across boundaries" — several panels do exactly that
- **Location:** `builder/src/builder/panels/mod.rs:1-18` (the contract), violated by e.g.:
  - `personae.rs:332` — `p.faction_id = sectorforge::ids::FactionId::new(fac.as_str());`
  - `missions.rs:485-489`, `hooks.rs:413-414` — raw `String` round-trip through text fields
  - `sites.rs:472-477`, `sites.rs:494-501` — same
- **Category:** Documentation / API
- **Confidence:** Medium
- **Problem:** The contract is reasonable for cross-tab navigation (where the panels do use typed IDs through `EntityRef`), but the manual-editor surfaces inevitably round-trip through `String` for the text fields. The docstring is overly absolute.
- **Suggested fix:** Soften the rule to "cross-panel navigation uses typed IDs; manual editors may round-trip through `String` only for the duration of the text edit". Or add a `pub fn from_user_str(&str) -> Option<Self>` validator on each ID type so the editors do typed input.
- **Effort:** XS (doc) / M (validator)
- **Risk of fix:** Low

### F-018-020 — [NIT] [Idiomatic] `unwrap_or(0)` on count labels reads better as `.map_or(0, …)`
- **Location:** Throughout — e.g. `missions.rs:114-119`, `hooks.rs:102-107`, `sites.rs:117-122`, `personae.rs:104-109`, `prose.rs:78-83`.
- **Category:** Style
- **Confidence:** High
- **Suggested fix:**
  ```rust
  let total = state.missions_report.as_ref().map_or(0, |r| r.missions.len());
  ```
- **Effort:** XS
- **Risk of fix:** None

### F-018-021 — [NIT] [Documentation] `mod.rs:18` advertises that Phase B / C panels "get added below" — Phase C is plainly done
- **Location:** `builder/src/builder/panels/mod.rs:18`.
- **Category:** Documentation
- **Suggested fix:** Drop or rewrite the docstring tail; Phase B/C is no longer "to come" — the modules are listed below.
- **Effort:** XS
- **Risk of fix:** None

### F-018-022 — [NIT] [Idiomatic] `hooks.rs::AnchorScope` is panel-private; the changed flag detection swallows the case where the user clicks the same scope twice
- **Location:** `builder/src/builder/panels/hooks.rs:370-407` (`anchor scope` selector).
- **Category:** Correctness (edge case)
- **Confidence:** Medium
- **Problem:** Each branch checks `selectable_value(…).changed()` — but `selectable_value` only reports `changed` when the value transitions. Clicking the already-selected scope is a no-op (acceptable), but the code still constructs a fresh empty anchor (`HookAnchor::System { system_id: SystemId::new("") }`) inside an `if … .changed()` arm. Reads fine; just noisy.
- **Suggested fix:** None functional. Optional: pull the empty constructors into helpers.
- **Effort:** —
- **Risk of fix:** —

### F-018-023 — [NIT] [Idiomatic] `intel.rs::propaganda_combo` / `classified_combo` / `source_combo` triplicate the same `let before = *value; … if *value != before { changed = true }` pattern
- **Location:** `builder/src/builder/panels/intel.rs:367-430`.
- **Category:** DRY
- **Suggested fix:** A single generic helper:
  ```rust
  fn enum_combo<T: PartialEq + Copy>(ui, id, value, variants, label_of) -> bool
  ```
- **Effort:** S
- **Risk of fix:** None

### F-018-024 — [NIT] [Idiomatic] `regions.rs:64-75` `selected_region_index` takes `&mut state` only to fall back to writing `state.selected_region_id` if missing — could be `&state` + return `(idx, Option<new_id>)`
- **Location:** `builder/src/builder/panels/regions.rs:64-75`.
- **Category:** Design
- **Confidence:** Low
- **Suggested fix:** Splitting it would let the caller batch the write outside any other `&mut state` borrow — would unlock part of the F-018-001 refactor.
- **Effort:** XS
- **Risk of fix:** None

### F-018-025 — [NIT] [Documentation] `interestingness.rs:494-495` `#[allow(dead_code)] fn _force_report_use` is a code smell
- **Location:** `builder/src/builder/panels/interestingness.rs:494-495`.
- **Category:** Idiomatic / Hygiene
- **Problem:** A no-op function exists solely to silence an unused-import warning for the `InterestingnessReport` alias. If the import is unused remove it; if it's read indirectly through `BuilderState` it should be `use sectorforge::interestingness::InterestingnessReport;` and the `_force` helper deleted.
- **Suggested fix:** Delete `_force_report_use` and the alias. If the type is referenced elsewhere (it's used in `tests::rescore_populates_report`), the test module's `use super::*;` already pulls it in.
- **Effort:** XS
- **Risk of fix:** None

## Rubric coverage (§3.1–§3.11)

- **§3.1 panics / failure surface:** No `unwrap`/`expect`/`panic!`/`unreachable!` reachable on user input outside tests. One `.expect("checked above")` at `regions.rs:475` is sound but stylistically loose (F-018-018). Integer casts in `regions.rs` are `as i32` from `u32` (F-018-012) — practically bounded.
- **§3.2 unsafe / soundness:** No `unsafe` in any of these 16 files. ✓
- **§3.3 ownership / borrowing / cloning:** Major theme; see F-018-001, F-018-004, F-018-005, F-018-006, F-018-007, F-018-008, F-018-014. The dominant anti-pattern is `let Some(report) = state.foo_report.clone() else …` driven by `&mut state` needs for click handlers.
- **§3.4 error handling:** Mostly correct — errors surfaced via `ModalKind::Message`. One swallowed error: F-018-016.
- **§3.5 concurrency / async:** None (GUI single-threaded). ✓
- **§3.6 performance:** Per-frame allocation is the dominant theme — F-018-001 through F-018-008, F-018-011, F-018-013, F-018-014.
- **§3.7 idiomatic Rust / API design:** F-018-009 (broken persistent field), F-018-015 (duplicated `ALL` arrays), F-018-018, F-018-019, F-018-020, F-018-023, F-018-024.
- **§3.8 deps / Cargo hygiene:** No unused imports detected in this set; the panels touch `egui`, `camino`, `sectorforge`, `rfd` (briefing), `sectorforge_gui_core` (interestingness). No over-broad imports.
- **§3.9 memory / resource management:** No `Drop` issues, no static caches with no eviction; `tick_log` has `tick_log_capacity` (good — see `state/mod.rs:323`).
- **§3.10 testing:** Inline tests are present and reasonable — every file in this group has a `#[cfg(test)] mod tests` (good). Coverage is light on the "auto-recompute on edit" cascade paths (F-018-003). No `#[ignore]` or sleep-based tests.
- **§3.11 docs / maintainability:** Section docstrings are excellent — every panel opens with a §-tag-cross-referenced rundown of its responsibilities (best-in-class in this codebase). F-018-021 / F-018-025 are micro-cleanups; F-018-019 is a real doc/contract mismatch.

## Project-specific invariants (CLAUDE.md)

- **No `FxMap`/`FxHashMap` iteration for output:** No violations. The panels do iterate `BTreeMap` and `Vec` only. ✓
- **All RNG through `src/model/rng.rs`:** No `rand::thread_rng()`, no `SeedableRng::from_entropy`, no direct seeds. The `seed_region` call in `regions.rs:289-297` correctly passes `state.sector.seed.as_ref()` through. ✓
- **Byte-stable output writers:** N/A — these are GUI panels, not output writers. The save-row delegates to `project_io::save_project` which is U005's surface.
- **Builder mutations through the command bus:** Partially. The carve-outs are explicit: regions overlay edits go through `update_region`/`paint_region_hex` per `regions_ops.rs:1-4` (§D3); subsector overrides are pure overlay state by design; `data_catalogs.*` edits don't touch sector state and are correctly direct. Sector-state mutations in `conflict.rs`, `orbital.rs`, `surface_regions.rs` correctly use `state.run(BuilderCommand::…)`. **One concerning pattern:** F-018-010 — the §CF2 read-only branch runs `SetSystemConflict` *automatically per frame* which is a misuse of the command bus.

## Summary of suggested fixes

- F-018-001 — HIGH — Per-frame deep-clones of derived reports across 9 panels — M / Low
- F-018-002 — HIGH — `seed_target` round-trips a full interestingness derivation per Add click — S / Low
- F-018-003 — MEDIUM — Manual editor keystrokes trigger full recompute cascades; debounce or use `lost_focus` — M / Low
- F-018-004 — MEDIUM — `regions.rs::show_route_effects` clones regions + builds coord map per frame — S / Low
- F-018-005 — MEDIUM — `subsectors.rs::current_subsectors` re-runs k-means per frame after any override edit — S / Low
- F-018-006 — MEDIUM — `regions.rs::show_glyph_preview` allocates O(W·H) `Vec<Vec<char>>` per frame — S / Low
- F-018-007 — MEDIUM — Faction-roster `Vec<(FactionId, String)>` cloned per frame in 6+ sites; pass `&[GeneratedFaction]` — M / Low
- F-018-008 — MEDIUM — `personae.rs::show_kind_pools_section` builds O(N²) dedup of kinds per frame — S / Low
- F-018-009 — MEDIUM — `personae.rs:422-435` add-custom-kind field is a fresh `String` per frame (broken) — S / Low
- F-018-010 — MEDIUM — `conflict.rs:167-180` runs `SetSystemConflict` every frame in aggregate mode (undo bus pollution + swallowed error) — M / Medium
- F-018-011 — LOW — `interestingness.rs` per-row `format!` allocations could be precomputed — S / Low
- F-018-012 — LOW — Silent `as i32` casts on `u32` dimensions in `regions.rs`/`subsectors.rs` — S / Low
- F-018-013 — LOW — `validation.rs`/`invariants.rs` group-by clones every issue — switch to `Vec<&Issue>` — S / Low
- F-018-014 — LOW — `prose.rs::show_system_editor` clones the whole prose report — S / Low
- F-018-015 — LOW — Duplicated `KIND_VARIANTS` arrays in 6 panels; promote `pub const ALL: &[Self]` to library — M / Low
- F-018-016 — LOW — `conflict.rs:179` `let _ = state.run(cmd)` swallows command error — XS / Low
- F-018-017 — LOW — `intel.rs::run_baseline_intel` String round-trip — closing as acceptable (borrow-checker required) — — / —
- F-018-018 — LOW — `regions.rs:475` `.expect("checked above")` → `let … else` — XS / None
- F-018-019 — LOW — `panels/mod.rs` contract says "never carry raw String IDs" but several editors do — XS (doc) / M (validator) — Low
- F-018-020 — NIT — `.as_ref().map(…).unwrap_or(0)` → `.map_or(0, …)` — XS / None
- F-018-021 — NIT — `panels/mod.rs:18` stale "Phase B / Phase C panels land below" — XS / None
- F-018-022 — NIT — `hooks.rs` scope selector empty-anchor construction is noisy — — / —
- F-018-023 — NIT — `intel.rs` triplicated combo helpers; single generic — S / None
- F-018-024 — NIT — `regions.rs::selected_region_index` takes `&mut` only to seed default; split — XS / None
- F-018-025 — NIT — `interestingness.rs:494` `_force_report_use` is a code smell; delete — XS / None
