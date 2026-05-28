---
unit_id: U017
crate: builder
paths:
  - builder/src/builder/panels/history.rs
  - builder/src/builder/panels/control.rs
  - builder/src/builder/panels/relations.rs
  - builder/src/builder/panels/factions.rs
  - builder/src/builder/panels/economy.rs
loc_reviewed: 5972
reviewed_by: agent
health_score: 2
finding_counts: { critical: 0, high: 7, medium: 15, low: 11, nit: 8 }
top_risks:
  - "All five panels bypass the command bus entirely — breaks undo/redo for every roster, chronicle, claim, presence, override, and faction edit (F-017-001, F-017-002, F-017-003, F-017-004, F-017-005)."
  - "history.rs:1409 manual event ids are derived from `std::collections::hash_map::DefaultHasher` — not version-stable, contaminates `sector.json` golden bytes (F-017-006)."
  - "Per-frame full clones of sector.systems / sector.factions / chronicle.events / relations.pairs in every panel render (F-017-007 .. F-017-011)."
  - "control.rs:1231,1240 `partial_cmp(...).unwrap()` on f32 panics on NaN (F-017-012)."
---

# Review: builder PANELS group B — history / control / relations / factions / economy

## Summary

Group B panels are functionally dense and well-organised by section markers, but they are the largest single source of command-bus and determinism-discipline violations in the builder crate. None of the five panels routes its writes through `state.run(BuilderCommand::...)`; every mutation pokes directly at `state.sector.*`, `state.data_catalogs.*`, or builder side-tables, so every edit performed here is invisible to undo/redo (§R4 violation, CLAUDE.md hard rule). On top of that, the panels lean on a copy-then-edit pattern in render code: each frame snapshots the entire faction roster / system list / chronicle / pair matrix into a fresh `Vec` whose sole purpose is to avoid a borrow conflict during the frame. With ~6k LOC of UI on selected entities this is a steady-state allocator pressure problem the moment a chronicle, roster, or matrix grows. There are also several smaller correctness issues (a non-version-stable `DefaultHasher` baked into chronicle event ids, two `partial_cmp(...).unwrap()` panics, `pop`/`focus_anchor` clones, deprecated egui APIs).

## Findings

### F-017-001 — [HIGH] [Command-bus] history.rs mutates `sector.chronicle.events` directly
- **Location:** `builder/src/builder/panels/history.rs:411-465` (events editor row click), `:487-647` (`show_selected_event_inspector` writes back to `ev.date`/`kind`/`era_label`/`weight`/`summary`/`narrative`/`factions`/`consequences`/`manual` via `&mut state.sector.chronicle.events[idx]`), `:632-646` (`delete`/`highlight`), `:908-918` (wizard commits an event with `state.sector.chronicle.events.push(...)` then re-sorts).
- **Category:** Project invariant / Command bus (§R4)
- **Confidence:** High
- **Blast radius:** Every chronicle edit, manual-event commit, and delete bypasses undo/redo. The CLAUDE.md rule is explicit: "Mutations in the builder always go through the command bus. Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel — that breaks undo/redo (§R4)."
- **Why it matters:** Users lose the ability to undo a chronicle change, and any side effect that other observers expect from `state.run(...)` (validation invalidation hooks, derivation cache, snapshots) is skipped or runs in the wrong order. The panel reaches for a stand-in helper `on_chronicle_mutated`, but that only sets dirty flags — it doesn't snapshot.
- **Evidence:** The `BuilderCommand` enum (builder/src/builder/command.rs:92-…) has no chronicle variants; this panel — like the other four in this unit — is the reason that gap matters.
- **Suggested fix:** Add `BuilderCommand::AddChronicleEvent { event }`, `RemoveChronicleEvent { id }`, `UpdateChronicleEvent { id, field-bundle }`, `SetChronicleConfig { cfg }` (and similar for eras / event_rules). Route every edit through `state.run(...)`. Keep `recompute_chronicle` as a separate `RecomputeChronicle` command (or non-undoable post-pass that runs after every chronicle command and is the only mutator of derived event fields).
- **Effort:** L (command surface + dispatch + tests; many edit sites)
- **Risk of fix:** Medium — large surface, but mechanical once one shape is agreed.

### F-017-002 — [HIGH] [Command-bus] control.rs mutates presence rows, claims, primary_factions, and system control state directly
- **Location:** `builder/src/builder/panels/control.rs:303-333` (presence edits + remove), `:428-446` (add presence row), `:504-543` (set system control state / primary factions / recompute), `:674-679` (apply_faction_power writes to `state.sector.factions`), `:826-832` (bulk-convert claims), `:855-873` (`apply_bulk_convert` mutates `c.claim_type` directly), `:1015-1018` (remove claim), `:1077-1087` (add claim).
- **Category:** Project invariant / Command bus (§R4)
- **Confidence:** High
- **Blast radius:** Every control / claim edit is non-undoable. Bulk convert in particular can rewrite hundreds of claims in one click with no snapshot.
- **Suggested fix:** Introduce `BuilderCommand::{AddPresence, RemovePresence, UpdatePresence, SetDominanceLock, AddClaim, RemoveClaim, BulkConvertClaims, SetSystemControlState, SetPrimaryFactions, ApplyFactionPower}`. Have each click in the panel build a command, then `state.run(...)` it.
- **Effort:** L
- **Risk of fix:** Medium.

