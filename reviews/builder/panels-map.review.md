---
unit_id: U016
crate: sectorforge-builder
paths:
  - builder/src/builder/panels/map/mod.rs
  - builder/src/builder/panels/map/context_menu.rs
  - builder/src/builder/panels/map/interactions.rs
  - builder/src/builder/panels/map/dialogs.rs
  - builder/src/builder/panels/map/cache.rs
  - builder/src/builder/panels/system.rs
  - builder/src/builder/panels/system_map.rs
  - builder/src/builder/panels/world.rs
  - builder/src/builder/panels/routes.rs
loc_reviewed: 8305
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 9, medium: 17, low: 12, nit: 6 }
top_risks:
  - "Pervasive direct writes to GeneratedSystem / GeneratedWorld bypass the command bus and break undo/redo (F-016-001..006)"
  - "Per-frame .clone() of sector slices (routes vec, regions, factions, world vecs) inside hot ScrollArea/render paths (F-016-010..014)"
  - "AddWorld / DuplicateWorld / AddWorldAtOrbit dispatch a command then patch the result via direct iter_mut() — undo will leave dangling state (F-016-002)"
---

# Review: U016 — Builder panels group A (map + system + world + routes)

## Summary

The map panel and its right-click subsystem are well-structured: actions are
modeled as pure `SectorMenuAction` / `SystemMenuAction` enums whose `apply_*`
functions are unit-testable without an egui context, and the dismiss/staleness
logic is thoroughly covered. The dominant systemic problems are (1) the §R4
command-bus invariant being violated wholesale by `system.rs` and `world.rs` —
nearly every editable field in the right-hand inspector mutates
`state.sector.systems[idx]` directly and then sets `state.dirty = true` rather
than dispatching a `BuilderCommand`, so the in-app Undo button silently drops
those edits — and (2) per-frame heap traffic inside egui callbacks (full
`state.sector.routes.clone()` on every paint of the bulk-ops summary, full
`presences.clone()` / `claims.clone()` per world inspector frame, etc.). The
right-click menu code itself is largely correct; the bus violations cluster in
the legacy inspector panels.

## Findings

### F-016-001 — [HIGH] [Correctness/Invariant] `system.rs` inspector mutates `state.sector.systems[..]` directly, bypassing the command bus
- **Location:** `builder/src/builder/panels/system.rs:204-208, 222-225, 387-391, 423-425, 535-548, 587-606, 658-666, 769-775, 1321-1328, 1335-1341`
- **Category:** Project-specific invariant (CLAUDE.md §R4)
- **Confidence:** High
- **Blast radius:** Every Identity / Star / Tags / Notes / Factions / Control / Bulk edit on the SYSTEM tab. None of these are undoable.
- **Problem:** The CLAUDE.md command-bus rule says "Mutations in the builder always go through the command bus". `show_identity_section`, `show_star_section`, `show_tags_notes_section`, `show_worlds_link`, `show_control_section`, `apply_bulk_primary_faction`, and `apply_bulk_clear_factions` all push edits straight into `state.sector.systems[sys_idx].<field> = ...`. Example: lines 423-425 set `state.sector.systems[sys_idx].kind = kind_choice; state.dirty = true;` and lines 587-595 / 597-607 overwrite `tags` / `notes` the same way. None of these are wrapped in a `BuilderCommand`, so Ctrl-Z silently leaves the sector in the mutated state.
- **Why it matters:** §R4 is listed as a HARD rule. Undo/redo coverage is part of the public guarantee. Users editing the inspector get an inconsistent surface: a click on `DELETE` is undoable, but a kind change is not.
- **Evidence:** Read of file plus inventory of `state.sector.systems\[` writes (69 hits across `system.rs`/`world.rs`).
- **Suggested fix:** Introduce `BuilderCommand::SetSystemKind { id, before, after }`, `SetSystemName` (or reuse `RenameSystem`), `SetSystemTags`, `SetSystemNotes`, `SetSystemControlState`, and `SetSystemPrimaryFactions`. Each section captures `before` from the current slice, builds the command, and routes through `state.run(...)`. The two faction-bulk helpers (`apply_bulk_primary_faction`, `apply_bulk_clear_factions`) can wrap the per-system updates as a batch command or loop over single-id commands.
  ```rust
  // before
  state.sector.systems[sys_idx].kind = kind_choice;
  state.dirty = true;
  state.mark_validation_dirty();
  // after
  let before = state.sector.systems[sys_idx].kind;
  let cmd = BuilderCommand::SetSystemKind { id: id.clone(), before, after: kind_choice };
  if let Err(e) = state.run(cmd) { /* modal */ }
  ```
- **Effort:** L
- **Risk of fix:** Medium (touches the inspector contract for every panel that uses indexed writes — undo coverage tests should be added in lockstep).

### F-016-002 — [HIGH] [Correctness/Invariant] `AddWorld` + `DuplicateWorld` + `AddWorldAtOrbit` patch via `iter_mut()` after `state.run(...)`
- **Location:** `builder/src/builder/panels/map/context_menu.rs:321-329`, `builder/src/builder/panels/system_map.rs:300-315, 347-353`, `builder/src/builder/panels/system.rs:656-668`
- **Category:** Project-specific invariant (CLAUDE.md §R4) + Determinism
- **Confidence:** High
- **Blast radius:** Every "add world" code path from right-click menus and the SYSTEM tab. Orbit / copied-payload mutations are lost on undo.
- **Problem:** After dispatching `BuilderCommand::AddWorld { ... }`, the panel runs
  `if let Some(sys) = state.sector.systems.iter_mut().find(|s| s.id == id) { if let Some(w) = sys.worlds.iter_mut().find(|w| w.index == next_index) { w.orbit = next_orbit; } }`
  and then sets `state.dirty = true`. The patch step itself is not in the command log, so undo of the `AddWorld` rolls back the *world creation* but never sees the orbit/payload patch; redo will rebuild the world with the default orbit. `duplicate_world` (`system_map.rs:300-315`) clobbers `*w = source_payload.clone(); w.id = new_id; w.index = new_idx; w.name = new_name; w.orbit = source_payload.orbit;` outside the bus — so a duplicated world's customisation evaporates on undo/redo.