### F-017-003 — [HIGH] [Command-bus] relations.rs mutates `data_catalogs.relations.overrides`, `pair_overrides`, `kind_rules`, `disposition_rules`, and `config.generation.relations` directly
- **Location:** `builder/src/builder/panels/relations.rs:483-499` (`upsert_override`), `:501-507` (`remove_override`), `:629-690` (`§REL5 pair_overrides` add/remove/edit), `:712-787` (`§REL3 kind_rules` add/remove/edit), `:791-872` (`§REL4 disposition_rules` add/remove/edit), `:120-166` (`feed_conflict` + `min_world_presence` edits write straight to `state.config.generation.relations.min_world_presence` and `data_catalogs.relations`).
- **Category:** Project invariant / Command bus (§R4)
- **Confidence:** High
- **Blast radius:** Diplomacy editing is one of the most-undone surfaces in any RPG editor; every change here is silently non-undoable.
- **Suggested fix:** Add a `BuilderCommand::SetRelationsCatalog { cfg }` (coarse) or, better, granular variants (`UpsertRelationOverride`, `RemoveRelationOverride`, `AddPairOverride`, `RemovePairOverride`, …, `SetRelationsMinWorldPresence`). Move the trailing `recompute_relations()` into a non-undoable post-pass triggered by the dispatcher.
- **Effort:** M
- **Risk of fix:** Low — the catalog is already cloned defensively in many places, command serialisation is straightforward.

### F-017-004 — [HIGH] [Command-bus] factions.rs mutates `data_catalogs.factions` and `selected_faction_id` directly
- **Location:** `builder/src/builder/panels/factions.rs:114-123` (scaffold roster), `:307-309` (`remove_id` -> `delete_row`), `:361-374` (inspector write-back), `:843-882` (`add_new_row`), `:884-913` (`duplicate_row`), `:915-932` (`delete_row`).
- **Category:** Project invariant / Command bus (§R4)
- **Confidence:** High
- **Blast radius:** Faction roster edits — add, duplicate, delete, identity, hierarchy, style overrides, legend visibility — none are undoable. This panel writes the on-disk source of truth (factions.toml) so silent edits are particularly costly.
- **Suggested fix:** `BuilderCommand::{AddFactionRow, DuplicateFactionRow, RemoveFactionRow, UpdateFactionDef}`. The existing `AddFaction`/`RemoveFaction` commands at command.rs:155-165 act on `sector.factions` (generated), not on `data_catalogs.factions` (catalog); the new variants must target the catalog.
- **Effort:** M
- **Risk of fix:** Low.

### F-017-005 — [HIGH] [Command-bus] economy.rs mutates `world_economy_overrides`, `world_strategic_overrides`, `system_*_overrides`, `data_catalogs.economy`, and `economy_*` toggles directly
- **Location:** `builder/src/builder/panels/economy.rs:268-275`, `:319-326` (clear / pin per-world overrides), `:395-428` (per-system tithe/supply/priority overrides), `:447-454` (clear-system), `:513-527` (lifeline toggle and min-score), `:573-580` (heatmap mode picker), `:598-608` (create defaults), `:643-650` (catalog change), `:663-717` (`by_world_type` add/remove/edit), `:720-748` (`by_tech_level`), `:750-778` (`by_population`).
- **Category:** Project invariant / Command bus (§R4)
- **Confidence:** High
- **Blast radius:** Same as F-017-001..004. The economy panel additionally triggers `state.recompute_economy()` on every drag-value change, so a single slider scrub silently mutates several side-tables and recomputes the full report many times per frame.
- **Suggested fix:** `BuilderCommand::{SetWorldEconomyOverride, ClearWorldEconomyOverride, SetWorldStrategicOverride, ClearWorldStrategicOverride, SetSystemTitheOverride, …, SetEconomyConfig}`. Also debounce the `recompute_economy()` call (only run on drag-released, or coalesce per-frame).
- **Effort:** M
- **Risk of fix:** Low — same shape as F-017-003.