- **Why it matters:** This is the second hard-rule violation, and the existing comments (`system_map.rs:317-320`, `277-278`) explicitly acknowledge the bypass — but the §R4 rule does not have a documented exemption.
- **Evidence:** Read of files; comments in `system_map.rs` admit the bypass.
- **Suggested fix:** Add `BuilderCommand::SetWorldOrbit` is already wired (`SetWorldOrbit` exists in `command.rs:280-284`). Use a two-step *atomic* dispatch: first `AddWorld`, then immediately follow with `SetWorldOrbit` from the panel — both will be undoable. For `duplicate_world`, add a `BuilderCommand::DuplicateWorld { system, source }` that snapshots the source payload and writes a new world atomically (or, as a smaller fix, follow `AddWorld` with a new `SetWorldPayload { world, before: None, after: payload }` command).
- **Effort:** M
- **Risk of fix:** Low (additive commands; existing `AddWorld` callers stay valid).

### F-016-003 — [HIGH] [Correctness/Invariant] World inspector overwrites `world` / `tags` / `notes` / `factions` / `claims` directly
- **Location:** `builder/src/builder/panels/world.rs:197-211, 252-255, 262, 287-289, 296-301, 307-310, 332-335, 340-343, 349-352, 394-398, 440-445, 658-663, 766-782, 823-826, 897-906`
- **Category:** Project-specific invariant (CLAUDE.md §R4)
- **Confidence:** High
- **Blast radius:** Identity, classification, environment, society, features (W5), tags, notes, faction presence (§10), claims (§W7) — i.e. nearly the entire WORLD tab.
- **Problem:** Same pattern as F-016-001. E.g. `combo_enum::<WorldType>` reaches `&mut state.sector.systems[sys_idx].worlds[w_idx].world.world_type` and writes a fresh `Arc<str>` whenever the combo changes; `show_features_section` calls `.notable_features.push(Arc::from(...))` / `.remove(i)`; `show_factions_section` removes / pushes `WorldFactionPresence` records; `show_claims_section` removes / pushes `FactionClaim` records. All of them only set `state.dirty = true`. None of these can be undone.
- **Why it matters:** The WORLD tab is the primary mechanic for the §W1..§W7 spec. Every authored value is silently outside undo/redo.
- **Suggested fix:** Mirror the new commands suggested in F-016-001 for the world surface: `SetWorldField { world, before, after }` per editable field (or a coarser `SetWorldDto`). Faction presence and claims editors should dispatch `AddWorldPresence` / `RemoveWorldPresence` / `AddClaim` / `RemoveClaim` commands. The existing `RenameWorld` / `SetWorldOrbit` commands already model the pattern.
- **Effort:** L
- **Risk of fix:** Medium.

### F-016-004 — [HIGH] [Correctness/Invariant] Routes inspector mutates per-route fields outside the bus, then bulk-replaces on detection
- **Location:** `builder/src/builder/panels/routes.rs:142-218, 222-241, 243-301`
- **Category:** Project-specific invariant
- **Confidence:** Medium
- **Blast radius:** Single-route edits coming from the inspector.
- **Problem:** `show_route_inspector` clones a `GeneratedRoute` into `draft`, lets every sub-editor mutate it freely (`show_tags_editor` pushes a literal `Arc::from("tag")` on every "Add tag" click, `show_controls_editor` re-derives controls in place, etc.), then at line 215-219 does `if draft != original { replace_route_at(state, idx, draft); }`. `replace_route_at` routes through `ReplaceRoutes` so each click does *eventually* hit the bus — but the implementation pushes a `ReplaceRoutes` with **`before: Vec::new()`** (see also F-016-008 below). The `before` field is what undo reads, so undoing a route edit will not restore the prior state and the panel test never asserts revert.
- **Why it matters:** The route inspector looks bus-compliant but its commands carry empty `before` payloads — every edit silently invalidates undo for the routes vector.
- **Evidence:** `routes.rs:413-419`, `503-528`, `902-919` all build `BuilderCommand::ReplaceRoutes { before: Vec::new(), after: routes }`.
- **Suggested fix:** `replace_routes` should pass `before: state.sector.routes.to_vec()` (the snapshot the command is replacing). The bus's `revert` for `ReplaceRoutes` should restore `before`; if it currently doesn't, fix the apply/revert pair. Add a tiny test: `route_inspector_edit_undoes_via_replace_routes_before` that exercises the full path.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-005 — [HIGH] [Correctness/Invariant] Hidden-routes "Build" / "Remove" passes `before: Vec::new()` to `ReplaceRoutes`
- **Location:** `builder/src/builder/panels/routes.rs:882-921, 962-969`
- **Category:** Project-specific invariant
- **Confidence:** High
- **Blast radius:** "Hidden routes" sub-panel and the "Run connector now" button.
- **Problem:** Same root cause as F-016-004 — all three callers of `replace_routes` (single-edit, bulk, hidden-routes, ensure-connected) feed `before: Vec::new()` into `ReplaceRoutes`, so the entire routes vector is unrecoverable through Undo after any of these clicks.
- **Suggested fix:** As in F-016-004; one fix covers both. Centralise the snapshot in `replace_routes`.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-006 — [HIGH] [Correctness/Invariant] `set_system_control_state` is the bus-skipping bulk path from the right-click menu
- **Location:** `builder/src/builder/panels/system.rs:1346-1356, 769-775`
- **Category:** Project-specific invariant
- **Confidence:** High
- **Blast radius:** Multi-selection right-click "Flip control state" + per-system control combo.
- **Problem:** Both the single-system `show_control_section` and the bulk-ops `apply_bulk_control_state` call `state.sector.set_system_control_state(&id, value)` — a sector mutation method that mutates `GeneratedSector` directly. `state.dirty = true` is set but no `BuilderCommand` is dispatched. The MAP tab right-click menu's `MultiFlipControlState` (`context_menu.rs:423-425`) routes here, so the right-click "flip control state" ribbon is also non-undoable.
- **Suggested fix:** Introduce `BuilderCommand::SetSystemControlState { id, before: Option<SystemState>, after: Option<SystemState> }` and route both panel paths through it. The existing apply/revert plumbing in `command.rs` should pair cleanly with the existing `sector.set_system_control_state` helper.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-007 — [HIGH] [Performance / Allocation] Hot-path full clone of `state.sector.routes` on every paint of bulk ops & ensure-connected
- **Location:** `builder/src/builder/panels/routes.rs:401, 503, 902, 910, 957-961, 963-969`
- **Category:** Performance (§3.6 — per-frame hot path)
- **Confidence:** High
- **Blast radius:** ROUTES tab — every frame while the tab is open.
- **Problem:** Multiple per-frame call sites clone the entire routes vector even before the user clicks anything. `show_ensure_connected` does:
  ```rust
  let components = route_component_count(&state.sector, &state.sector.routes);
  let added = ensure_connected_routes(state, state.sector.routes.clone()).1;
  ```
  That second line runs every frame the tab is visible, cloning `Vec<GeneratedRoute>` and running a union-find on every system pair just to display the "would add N bridge route(s)" label. With ~hundreds of routes, this is a steady GC-like churn on the main thread.