### F-017-006 — [HIGH] [Determinism] Manual event ids use `std::collections::hash_map::DefaultHasher`
- **Location:** `builder/src/builder/panels/history.rs:1409-1415` (`hash_str`), used at `:953-957` (`build_manual_event` constructs `evt-manual-{slug}-{:x}`).
- **Category:** Project invariant (determinism / byte-stable output)
- **Confidence:** High
- **Blast radius:** The generated id ends up in `state.sector.chronicle.events` and from there in `sector.json` exports. `DefaultHasher` is explicitly **not** version-stable (the std docs warn that the algorithm and seed may change across Rust releases), and on the same compiler it is seeded from a process-global random state on first use — so two builders compiling against different toolchains, or even two processes in some patterns, will emit different ids for the same narrative. That kills the golden tests (`cargo test --test it -- golden`) the moment a manual event slips in, and it violates the workspace rule that all hash-derived ids go through `src/model/rng.rs` (blake3).
- **Suggested fix:** Replace `hash_str` with a `blake3::hash` of `(kind_slug, narrative, date, anchor-canonical-string, sorted faction list)` truncated to 64 bits, or thread the existing stage RNG (history wizard could carry a stable counter / seed contributed by the chronicle config).
  ```rust
  // before
  fn hash_str(s: &str) -> u64 {
      use std::collections::hash_map::DefaultHasher;
      let mut h = DefaultHasher::new();
      s.hash(&mut h);
      h.finish()
  }
  // after
  fn stable_hash(s: &str) -> u64 {
      let h = blake3::hash(s.as_bytes());
      u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap())
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — the test in the same file already accepts any `evt-manual-` prefix.

### F-017-007 — [HIGH] [Performance] Per-frame full clone of `chronicle.events` and `factions` in history panel
- **Location:** `builder/src/builder/panels/history.rs:437` (`let events = state.sector.chronicle.events.clone();` inside §H4 grid), `:566-571` (`factions_snapshot: Vec<(FactionId, String)>` rebuilt per frame), `:663-704` (wizard rebuilds `systems`, `worlds`, `routes`, `regions`, `factions` snapshots every frame even when the wizard is closed, and re-clones them all again when an event commits).
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — every frame the panel is visible. A 5k-event chronicle pays a 5k-row deep clone (each `HistoryEvent` carries `Vec<HistoryConsequence>`, `Vec<HistoryEntityRef>`, `Vec<FactionId>`, and several `String`s) just to iterate.
- **Suggested fix:** Iterate by index over `state.sector.chronicle.events.iter()` and stash only the click intents (`Vec<Intent>`) to apply at frame end — the panel already does this for `selected` (`selected_history_event`). For the wizard, only build the picker snapshots when `state.history_wizard.is_some()`.
- **Effort:** S
- **Risk of fix:** Low.

### F-017-008 — [HIGH] [Performance] Per-frame full clone of `sector.systems`, `sector.factions`, world `factions`, and `claims` in control panel
- **Location:** `builder/src/builder/panels/control.rs:168-174` (`let presences = …factions.clone(); let factions: Vec<(FactionId, String)> = …iter().map(...).collect();` inside `show_world_presence_editor`), `:199` (`for rel in state.sector.relations.pairs.clone()` — wrong file, this is actually relations.rs, ignore), `:380-413` (the same factions-snapshot pattern in `show_add_presence_row`), `:697-709` (`contested_worlds` walks every system/world/claim per frame and allocates a `BTreeSet<FactionId>` per world), `:757-762` (factions list rebuilt in `show_bulk_convert`), `:908-913` (same in `show_world_list`), `:954-964` (whole `w.claims.clone()` per world row).
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path. Bulk convert + contested + world list together scan all worlds × all claims twice per frame and allocate at least one `Vec` per row.
- **Suggested fix:** Cache `factions: Arc<[(FactionId, String)]>` on `BuilderState` (rebuilt on faction-roster mutation only). Stop cloning `w.claims`; iterate it by reference and emit only `(world_idx, claim_idx, action)` intents at frame end. Memoise `contested_worlds` behind a derivation key (`(sector_revision, claim_revision)`).
- **Effort:** M
- **Risk of fix:** Low.

### F-017-009 — [HIGH] [Performance] Per-frame clone of the entire relations matrix
- **Location:** `builder/src/builder/panels/relations.rs:199` (`for rel in state.sector.relations.pairs.clone()` inside the matrix grid), `:279-292` (cell editor `.cloned()` on a single pair — fine), `:330-447` (cell editor allocates fresh `RelationOverride`, six `String`s, multiple combos every frame even with no edits), `:616-617` (`let factions: Vec<FactionId> = state.sector.factions.iter().map(|f| f.id.clone()).collect();` per frame).
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path. The matrix grows O(F²) with faction count; for F=20 that's 190 rows cloned per frame including the dense `metrics` struct.
- **Suggested fix:** Iterate `state.sector.relations.pairs.iter()` by reference and stash a single `Option<(usize, ClickKind)>` for the click. Build the `factions` faction-id list once at the start of `show` and pass by `&`.
- **Effort:** S
- **Risk of fix:** Low.

### F-017-010 — [HIGH] [Performance] Per-frame iteration `state.sector.factions.iter().map(...).collect::<Vec<_>>()` repeated across all five panels
- **Location:** Repeats of the pattern `let factions: Vec<(FactionId, String)> = state.sector.factions.iter().map(|f| (f.id.clone(), f.name.to_string())).collect();` — `control.rs:169-174`, `:362-366`, `:757-762`, `:908-913`; `history.rs:566-571`, `:699-704`; `relations.rs:616`; `economy.rs:176-187` (world list rebuild every frame). Each `f.name.to_string()` is a `Arc<str> → String` copy.
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path; every panel rebuilds the same picker buffers from scratch every frame even with no edits.
- **Suggested fix:** Cache a `pub(crate) fn faction_picker_options(&self) -> &[(FactionId, Arc<str>)]` on `BuilderState`, invalidated when faction list changes (i.e. by routing roster edits through commands per F-017-004). All five panels can then borrow it.
- **Effort:** M
- **Risk of fix:** Low.

### F-017-011 — [HIGH] [Performance] Economy panel reruns `recompute_economy()` on every slider tick
- **Location:** `builder/src/builder/panels/economy.rs:268-275`, `:318-326`, `:395-428`, `:447-454` — each `if changed { state.recompute_economy(); }` fires inside a drag/slider edit. The recompute walks every system and world.
- **Category:** Performance / GUI frame budget
- **Confidence:** High
- **Blast radius:** Dragging a single slider can fire `recompute_economy` 30+ times per second; for a sector with hundreds of worlds, that's a multi-millisecond hit every frame.
- **Suggested fix:** Coalesce: set a `pending_recompute: bool` field and call it once at end of frame, or only when the slider response reports `drag_released()` rather than `changed()`. Same applies to history `recompute_chronicle` and relations `recompute_relations` (both already gate via `*_auto_recompute`, but that's user-facing).
- **Effort:** S
- **Risk of fix:** Low.

### F-017-012 — [MEDIUM] [Panics] `partial_cmp(...).unwrap()` on f32 in control overlay builder
- **Location:** `builder/src/builder/panels/control.rs:1231` and `:1240`.
- **Category:** Panics & failure surface
- **Confidence:** High that it panics if NaN reaches it; Low that NaN reaches it via the current edit paths (sliders clamp 0..=100). MEDIUM rather than HIGH because no current code path emits NaN, but the overlay also runs against `sector` loaded from JSON.
- **Why it matters:** A single NaN dimension loaded from a hand-edited JSON file crashes the MAP panel.
- **Suggested fix:** `.max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))`, used consistently with the existing pattern at `economy.rs:537-540`.
- **Effort:** XS
- **Risk of fix:** None.

### F-017-013 — [MEDIUM] [Idiomatic] Massive copy-paste of picker / chip / save-row patterns across the five panels
- **Location:** `history.rs:730-826` (anchor pickers — 5 near-identical combo blocks); `control.rs:401-447` (presence picker), `:780-836` (claim picker), `:1024-1087` (add-claim row); `relations.rs:698-708` (`faction_combo`) duplicated with `factions.rs:477-501` (`kind_combo`/`disposition_combo`); save rows at `history.rs:1189-1214`, `relations.rs:876-903`, `factions.rs:934-951`, `economy.rs:633-661` — all 4 do the same `add_enabled + save_project + modal-on-error` dance.
- **Category:** Idiomatic Rust / DRY (§3.7)
- **Confidence:** High
- **Blast radius:** Maintenance — 4 copies of the save logic, ~6 copies of the "build a `Vec<(Id, String)>` picker" idiom. Any bug fix (e.g. F-017-014 below) has to be made in N places.
- **Suggested fix:** Extract into `panels/mod.rs`:
  - `pub(crate) fn save_row(ui, state, label, default_path, current_path: &mut Option<String>, key: &str)` for the save row.
  - `pub(crate) fn id_picker<Id: Clone + Eq + fmt::Display>(ui, salt, options, value: &mut Id)` for the picker.
  - `pub(crate) fn pick_one<T: Copy + Eq>(ui, salt, options: &[(T, &str)], value: &mut Option<T>, none_label: &str) -> bool` for the (inherit)-style combos used in relations.rs.
- **Effort:** M
- **Risk of fix:** Low.

### F-017-014 — [MEDIUM] [Correctness] `data_catalogs.economy` "create defaults" sets `dirty` but not `dirty_files`
- **Location:** `builder/src/builder/panels/economy.rs:598-608` — creates the catalog and sets `state.dirty = true;` but never inserts the new path into `state.dirty_files`. The save-on-quit hook (or any "save only changed files" optimisation) will skip this file.
- **Category:** Correctness
- **Confidence:** High
- **Blast radius:** Quietly drops an entire freshly-scaffolded `economy.toml` from "files to save".
- **Suggested fix:** After setting `state.config.inputs.economy = Some("data/worlds/economy.toml".into());`, also `state.dirty_files.insert("data/worlds/economy.toml".to_string());`. Compare with the equivalent block in `factions.rs:114-123` which does it correctly.
- **Effort:** XS
- **Risk of fix:** None.

### F-017-015 — [MEDIUM] [Correctness] `show_world_override_editor` early-returns from inside a closure with side effects
- **Location:** `builder/src/builder/panels/economy.rs:268-275` and `:318-326`. The `return` statement is inside `egui::Frame::group(ui.style()).show(ui, |ui| { … if pinned && ui.button(...).clicked() { state.world_economy_overrides.remove(&world_id); state.recompute_economy(); return; } if changed { … } });`. The bare `return` exits the closure, not `show_world_override_editor`, so when the user clicks "Clear override" *and* a slider also reports `changed` in the same frame, the cleared-then-immediately-reinserted override wins.
- **Category:** Correctness
- **Confidence:** Medium (depends on egui event ordering; sliders rarely fire `changed` and a button click in the same frame, but possible)
- **Blast radius:** "Clear override" silently does nothing in rare repro scenarios.
- **Suggested fix:** Replace `return;` with explicit branch: collect both decisions, then `if cleared { remove; recompute; } else if changed { insert; recompute; }` after the frame.
- **Effort:** S
- **Risk of fix:** Low.

### F-017-016 — [MEDIUM] [Correctness] `show_cell_editor` early-returns from inside `Clear override` branch *without* persisting concurrent slider edits
- **Location:** `builder/src/builder/panels/relations.rs:425-429`. `if pinned && ui.button("Clear override for pair").clicked() { remove_override(state, &pair); on_catalog_edited(state); return; }` is inside the outer fn so the `return` does exit it — but it also discards `changed` that may have been set by the same frame's slider drag (the slider response is processed before the button). Net effect: a slider edit immediately followed by a click on "Clear" silently loses the edit (which is probably what the user wanted) — but also silently loses the recompute that the slider edit would have triggered, leaving the matrix out of date.
- **Category:** Correctness
- **Confidence:** Medium
- **Suggested fix:** Drop the `return;` and let the function fall through; the subsequent `if changed { upsert_override(...) }` will be skipped because nothing exists to upsert after the remove. Or, gate on the action flags explicitly: `if cleared { … } else if changed { … }`.
- **Effort:** XS
- **Risk of fix:** None.

### F-017-017 — [MEDIUM] [Performance / Determinism] `build_overlay_cells` returns `std::collections::HashMap` instead of `BTreeMap`/`FxHashMap`
- **Location:** `builder/src/builder/panels/control.rs:1119`, `:1131-1147`, `:1149-1178`, `:1188-1190`, `:1218-1258` — all build `std::collections::HashMap<SystemId, HeatCell>` with default `SipHash` hasher.
- **Category:** Performance + determinism caution
- **Confidence:** High
- **Blast radius:** The result is consumed as lookup only (gui-core/src/sector_view.rs:109 reads it by key), so this is not a determinism violation per §3 — but the SipHash randomisation cost per insert in a hot per-frame builder is wasteful, and the public `pub fn build_overlay_cells -> Option<HashMap<...>>` signature leaks a non-deterministic type onto a public API that may later be iterated for tests or exports.
- **Suggested fix:** Switch to `sectorforge::FxHashMap` (already aliased in `src/lib.rs`) for lookup-only callers, or `BTreeMap` if any caller plans to iterate for golden bytes. Update the public signature accordingly.
- **Effort:** S
- **Risk of fix:** Low.

### F-017-018 — [MEDIUM] [Idiomatic] `unwrap()` after `find` in wizard anchor construction
- **Location:** `builder/src/builder/panels/history.rs:993-1029` — five `.clone().unwrap()` calls (`anchor_system.clone().unwrap()`, `anchor_world`, etc.). Reachable only if the `wizard_anchor_ready` gate at `:925-933` is correct, but the panel calls `build_manual_event` from the commit button which is `add_enabled(ready, …)`. Still, a future refactor that loosens the gate would crash.
- **Category:** Panics
- **Confidence:** Medium (currently unreachable; brittle for future change)
- **Suggested fix:** Replace with `let-else` and an `anyhow::bail!` / log + early-return, e.g.:
  ```rust
  let Some(system_id) = w.anchor_system.clone() else { return None; };
  ```
  and have `build_manual_event` return `Option<HistoryEvent>`.
- **Effort:** S
- **Risk of fix:** Low.

### F-017-019 — [MEDIUM] [Idiomatic] `state.recompute_*` from inside per-row closures is invisible from the call graph
- **Location:** Same locations as F-017-011 in economy.rs, plus `relations.rs:444-447` (`if changed { upsert_override; on_catalog_edited; }` which internally may recompute), `history.rs:643-647`.
- **Category:** Idiomatic / maintainability
- **Why it matters:** A reader scanning a slider edit has to follow `on_catalog_edited` → `state.history_auto_recompute` → `state.recompute_chronicle` to see what the click costs. This hides the per-frame recompute behind two layers of indirection, which is why F-017-011 went unfixed.
- **Suggested fix:** Make recomputes explicit and queued: `state.queue_post_frame(Recompute::Chronicle)`; consume the queue at the top of `BuilderState::frame_end`. Removes the `auto_recompute` boolean and the slider-tick storm in one go.
- **Effort:** M
- **Risk of fix:** Medium — touches every panel that holds an auto-recompute toggle.

### F-017-020 — [MEDIUM] [Documentation] Five panels carry section markers (§H1, §C1, §REL1, …) in comments but no docs explaining what the markers reference
- **Location:** `history.rs:1-16` (the only one that lists the markers in the file header), `control.rs:1-18`, `relations.rs:1-22`, `economy.rs:1-18`, `factions.rs:1-6` (almost no markers).
- **Category:** Documentation / maintainability
- **Confidence:** High
- **Why it matters:** CLAUDE.md says to use `§<tag>` references against `docs/BUILDER_REQS.txt` / `IMPROVEMENT.txt` / etc., but none of the modules link back to that doc. A reader can't tell whether `§REL1` is from `BUILDER_REQS.txt` or `IMPROVEMENT.txt`.
- **Suggested fix:** Add one line at the top of each file: `//! See [docs/BUILDER_REQS.txt](../../../docs/BUILDER_REQS.txt) §H1..§H8 (or appropriate doc).`
- **Effort:** XS
- **Risk of fix:** None.

### F-017-021 — [LOW] [Idiomatic] Deprecated egui APIs
- **Location:** `factions.rs:682` and `:699` use `.id_source(salt)` (deprecated in egui 0.28, renamed `id_salt`). `control.rs:994-998` uses `egui::Frame::none()` plus `.rounding(4.0)` (both deprecated in egui 0.29 in favour of `Frame::NONE` / `Frame::default()` and `corner_radius`).
- **Category:** Idiomatic / dependency hygiene
- **Confidence:** High
- **Suggested fix:** `.id_source(salt)` → `.id_salt(salt)` (consistent with the rest of the codebase). `egui::Frame::none().rounding(4.0)` → `egui::Frame::NONE.corner_radius(4.0)` or `egui::Frame { rounding: 4.0.into(), ..Default::default() }`.
- **Effort:** XS
- **Risk of fix:** None.

### F-017-022 — [LOW] [Idiomatic] `unused_variables` warnings: `_w` / `_systems` / `_regions` / `path_label` / `has_path`
- **Location:** `history.rs:984-989` (`_systems`, `_regions` parameters), `:1127` (`_w`); `relations.rs:901` (`let _ = has_path;` to silence lint).
- **Category:** Idiomatic (dead code)
- **Suggested fix:** Drop the unused parameters from the signatures (changes 1 callsite each).
- **Effort:** XS