- **Why it matters:** §3.6: "Hot-path allocation; per-frame heap traffic in GUI render paths" — flag at HIGH because the panel is the default tab for route editing and the work is purely preview.
- **Suggested fix:** Cache the `(components, would_add)` summary on `BuilderState` (or a derivations cache), keyed by a digest of `sector.routes` + `sector.systems`. Recompute only when the routes/systems slice digest changes — the same pattern as `map/cache.rs`. Until that lands, at minimum gate the preview behind `if ui.is_visible() { ... }` so collapsed sections don't pay the cost.
  ```rust
  // before
  let added = ensure_connected_routes(state, state.sector.routes.clone()).1;
  // after (sketch)
  let added = state.derivations.ensure_connected_preview(state);
  ```
- **Effort:** M
- **Risk of fix:** Low.

### F-016-008 — [HIGH] [Performance / Allocation] World inspector clones large vectors per frame
- **Location:** `builder/src/builder/panels/world.rs:629, 794, 948`
- **Category:** Performance (§3.6)
- **Confidence:** High
- **Blast radius:** WORLD tab — every frame while the tab is open.
- **Problem:** `show_factions_section` clones the entire `Vec<WorldFactionPresence>` per frame just to read it (`let presences = state.sector.systems[sys_idx].worlds[w_idx].factions.clone();`). `show_claims_section` does the same with `claims.clone()`. `show_control_section` clones the entire `SystemControlSummary`. None of these need to be cloned to render — they only need a borrow.
- **Why it matters:** Each clone allocates Vec + per-row `Arc<str>` reference bumps. With many faction rows, this multiplies. The clones survive into the closure but the closure never mutates them; they're used for `iter()` and `.len()`.
- **Suggested fix:** Replace the clone-then-iterate pattern with a `for ... in &state.sector.systems[sys_idx].worlds[w_idx].factions` loop, holding indices for deferred mutation. The `remove_idx: Option<usize>` pattern already used here works fine with borrowed iteration.
  ```rust
  // before
  let presences = state.sector.systems[sys_idx].worlds[w_idx].factions.clone();
  for (i, p) in presences.iter().enumerate() { ... }
  // after
  let len = state.sector.systems[sys_idx].worlds[w_idx].factions.len();
  for i in 0..len {
      let p = &state.sector.systems[sys_idx].worlds[w_idx].factions[i];
      // ...
  }
  ```
- **Effort:** S
- **Risk of fix:** Low.

### F-016-009 — [HIGH] [Performance / Allocation] System inspector clones tag/note arc-string lists + spawns `feature_weights_for_world` per frame
- **Location:** `builder/src/builder/panels/system.rs:566-577, 730`, `builder/src/builder/panels/world.rs:370, 459-499`
- **Category:** Performance (§3.6)
- **Confidence:** High
- **Blast radius:** Every frame on SYSTEM/WORLD tab.
- **Problem:**
  - `system.rs:566-577` formats `tags_src`/`notes_src` by allocating `Vec<String>` from `Arc<str>` and `.join(",")` every frame, even before the user clicks the field. With long tag/note vectors that's per-frame heap churn.
  - `world.rs:370` calls `feature_weights_for_world(state, sys_idx, w_idx)` *unconditionally* on every paint of the WORLD tab; that function (lines 459-499) re-synthesises the project input, rebuilds the world pool, and re-applies authored features — all on every frame. That's likely tens of milliseconds of allocation per frame depending on catalog size.
- **Why it matters:** This is exactly the "recompute inside loops / per-frame in GUI render paths" anti-pattern from §3.6.
- **Suggested fix:**
  - For the tags/notes display, only build the joined string when the buffer is empty (first reseed) or when the buffer key is missing — `persistent_singleline` already handles the persistent state; pass a closure that materialises the source string only when needed.
  - For `feature_weights_for_world`, lazy-derive into a `BuilderState::feature_weight_cache` keyed by (world digest, world type, star colour). Or at minimum hoist into the collapsing-header closure so collapsed headers don't pay the cost (currently the call is outside the `show` body).
- **Effort:** M
- **Risk of fix:** Low.

### F-016-010 — [HIGH] [Correctness] Direct writes to `state.sector_context_menu` / `state.system_context_menu` / `state.scroll_target` from the panels
- **Location:** `builder/src/builder/panels/map/mod.rs:44-51, 87-90`, `builder/src/builder/panels/map/interactions.rs:175, 178, 184, 197, 211, 216, 268`, `builder/src/builder/panels/system_map.rs:419`, `builder/src/builder/panels/system.rs:87-91, 287`
- **Category:** Project-specific invariant (§R4 — narrower interpretation)
- **Confidence:** Medium
- **Blast radius:** Every transient UI state field touched outside the command bus.
- **Problem:** These mutations are *transient UI state* (drag in progress, rect select, scroll target, context menu open/dismiss) and are intentionally not undoable — that's correct. **However**, they live on `BuilderState` and are not visibly distinguished from sector mutations. The CLAUDE.md rule literally says "Mutations in the builder always go through the command bus. Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel". A reviewer following the rule literally would block these. They are also not snapshotted by `SessionFile::from_state` (already tested — see `mod.rs:826-833, 1325-1337`).
- **Why it matters:** The rule needs a documented carve-out, or the panels need to call a typed setter. Right now the rule is honoured in spirit (these are not undoable mutations) but violated in letter, which makes mechanical enforcement of the rule (e.g. a lint or grep gate) impossible.
- **Suggested fix:** Add a documented exception list to CLAUDE.md (`partial_regen_anchor`, `sector_context_menu`, `system_context_menu`, `drag_system`, `rect_select`, `pending_*`, `scroll_target`, UI-only `map_tool`, `selected_*_id`, `system_layout`, `system_view_side`, `hex_size`). Alternatively, move every "transient UI" field onto a `BuilderState::ui: UiState` sub-struct so writes to the sector slice are visually clearly distinguished.
- **Effort:** S (documentation) or M (refactor sub-struct).
- **Risk of fix:** Low.

### F-016-011 — [MEDIUM] [Correctness] `apply_bulk_reseed` proceeds past failures silently, only after the first error
- **Location:** `builder/src/builder/panels/system.rs:1360-1378`
- **Category:** Error handling (§3.4)
- **Confidence:** High
- **Blast radius:** Multi-system reseed from the right-click menu and the §S4 bulk-ops pane.
- **Problem:** The loop calls `state.generate_system_here(coord, index, None)` on each target and `return`s on the first error after setting a modal. The systems that succeeded before the failure remain reseeded (state is left half-applied), and the modal message tells the user nothing about which system failed mid-batch.
- **Suggested fix:** Collect a `Vec<(SystemId, BuilderError)>` of failures, then either show a combined modal naming each failure or wrap the batch as a single command that restores prior state on failure. At minimum log the failing id.
  ```rust
  let mut errors = Vec::new();
  for (id, coord, index) in targets {
      if let Err(e) = state.generate_system_here(coord, index, None) {
          errors.push((id, e));
      }
  }
  if !errors.is_empty() { state.modal = Some(ModalKind::Message(format!("{} reseed(s) failed: ...", errors.len()))); }
  ```
- **Effort:** S
- **Risk of fix:** Low.

### F-016-012 — [MEDIUM] [Performance] `system.rs:show` clones full system Vec snapshots via `to_vec()` / repeated lookups every frame
- **Location:** `builder/src/builder/panels/system.rs:730, 696-710, 617-633`
- **Category:** Performance (§3.6)
- **Blast radius:** Per-frame allocation on SYSTEM tab.
- **Problem:**
  - `show_factions_section` (730): `let primary: Vec<_> = state.sector.systems[sys_idx].primary_factions.to_vec();` — clones the whole vec just to iterate it for rendering.
  - `show_routes_section` (697-710): clones a fully-derived tuple list including each `RouteId`/`SystemId` per route that touches the system, every frame.
  - `show_worlds_link` (619-624): clones an `(Id, String)` vec just to display a list.
  Each is small in isolation; together (running every frame for the inspector) they add up.
- **Suggested fix:** Iterate by reference; only clone the few values the closure later mutates. Where a deferred mutation forces a clone, capture only the index into the sector slice and re-read it in the mutation branch.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-013 — [MEDIUM] [Performance] World feature-add scroll-area builds full BTreeSet of existing features per frame
- **Location:** `builder/src/builder/panels/world.rs:412-447`
- **Category:** Performance (§3.6)
- **Blast radius:** WORLD tab features section, every frame.
- **Problem:** `let already: std::collections::BTreeSet<String> = state.sector.systems[sys_idx].worlds[w_idx].world.notable_features.iter().map(|s| s.to_string()).collect();` — allocates a BTreeSet + N owned Strings every frame just to filter the candidate list. The set is then queried via `already.contains(&key)` in the inner loop over `NotableFeature::VARIANTS` (≥90 entries).
- **Suggested fix:** Build the lookup once *per frame* but reuse `&str` keys (no `String` per element) by collecting `BTreeSet<&str>` from `notable_features.iter().map(|s| s.as_ref())`. Even better, hoist into a panel-scoped scratch field that is cleared at frame start. Or, since the feature list per world is typically tiny (< 20), use linear `.iter().any(|f| f.as_ref() == key)` and drop the BTreeSet entirely.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-014 — [MEDIUM] [Performance] `route_picker` allocates one `String` per route per frame for combo labels
- **Location:** `builder/src/builder/panels/routes.rs:92-103`
- **Category:** Performance (§3.6)
- **Blast radius:** Every frame on ROUTES tab.
- **Problem:** The route picker iterates `for route in &state.sector.routes { let text = format!("{}  {} -> {}  d={}", ...); }` even when the dropdown is collapsed. With hundreds of routes the formatting churn is noticeable.
- **Suggested fix:** egui's `ComboBox::show_ui` closure only executes when the combo is open — move the inner `for` loop inside `show_ui` (the file *does* put it inside, so the `format!` is only paid when the dropdown is open: re-verify, but the call at line 89-104 already lives inside `show_ui`). The bigger waste is the `selected_text(label)` allocation per frame; cache the label on the selected route id (rebuild only when `selected_route_id` changes).
- **Effort:** S
- **Risk of fix:** Low.

### F-016-015 — [MEDIUM] [Correctness] `MultiDeleteAllConfirmed` clears selection even on partial failure
- **Location:** `builder/src/builder/panels/map/context_menu.rs:402-419`
- **Category:** Error handling
- **Problem:** The loop calls `state.run(BuilderCommand::RemoveSystem { ... })` per id; on first error it sets a modal and `break`s, but then **unconditionally** runs `state.selected_systems.clear(); state.selected_system_id = None;`. So a partial failure (e.g. a system that other state refused to remove) leaves the user with their selection wiped even though only some systems were removed. They cannot retry without re-selecting.
- **Suggested fix:** Move the clear into the success branch (after the full loop completes without errors), or recompute selection to "ids that still exist".
- **Effort:** S
- **Risk of fix:** Low.

### F-016-016 — [MEDIUM] [Correctness] `handle_drag_drop` bounds check casts `i32 -> u32` without overflow guard
- **Location:** `builder/src/builder/panels/map/interactions.rs:404-408`, `builder/src/builder/panels/system.rs:433-436`
- **Category:** Idiomatic / panic surface (§3.7 / §3.1)
- **Problem:** `(coord.q as u32) >= state.sector.width` — `as u32` on a negative `i32` is well-defined but wraps to a huge value, which then accidentally satisfies the `>= width` guard. That's *coincidentally* the right behaviour here, but only because of the wraparound. If `coord.q < 0` had to be rejected for a different reason later, this is a footgun.
- **Suggested fix:** Use `u32::try_from(coord.q).map_or(false, |q| q < sector.width)`. The intent is clearer and the cast is no longer load-bearing.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-017 — [MEDIUM] [Performance] `sector_view_digest` rebuilds a fresh `serde::Serialize` Slice and hashes it every frame
- **Location:** `builder/src/builder/panels/map/cache.rs:16-41, 43-98`
- **Category:** Performance (§3.6)
- **Problem:** `refresh_map_cache` is called from `interactions::show_hex_map` on every frame. The digest is recomputed even when nothing changed: it allocates `systems: Vec<(&str, i32, i32)>`, `routes: Vec<(&str, &str, &str)>`, `regions: Vec<(&str, Vec<(i32, i32)>)>`, `sub_sys`, `sub_cap`, then serialises through `digest_input`. That allocation graph is paid every frame. The early-out compares the freshly computed digest to the cached one only after paying the cost of building it.
- **Suggested fix:** Track a sector-mutation generation counter on `BuilderState` (bumped by every successful `state.run(...)` and every region/subsector overrides update). The cache key compares counters first; only rehash when the counter changes. Alternatively maintain a per-mutation digest invalidation flag.
- **Effort:** M
- **Risk of fix:** Low.