### F-017-023 — [LOW] [Idiomatic] `dead_code`: `show_filter_bar` takes `_state: &mut BuilderState` but only reads/writes `ui.data_mut`
- **Location:** `builder/src/builder/panels/factions.rs:177`. The `&mut` borrow is wasted.
- **Suggested fix:** `fn show_filter_bar(ui: &mut Ui)`. Then drop the call-side `state` arg at `:87`.
- **Effort:** XS

### F-017-024 — [LOW] [Performance] `count_bulk_matches` walks the entire sector every frame even when no edit is pending
- **Location:** `builder/src/builder/panels/control.rs:815-853`. The function runs on every frame inside the CL4 collapsing header even if the user is just hovering the section.
- **Category:** Performance
- **Suggested fix:** Cache `(faction, claim_type) → count` keyed on a `claims_revision: u64` that bumps whenever any claim mutates. Or only count inside the click handler.
- **Effort:** S

### F-017-025 — [LOW] [Idiomatic] `stance_combo` warns dead-code for the `STANCES` constant in some build configurations
- **Location:** `builder/src/builder/panels/relations.rs:44-51` — `STANCES` is only used inside `stance_combo` (`:573-586`); if the panel is feature-gated away the constant becomes dead.
- **Suggested fix:** Add `#[cfg_attr(not(test), allow(dead_code))]` or fold into the function as a `const`.
- **Effort:** XS