### F-016-018 — [MEDIUM] [Correctness] `system_combo` for routes lets the user pick the same system on both endpoints (`from == to`)
- **Location:** `builder/src/builder/panels/routes.rs:148-154, 312-320, 374-379`
- **Category:** Correctness / data integrity
- **Problem:** `system_combo` writes the chosen `SystemId` directly into `draft.from_system_id` / `draft.to_system_id`; nothing rejects `from == to`. `canonicalize_route_endpoints` (`374-379`) then derives `route.id = ids::route_id(&from, &to)` even when both are equal, producing a degenerate route. (`add_route_between` in `interactions.rs:370-378` rejects this, but the inspector path does not.)
- **Suggested fix:** After the endpoint combos, before the canonicalize step, check `if draft.from_system_id == draft.to_system_id { /* show modal + revert draft */ }`. Or filter the combo entries by `id != draft.<other>`.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-019 — [MEDIUM] [Correctness] `route_inspector` ID field accepts arbitrary user text and overwrites the canonical `RouteId`
- **Location:** `builder/src/builder/panels/routes.rs:142-146, 374-379`
- **Category:** Determinism / data integrity
- **Problem:** The user can type a fresh string into the "id" field; `draft.id = RouteId::new(id_buf.trim())` writes it straight to the route. Two ticks later, if the user edits an endpoint, `canonicalize_route_endpoints` overwrites the id back to the deterministic value derived from the endpoints. So the field is silently lossy — the user thinks they renamed the route, but the next endpoint change drops the rename. The same lossy behaviour also breaks any external link (e.g. `selected_route_id`) that captured the typed id.
- **Suggested fix:** Make the field read-only (display `monospace`), or hide it behind an explicit "Custom id" toggle that disables the canonicalise step. RouteId is structurally `ids::route_id(from, to)` everywhere else in the codebase; let the panel match.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-020 — [MEDIUM] [Performance] `lifeline_route_ids` / `stranded_system_ids` / `build_overlay_cells` / `compute(heatmap)` run every frame
- **Location:** `builder/src/builder/panels/map/interactions.rs:65-91, 124-132`
- **Category:** Performance (§3.6)
- **Problem:** Each of `crate::builder::panels::economy::lifeline_route_ids(state)`, `crate::builder::panels::economy::stranded_system_ids(state)`, `crate::builder::panels::control::build_overlay_cells(...)`, and `sectorforge_gui_core::heatmap::compute(...)` runs in `show_hex_map`'s body. If their bodies do non-trivial work (graph traversals, allocation of cell vectors), every map frame pays the cost — even though their inputs only change on sector mutation.
- **Why it matters:** The MAP tab is the default tab. Every frame is paying for overlays that change only on edit.
- **Suggested fix:** Add an `OverlayCache` keyed by the same kind of mutation-generation counter from F-016-017 and store the four overlay artefacts there. Recompute only when the generation counter or the relevant `state.map_heatmap_mode` / `state.control_overlay` changes.
- **Effort:** M
- **Risk of fix:** Low.

### F-016-021 — [MEDIUM] [Correctness] `Toggle pin` from menu inserts on an already-pinned id (no double-toggle guard needed but mutation isn't atomic-conditional)
- **Location:** `builder/src/builder/panels/map/context_menu.rs:350-356`
- **Category:** Correctness (minor)
- **Problem:** The pattern `if contains { remove } else { insert }` is fine, but the panel does it via a contains() → branch → mutation sequence on a BTreeSet; concurrent `apply_sector_menu_action` calls (none in this single-threaded model, but still) would race. More importantly the analogous bulk paths `MultiPinAll` / `MultiUnpinAll` do not honour the toggle semantics — they unconditionally insert/remove regardless of the prior state.
- **Suggested fix:** None functionally required, but document the semantics: "MultiPinAll is idempotent insert; MultiUnpinAll is idempotent remove; TogglePin is per-id flip." If pin-state should be undoable, add `BuilderCommand::SetPinned { id, before, after }`.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-022 — [MEDIUM] [Performance] `show_world_picker` allocates labels for every world in every system per frame
- **Location:** `builder/src/builder/panels/world.rs:108-122`
- **Category:** Performance (§3.6)
- **Problem:** The inner double loop `for sys in &state.sector.systems { for w in &sys.worlds { let label = format!("{} — {} ({})", w.id, w.name, sys.name); ... } }` runs whenever the combo is open. Inside `show_ui` it's only paid on open, so this is mild — but `selected_text(label)` (line 109) is paid every frame. With dozens of worlds across many systems, the open-combo path is also non-trivial.
- **Suggested fix:** Cache `selected_text` per `selected_world_id` change. Inside `show_ui`, replace per-entry `format!` with `RichText::new` + monospace where possible to skip the allocation.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-023 — [MEDIUM] [Correctness] `state.sector_context_menu.as_mut().unwrap()` in tests assumes a runtime invariant (not test-only)
- **Location:** `builder/src/builder/panels/map/context_menu.rs:768-770, 777-780`
- **Category:** Panic surface (§3.1)
- **Problem:** Inside the render closure for "Confirm DELETE ALL?", `state.sector_context_menu.as_mut().unwrap_or(...)` is correct, but the equivalent flip lines (768-770, 777-780) call `state.sector_context_menu.as_mut()` without unwrap and silently no-op if the menu has been dismissed mid-frame. That's correct behaviour but easy to overlook — and a future refactor could reintroduce an unwrap. Add a `let Some(menu) = state.sector_context_menu.as_mut() else { return; };` guard at the top of the multi-selection render to make the assumption explicit.
- **Suggested fix:** Above.
- **Effort:** XS
- **Risk of fix:** Low.

### F-016-024 — [MEDIUM] [Performance] System map embedded view rebuilds `feature_weights` + recomputes selection override per frame
- **Location:** `builder/src/builder/panels/system.rs:240-258`, `builder/src/builder/panels/system_map.rs:362-377`
- **Category:** Performance
- **Problem:** `menu_selection_override` is called every frame, dereferences `state.system_context_menu` / `state.sector.systems.get(sys_idx)` / `sys.worlds.iter().find(...)`. The find is O(N worlds) every frame even when no menu is open. Trivial when N is small, but the call is also wrapped in `unwrap_or_else(|| match state.selected_world_id ... )` which itself does another O(N) find. Hoist behind the `state.system_context_menu.is_some()` short-circuit and cache the index.
- **Suggested fix:** Short-circuit on `system_context_menu.is_none()` first; cache the selection per `(selected_world_id, system_context_menu.as_ref().map(|m| &m.target))` digest.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-025 — [MEDIUM] [Correctness] `show_chronicle_section` collects rows by snapshotting full strings instead of borrowing
- **Location:** `builder/src/builder/panels/world.rs:990-1006`
- **Category:** Performance + idiom
- **Problem:** `let rows: Vec<(String, String, String, String, bool)> = { ... events.iter().map(|e| (e.id.clone(), e.date.clone(), kind_label(e.kind).to_string(), e.narrative.clone(), e.manual)).collect() };` allocates 4 owned strings per chronicle event per frame. Comment says "Snapshot rows up-front so the closure body can mutate `state` freely." That's true, but the borrow only blocks `state.focus_entity` once at line 1036 — it would be cheaper to capture indices and re-borrow per render.
- **Suggested fix:** Use a `Vec<usize>` of event indices, then re-borrow per render iteration. Or move the `focus_entity` outside the rendering loop (already done via `jump_to`) and keep the borrow of `events: &[Event]`.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-026 — [LOW] [Idiom] Tag editor inserts a literal placeholder string `"tag"` on add
- **Location:** `builder/src/builder/panels/routes.rs:238-241`
- **Category:** Idiomatic / UX
- **Problem:** `if ui.button("Add tag").clicked() { route.tags.push(Arc::from("tag")); }` — adds a tag literally named "tag". Two clicks → two routes carrying duplicate `"tag"`. The single-line text editor at line 226-228 will let the user rename it, but the experience is awkward.
- **Suggested fix:** Pop a small inline `text_edit_singleline` next to the button or generate a unique placeholder (`format!("tag-{}", route.tags.len() + 1)`).
- **Effort:** S
- **Risk of fix:** Low.

### F-016-027 — [LOW] [Idiom] `state.scroll_target.map_or(false, |t| t == ...)` should be `.is_some_and(|t| ...)`
- **Location:** `builder/src/builder/panels/system.rs:84-87`
- **Category:** Idiomatic Rust
- **Problem:** `state.scroll_target.map_or(false, |t| t == SYS_STAR_GRID_ANCHOR)` is the canonical "use `is_some_and`" anti-pattern.
- **Suggested fix:** `state.scroll_target.is_some_and(|t| t == SYS_STAR_GRID_ANCHOR)`.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-028 — [LOW] [Idiom] `egui::Frame::none()` deprecated in egui 0.29+
- **Location:** `builder/src/builder/panels/world.rs:799`
- **Category:** Idiomatic / forward-compat
- **Problem:** `egui::Frame::none()` was renamed to `egui::Frame::NONE` (or `Frame::new()`) in recent egui; using the deprecated form will emit warnings under newer versions.
- **Suggested fix:** Verify against the workspace egui pin; if deprecated, switch.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-029 — [LOW] [Idiom] Loop body `for id in state.selected_systems.iter().cloned().collect::<Vec<_>>()` is the "clone-into-vec-to-avoid-borrow" anti-pattern
- **Location:** `builder/src/builder/panels/system.rs:1202, 1207`, `builder/src/builder/panels/map/context_menu.rs:391, 397, 403`
- **Category:** Performance / Idiomatic
- **Problem:** The pattern allocates a fresh Vec on every click to satisfy the borrow checker. For pin/unpin/delete operations that touch dozens of ids, this is fine but verbose. A clearer pattern is `let ids = state.selected_systems.iter().cloned().collect::<Vec<_>>();` once before the loop, but better yet: since `BTreeSet<SystemId>` is `Clone`, capture a snapshot once: `let ids: Vec<_> = state.selected_systems.iter().cloned().collect();`. (Several call sites already do this; the LOW finding is to apply the pattern consistently.)
- **Suggested fix:** Hoist the `Vec` once per action.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-030 — [LOW] [Idiom] `RouteId::new(id_buf.trim())` accepts empty / whitespace-only ids
- **Location:** `builder/src/builder/panels/routes.rs:142-145`
- **Category:** Correctness (low — overshadowed by F-016-019)
- **Problem:** No length / charset validation on the typed id. Combined with F-016-019, a user can type `""` and corrupt the route.
- **Suggested fix:** Reject empty / non-ASCII inputs before writing.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-031 — [LOW] [Documentation] `apply_sector_menu_action` has no docs on its return contract beyond a single sentence
- **Location:** `builder/src/builder/panels/map/context_menu.rs:232-234`
- **Category:** Documentation
- **Problem:** The doc-comment says "Returns the menu's close intent (always `true`: every action dismisses the menu)" — but several branches `return true;` early (lines 334, 449-451, 469-472, 497-499) to *suppress* a no-op command. The semantics are correct but the doc statement is misleading.
- **Suggested fix:** Tighten the doc: "Returns `true`: every action dismisses the menu, including no-op early returns where the menu still closes without dispatch."
- **Effort:** XS
- **Risk of fix:** None.