### F-017-026 — [LOW] [Idiomatic] `presence_changed` lives in panel code but is really a domain question
- **Location:** `builder/src/builder/panels/control.rs:344-349`. The note explains why it exists ("`WorldFactionPresence` doesn't derive `PartialEq`"), but the right fix is to either derive `PartialEq` on the model (it's `Arc<str>` — `PartialEq` is fine on `Arc<str>`) or to add an `impl WorldFactionPresence { pub fn edits_equal(&self, other: &Self) -> bool }` next to the type.
- **Suggested fix:** Either derive `PartialEq` on `WorldFactionPresence` in `sectorforge::sector_model` (preferred), or move this helper into the domain crate next to the type.
- **Effort:** S
- **Risk of fix:** Low — `Arc<str>` is `PartialEq`.

### F-017-027 — [LOW] [Idiomatic] `existing_override` calls `.cloned()` on a found `&RelationOverride`
- **Location:** `builder/src/builder/panels/relations.rs:468-481`. The cloned override is mutated for the editor; a frame later it's either upserted (clone discarded) or thrown away. The clone is necessary, just flag it as a structural cost of the override-snapshot model — consider a `RelationOverrideDraft` that holds field-by-field `Option<T>` and a `apply(&mut RelationOverride)` to avoid the full clone.
- **Suggested fix:** Sketch — `struct OverrideDraft { changes: BitFlags<…>, public: Option<RelationAttitude>, … }`; apply when the frame commits.
- **Effort:** M (defer unless overrides become hot)
- **Risk of fix:** Low.

### F-017-028 — [LOW] [Idiomatic] `format!("{:?}", …)` for user-facing labels
- **Location:** `relations.rs:218-221` (`rel.public_attitude` does have a `.label()`, used at 217 — but `:218` uses `attitude_color(rel.public_attitude), rel.public_attitude.label()` correctly; however `factions.rs:399` uses `disposition_combo(…)`'s combo emits `KNOWN_DISPOSITIONS` as raw strings); `control.rs:218`, `:265-280`, `:286-289`, `:483-501`, `:565-593` (`format!("{:?}", current)`); `economy.rs:553` (`risk {:?}`).
- **Category:** Idiomatic / API
- **Why it matters:** Debug formatting is not stable in any sense, and it bakes Rust-y CamelCase identifiers into the UI. `DominanceState`, `SystemState`, `Stance`, etc. already have `.label()` or `Display` impls in some cases — use them. For ones that don't, add `.label()` next to the type.
- **Suggested fix:** Replace `format!("{:?}", x)` in user-facing strings with `x.label()` (add the impl if missing).
- **Effort:** S

### F-017-029 — [LOW] [Documentation] Public helper `kind_label` is `pub(crate)` with no doc
- **Location:** `builder/src/builder/panels/history.rs:1266`. The function is reachable from other modules (`pub(crate)`) but has no `///` doc.
- **Suggested fix:** Add `/// Stable display label for a chronicle [`EventKind`]; round-trips with [`parse_event_kind_str`].`
- **Effort:** XS

### F-017-030 — [LOW] [Idiomatic] `if let Some(...).clone()` ladder where `let-else` reads better
- **Location:** `economy.rs:218-232`, `:611-616`, `factions.rs:227-229`. The `let Some(...) = ... else { … return; };` idiom is already used in the same files (e.g. `economy.rs:611`), so this is purely a consistency nit.
- **Suggested fix:** Use `let-else`.
- **Effort:** XS

### F-017-031 — [LOW] [Testing] No tests for the §H8 helper `world_chronicle_events`'s sort-stability with duplicate dates
- **Location:** `builder/src/builder/panels/history.rs:1422-1442` (function), `:1513-1556` (test only checks 1 event present).
- **Category:** Testing
- **Suggested fix:** Add a test seeding two events at the same date with different ids and assert they sort by id second.
- **Effort:** XS

### F-017-032 — [NIT] [Doc] Magic number `110` and `107` in `short_narrative`
- **Location:** `builder/src/builder/panels/history.rs:1177-1185`.
- **Suggested fix:** `const NARRATIVE_PREVIEW_CHARS: usize = 110;` and derive the take-count from it.

### F-017-033 — [NIT] [Doc] Magic numbers in `attitude_color`/`metric_text`/`tension_text`
- **Location:** `relations.rs:236-269`. Thresholds 70/40/15 and colour bands repeated.
- **Suggested fix:** Promote to named constants (`HIGH_TENSION`, `MED_TENSION`, …) so the tier breakpoints are searchable.

### F-017-034 — [NIT] [Doc] `EVENT_KINDS`, `SYSTEM_STATES`, `CLAIM_TYPES`, `INFLUENCE_TIERS`, `DOMINANCE_STATES`, `ATTITUDES`, `STANCES`, `TREATIES`, `TITHE_STATES`, `SUPPLY_RISKS`, `PRIORITIES` are file-private but each enum lives in a domain crate — they're tracking the enum manually
- **Location:** every file in this unit.
- **Why it matters:** When the domain crate adds a variant (e.g. a new `EventKind`), the panel silently doesn't display it. There is no exhaustive `match` to fail loudly.
- **Suggested fix:** Promote these to `pub const ALL: &[Self]` on each enum in the domain crate. Then the panel consumes the source of truth. Pair with `#[non_exhaustive]` on the public enum.
- **Effort:** M (cross-cutting, but high leverage).

### F-017-035 — [NIT] [Style] `Color32::from_rgb(…)` literals repeated
- **Location:** Every file uses `Color32::from_rgb(220, 170, 80)`, `(230, 90, 90)`, etc. for the same "warn" / "error" palette.
- **Suggested fix:** Centralise palette constants in `gui-core/src/palette.rs` (`pub const WARN_AMBER`, `DANGER_RED`, `GOOD_GREEN`, `MUTED_GRAY`). Saves about 40 magic-number triples.
- **Effort:** S

### F-017-036 — [NIT] [Style] Inconsistent button glyphs
- **Location:** `× remove` vs `×` vs `× clear` vs `× delete` appear in different rows of the same panel (e.g. control.rs `:204` `× remove`, `:1004` `×`, `:1015` no glyph, `:444` `× clear`).
- **Suggested fix:** Adopt one of {`✕`, `×`} consistently and centralise as `pub const REMOVE_GLYPH: &str = "✕";`.

### F-017-037 — [NIT] [Style] `let _ = id_salt;` swallow at relations.rs:607
- **Location:** `relations.rs:607` — variable accepted but unused. Either use it for `id_salt` (currently the `Slider`/`Checkbox` use default ids) or drop it from the signature.
- **Suggested fix:** Pass `id_salt` to the `Slider::new`-derived `Id` to avoid duplicate-id warnings, or remove the parameter from the signature. Currently every slider in the editor uses the same internal id because they share the same ui scope; egui silently de-duplicates but the cleanup is good practice.

### F-017-038 — [NIT] [Style] `let _ = anchor;` at history.rs:636
- **Location:** `history.rs:636`. Variable is cloned then immediately discarded. Drop the clone.
- **Suggested fix:** Remove lines `:633` (`let anchor = …;`) and `:636` (`let _ = anchor;`).

### F-017-039 — [NIT] [Idiomatic] `Vec::with_capacity` not used where size is obviously known
- **Location:** `history.rs:1146`, `factions.rs:259` allocate `Vec::new()` for known-size results. Tiny per-frame nits compared to F-017-007..010.
- **Suggested fix:** `Vec::with_capacity(state.sector.chronicle.events.len())`.

## §3 rubric coverage

- **3.1 Panics & failure surface** — Findings F-017-012 (partial_cmp.unwrap), F-017-018 (5 `.clone().unwrap()` in wizard). `expect("checked")` at factions.rs:129, 316, 363, etc. is gated by an `is_none()` check on the same field and is acceptable.
- **3.2 unsafe & soundness** — No findings. No `unsafe` blocks in any of the five files.
- **3.3 Ownership / borrowing / lifetimes / cloning** — Findings F-017-007..010 (per-frame collect/clone), F-017-027 (override clone), F-017-038 (dead clone). The clone-then-edit pattern is structural to avoid double-borrows in egui closures; flagging only the egregious ones per brief.
- **3.4 Error handling** — Findings: `let _ = state.run(cmd)` is not used in these files because they don't use the command bus at all (see F-017-001..005). Save errors are correctly surfaced via `ModalKind::Message` in factions/economy/relations/history. No findings beyond the command-bus thread.
- **3.5 Concurrency & async** — N/A. Single-threaded UI; no async, no thread spawns.
- **3.6 Performance** — Findings F-017-007..011, F-017-017, F-017-024.
- **3.7 Idiomatic Rust & API design** — Findings F-017-013 (copy-paste), F-017-019 (hidden recompute), F-017-021 (deprecated egui), F-017-022..023, F-017-026, F-017-028, F-017-030, F-017-034 (manual enum lists).
- **3.8 Dependencies & Cargo hygiene** — No unused imports detected. `std::collections::{HashMap, BTreeSet, BTreeMap}` are imported per-use which is fine. F-017-017 is the only over-broad type choice.
- **3.9 Memory & resource management** — No `Drop`/RAII / static-mut concerns. Side-tables (`world_economy_overrides`, `system_*_overrides`, `dominance_locked`, `primary_factions_locked`) grow without eviction, but each is keyed by an id that goes away when the underlying world/system is removed — a memory leak only if commands ever remove a world without cleaning these up. Worth covering in U018 or U019 (the side-table owners).
- **3.10 Testing** — Inline tests cover the model-level invariants (chronicle preserves manual events, contested-world predicate, override pinning, economy override pinning) but exercise none of the panel render code. F-017-031.
- **3.11 Documentation** — Findings F-017-020 (section-marker provenance), F-017-029, F-017-032..033 (magic numbers).

## Summary of suggested fixes

- F-017-001 — HIGH — Route history panel writes through `BuilderCommand`s (`AddChronicleEvent`, `RemoveChronicleEvent`, `UpdateChronicleEvent`, `SetChronicleConfig`, …) — L / Medium
- F-017-002 — HIGH — Route control panel writes through `BuilderCommand`s for presence / claim / system-control mutations — L / Medium
- F-017-003 — HIGH — Route relations panel writes through `BuilderCommand`s for relations catalog mutations — M / Low
- F-017-004 — HIGH — Route factions panel catalog edits through `BuilderCommand`s (`AddFactionRow`, `UpdateFactionDef`, …) — M / Low
- F-017-005 — HIGH — Route economy panel overrides / catalog through `BuilderCommand`s — M / Low
- F-017-006 — HIGH — Replace `DefaultHasher` in `hash_str` with `blake3` truncation; restores byte-stability — S / Low
- F-017-007 — HIGH — Stop full-cloning `chronicle.events` / `factions` per frame; iterate by reference and defer intents — S / Low
- F-017-008 — HIGH — Stop per-frame cloning in control panel; memoise `contested_worlds`; share faction snapshot — M / Low
- F-017-009 — HIGH — Iterate `state.sector.relations.pairs` by reference in the matrix grid — S / Low
- F-017-010 — HIGH — Cache faction picker options on `BuilderState`; share across the five panels — M / Low
- F-017-011 — HIGH — Coalesce `recompute_economy` (and chronicle/relations) per frame — S / Low
- F-017-012 — MEDIUM — `.partial_cmp(...).unwrap_or(Equal)` in control overlay builder — XS / None
- F-017-013 — MEDIUM — Extract save_row + id_picker + pick_one helpers in `panels/mod.rs` — M / Low
- F-017-014 — MEDIUM — Insert into `dirty_files` when scaffolding `economy.toml` — XS / None
- F-017-015 — MEDIUM — Replace bare `return` in nested closure with explicit branch in economy world override editor — S / Low
- F-017-016 — MEDIUM — Drop early `return;` in relations cell editor Clear branch — XS / None
- F-017-017 — MEDIUM — Switch `build_overlay_cells` return type to `FxHashMap`/`BTreeMap` — S / Low
- F-017-018 — MEDIUM — Replace `.unwrap()` ladder in `wizard_anchor` with `let-else` and `Option` return — S / Low
- F-017-019 — MEDIUM — Move auto-recompute into a queued post-frame pass instead of per-edit calls — M / Medium
- F-017-020 — MEDIUM — Link each panel's `§` markers to their source doc — XS / None
- F-017-021 — LOW — Migrate deprecated egui APIs (`id_source` → `id_salt`, `Frame::none/rounding` → `Frame::NONE/corner_radius`) — XS / None
- F-017-022 — LOW — Remove unused parameters / `let _ = …` silences — XS / None
- F-017-023 — LOW — `show_filter_bar` should take `&mut Ui` only — XS / None
- F-017-024 — LOW — Cache `count_bulk_matches` against a claims revision — S / Low
- F-017-025 — LOW — Quiet potential dead-code on `STANCES` — XS / None
- F-017-026 — LOW — Derive `PartialEq` on `WorldFactionPresence` or move `presence_changed` next to the type — S / Low
- F-017-027 — LOW — Track override edits as a draft instead of cloning full `RelationOverride` — M / Low
- F-017-028 — LOW — Replace `format!("{:?}", x)` in user-facing labels with `.label()` — S / —
- F-017-029 — LOW — Doc-string `kind_label` (and friends) — XS / None
- F-017-030 — LOW — `let-else` consistency in economy / factions — XS / None
- F-017-031 — LOW — Test `world_chronicle_events` sort stability under duplicate dates — XS / None
- F-017-032 — NIT — Name magic constants in `short_narrative` — XS
- F-017-033 — NIT — Name colour thresholds in relations badges — XS
- F-017-034 — NIT — Hoist `ALL`/`VARIANTS` slices into the domain enums; mark them `#[non_exhaustive]` — M
- F-017-035 — NIT — Centralise `WARN_AMBER`/`DANGER_RED`/`GOOD_GREEN` constants in `gui-core/src/palette.rs` — S
- F-017-036 — NIT — Adopt a single remove-glyph string — XS
- F-017-037 — NIT — Use or remove `id_salt` parameter on `u8_slider` in relations.rs — XS
- F-017-038 — NIT — Drop dead `anchor` clone at history.rs:633-636 — XS
- F-017-039 — NIT — `Vec::with_capacity` where size is known — XS