### F-016-032 — [LOW] [Idiom] Duplicate constant `SYS_STAR_GRID_ANCHOR` in two files
- **Location:** `builder/src/builder/panels/system.rs:31`, `builder/src/builder/panels/system_map.rs:58`
- **Category:** Maintainability (DRY)
- **Problem:** The constant is duplicated with a comment "must match" between two modules. A future renamer who edits one will silently break in-system scroll-to-star.
- **Suggested fix:** Move to a shared module (e.g. `builder/src/builder/panels/anchors.rs`) and import from both call sites.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-033 — [LOW] [Documentation] Region-erase right-click path silently no-ops when no region owns the hex
- **Location:** `builder/src/builder/panels/map/context_menu.rs:249-261`
- **Category:** UX
- **Problem:** `EraseRegion { coord }` looks for any region containing the hex; if none, the function silently returns without telling the user. The right-click menu hides the item with `add_enabled(erase_enabled, ...)` so this is defensive coverage — but a panel/event refactor could expose the silent path.
- **Suggested fix:** Add a debug-assert or modal in the silent branch.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-034 — [LOW] [Performance] `feature_weights_for_world` uses `BTreeMap<String, f64>` keyed by `format!("{:?}", feature)`
- **Location:** `builder/src/builder/panels/world.rs:480-498`
- **Category:** Performance / Idiom
- **Problem:** Keying by `format!("{:?}", wf.feature)` allocates a fresh String for every weight, both in the cache builder and the consumer (line 434 `weights.get(key.as_str())` where `key = format!("{v:?}", v)`). Use the enum itself as the key (`BTreeMap<NotableFeature, f64>`) — `NotableFeature` already derives `Ord`/`PartialEq` (verify).
- **Suggested fix:** Above.
- **Effort:** S
- **Risk of fix:** Low.

### F-016-035 — [LOW] [Idiom] `state.modal = Some(ModalKind::Message(format!(...)))` repeated ~30 times across the files
- **Location:** Across all reviewed files (system.rs:407, 460, 654, 771, 997, 1018, 1144, 1176, 1307, 1350, 1374; map/*; routes.rs:110, 418, 526, 898, 914, 965)
- **Category:** Idiomatic / maintainability
- **Problem:** The pattern is repeated dozens of times. A typed helper `state.modal_error(msg: impl Into<String>)` or `state.notify_error(format!(...))` would clean this up; could also feed a future toast/timeline system.
- **Suggested fix:** Introduce a small helper on `BuilderState`.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-036 — [LOW] [Idiom] `claim_chip_colours` uses raw RGB constants — not named in a shared palette
- **Location:** `builder/src/builder/panels/world.rs:926-940`
- **Category:** Idiom / maintainability
- **Problem:** RGB triplets buried in a `match` arm. `gui-core` already has a palette module — these chip colours should be hoisted there for theming consistency.
- **Suggested fix:** Move to `gui-core::palette` or a `world_chips.rs`.
- **Effort:** S
- **Risk of fix:** None.

### F-016-037 — [LOW] [Performance] `show_route_picker` rebuilds combo `String` `selected_text` even when no route selected
- **Location:** `builder/src/builder/panels/routes.rs:84-90`
- **Category:** Performance
- **Problem:** `let label = current.as_ref().map(ToString::to_string).unwrap_or_else(|| "(none)".into());` allocates per frame even when unchanged.
- **Suggested fix:** Cache the label string on selection change or use `&str` literals where possible.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-038 — [NIT] [Style] `kind` repeated in `format!("{:?}", working.necron_phase)` (and many other `{:?}` formats)
- **Location:** `builder/src/builder/panels/system.rs:881-892, 896-911, 919-933, 937-949` etc.
- **Category:** Style / NIT
- **Problem:** Debug formatting on enum values is fine for prototyping but produces less polished UI text (`NecronPhase::Awakening` → "Awakening" is acceptable, but locale/space normalisation isn't applied).
- **Suggested fix:** Add a `Display` impl per enum, or a `label()` method in the spec module; switch the combo labels.
- **Effort:** S (per enum).
- **Risk of fix:** None.

### F-016-039 — [NIT] [Style] Inconsistent capitalisation in modal/button labels ("Place"/"Cancel" vs "PLACE SYSTEM HERE…" vs "✕ DELETE ALL")
- **Location:** `builder/src/builder/panels/map/dialogs.rs:27-33, 71-77`, `builder/src/builder/panels/map/context_menu.rs:530-580, 583-718`
- **Category:** Style / consistency
- **Problem:** Modal dialogs use sentence case; right-click menus use uppercase. The convention isn't documented; a contributor will not know which to pick.
- **Suggested fix:** Document the convention in the panels module header or `docs/STYLE.md`.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-040 — [NIT] [Idiom] `let _ = coord;` to silence unused warning instead of `#[allow(unused_variables)]`
- **Location:** `builder/src/builder/panels/map/context_menu.rs:607`, `builder/src/builder/panels/system_map.rs:550`
- **Category:** Style
- **Problem:** Manual `let _ = ...` to silence unused. The `coord`/`orbit` args are part of the function's API contract; suppressing the warning at the call site is misleading.
- **Suggested fix:** Rename to `_coord` / `_orbit` in the signature for that branch, or restructure to take an option.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-041 — [NIT] [Style] `match (right, bottom)` in `menu_anchor_pivot` reads as 2×2 truth table — extract to a helper or comment
- **Location:** `builder/src/builder/panels/map/context_menu.rs:1063-1071`
- **Category:** Documentation
- **Problem:** The match arm semantics are obvious to anyone familiar with `Align2`, but a brief comment ("(right, bottom) tuple → pivot quadrant") would help reviewers skimming the file.
- **Suggested fix:** One-line comment.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-042 — [NIT] [Idiom] `egui::Vec2::new(0.0, 0.0)` and `egui::vec2(0.0, 0.0)` are mixed
- **Location:** `builder/src/builder/panels/map/dialogs.rs:22, 67`, `builder/src/builder/panels/system_map.rs:710`
- **Category:** Style
- **Problem:** Both forms appear within the unit. Pick one (`egui::Vec2::ZERO` is the cleanest).
- **Suggested fix:** Replace with `egui::Vec2::ZERO`.
- **Effort:** XS
- **Risk of fix:** None.

### F-016-043 — [NIT] [Style] Per-arm `format!("{v:?}")` inside combo loops
- **Location:** `builder/src/builder/panels/system.rs:884-891, 898-910, 921-933, 940-948`, `builder/src/builder/panels/routes.rs:482-486` etc.
- **Category:** Style + minor perf
- **Problem:** Closing on the `selectable_value`'s third argument allocates a String per option per frame the menu is open. With ~10 enum variants × tens of combos that's notable.
- **Suggested fix:** Add a `&'static str` `label()` method on each enum once; reuse.
- **Effort:** S
- **Risk of fix:** None.

## §3 Rubric coverage

- **3.1 Panics & failure surface** — every `.unwrap()`/`.expect()` in this unit is in `#[cfg(test)]` modules. No panic-on-input findings.
- **3.2 unsafe** — none present. No findings.
- **3.3 Ownership / cloning** — see F-016-008, F-016-009, F-016-012, F-016-013, F-016-014, F-016-022, F-016-025, F-016-029, F-016-034.
- **3.4 Error handling** — see F-016-011, F-016-015. Most error paths route to `ModalKind::Message` via `format!`; consistent (F-016-035 cleans up the boilerplate).
- **3.5 Concurrency / async** — N/A. No findings.
- **3.6 Performance** — large bucket: F-016-007 through F-016-009, F-016-013, F-016-014, F-016-017, F-016-020, F-016-022, F-016-024, F-016-025, F-016-037, F-016-043. Per-frame allocation is the dominant theme.
- **3.7 Idiomatic Rust** — F-016-016, F-016-026, F-016-027, F-016-028, F-016-029, F-016-030, F-016-038, F-016-040, F-016-041, F-016-042, F-016-043.
- **3.8 Dependencies** — no unused/over-broad imports flagged. No findings.
- **3.9 Memory & resource management** — no leaks; transient state correctly cleared in tested round-trips. No findings beyond F-016-007/008 throughput pressure.
- **3.10 Testing & verification** — coverage is solid for the right-click action surface (`context_menu.rs` and `system_map.rs` are unit-tested action-by-action). The legacy SYSTEM/WORLD/ROUTES inspectors are *not* tested for command-bus dispatch — see F-016-001/F-016-003/F-016-004. Recommend a per-section test pattern: "edit field → undo → field is restored to original".
- **3.11 Documentation & maintainability** — module-level docstrings are good; per-function docs sparse. F-016-031, F-016-032, F-016-033, F-016-039, F-016-041.

## Summary of suggested fixes

- F-016-001 — HIGH — Inspector edits in `system.rs` bypass command bus — L/Medium
- F-016-002 — HIGH — AddWorld/Duplicate/AddWorldAtOrbit patch via iter_mut() — M/Low
- F-016-003 — HIGH — World inspector overwrites world/tags/notes/factions/claims directly — L/Medium
- F-016-004 — HIGH — Route inspector ReplaceRoutes carries empty `before` — S/Low
- F-016-005 — HIGH — Hidden-routes / ensure-connected ReplaceRoutes carries empty `before` — S/Low
- F-016-006 — HIGH — Multi-flip / single-flip control state bypasses bus — S/Low
- F-016-007 — HIGH — Per-frame `state.sector.routes.clone()` + ensure-connected preview in ROUTES tab — M/Low
- F-016-008 — HIGH — World inspector full presences/claims/control clone per frame — S/Low
- F-016-009 — HIGH — `feature_weights_for_world` rebuilds pool every frame, tags/notes per-frame join — M/Low
- F-016-010 — HIGH — Direct writes to transient UI fields violate command-bus rule literally — S/Low
- F-016-011 — MEDIUM — `apply_bulk_reseed` half-applies on failure — S/Low
- F-016-012 — MEDIUM — `to_vec()` on primary factions / routes lists per frame — S/Low
- F-016-013 — MEDIUM — World features BTreeSet of owned Strings per frame — S/Low
- F-016-014 — MEDIUM — `route_picker` per-route format allocations — S/Low
- F-016-015 — MEDIUM — `MultiDeleteAllConfirmed` clears selection on partial failure — S/Low
- F-016-016 — MEDIUM — `coord.q as u32` cast smuggles negatives through the bounds check — S/Low
- F-016-017 — MEDIUM — `sector_view_digest` rebuilds full Slice every frame — M/Low
- F-016-018 — MEDIUM — Route inspector lets `from_system_id == to_system_id` — S/Low
- F-016-019 — MEDIUM — Route id field accepts user text then silently re-canonicalises — S/Low
- F-016-020 — MEDIUM — Overlay/heatmap/lifeline computes run every map frame — M/Low
- F-016-021 — MEDIUM — Pin toggle semantics not undoable — S/Low
- F-016-022 — MEDIUM — World picker per-frame label allocations — S/Low
- F-016-023 — MEDIUM — Add `let Some(menu) = ...` guard in multi-selection render — XS/Low
- F-016-024 — MEDIUM — `menu_selection_override` runs O(N worlds) lookup per frame — S/Low
- F-016-025 — MEDIUM — Chronicle snapshot clones 4 owned strings per event per frame — S/Low
- F-016-026 — LOW — Route tag editor seeds literal "tag" string — S/Low
- F-016-027 — LOW — Use `is_some_and` over `map_or(false, ...)` — XS/None
- F-016-028 — LOW — `egui::Frame::none()` deprecated — XS/None
- F-016-029 — LOW — Inconsistent `iter().cloned().collect()` pattern — XS/None
- F-016-030 — LOW — RouteId accepts empty / whitespace — XS/None
- F-016-031 — LOW — `apply_sector_menu_action` doc misleading on early returns — XS/None
- F-016-032 — LOW — `SYS_STAR_GRID_ANCHOR` constant duplicated across two files — XS/None
- F-016-033 — LOW — `EraseRegion` silently no-ops with no owning region — XS/None
- F-016-034 — LOW — `feature_weights_for_world` keys map by `format!("{:?}", feature)` — S/Low
- F-016-035 — LOW — Repeated `ModalKind::Message(format!(...))` boilerplate — XS/None
- F-016-036 — LOW — `claim_chip_colours` raw RGBs not hoisted to palette — S/None
- F-016-037 — LOW — Route picker `selected_text` reallocates label per frame — XS/None
- F-016-038 — NIT — Debug-formatted enum variants in UI text — S/None
- F-016-039 — NIT — Inconsistent label casing across dialogs / menus — XS/None
- F-016-040 — NIT — `let _ = coord;` to silence unused — XS/None
- F-016-041 — NIT — `menu_anchor_pivot` truth-table match needs a comment — XS/None
- F-016-042 — NIT — Mixed `Vec2::new(0,0)` vs `vec2(0,0)` vs `Vec2::ZERO` — XS/None
- F-016-043 — NIT — `format!("{v:?}")` in combo loops allocates per option per frame — S/None
